//! HTTP metrics middleware and the `/metrics` router.

use std::time::Instant;

use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::http::header::CONTENT_TYPE;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use metrics_exporter_prometheus::PrometheusHandle;

/// Record `http_requests_total` and `http_request_duration_seconds` per route.
pub async fn track_metrics(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    let method = request.method().to_string();

    let response = next.run(request).await;

    let status = response.status().as_u16().to_string();
    let labels = [("method", method), ("route", path), ("status", status)];
    metrics::counter!("http_requests_total", &labels).increment(1);
    metrics::histogram!("http_request_duration_seconds", &labels)
        .record(start.elapsed().as_secs_f64());
    response
}

async fn render(handle: PrometheusHandle) -> Response {
    handle.run_upkeep();
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        handle.render(),
    )
        .into_response()
}

/// Router served on `METRICS_BIND`.
pub fn metrics_router(handle: PrometheusHandle) -> Router {
    Router::new().route(
        "/metrics",
        get(move || {
            let handle = handle.clone();
            async move { render(handle).await }
        }),
    )
}
