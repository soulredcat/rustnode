//! Forwards an allowlisted HTTP request to the official node unchanged.

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{self, HeaderMap, HeaderName, HeaderValue};
use hyper::{Request, Response, StatusCode, Uri};

use crate::{GatewayBody, State, json_error};

/// Headers that describe one hop and must not be copied across the proxy.
const HOP_BY_HOP: [HeaderName; 8] = [
    header::CONNECTION,
    HeaderName::from_static("keep-alive"),
    header::PROXY_AUTHENTICATE,
    header::PROXY_AUTHORIZATION,
    header::TE,
    header::TRAILER,
    header::TRANSFER_ENCODING,
    header::UPGRADE,
];

pub(crate) struct Forwarded {
    pub(crate) response: Response<GatewayBody>,
    pub(crate) upstream_error: bool,
}

/// A request that could not be forwarded, rendered as a JSON error reply.
pub(crate) struct Failure {
    status: StatusCode,
    detail: String,
    upstream_error: bool,
}

impl From<Failure> for Forwarded {
    fn from(failure: Failure) -> Self {
        Self {
            response: json_error(failure.status, &failure.detail),
            upstream_error: failure.upstream_error,
        }
    }
}

/// Sends the request to the official node. Hop-by-hop headers are already
/// stripped from the returned response.
/// The request with its body read, within `max_request_bytes`.
pub(crate) async fn read(
    state: &State,
    request: Request<Incoming>,
) -> Result<(hyper::http::request::Parts, Bytes), Failure> {
    let (parts, body) = request.into_parts();

    let declared_too_large = parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > state.config.max_request_bytes as u64);
    if declared_too_large {
        return Err(client_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body too large",
        ));
    }

    let body = match Limited::new(body, state.config.max_request_bytes)
        .collect()
        .await
    {
        Ok(collected) => collected.to_bytes(),
        Err(error) if error.is::<http_body_util::LengthLimitError>() => {
            return Err(client_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large",
            ));
        }
        Err(_) => {
            return Err(client_error(
                StatusCode::BAD_REQUEST,
                "could not read request body",
            ));
        }
    };
    Ok((parts, body))
}

/// Sends a request whose body was already read.
pub(crate) async fn send_parts(
    state: &State,
    parts: hyper::http::request::Parts,
    body: Bytes,
) -> Result<Response<Incoming>, Failure> {
    let path_and_query = parts
        .uri
        .path_and_query()
        .map_or("/", |value| value.as_str());
    let Ok(uri) = format!("{}{path_and_query}", state.upstream.http_base).parse::<Uri>() else {
        return Err(client_error(
            StatusCode::BAD_REQUEST,
            "invalid request target",
        ));
    };

    let mut upstream_request = Request::new(Full::new(body));
    *upstream_request.method_mut() = parts.method;
    *upstream_request.uri_mut() = uri;
    copy_end_to_end_headers(&parts.headers, upstream_request.headers_mut());
    upstream_request.headers_mut().remove(header::HOST);
    upstream_request
        .headers_mut()
        .remove(header::CONTENT_LENGTH);

    let sent = tokio::time::timeout(
        state.config.upstream_timeout,
        state.client.request(upstream_request),
    )
    .await;
    match sent {
        Ok(Ok(mut response)) => {
            strip_hop_by_hop(response.headers_mut());
            Ok(response)
        }
        Ok(Err(error)) => Err(upstream_failure(
            StatusCode::BAD_GATEWAY,
            &format!("official node unavailable: {error}"),
        )),
        Err(_) => Err(upstream_failure(
            StatusCode::GATEWAY_TIMEOUT,
            "official node did not answer in time",
        )),
    }
}

/// Forwards the request and streams the official response back.
pub(crate) async fn forward(state: &State, request: Request<Incoming>) -> Forwarded {
    match read(state, request).await {
        Ok((parts, body)) => forward_parts(state, parts, body).await,
        Err(failure) => failure.into(),
    }
}

/// Forwards a request whose body was already read.
pub(crate) async fn forward_parts(
    state: &State,
    parts: hyper::http::request::Parts,
    body: Bytes,
) -> Forwarded {
    match send_parts(state, parts, body).await {
        Ok(response) => Forwarded {
            response: response.map(|body| body.map_err(Into::into).boxed()),
            upstream_error: false,
        },
        Err(failure) => failure.into(),
    }
}

fn copy_end_to_end_headers(source: &HeaderMap<HeaderValue>, target: &mut HeaderMap<HeaderValue>) {
    for (name, value) in source {
        target.append(name.clone(), value.clone());
    }
    strip_hop_by_hop(target);
}

fn strip_hop_by_hop(headers: &mut HeaderMap<HeaderValue>) {
    let listed: Vec<HeaderName> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
        .collect();
    for name in listed.iter().chain(HOP_BY_HOP.iter()) {
        headers.remove(name);
    }
}

fn client_error(status: StatusCode, detail: &str) -> Failure {
    Failure {
        status,
        detail: detail.to_owned(),
        upstream_error: false,
    }
}

pub(crate) fn upstream_failure(status: StatusCode, detail: &str) -> Failure {
    Failure {
        status,
        detail: detail.to_owned(),
        upstream_error: true,
    }
}

#[cfg(test)]
mod tests {
    use hyper::header::{self, HeaderMap, HeaderValue};

    use super::strip_hop_by_hop;

    #[test]
    fn hop_by_hop_and_connection_listed_headers_are_removed() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONNECTION,
            HeaderValue::from_static("keep-alive, x-trace"),
        );
        headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));
        headers.insert("x-trace", HeaderValue::from_static("1"));
        headers.insert(
            header::TRANSFER_ENCODING,
            HeaderValue::from_static("chunked"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("secret"));
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        strip_hop_by_hop(&mut headers);

        assert!(headers.get(header::CONNECTION).is_none());
        assert!(headers.get("keep-alive").is_none());
        assert!(headers.get("x-trace").is_none());
        assert!(headers.get(header::TRANSFER_ENCODING).is_none());
        assert_eq!(headers.get("x-api-key").unwrap(), "secret");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }
}
