//! `/ws` passthrough: the official node's JSON-RPC subscription stream, relayed
//! frame by frame in both directions.

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::http::Response as HttpResponse;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use tokio_tungstenite::{WebSocketStream, connect_async_with_config};

use crate::{API_KEY_HEADER, GatewayBody, State, empty_body, full_body, json_error};

pub(crate) struct Upgraded {
    pub(crate) response: Response<GatewayBody>,
    pub(crate) upstream_error: bool,
}

pub(crate) async fn upgrade(state: &State, mut request: Request<Incoming>) -> Upgraded {
    let Some(key) = websocket_key(&request) else {
        return Upgraded {
            response: json_error(StatusCode::BAD_REQUEST, "expected a WebSocket upgrade"),
            upstream_error: false,
        };
    };

    let path_and_query = request
        .uri()
        .path_and_query()
        .map_or("/ws", |value| value.as_str());
    let Ok(mut upstream_request) =
        format!("{}{path_and_query}", state.upstream.ws_base).into_client_request()
    else {
        return Upgraded {
            response: json_error(StatusCode::BAD_REQUEST, "invalid request target"),
            upstream_error: false,
        };
    };
    if let Some(api_key) = request.headers().get(API_KEY_HEADER) {
        upstream_request
            .headers_mut()
            .insert(API_KEY_HEADER, api_key.clone());
    }

    let config = ws_config(state.config.max_ws_message_bytes);
    let connected = tokio::time::timeout(
        state.config.upstream_timeout,
        connect_async_with_config(upstream_request, Some(config), true),
    )
    .await;
    let upstream = match connected {
        Ok(Ok((stream, _response))) => stream,
        Ok(Err(WsError::Http(rejection))) => {
            return Upgraded {
                response: official_rejection(*rejection),
                upstream_error: false,
            };
        }
        Ok(Err(error)) => {
            return Upgraded {
                response: json_error(
                    StatusCode::BAD_GATEWAY,
                    &format!("official node WebSocket unavailable: {error}"),
                ),
                upstream_error: true,
            };
        }
        Err(_) => {
            return Upgraded {
                response: json_error(
                    StatusCode::GATEWAY_TIMEOUT,
                    "official node WebSocket did not answer in time",
                ),
                upstream_error: true,
            };
        }
    };

    let on_upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        match on_upgrade.await {
            Ok(upgraded) => {
                let client = WebSocketStream::from_raw_socket(
                    TokioIo::new(upgraded),
                    Role::Server,
                    Some(config),
                )
                .await;
                relay(client, upstream).await;
            }
            Err(error) => eprintln!("api /ws upgrade failed: {error}"),
        }
    });

    let mut response = Response::new(empty_body());
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    let headers = response.headers_mut();
    headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(accept) = HeaderValue::from_str(&derive_accept_key(key.as_bytes())) {
        headers.insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    }
    Upgraded {
        response,
        upstream_error: false,
    }
}

/// The official node refused the upgrade (missing API key, WebSocket disabled,
/// ...): the client gets that same status, content type, and body.
fn official_rejection(rejection: HttpResponse<Option<Vec<u8>>>) -> Response<GatewayBody> {
    let (parts, body) = rejection.into_parts();
    let mut response = Response::new(full_body(Bytes::from(body.unwrap_or_default())));
    *response.status_mut() = parts.status;
    if let Some(content_type) = parts.headers.get(header::CONTENT_TYPE) {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type.clone());
    }
    response
}

fn websocket_key(request: &Request<Incoming>) -> Option<HeaderValue> {
    let headers = request.headers();
    let has_token = |name: header::HeaderName, token: &str| {
        headers.get_all(name).iter().any(|value| {
            value.to_str().is_ok_and(|text| {
                text.split(',')
                    .any(|part| part.trim().eq_ignore_ascii_case(token))
            })
        })
    };
    let version_13 = headers
        .get(header::SEC_WEBSOCKET_VERSION)
        .is_some_and(|value| value == "13");
    if has_token(header::UPGRADE, "websocket")
        && has_token(header::CONNECTION, "upgrade")
        && version_13
    {
        headers.get(header::SEC_WEBSOCKET_KEY).cloned()
    } else {
        None
    }
}

fn ws_config(max_message_bytes: usize) -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(max_message_bytes))
        .max_frame_size(Some(max_message_bytes))
}

/// Relays data and close frames until either side closes or fails, then closes
/// the other side. Ping/Pong stay hop-local: each side's library answers them.
async fn relay<C, U>(client: WebSocketStream<C>, upstream: WebSocketStream<U>)
where
    C: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    U: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();

    let client_to_upstream = async {
        while let Some(Ok(message)) = client_rx.next().await {
            let closing = matches!(message, Message::Close(_));
            if forwards(&message) && upstream_tx.send(message).await.is_err() {
                break;
            }
            if closing {
                break;
            }
        }
    };
    let upstream_to_client = async {
        while let Some(Ok(message)) = upstream_rx.next().await {
            let closing = matches!(message, Message::Close(_));
            if forwards(&message) && client_tx.send(message).await.is_err() {
                break;
            }
            if closing {
                break;
            }
        }
    };

    tokio::select! {
        () = client_to_upstream => {}
        () = upstream_to_client => {}
    }
    let _ = client_tx.close().await;
    let _ = upstream_tx.close().await;
}

fn forwards(message: &Message) -> bool {
    matches!(
        message,
        Message::Text(_) | Message::Binary(_) | Message::Close(_)
    )
}
