//! OpenAPI 3.1 document (`GET /api/openapi.json`) and the Swagger UI (`GET /api/docs`).
//!
//! The UI assets are embedded in the binary (`utoipa-swagger-ui`, feature
//! `vendored`), so the page loads nothing from third-party hosts.

use std::sync::{Arc, LazyLock};

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use super::error::{ErrorDetailDto, ErrorEnvelopeDto};
use super::handlers::{self, HealthzDto, ReadyzDto, ScrapeAcceptedDto};
use super::subscriptions;
use crate::domain::{
    ConfirmDto, FacetsDto, GenetikFacetDto, HighlightDto, HistoryBucket, HistoryDto,
    HistoryPointDto, MetadataDto, OfferDto, OfferHistoryMode, OfferHistoryPageDto,
    OfferHistoryRowDto, OfferHistoryRows, OfferPhaseRowDto, PharmacyDto, PharmacySeriesDto,
    PharmacySeriesPointDto, RangeDto, RatingDistributionDto, RatingDto, RatingHistoryPointDto,
    ReviewDto, ReviewSummaryDto, ReviewsResponseDto, RuleDto, RuleInputDto, RuleKind,
    RulesUpdateDto, RunDetailDto, RunDto, RunErrorDto, RunStatus, RunTrigger, RunsResponseDto,
    ScheduleDto, SortDto, StrainDetailDto, StrainDto, StrainListItemDto, StrainsPageDto,
    SubscriptionAcceptedDto, SubscriptionCreateDto, SubscriptionDto, TrendDirection, TrendDto,
};

/// Path of the JSON document.
pub const OPENAPI_PATH: &str = "/api/openapi.json";
/// Path of the documentation page.
pub const DOCS_PATH: &str = "/api/docs";

/// Name of the bearer security scheme (`ADMIN_TOKEN`).
pub const ADMIN_SECURITY: &str = "admin_token";

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_default();
        components.add_security_scheme(
            ADMIN_SECURITY,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "`ADMIN_TOKEN` des Backends; leerer Token ⇒ Endpoint antwortet 404.",
                    ))
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "GreenMedical Livebestand API",
        description = "JSON-API des GreenMedical Price Scrapers: Sorten, Angebote, Preisverlauf, Bewertungen, \
                       Scrape-Läufe, Exporte und Preisalarm-Abos.\n\n\
                       Zeitstempel sind RFC 3339 UTC (optional mit Sekundenbruchteilen). Fehler haben immer die Form \
                       `{\"error\":{\"code\":\"…\",\"message\":\"…\"}}` (`code`: `not_found` | `bad_request` | \
                       `unauthorized` | `conflict` | `no_data` | `internal`; `405` trägt `bad_request`, `408` trägt `internal`).",
        license(name = "MIT"),
    ),
    paths(
        handlers::healthz,
        handlers::readyz,
        handlers::metadata,
        handlers::strains,
        handlers::strain_detail,
        handlers::strain_history,
        handlers::strain_offer_history,
        handlers::strain_reviews,
        handlers::runs_list,
        handlers::run_detail,
        handlers::pharmacies,
        handlers::export_csv,
        handlers::export_json,
        handlers::admin_scrape,
        subscriptions::create,
        subscriptions::confirm,
        subscriptions::manage_get,
        subscriptions::manage_put,
        subscriptions::manage_delete,
    ),
    components(schemas(
        ErrorEnvelopeDto, ErrorDetailDto, HealthzDto, ReadyzDto, ScrapeAcceptedDto,
        RunStatus, RunTrigger, RunDto, RunErrorDto, RunDetailDto, RunsResponseDto,
        OfferDto, SortDto, RatingDto, TrendDirection, TrendDto, StrainDto, StrainDetailDto,
        StrainListItemDto, GenetikFacetDto, RangeDto, FacetsDto, StrainsPageDto,
        HighlightDto, ScheduleDto, MetadataDto,
        HistoryBucket, HistoryPointDto, PharmacySeriesPointDto, PharmacySeriesDto, HistoryDto,
        OfferHistoryMode, OfferHistoryRowDto, OfferPhaseRowDto, OfferHistoryRows, OfferHistoryPageDto,
        ReviewDto, RatingDistributionDto, ReviewSummaryDto, RatingHistoryPointDto, ReviewsResponseDto,
        PharmacyDto, handlers::PharmaciesResponseDto,
        RuleKind, RuleInputDto, RuleDto, SubscriptionCreateDto, RulesUpdateDto, ConfirmDto,
        SubscriptionDto, SubscriptionAcceptedDto,
    )),
    modifiers(&Security),
    tags(
        (name = "health", description = "Liveness und Readiness"),
        (name = "strains", description = "Sorten des letzten usable Laufs, Detail, Preisverlauf, Angebotshistorie und Bewertungen"),
        (name = "runs", description = "Scrape-Läufe"),
        (name = "pharmacies", description = "Apotheken"),
        (name = "export", description = "Vollständiger Export eines Laufs (CSV/JSON)"),
        (name = "admin", description = "Manuelle Läufe (Bearer-Token)"),
        (name = "subscriptions", description = "Preisalarm-Abos per E-Mail; alle Antworten `Cache-Control: no-store`"),
    )
)]
pub struct ApiDoc;

