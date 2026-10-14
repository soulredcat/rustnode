#![forbid(unsafe_code)]
//! HTTP and `/ws` gateway serving the `nixiaproject` API surface.
//!
//! Only the endpoints in `docs/NIXIA_API_SURFACE.md` are accepted; everything
//! else answers `404`/`405`. Accepted requests are forwarded unchanged to the
//! official node until a native handler replaces them (`TODO.md`, N1+).

mod metrics;
mod proxy;
mod routes;
mod ws;

use std::convert::Infallible;
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alephium_conf::ApiConfig;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::header::{self, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

use crate::metrics::Metrics;
pub use crate::routes::{Route, RouteMatch, route};

type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub(crate) type GatewayBody = BoxBody<Bytes, BoxError>;

const METRICS_LOG_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub enum GatewayError {
    InvalidUpstream(String),
    Bind(io::Error),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUpstream(detail) => write!(formatter, "invalid upstream URL: {detail}"),
            Self::Bind(error) => write!(formatter, "cannot bind API listener: {error}"),
        }
    }
}

impl std::error::Error for GatewayError {}

/// Base URLs of the official node, without a trailing slash.
#[derive(Debug)]
pub(crate) struct Upstream {
    pub(crate) http_base: String,
    pub(crate) ws_base: String,
}

impl Upstream {
    fn parse(url: &str) -> Result<Self, GatewayError> {
        let uri = url
            .parse::<Uri>()
            .map_err(|error| GatewayError::InvalidUpstream(format!("{url}: {error}")))?;
        if uri.scheme_str() != Some("http") {
            return Err(GatewayError::InvalidUpstream(format!(
                "{url}: only http:// is supported"
            )));
        }
        let Some(authority) = uri.authority() else {
            return Err(GatewayError::InvalidUpstream(format!(
                "{url}: missing host"
            )));
        };
        let path_is_root = uri
            .path_and_query()
            .is_none_or(|value| value.as_str() == "/" || value.as_str().is_empty());
        if !path_is_root {
            return Err(GatewayError::InvalidUpstream(format!(
                "{url}: must not have a path or query"
            )));
        }
        Ok(Self {
            http_base: format!("http://{authority}"),
            ws_base: format!("ws://{authority}"),
        })
    }
}

pub(crate) struct State {
    pub(crate) config: ApiConfig,
    pub(crate) upstream: Upstream,
    pub(crate) client: Client<HttpConnector, Full<Bytes>>,
    pub(crate) metrics: Metrics,
}

pub struct Gateway {
    listener: TcpListener,
    state: Arc<State>,
}

impl Gateway {
    pub async fn bind(config: ApiConfig) -> Result<Self, GatewayError> {
        let upstream = Upstream::parse(&config.upstream_url)?;
        let listener = TcpListener::bind(&config.bind_address)
            .await
            .map_err(GatewayError::Bind)?;

        let mut connector = HttpConnector::new();
        connector.set_nodelay(true);
        connector.set_connect_timeout(Some(config.upstream_timeout));
        let client = Client::builder(TokioExecutor::new())
            .pool_idle_timeout(Duration::from_secs(90))
            .build(connector);

        Ok(Self {
            listener,
            state: Arc::new(State {
                config,
                upstream,
                client,
                metrics: Metrics::default(),
            }),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn upstream_base(&self) -> &str {
        &self.state.upstream.http_base
    }

    /// Accepts connections until the process stops. At most
    /// `max_connections` are served at once; further clients wait in the
    /// listen backlog.
    pub async fn run(self) {
        let metrics_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(METRICS_LOG_INTERVAL);
            interval.tick().await;
            loop {
                interval.tick().await;
                for line in metrics_state.metrics.report() {
                    eprintln!("{line}");
                }
            }
        });

        let permits = Arc::new(Semaphore::new(self.state.config.max_connections.max(1)));
        loop {
            let Ok(permit) = Arc::clone(&permits).acquire_owned().await else {
                return;
            };
            let (stream, _peer) = match self.listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    eprintln!("api accept failed: {error}");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
            };
            let _ = stream.set_nodelay(true);
            let state = Arc::clone(&self.state);
            tokio::spawn(async move {
                let header_read_timeout = state.config.header_read_timeout;
                let service = service_fn(move |request| {
                    let state = Arc::clone(&state);
                    async move { Ok::<_, Infallible>(handle(&state, request).await) }
                });
                let mut builder = http1::Builder::new();
                builder
                    .timer(TokioTimer::new())
                    .header_read_timeout(header_read_timeout);
                let _ = builder
                    .serve_connection(TokioIo::new(stream), service)
                    .with_upgrades()
                    .await;
                drop(permit);
            });
        }
    }
}

async fn handle(state: &State, request: Request<Incoming>) -> Response<GatewayBody> {
    let started = Instant::now();
    match route(request.method(), request.uri().path()) {
        RouteMatch::Found(Route::WebSocket) => {
            let upgraded = ws::upgrade(state, request).await;
            state
                .metrics
                .record(Route::WebSocket, started.elapsed(), upgraded.upstream_error);
            upgraded.response
        }
        RouteMatch::Found(matched) => {
            let forwarded = proxy::forward(state, request).await;
            state
                .metrics
                .record(matched, started.elapsed(), forwarded.upstream_error);
            forwarded.response
        }
        RouteMatch::MethodNotAllowed => {
            state.metrics.record_rejected();
            json_error(StatusCode::METHOD_NOT_ALLOWED, "method not allowed")
        }
        RouteMatch::NotFound => {
            state.metrics.record_rejected();
            json_error(
                StatusCode::NOT_FOUND,
                "rustnode serves only the nixiaproject API surface",
            )
        }
    }
}

pub(crate) fn json_error(status: StatusCode, detail: &str) -> Response<GatewayBody> {
    let body = format!("{{\"detail\":{}}}", json_string(detail));
    let mut response = Response::new(
        Full::new(Bytes::from(body))
            .map_err(|never| match never {})
            .boxed(),
    );
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

pub(crate) fn empty_body() -> GatewayBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn json_string(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('"');
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            control if control.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => escaped.push(other),
        }
    }
    escaped.push('"');
    escaped
}
