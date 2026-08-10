use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

/// Metrics collector with per-route counters
#[derive(Default, Clone)]
pub struct Metrics {
    request_counts: Arc<Mutex<HashMap<String, AtomicU64>>>,
    total_requests: Arc<AtomicU64>,
}

impl Metrics {
    /// Record a request for the given route
    pub fn record_request(&self, route: &str) {
        let key = route.to_string();
        let mut counts = self.request_counts.lock().unwrap();
        let counter = counts
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0));
        counter.fetch_add(1, Ordering::Relaxed);
        drop(counts);

        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Get metrics in Prometheus text format
    pub fn to_prometheus(&self) -> String {
        let mut output = String::new();

        // Total requests
        output.push_str("# HELP riply_total_requests Total number of requests\n");
        output.push_str("# TYPE riply_total_requests counter\n");
        output.push_str(&format!(
            "riply_total_requests {}\n",
            self.total_requests.load(Ordering::Relaxed)
        ));

        // Per-route requests
        output.push_str("# HELP riply_route_requests Requests per route\n");
        output.push_str("# TYPE riply_route_requests counter\n");

        let counts = self.request_counts.lock().unwrap();
        for (route, counter) in counts.iter() {
            output.push_str(&format!(
                "riply_route_requests{{route=\"{}\"}} {}\n",
                route,
                counter.load(Ordering::Relaxed)
            ));
        }

        output
    }
}

/// Middleware to track request metrics
pub struct MetricsLayer {
    metrics: Metrics,
}

impl MetricsLayer {
    pub fn new(metrics: Metrics) -> Self {
        Self { metrics }
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Wrap a handler to track metrics
    pub fn layer<T>(
        &self,
        handler: impl std::future::Future<Output = T> + Send + 'static,
        route: String,
    ) -> impl std::future::Future<Output = T> + Send + 'static {
        let metrics = self.metrics.clone();
        let start = Instant::now();
        async move {
            metrics.record_request(&route);
            let _elapsed = start.elapsed();
            handler.await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_counter() {
        let metrics = Metrics::default();
        metrics.record_request("/api/test");
        metrics.record_request("/api/test");

        let prom = metrics.to_prometheus();
        assert!(prom.contains("riply_total_requests 2"));
        assert!(prom.contains("riply_route_requests"));
    }

    #[test]
    fn test_metrics_multiple_routes() {
        let metrics = Metrics::default();
        metrics.record_request("/api/test");
        metrics.record_request("/api/other");

        let prom = metrics.to_prometheus();
        assert!(prom.contains("route=\"/api/test\""));
        assert!(prom.contains("route=\"/api/other\""));
    }
}
