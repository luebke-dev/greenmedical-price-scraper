//! Request handlers.

use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, SubsecRound, Utc};
use constant_time_eq::constant_time_eq;
use serde::Deserialize;
use serde_json::json;

use super::error::ApiError;
use super::extract::{ApiPath, ApiQuery};
use crate::db::reviews::ReviewSort;
use crate::db::snapshot::Snapshot;
use crate::db::{offers, pharmacies, reviews, runs, strains};
use crate::domain::{
    self, HistoryBucket, HistoryDto, ReviewsResponseDto, RunDetailDto, RunStatus, RunTrigger,
    RunsResponseDto, StrainDetailDto, StrainDto,
};
use crate::scrape::run::{StartError, execute_run, start_run};
use crate::state::SharedState;

const CACHE_CONTROL_STRAINS: &str = "public, max-age=300";
const MAX_HISTORY_DAYS: i64 = 730;
const DEFAULT_HISTORY_DAYS: i64 = 90;

fn json_bytes(body: Bytes) -> Response {
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

pub async fn not_found() -> ApiError {
    ApiError::not_found("Unbekannter Pfad")
}

pub async fn method_not_allowed() -> ApiError {
    ApiError::method_not_allowed()
}

pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn readyz(State(state): State<SharedState>) -> Response {
    if state.shutdown.is_cancelled() || !state.ready.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "db": "unknown", "reason": "shutting_down_or_starting" })),
        )
            .into_response();
    }
    let check = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query_scalar!(r#"SELECT 1 AS "one!""#).fetch_one(&state.pool),
    )
    .await;
    match check {
        Ok(Ok(_)) => Json(json!({ "status": "ready", "db": "ok" })).into_response(),
        Ok(Err(err)) => {
            tracing::warn!(%err, "readiness database check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "not_ready", "db": "error" })),
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "db": "timeout" })),
        )
            .into_response(),
    }
}

async fn snapshot(state: &SharedState) -> Result<std::sync::Arc<Snapshot>, ApiError> {
    state
        .snapshot
        .get_or_load(&state.pool)
        .await?
        .ok_or_else(ApiError::no_data)
}

pub async fn metadata(State(state): State<SharedState>) -> Result<Response, ApiError> {
    let snapshot = snapshot(&state).await?;
    Ok(json_bytes(snapshot.metadata_json.clone()))
}

fn etag_matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| {
            value.split(',').map(str::trim).any(|candidate| {
                candidate == "*" || candidate == etag || candidate.strip_prefix("W/") == Some(etag)
            })
        })
}

pub async fn strains(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let snapshot = snapshot(&state).await?;
    let cache_headers = [
        (
            header::ETAG,
            HeaderValue::from_str(&snapshot.etag).expect("valid etag"),
        ),
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static(CACHE_CONTROL_STRAINS),
        ),
    ];
    if etag_matches(&headers, &snapshot.etag) {
        return Ok((StatusCode::NOT_MODIFIED, cache_headers).into_response());
    }
    Ok((
        cache_headers,
        [(header::CONTENT_TYPE, "application/json")],
        snapshot.strains_json.clone(),
    )
        .into_response())
}

