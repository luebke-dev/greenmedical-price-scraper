//! Error envelope: `{"error":{"code","message"}}`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

/// Fehler-Envelope aller Fehlerantworten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelopeDto {
    pub error: ErrorDetailDto,
}

/// `code` ist ein offener String: `not_found` | `bad_request` | `unauthorized` | `conflict` | `no_data` | `internal`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetailDto {
    #[schema(example = "not_found")]
    pub code: String,
    #[schema(example = "Sorte 42 nicht gefunden")]
    pub message: String,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorInner<'a>,
}

#[derive(Serialize)]
struct ErrorInner<'a> {
    code: &'a str,
    message: &'a str,
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    pub fn no_data() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "no_data",
            message: "Noch kein erfolgreicher Scrape-Lauf vorhanden".into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "Ungültiges oder fehlendes Bearer-Token".into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            message: message.into(),
        }
    }

    pub fn internal() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal",
            message: "Interner Fehler".into(),
        }
    }

    /// `429` when the per-IP rate limit of the subscription endpoints is exhausted.
    pub fn too_many_requests() -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "bad_request",
            message: "Zu viele Anfragen, bitte später erneut versuchen".into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "unavailable",
            message: message.into(),
        }
    }

    /// `405` for a known path with an unsupported method. The contract's code
    /// enum has no dedicated value, so the closest client-error code is used.
    pub fn method_not_allowed() -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "bad_request",
            message: "Methode nicht erlaubt".into(),
        }
    }

    /// `408` when `HTTP_REQUEST_TIMEOUT` elapsed before the handler answered.
    pub fn timeout() -> Self {
        Self {
            status: StatusCode::REQUEST_TIMEOUT,
            code: "internal",
            message: "Zeitüberschreitung bei der Verarbeitung der Anfrage".into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorBody {
            error: ErrorInner {
                code: self.code,
                message: &self.message,
            },
        });
        (self.status, body).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(%err, "database error");
        Self::internal()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!(%err, "internal error");
        Self::internal()
    }
}
