//! End-to-end gateway tests against a mock official node.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use alephium_api::Gateway;
use alephium_conf::ApiConfig;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse as WsErrorResponse, Request as WsRequest, Response as WsResponse,
};

#[derive(Default)]
struct Upstream {
    http_requests: AtomicUsize,
    ws_api_key: std::sync::Mutex<Option<String>>,
}

/// Mock official node: echoes HTTP requests as JSON and echoes `/ws` text frames.
async fn spawn_upstream() -> (SocketAddr, Arc<Upstream>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind upstream");
    let address = listener.local_addr().expect("upstream address");
    let seen = Arc::new(Upstream::default());
    let state = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let state = Arc::clone(&state);
            tokio::spawn(async move {
                let mut head = [0u8; 1024];
                let peeked = stream.peek(&mut head).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&head[..peeked]).to_ascii_lowercase();
                if text.starts_with("get /ws") && text.contains("upgrade: websocket") {
                    serve_ws(stream, &state).await;
                } else {
                    let service = service_fn(move |request| {
                        let state = Arc::clone(&state);
                        async move { Ok::<_, Infallible>(echo(&state, request).await) }
                    });
                    let _ = http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                }
            });
        }
    });
    (address, seen)
}

async fn echo(state: &Upstream, request: Request<Incoming>) -> Response<Full<Bytes>> {
    state.http_requests.fetch_add(1, Ordering::SeqCst);
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = request
        .into_body()
        .collect()
        .await
        .map(|body| body.to_bytes())
        .unwrap_or_default();

    if query.contains("slow=1") {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    if path.ends_with("/group") {
        return Response::new(Full::new(Bytes::from_static(b"{\"group\":3}")));
    }
    if query.contains("txId=missing") {
        let mut response =
            Response::new(Full::new(Bytes::from_static(b"{\"detail\":\"not found\"}")));
        *response.status_mut() = StatusCode::NOT_FOUND;
        return response;
    }
    let json = format!(
        "{{\"method\":\"{method}\",\"path\":\"{path}\",\"query\":\"{query}\",\"apiKey\":\"{api_key}\",\"body\":\"{}\"}}",
        String::from_utf8_lossy(&body)
    );
    let mut response = Response::new(Full::new(Bytes::from(json)));
    response
        .headers_mut()
        .insert("content-type", "application/json".parse().expect("header"));
    response
}

// The handshake callback's error type is fixed by tungstenite's `Callback` trait.
#[allow(clippy::result_large_err)]
async fn serve_ws(stream: tokio::net::TcpStream, state: &Upstream) {
    let callback = |request: &WsRequest, response: WsResponse| {
        let key = request
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if key.as_deref() == Some("wrong") {
            let mut rejection =
                WsErrorResponse::new(Some("{\"detail\":\"Invalid api key\"}".into()));
            *rejection.status_mut() = StatusCode::UNAUTHORIZED;
            return Err(rejection);
        }
        *state.ws_api_key.lock().expect("lock") = key;
        Ok(response)
    };
    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(stream, callback).await else {
        return;
    };
    while let Some(Ok(message)) = socket.next().await {
        match message {
            Message::Text(text) => {
                let reply = format!("upstream:{}", text.as_str());
                if socket.send(Message::Text(reply.into())).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

async fn spawn_gateway(upstream: &str, tweak: impl FnOnce(&mut ApiConfig)) -> SocketAddr {
    let mut config = ApiConfig {
        bind_address: "127.0.0.1:0".to_owned(),
        upstream_url: upstream.to_owned(),
        ..ApiConfig::default()
    };
    tweak(&mut config);
    let gateway = Gateway::bind(config).await.expect("bind gateway");
    let address = gateway.local_addr().expect("gateway address");
    tokio::spawn(gateway.run());
    address
}

fn client() -> Client<HttpConnector, Full<Bytes>> {
    Client::builder(TokioExecutor::new()).build(HttpConnector::new())
}

async fn send(
    method: &str,
    url: String,
    body: &'static [u8],
    api_key: Option<&str>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(url);
    if let Some(key) = api_key {
        builder = builder.header("x-api-key", key);
    }
    let request = builder
        .body(Full::new(Bytes::from_static(body)))
        .expect("request");
    let response = client().request(request).await.expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn forwards_get_with_query_and_api_key() {
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/blockflow/blocks?fromTs=1&toTs=2"),
        b"",
        Some("secret"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"method\":\"GET\""), "{body}");
    assert!(body.contains("\"path\":\"/blockflow/blocks\""), "{body}");
    assert!(body.contains("\"query\":\"fromTs=1&toTs=2\""), "{body}");
    assert!(body.contains("\"apiKey\":\"secret\""), "{body}");
}

#[tokio::test]
async fn forwards_post_body() {
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, body) = send(
        "POST",
        format!("http://{gateway}/contracts/call-contract"),
        b"group-0",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"method\":\"POST\""), "{body}");
    assert!(body.contains("\"body\":\"group-0\""), "{body}");
}

#[tokio::test]
async fn upstream_status_and_body_pass_through() {
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/transactions/status?txId=missing"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "{\"detail\":\"not found\"}");
}

#[tokio::test]
async fn endpoints_outside_the_surface_never_reach_upstream() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, _) = send(
        "GET",
        format!("http://{gateway}/infos/self-clique"),
        b"",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send("POST", format!("http://{gateway}/wallets"), b"{}", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(
        "GET",
        format!("http://{gateway}/transactions/submit"),
        b"",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn oversized_body_is_rejected_before_forwarding() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.max_request_bytes = 4;
    })
    .await;

    let (status, _) = send(
        "POST",
        format!("http://{gateway}/transactions/submit"),
        b"0123456789",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unreachable_upstream_is_bad_gateway() {
    let closed = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = closed.local_addr().expect("address");
    drop(closed);
    let gateway = spawn_gateway(&format!("http://{address}"), |_| {}).await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/mempool/transactions"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(body.starts_with("{\"detail\":"), "{body}");
}

#[tokio::test]
async fn slow_upstream_is_gateway_timeout() {
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.upstream_timeout = Duration::from_millis(100);
    })
    .await;

    let (status, _) = send(
        "GET",
        format!("http://{gateway}/blockflow/blocks?slow=1"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
}

#[tokio::test]
async fn invalid_upstream_urls_are_rejected() {
    for url in [
        "https://127.0.0.1:12973",
        "http://127.0.0.1:12973/api",
        "127.0.0.1:12973",
    ] {
        let config = ApiConfig {
            bind_address: "127.0.0.1:0".to_owned(),
            upstream_url: url.to_owned(),
            ..ApiConfig::default()
        };
        assert!(Gateway::bind(config).await.is_err(), "{url}");
    }
}

#[tokio::test]
async fn websocket_relays_both_ways_and_forwards_api_key() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let mut request = format!("ws://{gateway}/ws")
        .into_client_request()
        .expect("ws request");
    request
        .headers_mut()
        .insert("x-api-key", "secret".parse().expect("header"));
    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("connect through gateway");

    let subscribe = r#"{"jsonrpc":"2.0","id":1,"method":"subscribe","params":["block"]}"#;
    socket
        .send(Message::Text(subscribe.into()))
        .await
        .expect("send");
    let reply = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("reply in time")
        .expect("open")
        .expect("message");

    assert_eq!(reply, Message::Text(format!("upstream:{subscribe}").into()));
    assert_eq!(
        seen.ws_api_key.lock().expect("lock").as_deref(),
        Some("secret")
    );
    socket.close(None).await.expect("close");
}

#[tokio::test]
async fn websocket_rejected_by_upstream_keeps_the_official_status_and_body() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let mut request = format!("ws://{gateway}/ws")
        .into_client_request()
        .expect("ws request");
    request
        .headers_mut()
        .insert("x-api-key", "wrong".parse().expect("header"));
    let error = tokio_tungstenite::connect_async(request)
        .await
        .expect_err("upstream rejects the key");

    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("expected an HTTP rejection, got {error}");
    };
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.body().as_deref(),
        Some(&b"{\"detail\":\"Invalid api key\"}"[..])
    );
    assert_eq!(*seen.ws_api_key.lock().expect("lock"), None);
}