static OPENAPI: LazyLock<utoipa::openapi::OpenApi> = LazyLock::new(ApiDoc::openapi);

/// The generated document (built once).
pub fn openapi() -> &'static utoipa::openapi::OpenApi {
    &OPENAPI
}

async fn openapi_json() -> axum::response::Response {
    use axum::response::IntoResponse;
    axum::Json(openapi().clone()).into_response()
}

fn swagger_config() -> Arc<utoipa_swagger_ui::Config<'static>> {
    static CONFIG: LazyLock<Arc<utoipa_swagger_ui::Config<'static>>> =
        LazyLock::new(|| Arc::new(utoipa_swagger_ui::Config::from(OPENAPI_PATH)));
    CONFIG.clone()
}

/// Serve one embedded Swagger UI file (`index.html` for the page itself).
async fn swagger_file(file: &str) -> axum::response::Response {
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse;
    match utoipa_swagger_ui::serve(file, swagger_config()) {
        Ok(Some(asset)) => (
            [(header::CONTENT_TYPE, asset.content_type)],
            asset.bytes.into_owned(),
        )
            .into_response(),
        Ok(None) => super::error::ApiError::not_found("Unbekannter Pfad").into_response(),
        Err(err) => {
            tracing::error!(%err, file, "swagger ui asset error");
            (StatusCode::INTERNAL_SERVER_ERROR, "swagger ui error").into_response()
        }
    }
}

async fn docs_index() -> axum::response::Response {
    swagger_file("index.html").await
}

async fn docs_asset(
    axum::extract::Path(rest): axum::extract::Path<String>,
) -> axum::response::Response {
    let file = rest.trim_start_matches('/');
    let file = if file.is_empty() { "index.html" } else { file };
    swagger_file(file).await
}

/// Router serving `/api/docs` (Swagger UI, embedded assets) and `/api/openapi.json`.
pub fn docs_router<S>() -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    axum::Router::new()
        .route(OPENAPI_PATH, axum::routing::get(openapi_json))
        .route(DOCS_PATH, axum::routing::get(docs_index))
        .route(&format!("{DOCS_PATH}/"), axum::routing::get(docs_index))
        .route(
            &format!("{DOCS_PATH}/{{*rest}}"),
            axum::routing::get(docs_asset),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_lists_every_route_and_schema() {
        let doc = openapi();
        let json = serde_json::to_value(doc).unwrap();
        assert_eq!(json["openapi"], "3.1.0");
        let paths = json["paths"].as_object().unwrap();
        for path in [
            "/healthz",
            "/readyz",
            "/api/v1/metadata",
            "/api/v1/strains",
            "/api/v1/strains/{id}",
            "/api/v1/strains/{id}/history",
            "/api/v1/strains/{id}/offer-history",
            "/api/v1/strains/{id}/reviews",
            "/api/v1/runs",
            "/api/v1/runs/{id}",
            "/api/v1/pharmacies",
            "/api/v1/export.csv",
            "/api/v1/export.json",
            "/api/v1/admin/scrape",
            "/api/v1/subscriptions",
            "/api/v1/subscriptions/confirm",
            "/api/v1/subscriptions/manage",
        ] {
            assert!(paths.contains_key(path), "missing path {path}");
        }
        let manage = &paths["/api/v1/subscriptions/manage"];
        for method in ["get", "put", "delete"] {
            assert!(manage[method].is_object(), "manage lacks {method}");
        }
        let schemas = json["components"]["schemas"].as_object().unwrap();
        for schema in [
            "ErrorEnvelopeDto",
            "StrainsPageDto",
            "StrainDetailDto",
            "MetadataDto",
            "ScheduleDto",
            "HistoryDto",
            "OfferHistoryPageDto",
            "ReviewsResponseDto",
            "RunDetailDto",
            "PharmacyDto",
            "SubscriptionCreateDto",
            "SubscriptionDto",
            "RuleKind",
        ] {
            assert!(schemas.contains_key(schema), "missing schema {schema}");
        }
        assert_eq!(
            json["components"]["securitySchemes"][ADMIN_SECURITY]["scheme"],
            "bearer"
        );
        assert_eq!(
            json["paths"]["/api/v1/admin/scrape"]["post"]["security"][0][ADMIN_SECURITY],
            serde_json::json!([])
        );
        // Every documented query parameter of /strains.
        let params: Vec<&str> = json["paths"]["/api/v1/strains"]["get"]["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        for name in [
            "q",
            "genetik",
            "price_min",
            "price_max",
            "thc_min",
            "thc_max",
            "cbd_min",
            "cbd_max",
            "rating_min",
            "sort",
            "dir",
            "limit",
            "offset",
        ] {
            assert!(params.contains(&name), "strains lacks query param {name}");
        }
        for field in ["next_run_at", "scrape_running", "schedule"] {
            assert!(
                schemas["MetadataDto"]["properties"][field].is_object(),
                "MetadataDto lacks {field}"
            );
        }
        // Serde field names survive.
        assert!(schemas["RatingDistributionDto"]["properties"]["1"].is_object());
        assert_eq!(schemas["RuleKind"]["enum"][0], "strain_available");
    }
}
