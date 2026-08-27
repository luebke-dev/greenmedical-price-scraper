//! HTTP API (`/api/v1`), health endpoints and middleware stack.

pub mod error;
pub mod extract;
pub mod handlers;
pub mod metrics;
pub mod openapi;
pub mod rate_limit;
pub mod strains_page;
pub mod subscriptions;

use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::state::SharedState;

pub use error::ApiError;
pub use extract::{ApiJson, ApiPath, ApiQuery};
pub use metrics::metrics_router;

const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Abort handlers that exceed `HTTP_REQUEST_TIMEOUT` with an enveloped `408`.
///
/// `tower_http::timeout::TimeoutLayer` answers with an empty body, which
/// violates the error contract; this middleware produces the JSON envelope.
pub async fn request_timeout(
    State(timeout): State<Duration>,
    request: Request,
    next: Next,
) -> Response {
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            tracing::warn!(timeout_ms = timeout.as_millis() as u64, "request timed out");
            ApiError::timeout().into_response()
        }
    }
}

/// Build the API router with all middleware for the given state.
pub fn build_router(state: SharedState) -> Router {
    let api = Router::new()
        .route("/metadata", get(handlers::metadata))
        .route("/strains", get(handlers::strains))
        .route("/strains/{id}", get(handlers::strain_detail))
        .route("/strains/{id}/history", get(handlers::strain_history))
        .route(
            "/strains/{id}/offer-history",
            get(handlers::strain_offer_history),
        )
        .route("/strains/{id}/reviews", get(handlers::strain_reviews))
        .route("/runs", get(handlers::runs_list))
        .route("/runs/{id}", get(handlers::run_detail))
        .route("/pharmacies", get(handlers::pharmacies))
        .route("/export.csv", get(handlers::export_csv))
        .route("/export.json", get(handlers::export_json))
        .route("/admin/scrape", post(handlers::admin_scrape));

    // Every subscription response is `Cache-Control: no-store`, errors included.
    let subscriptions = Router::new()
        .route("/", post(subscriptions::create))
        .route("/confirm", post(subscriptions::confirm))
        .route(
            "/manage",
            get(subscriptions::manage_get)
                .put(subscriptions::manage_put)
                .delete(subscriptions::manage_delete),
        )
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));
    let api = api.nest("/subscriptions", subscriptions);

    let mut router = Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/readyz", get(handlers::readyz))
        .merge(openapi::docs_router())
        .nest("/api/v1", api)
        .fallback(handlers::not_found)
        // Applies to every method router registered so far, including the nested ones.
        .method_not_allowed_fallback(handlers::method_not_allowed);

    let origins = state.config.cors_origins();
    if !origins.is_empty() {
        let allowed: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::IF_NONE_MATCH,
            ])
            .expose_headers([header::ETAG, header::CONTENT_DISPOSITION, X_REQUEST_ID])
            .max_age(Duration::from_secs(3600));
        router = router.layer(cors);
    }

    let trace = TraceLayer::new_for_http()
        .make_span_with(|request: &axum::http::Request<_>| {
            let request_id = request
                .headers()
                .get(X_REQUEST_ID)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("-");
            tracing::info_span!(
                "http",
                method = %request.method(),
                uri = %request.uri().path(),
                request_id = %request_id,
            )
        })
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    // Outermost first. Metrics sit outside the timeout so timed-out requests
    // are counted with their 408 status.
    router
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(X_REQUEST_ID, MakeRequestUuid))
                .layer(PropagateRequestIdLayer::new(X_REQUEST_ID))
                .layer(trace)
                .layer(axum::middleware::from_fn(metrics::track_metrics))
                .layer(axum::middleware::from_fn_with_state(
                    state.config.http_request_timeout,
                    request_timeout,
                ))
                .layer(CompressionLayer::new()),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(200)).await;
        "done"
    }

    async fn fast() -> &'static str {
        "done"
    }

    fn app(timeout: Duration) -> Router {
        Router::new()
            .route("/slow", get(slow))
            .route("/fast", get(fast))
            .layer(axum::middleware::from_fn_with_state(
                timeout,
                request_timeout,
            ))
    }

    #[tokio::test]
    async fn timeout_returns_enveloped_408() {
        let response = app(Duration::from_millis(20))
            .oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "internal");
        assert!(
            value["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Zeitüberschreitung")
        );
    }

    #[tokio::test]
    async fn fast_requests_pass_the_timeout() {
        let response = app(Duration::from_millis(20))
            .oneshot(Request::builder().uri("/fast").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