#[tokio::test]
async fn websocket_without_upgrade_headers_is_bad_request() {
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, _) = send("GET", format!("http://{gateway}/ws"), b"", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// Added latency of the gateway: the same request sent directly to the mock
/// upstream and through the gateway. Run with
/// `cargo test --release -p alephium-api --test gateway -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "measurement, not a correctness test"]
async fn measure_added_latency() {
    const ROUNDS: usize = 5_000;
    let (upstream, _) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;
    let client = client();

    let mut direct = Vec::with_capacity(ROUNDS);
    let mut proxied = Vec::with_capacity(ROUNDS);
    for round in 0..ROUNDS * 2 {
        let through_gateway = round % 2 == 1;
        let target = if through_gateway { gateway } else { upstream };
        let request = Request::get(format!("http://{target}/mempool/transactions"))
            .body(Full::new(Bytes::new()))
            .expect("request");
        let started = Instant::now();
        let response = client.request(request).await.expect("response");
        let _ = response.into_body().collect().await.expect("body");
        let elapsed = started.elapsed().as_micros();
        if through_gateway {
            proxied.push(elapsed);
        } else {
            direct.push(elapsed);
        }
    }
    direct.sort_unstable();
    proxied.sort_unstable();
    let percentile = |values: &[u128], p: usize| values[(values.len() - 1) * p / 100];
    for p in [50, 95, 99] {
        println!(
            "p{p}: direct {} us, via gateway {} us, added {} us",
            percentile(&direct, p),
            percentile(&proxied, p),
            percentile(&proxied, p).saturating_sub(percentile(&direct, p))
        );
    }
}

/// Official `AddressSpec` P2PKH vector; its group is 1 on a 4-group network.
const P2PKH_ADDRESS: &str = "1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3";

/// `address-group` is verified against the official node, so it runs
/// natively with no `--route-mode` given.
#[tokio::test]
async fn address_group_is_native_by_default() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |_| {}).await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/addresses/{P2PKH_ADDRESS}/group"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "{\"group\":1}");
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn native_mode_defers_addresses_it_cannot_answer() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.route_modes = vec!["address-group=native".to_owned()];
    })
    .await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/addresses/{P2PKH_ADDRESS}:2/group"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "{\"group\":3}");
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 1);
}

