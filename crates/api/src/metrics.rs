//! Per-route request counters, logged periodically outside the request path.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::routes::Route;

#[derive(Debug, Default)]
struct RouteCounters {
    requests: AtomicU64,
    upstream_errors: AtomicU64,
    total_micros: AtomicU64,
    max_micros: AtomicU64,
}

#[derive(Debug, Default)]
pub(crate) struct Metrics {
    routes: [RouteCounters; Route::ALL.len()],
    rejected: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteSnapshot {
    pub(crate) requests: u64,
    pub(crate) upstream_errors: u64,
    pub(crate) total_micros: u64,
    pub(crate) max_micros: u64,
}

impl Metrics {
    pub(crate) fn record(&self, route: Route, elapsed: Duration, upstream_error: bool) {
        let counters = &self.routes[route.index()];
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        counters.requests.fetch_add(1, Ordering::Relaxed);
        counters.total_micros.fetch_add(micros, Ordering::Relaxed);
        counters.max_micros.fetch_max(micros, Ordering::Relaxed);
        if upstream_error {
            counters.upstream_errors.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_rejected(&self) {
        self.rejected.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn snapshot(&self, route: Route) -> RouteSnapshot {
        let counters = &self.routes[route.index()];
        RouteSnapshot {
            requests: counters.requests.load(Ordering::Relaxed),
            upstream_errors: counters.upstream_errors.load(Ordering::Relaxed),
            total_micros: counters.total_micros.load(Ordering::Relaxed),
            max_micros: counters.max_micros.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// One line per route that saw traffic, cumulative since start.
    pub(crate) fn report(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for route in Route::ALL {
            let snapshot = self.snapshot(route);
            if snapshot.requests == 0 {
                continue;
            }
            let average = snapshot.total_micros / snapshot.requests;
            lines.push(format!(
                "api {}: requests={} upstream_errors={} avg_us={average} max_us={}",
                route.name(),
                snapshot.requests,
                snapshot.upstream_errors,
                snapshot.max_micros
            ));
        }
        let rejected = self.rejected();
        if rejected > 0 {
            lines.push(format!(
                "api rejected (outside nixiaproject surface or invalid): {rejected}"
            ));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Metrics, RouteSnapshot};
    use crate::routes::Route;

    #[test]
    fn record_accumulates_per_route() {
        let metrics = Metrics::default();
        metrics.record(Route::MempoolTransactions, Duration::from_micros(10), false);
        metrics.record(Route::MempoolTransactions, Duration::from_micros(30), true);
        metrics.record_rejected();
        assert_eq!(
            metrics.snapshot(Route::MempoolTransactions),
            RouteSnapshot {
                requests: 2,
                upstream_errors: 1,
                total_micros: 40,
                max_micros: 30,
            }
        );
        assert_eq!(metrics.snapshot(Route::WebSocket).requests, 0);
        assert_eq!(metrics.rejected(), 1);
        assert_eq!(metrics.report().len(), 2);
    }
}
