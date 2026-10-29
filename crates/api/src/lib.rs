#![forbid(unsafe_code)]
//! HTTP and `/ws` gateway serving the `nixiaproject` API surface.
//!
//! Only the endpoints in `docs/NIXIA_API_SURFACE.md` are accepted; everything
//! else answers `404`/`405`. Each accepted route runs in one mode:
//! - `proxy`: forwarded unchanged to the official node;
//! - `shadow`: forwarded, and the native answer is computed and compared with
//!   the official one, which is what the client receives;
//! - `native`: answered by rustnode, falling back to the official node for any
//!   request the native handler declines.

pub mod json;
mod metrics;
mod native;
mod proxy;
mod routes;
mod shadow;
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

pub(crate) const API_KEY_HEADER: &str = "x-api-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Proxy,
    Shadow,
    Native,
}

impl RouteMode {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "proxy" => Some(Self::Proxy),
            "shadow" => Some(Self::Shadow),
            "native" => Some(Self::Native),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Shadow => "shadow",
            Self::Native => "native",
        }
    }
}

/// Default modes: `native` for verified handlers, `shadow` for other native
/// handlers, `proxy` elsewhere, then the `route-id=mode` overrides in order.
fn route_modes(overrides: &[String]) -> Result<[RouteMode; Route::ALL.len()], GatewayError> {
    let mut modes = Route::ALL.map(|route| {
        if native::verified(route) {
            RouteMode::Native
        } else if native::has_handler(route) {
            RouteMode::Shadow
        } else {
            RouteMode::Proxy
        }
    });
    for entry in overrides {
        let invalid = |reason: &str| GatewayError::InvalidRouteMode(format!("{entry}: {reason}"));
        let (id, mode) = entry
            .split_once('=')
            .ok_or_else(|| invalid("expected route-id=mode"))?;
        let route = Route::ALL
            .into_iter()
            .find(|route| route.id() == id)
            .ok_or_else(|| invalid("unknown route id"))?;
        let mode = RouteMode::parse(mode)
            .ok_or_else(|| invalid("mode must be proxy, shadow, or native"))?;
        if mode != RouteMode::Proxy && !native::has_handler(route) {
            return Err(invalid("this route has no native handler yet"));
        }
        modes[route.index()] = mode;
    }
    Ok(modes)
}

#[derive(Debug)]
pub enum GatewayError {
    InvalidUpstream(String),
    InvalidRouteMode(String),
    Bind(io::Error),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUpstream(detail) => write!(formatter, "invalid upstream URL: {detail}"),
            Self::InvalidRouteMode(detail) => write!(formatter, "invalid --route-mode {detail}"),
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
    pub(crate) modes: [RouteMode; Route::ALL.len()],
    pub(crate) chain: Option<alephium_chain::SharedChainStore>,
    pub(crate) mempool: Option<alephium_flow::SharedMempool>,
    pub(crate) state: Option<Arc<alephium_state::StateStore>>,
}

impl State {
    pub(crate) fn native_context(&self) -> native::Context<'_> {
        native::Context {
            groups: self.config.groups,
            chain: self.chain.as_ref(),
            mempool: self.mempool.as_ref(),
            state: self.state.as_deref(),
        }
    }
}

pub struct Gateway {
    listener: TcpListener,
    state: Arc<State>,
}

impl Gateway {
    pub async fn bind(config: ApiConfig) -> Result<Self, GatewayError> {
        let upstream = Upstream::parse(&config.upstream_url)?;
        let modes = route_modes(&config.route_modes)?;
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
                modes,
                chain: None,
                mempool: None,
                state: None,
            }),
        })
    }

    /// Serves native handlers that read followed blocks from `chain`.
    #[must_use]
    pub fn with_chain(mut self, chain: alephium_chain::SharedChainStore) -> Self {
        if let Some(state) = Arc::get_mut(&mut self.state) {
            state.chain = Some(chain);
        }
        self
    }

    /// Serves the mempool and `MemPooled` statuses from `mempool`, which the
    /// P2P sessions fill.
    #[must_use]
    pub fn with_mempool(mut self, mempool: alephium_flow::SharedMempool) -> Self {
        if let Some(state) = Arc::get_mut(&mut self.state) {
            state.mempool = Some(mempool);
        }
        self
    }

    /// Serves contract states from the followed world state.
    #[must_use]
    pub fn with_state(mut self, state: Arc<alephium_state::StateStore>) -> Self {
        if let Some(gateway_state) = Arc::get_mut(&mut self.state) {
            gateway_state.state = Some(state);
        }
        self
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn upstream_base(&self) -> &str {
        &self.state.upstream.http_base
    }

    /// Routes not in `proxy` mode, for the startup log.
    pub fn non_proxy_routes(&self) -> Vec<(Route, RouteMode)> {
        Route::ALL
            .into_iter()
            .map(|route| (route, self.state.modes[route.index()]))
            .filter(|(_, mode)| *mode != RouteMode::Proxy)
            .collect()
    }

    /// Accepts connections until the process stops. At most
    /// `max_connections` are served at once; further clients wait in the
    /// listen backlog.
    pub async fn run(self) {
        let metrics_state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(metrics_state.config.metrics_interval);
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
            let forwarded = match state.modes[matched.index()] {
                RouteMode::Proxy => proxy::forward(state, request).await,
                RouteMode::Shadow => shadow::forward_and_compare(state, matched, request).await,
                RouteMode::Native => {
                    let accepted = api_key_accepted(state, &request);
                    match proxy::read(state, request).await {
                        Ok((parts, body)) => {
                            let native = accepted
                                .then(|| {
                                    native::serve(
                                        matched,
                                        parts.uri.path(),
                                        parts.uri.query(),
                                        &body,
                                        &state.native_context(),
                                    )
                                })
                                .flatten();
                            match native {
                                Some(answer) => {
                                    state.metrics.record_native(matched);
                                    proxy::Forwarded {
                                        response: json_response(
                                            StatusCode::OK,
                                            Bytes::from(answer),
                                        ),
                                        upstream_error: false,
                                    }
                                }
                                None => proxy::forward_parts(state, parts, body).await,
                            }
                        }
                        Err(failure) => failure.into(),
                    }
                }
            };
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

/// The official node answers only after its `X-API-KEY` check passes: the key
/// must equal its configured `api-key`, or be absent when it has none. A native
/// answer is given only in that case; any other request is proxied so the
/// client gets the official error.
fn api_key_accepted(state: &State, request: &Request<Incoming>) -> bool {
    let presented = request
        .headers()
        .get(API_KEY_HEADER)
        .map(HeaderValue::as_bytes);
    presented == state.config.api_key.as_deref().map(str::as_bytes)
}

pub(crate) fn json_error(status: StatusCode, detail: &str) -> Response<GatewayBody> {
    let body = serde_json::json!({ "detail": detail }).to_string();
    json_response(status, Bytes::from(body))
}

pub(crate) fn json_response(status: StatusCode, body: Bytes) -> Response<GatewayBody> {
    let mut response = Response::new(full_body(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

pub(crate) fn full_body(bytes: Bytes) -> GatewayBody {
    Full::new(bytes).map_err(|never| match never {}).boxed()
}

pub(crate) fn empty_body() -> GatewayBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}