/// With an API key configured, only requests carrying exactly that key are
/// answered natively; a missing or wrong key goes to the official node, which
/// returns its own error.
#[tokio::test]
async fn native_answers_only_requests_the_official_key_check_accepts() {
    const KEY: &str = "0123456789abcdef0123456789abcdef";
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.route_modes = vec!["address-group=native".to_owned()];
        config.api_key = Some(KEY.to_owned());
    })
    .await;
    let url = format!("http://{gateway}/addresses/{P2PKH_ADDRESS}/group");

    let (status, body) = send("GET", url.clone(), b"", Some(KEY)).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "{\"group\":1}"));
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 0);

    for key in [
        None,
        Some("wrong"),
        Some("0123456789abcdef0123456789abcdeX"),
    ] {
        let (status, body) = send("GET", url.clone(), b"", key).await;
        assert_eq!(
            (status, body.as_str()),
            (StatusCode::OK, "{\"group\":3}"),
            "{key:?}"
        );
    }
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 3);
}

/// Without a configured key the official node rejects any request carrying
/// one, so such requests are not answered natively either.
#[tokio::test]
async fn native_without_configured_key_defers_requests_carrying_one() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.route_modes = vec!["address-group=native".to_owned()];
    })
    .await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/addresses/{P2PKH_ADDRESS}/group"),
        b"",
        Some("0123456789abcdef0123456789abcdef"),
    )
    .await;

    assert_eq!((status, body.as_str()), (StatusCode::OK, "{\"group\":3}"));
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn shadow_mode_always_returns_the_official_answer() {
    let (upstream, seen) = spawn_upstream().await;
    let gateway = spawn_gateway(&format!("http://{upstream}"), |config| {
        config.route_modes = vec!["address-group=shadow".to_owned()];
    })
    .await;

    let (status, body) = send(
        "GET",
        format!("http://{gateway}/addresses/{P2PKH_ADDRESS}/group"),
        b"",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "{\"group\":3}");
    assert_eq!(seen.http_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_route_modes_are_rejected() {
    for entry in [
        "address-group",
        "unknown-route=native",
        "address-group=fast",
        "contract-state=native",
        "contract-state=shadow",
    ] {
        let config = ApiConfig {
            bind_address: "127.0.0.1:0".to_owned(),
            route_modes: vec![entry.to_owned()],
            ..ApiConfig::default()
        };
        assert!(Gateway::bind(config).await.is_err(), "{entry}");
    }
    let config = ApiConfig {
        bind_address: "127.0.0.1:0".to_owned(),
        route_modes: vec![
            "contract-state=proxy".to_owned(),
            "mempool-transactions=proxy".to_owned(),
            "address-group=proxy".to_owned(),
        ],
        ..ApiConfig::default()
    };
    assert!(Gateway::bind(config).await.is_ok());
}
