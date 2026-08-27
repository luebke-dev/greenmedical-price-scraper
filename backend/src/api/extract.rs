//! Extractors whose rejections use the JSON error envelope.
//!
//! axum's own `Path`/`Query` answer `400 text/plain` when they cannot parse the
//! input. The contract requires `{"error":{"code":"bad_request",…}}` for every
//! error, so handlers use these thin wrappers instead.

use axum::extract::path::ErrorKind;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{FromRequestParts, Path, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use super::error::ApiError;

/// `axum::extract::Path` with an `ApiError` rejection.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiPath<T>(pub T);

/// `axum::extract::Query` with an `ApiError` rejection.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(Self(value)),
            Err(rejection) => Err(rejection.into()),
        }
    }
}

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(value)) => Ok(Self(value)),
            Err(rejection) => Err(rejection.into()),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        match rejection {
            PathRejection::FailedToDeserializePathParams(inner) => match inner.kind() {
                ErrorKind::ParseErrorAtKey {
                    key,
                    value,
                    expected_type,
                } => ApiError::bad_request(format!(
                    "Pfadparameter `{key}`: `{value}` ist kein gültiger Wert (erwartet: {expected_type})"
                )),
                ErrorKind::ParseError {
                    value,
                    expected_type,
                }
                | ErrorKind::ParseErrorAtIndex {
                    value,
                    expected_type,
                    ..
                } => ApiError::bad_request(format!(
                    "Pfadparameter `{value}` ist kein gültiger Wert (erwartet: {expected_type})"
                )),
                ErrorKind::UnsupportedType { .. } => {
                    tracing::error!(%inner, "path extractor misconfigured");
                    ApiError::internal()
                }
                other => ApiError::bad_request(format!("Ungültiger Pfadparameter: {other}")),
            },
            other => {
                tracing::error!(%other, "path extractor rejection");
                ApiError::internal()
            }
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        let text = rejection.body_text();
        let detail = text
            .strip_prefix("Failed to deserialize query string: ")
            .unwrap_or(&text);
        ApiError::bad_request(format!("Ungültige Query-Parameter: {detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use serde::Deserialize;
    use tower::ServiceExt;

    #[derive(Deserialize)]
    struct Params {
        limit: Option<i64>,
    }

    async fn by_id(ApiPath(id): ApiPath<i64>) -> String {
        id.to_string()
    }

    async fn list(ApiQuery(params): ApiQuery<Params>) -> String {
        params.limit.unwrap_or(0).to_string()
    }

    fn app() -> Router {
        Router::new()
            .route("/items/{id}", get(by_id))
            .route("/items", get(list))
    }

    async fn call(uri: &str) -> (StatusCode, Option<String>, serde_json::Value) {
        let response = app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        (status, content_type, json)
    }

    #[tokio::test]
    async fn valid_values_pass_through() {
        let (status, _, _) = call("/items/42").await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = call("/items?limit=3").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_path_param_is_enveloped() {
        let (status, content_type, body) = call("/items/abc").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(body["error"]["code"], "bad_request");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains("`abc`"), "{message}");
        assert!(message.contains("i64"), "{message}");
    }

    #[tokio::test]
    async fn invalid_query_param_is_enveloped() {
        let (status, content_type, body) = call("/items?limit=abc").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(body["error"]["code"], "bad_request");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with("Ungültige Query-Parameter: limit"),
            "{message}"
        );
        assert!(!message.contains("Failed to deserialize"), "{message}");
    }
}