pub async fn strain_detail(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<Json<StrainDetailDto>, ApiError> {
    let Some(row) = strains::get(&state.pool, id).await? else {
        return Err(ApiError::not_found(format!("Sorte {id} nicht gefunden")));
    };
    let snapshot = snapshot(&state).await?;
    let (strain, in_latest_run) = match snapshot.strains.iter().find(|s| s.id == id) {
        Some(strain) => (strain.clone(), true),
        None => (
            StrainDto {
                id: row.id,
                name: row.name.clone(),
                bezeichnung: row.bezeichnung.clone(),
                genetik: row.genetik.clone(),
                thc: row.thc_label.clone(),
                cbd: row.cbd_label.clone(),
                thc_value: domain::parse_percent(&row.thc_label),
                cbd_value: domain::parse_percent(&row.cbd_label),
                min_price: None,
                min_price_per_thc_gram: None,
                pharmacy_count: 0,
                offers: Vec::new(),
                sort: domain::SortDto {
                    price: None,
                    price_per_thc_gram: None,
                    thc: domain::parse_percent(&row.thc_label),
                    cbd: domain::parse_percent(&row.cbd_label),
                    rating: row.rating.as_ref().and_then(|r| r.value),
                },
                search: [
                    row.name.as_str(),
                    row.bezeichnung.as_str(),
                    row.genetik.as_str(),
                    row.thc_label.as_str(),
                    row.cbd_label.as_str(),
                ]
                .join(" ")
                .to_lowercase(),
                trend: None,
                rating: row.rating.clone(),
                product_uuid: row.product_uuid.clone(),
            },
            false,
        ),
    };
    Ok(Json(StrainDetailDto {
        strain,
        first_seen_at: row.first_seen_at,
        last_seen_at: row.last_seen_at,
        in_latest_run,
        run: snapshot.run.clone(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub bucket: Option<HistoryBucket>,
    pub include_partial: Option<bool>,
    pub pharmacies: Option<bool>,
}

pub async fn strain_history(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<HistoryQuery>,
) -> Result<Json<HistoryDto>, ApiError> {
    // Microsecond precision like every timestamp coming out of PostgreSQL
    // (and the precision the bounds are compared with there), so the echoed
    // `from`/`to` never carry nine fractional digits.
    let now = Utc::now().trunc_subsecs(6);
    let to = query.to.unwrap_or(now).trunc_subsecs(6);
    let from = query
        .from
        .unwrap_or_else(|| to - chrono::Duration::days(DEFAULT_HISTORY_DAYS))
        .trunc_subsecs(6);
    if from > to {
        return Err(ApiError::bad_request("`from` muss vor `to` liegen"));
    }
    if (to - from) > chrono::Duration::days(MAX_HISTORY_DAYS) {
        return Err(ApiError::bad_request(format!(
            "Zeitspanne darf höchstens {MAX_HISTORY_DAYS} Tage betragen"
        )));
    }
    let bucket = query.bucket.unwrap_or(HistoryBucket::Run);
    let include_partial = query.include_partial.unwrap_or(true);
    let with_pharmacies = query.pharmacies.unwrap_or(false);
    let tz = state.config.scrape_timezone;
    let tz_name = tz.name();

    if strains::get(&state.pool, id).await?.is_none() {
        return Err(ApiError::not_found(format!("Sorte {id} nicht gefunden")));
    }

    let points = match bucket {
        HistoryBucket::Run => {
            offers::history_by_run(&state.pool, id, from, to, include_partial).await?
        }
        HistoryBucket::Day => {
            offers::history_by_day(&state.pool, id, from, to, include_partial, tz_name).await?
        }
    };
    let pharmacies = if with_pharmacies {
        Some(match bucket {
            HistoryBucket::Run => {
                offers::pharmacy_series_by_run(&state.pool, id, from, to, include_partial).await?
            }
            HistoryBucket::Day => {
                offers::pharmacy_series_by_day(&state.pool, id, from, to, include_partial, tz_name)
                    .await?
            }
        })
    } else {
        None
    };

    Ok(Json(HistoryDto {
        strain_id: id,
        bucket,
        from,
        to,
        timezone: tz_name.to_owned(),
        points,
        pharmacies,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ReviewsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub sort: Option<ReviewSort>,
}

pub async fn strain_reviews(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<ReviewsQuery>,
) -> Result<Json<ReviewsResponseDto>, ApiError> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(ApiError::bad_request(
            "`limit` muss zwischen 1 und 500 liegen",
        ));
    }
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::bad_request("`offset` darf nicht negativ sein"));
    }
    if strains::get(&state.pool, id).await?.is_none() {
        return Err(ApiError::not_found(format!("Sorte {id} nicht gefunden")));
    }
    let response = reviews::response(
        &state.pool,
        id,
        query.sort.unwrap_or_default(),
        limit,
        offset,
    )
    .await?;
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

pub async fn runs_list(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<RunsQuery>,
) -> Result<Json<RunsResponseDto>, ApiError> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(ApiError::bad_request(
            "`limit` muss zwischen 1 und 500 liegen",
        ));
    }
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::bad_request("`offset` darf nicht negativ sein"));
    }
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(status) = status
        && RunStatus::parse(status).is_none()
    {
        return Err(ApiError::bad_request(format!(
            "Unbekannter Status {status:?}"
        )));
    }
    let (runs, total) = runs::list(&state.pool, limit, offset, status).await?;
    Ok(Json(RunsResponseDto { runs, total }))
}

pub async fn run_detail(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
) -> Result<Json<RunDetailDto>, ApiError> {
    let Some(run) = runs::get(&state.pool, id).await? else {
        return Err(ApiError::not_found(format!("Lauf {id} nicht gefunden")));
    };
    let errors = runs::errors(&state.pool, id).await?;
    Ok(Json(RunDetailDto { run, errors }))
}

pub async fn pharmacies(
    State(state): State<SharedState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let latest = state
        .snapshot
        .get_or_load(&state.pool)
        .await?
        .map(|s| s.run.id);
    let list = pharmacies::list(&state.pool, latest).await?;
    Ok(Json(json!({ "pharmacies": list })))
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub run_id: Option<i64>,
}

async fn export_snapshot(
    state: &SharedState,
    run_id: Option<i64>,
) -> Result<std::sync::Arc<Snapshot>, ApiError> {
    match run_id {
        None => snapshot(state).await,
        Some(id) => state
            .snapshot
            .get_run(&state.pool, id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("Lauf {id} nicht gefunden"))),
    }
}

