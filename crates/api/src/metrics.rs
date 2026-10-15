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
    native_served: AtomicU64,
    shadow_matches: AtomicU64,
    shadow_mismatches: AtomicU64,
    shadow_declined: AtomicU64,
}

/// Outcome of comparing a native answer with the official one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowResult {
    Match,
    Mismatch,
    /// The native handler deferred this request to the official node.
    Declined,
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
    pub(crate) native_served: u64,
    pub(crate) shadow_matches: u64,
    pub(crate) shadow_mismatches: u64,
    pub(crate) shadow_declined: u64,
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

    pub(crate) fn record_native(&self, route: Route) {
        self.routes[route.index()]
            .native_served
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Records a shadow comparison and returns the mismatch count before it.
    pub(crate) fn record_shadow(&self, route: Route, result: ShadowResult) -> u64 {
        let counters = &self.routes[route.index()];
        let counter = match result {
            ShadowResult::Match => &counters.shadow_matches,
            ShadowResult::Mismatch => &counters.shadow_mismatches,
            ShadowResult::Declined => &counters.shadow_declined,
        };
        let before = counter.fetch_add(1, Ordering::Relaxed);
        if result == ShadowResult::Mismatch {
            before
        } else {
            counters.shadow_mismatches.load(Ordering::Relaxed)
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
            native_served: counters.native_served.load(Ordering::Relaxed),
            shadow_matches: counters.shadow_matches.load(Ordering::Relaxed),
            shadow_mismatches: counters.shadow_mismatches.load(Ordering::Relaxed),
            shadow_declined: counters.shadow_declined.load(Ordering::Relaxed),
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
            let mut line = format!(
                "api {}: requests={} upstream_errors={} avg_us={average} max_us={}",
                route.name(),
                snapshot.requests,
                snapshot.upstream_errors,
                snapshot.max_micros
            );
            if snapshot.native_served > 0 {
                line.push_str(&format!(" native={}", snapshot.native_served));
            }
            let compared =
                snapshot.shadow_matches + snapshot.shadow_mismatches + snapshot.shadow_declined;
            if compared > 0 {
                line.push_str(&format!(
                    " shadow_match={} shadow_mismatch={} shadow_declined={}",
                    snapshot.shadow_matches, snapshot.shadow_mismatches, snapshot.shadow_declined
                ));
            }
            lines.push(line);
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

    use super::{Metrics, RouteSnapshot, ShadowResult};
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
                native_served: 0,
                shadow_matches: 0,
                shadow_mismatches: 0,
                shadow_declined: 0,
            }
        );
        assert_eq!(metrics.snapshot(Route::WebSocket).requests, 0);
        assert_eq!(metrics.rejected(), 1);
        assert_eq!(metrics.report().len(), 2);
    }

    #[test]
    fn shadow_results_are_counted_separately() {
        let metrics = Metrics::default();
        assert_eq!(
            metrics.record_shadow(Route::AddressGroup, ShadowResult::Mismatch),
            0
        );
        assert_eq!(
            metrics.record_shadow(Route::AddressGroup, ShadowResult::Mismatch),
            1
        );
        assert_eq!(
            metrics.record_shadow(Route::AddressGroup, ShadowResult::Match),
            2
        );
        metrics.record_shadow(Route::AddressGroup, ShadowResult::Declined);
        metrics.record_native(Route::AddressGroup);
        let snapshot = metrics.snapshot(Route::AddressGroup);
        assert_eq!(snapshot.shadow_matches, 1);
        assert_eq!(snapshot.shadow_mismatches, 2);
        assert_eq!(snapshot.shadow_declined, 1);
        assert_eq!(snapshot.native_served, 1);
    }
}