pub async fn export_csv(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<ExportQuery>,
) -> Result<Response, ApiError> {
    let snapshot = export_snapshot(&state, query.run_id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"greenmedical_flowers.csv\"",
            ),
        ],
        snapshot.csv.clone(),
    )
        .into_response())
}

pub async fn export_json(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<ExportQuery>,
) -> Result<Response, ApiError> {
    let snapshot = export_snapshot(&state, query.run_id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"flowers.json\"",
            ),
        ],
        snapshot.export_json.clone(),
    )
        .into_response())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    Some(token.trim())
}

pub async fn admin_scrape(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let Some(expected) = state.config.admin_token() else {
        return Err(ApiError::not_found("Unbekannter Pfad"));
    };
    let provided = bearer_token(&headers).unwrap_or_default();
    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        return Err(ApiError::unauthorized());
    }
    match start_run(&state, RunTrigger::Manual).await {
        Ok(handle) => {
            let run_id = handle.run_id;
            let worker_state = state.clone();
            // Tracked so `serve()` waits for the run (or its shutdown
            // bookkeeping) before the pool is closed.
            state.tasks.spawn(async move {
                if let Err(err) = execute_run(worker_state, handle).await {
                    tracing::error!(run_id, %err, "manual scrape run errored");
                }
            });
            Ok((
                StatusCode::ACCEPTED,
                Json(json!({ "run_id": run_id, "status": "running" })),
            )
                .into_response())
        }
        Err(StartError::InProgress) => Err(ApiError::conflict("scrape_in_progress")),
        Err(StartError::LockHeld) => Err(ApiError::conflict("scrape_locked_elsewhere")),
        Err(StartError::Db(err)) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_matching_handles_lists_and_weak_tags() {
        let mut headers = HeaderMap::new();
        assert!(!etag_matches(&headers, "\"run-1\""));
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("\"run-1\""));
        assert!(etag_matches(&headers, "\"run-1\""));
        assert!(!etag_matches(&headers, "\"run-2\""));
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"run-0\", W/\"run-2\""),
        );
        assert!(etag_matches(&headers, "\"run-2\""));
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(etag_matches(&headers, "\"run-9\""));
    }

    #[test]
    fn bearer_token_extraction() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer_token(&headers), None);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc"),
        );
        assert_eq!(bearer_token(&headers), Some("abc"));
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));
        assert_eq!(bearer_token(&headers), None);
    }
}
