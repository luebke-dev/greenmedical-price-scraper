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
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};

use super::error::ApiError;
use super::extract::{ApiPath, ApiQuery};
use super::strains_page::StrainsQuery;
use crate::db::reviews::ReviewSort;
use crate::db::snapshot::Snapshot;
use crate::db::{offers, pharmacies, reviews, runs, strains};
use crate::domain::{
    self, HistoryBucket, HistoryDto, MetadataDto, OfferHistoryMode, OfferHistoryPageDto,
    OfferHistoryRows, PharmacySeriesDto, ReviewsResponseDto, RunDetailDto, RunStatus, RunTrigger,
    RunsResponseDto, StrainDetailDto, StrainDto, StrainsPageDto, offer_history,
};
use crate::scrape::run::{StartError, execute_run, start_run};
use crate::state::SharedState;

const CACHE_CONTROL_STRAINS: &str = "public, max-age=300";
/// `/metadata` carries live fields (`next_run_at`, `scrape_running`), hence the shorter TTL.
const CACHE_CONTROL_METADATA: &str = "public, max-age=60";
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

/// `GET /healthz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HealthzDto {
    #[schema(example = "ok")]
    pub status: String,
}

/// `GET /readyz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReadyzDto {
    /// `ready` | `not_ready`
    #[schema(example = "ready")]
    pub status: String,
    /// `ok` | `error` | `timeout` | `unknown`
    #[schema(example = "ok")]
    pub db: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `202` of `POST /api/v1/admin/scrape`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScrapeAcceptedDto {
    pub run_id: i64,
    #[schema(example = "running")]
    pub status: String,
}

/// `GET /api/v1/pharmacies`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PharmaciesResponseDto {
    pub pharmacies: Vec<domain::PharmacyDto>,
}

/// Liveness
///
/// Antwortet immer `200`, solange der Prozess läuft.
#[utoipa::path(get, path = "/healthz", tag = "health",
    responses((status = 200, description = "Prozess lebt", body = HealthzDto)))]
pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Readiness
///
/// `200`, wenn der Start abgeschlossen ist und die Datenbank innerhalb von 2 s antwortet; sonst `503`.
#[utoipa::path(get, path = "/readyz", tag = "health",
    responses(
        (status = 200, description = "Bereit", body = ReadyzDto),
        (status = 503, description = "Nicht bereit (Start, Shutdown oder Datenbank nicht erreichbar)", body = ReadyzDto)))]
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

/// Kennzahlen des letzten usable Laufs
///
/// Günstigste/stärkste Angebote, Zähler und der Lauf selbst aus dem Snapshot, ergänzt um die
/// Live-Felder `next_run_at` (deterministisch aus Cron + Zeitzone, `null` bei `SCRAPE_ENABLED=false`),
/// `scrape_running` (DB-Abfrage, replikaübergreifend) und `schedule`. Pro Request serialisiert;
/// `Cache-Control: public, max-age=60`, kein `ETag` (die Antwort ändert sich mit der Zeit).
#[utoipa::path(get, path = "/api/v1/metadata", tag = "strains",
    responses(
        (status = 200, description = "Metadaten", body = MetadataDto,
            headers(("Cache-Control" = String, description = "`public, max-age=60`"))),
        (status = 404, description = "Noch kein usable Lauf (`no_data`)", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn metadata(State(state): State<SharedState>) -> Result<Response, ApiError> {
    let snapshot = snapshot(&state).await?;
    let scrape_running = runs::any_running(&state.pool).await?;
    let now = Utc::now();
    let metadata = MetadataDto {
        next_run_at: state.config.next_scrape_at(now),
        scrape_running,
        schedule: state.config.schedule_dto(),
        ..snapshot.metadata.clone()
    };
    let body = Bytes::from(serde_json::to_vec(&metadata).expect("serialisable"));
    let mut response = json_bytes(body);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(CACHE_CONTROL_METADATA),
    );
    Ok(response)
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

/// Sortenliste (serverseitig paginiert)
///
/// Filtert, sortiert und paginiert die Sorten des letzten usable Laufs im Speicher. Einträge ohne `offers`/`search`;
/// `facets` über alle Sorten des Laufs. Liefert `ETag` (`"run-<id>[-r<ms>]-<hash>"`) und `Cache-Control: public, max-age=300`;
/// `If-None-Match` ⇒ `304`.
#[utoipa::path(get, path = "/api/v1/strains", tag = "strains",
    params(StrainsQuery),
    responses(
        (status = 200, description = "Seite der Sortenliste", body = StrainsPageDto,
            headers(("ETag" = String, description = "`\"run-<id>[-r<ms>]-<fnv1a der normalisierten Query>\"`"))),
        (status = 304, description = "Unverändert (`If-None-Match`)"),
        (status = 400, description = "Ungültige Query-Parameter", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Noch kein usable Lauf (`no_data`)", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn strains(
    State(state): State<SharedState>,
    headers: HeaderMap,
    ApiQuery(query): ApiQuery<StrainsQuery>,
) -> Result<Response, ApiError> {
    let request = query.validate()?;
    let snapshot = snapshot(&state).await?;
    let etag = format!("\"{}-{}\"", snapshot.etag_base, request.hash());
    let cache_headers = [
        (
            header::ETAG,
            HeaderValue::from_str(&etag).expect("valid etag"),
        ),
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static(CACHE_CONTROL_STRAINS),
        ),
    ];
    if etag_matches(&headers, &etag) {
        return Ok((StatusCode::NOT_MODIFIED, cache_headers).into_response());
    }
    let page = request.page(&snapshot);
    Ok((cache_headers, Json(page)).into_response())
}

/// Sortendetail
///
/// Inklusive `offers` und `search`. Sorten, die im letzten Lauf nicht gelistet sind, kommen mit `in_latest_run: false`
/// und leeren Angeboten zurück.
#[utoipa::path(get, path = "/api/v1/strains/{id}", tag = "strains",
    params(("id" = i64, Path, description = "Sorten-ID")),
    responses(
        (status = 200, description = "Sorte", body = StrainDetailDto),
        (status = 400, description = "Ungültige ID", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Sorte unbekannt oder noch kein usable Lauf", body = crate::api::error::ErrorEnvelopeDto)))]
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Beginn des Zeitfensters (RFC 3339); Default `to − 90 Tage`.
    pub from: Option<DateTime<Utc>>,
    /// Ende des Zeitfensters (RFC 3339); Default jetzt. Höchstens 730 Tage nach `from`.
    pub to: Option<DateTime<Utc>>,
    /// `run` (Default) = ein Punkt je Lauf, `day` = Aggregation je Kalendertag (`SCRAPE_TIMEZONE`).
    pub bucket: Option<HistoryBucket>,
    /// Läufe mit Status `partial` einbeziehen (Default `true`).
    pub include_partial: Option<bool>,
    /// `true` ⇒ zusätzlich je Apotheke eine Serie (`pharmacies`).
    pub pharmacies: Option<bool>,
}

/// Validated `from`/`to` window shared by `/history` and `/offer-history`.
fn history_window(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), ApiError> {
    // Microsecond precision like every timestamp coming out of PostgreSQL
    // (and the precision the bounds are compared with there), so the echoed
    // `from`/`to` never carry nine fractional digits.
    let now = Utc::now().trunc_subsecs(6);
    let to = to.unwrap_or(now).trunc_subsecs(6);
    let from = from
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
    Ok((from, to))
}

async fn pharmacy_series(
    state: &SharedState,
    id: i64,
    bucket: HistoryBucket,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    include_partial: bool,
) -> Result<Vec<PharmacySeriesDto>, ApiError> {
    let tz_name = state.config.scrape_timezone.name();
    Ok(match bucket {
        HistoryBucket::Run => {
            offers::pharmacy_series_by_run(&state.pool, id, from, to, include_partial).await?
        }
        HistoryBucket::Day => {
            offers::pharmacy_series_by_day(&state.pool, id, from, to, include_partial, tz_name)
                .await?
        }
    })
}

/// Preisverlauf einer Sorte
///
/// `min`/`avg`/`max` je Lauf oder Tag, optional je Apotheke.
#[utoipa::path(get, path = "/api/v1/strains/{id}/history", tag = "strains",
    params(("id" = i64, Path, description = "Sorten-ID"), HistoryQuery),
    responses(
        (status = 200, description = "Preisverlauf", body = HistoryDto),
        (status = 400, description = "Ungültige Parameter oder Zeitfenster", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Sorte unbekannt", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn strain_history(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<HistoryQuery>,
) -> Result<Json<HistoryDto>, ApiError> {
    let (from, to) = history_window(query.from, query.to)?;
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
        Some(pharmacy_series(&state, id, bucket, from, to, include_partial).await?)
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct OfferHistoryQuery {
    /// Beginn des Zeitfensters (RFC 3339); Default `to − 90 Tage`.
    pub from: Option<DateTime<Utc>>,
    /// Ende des Zeitfensters (RFC 3339); Default jetzt. Höchstens 730 Tage nach `from`.
    pub to: Option<DateTime<Utc>>,
    /// `run` (Default) oder `day`.
    pub bucket: Option<HistoryBucket>,
    /// Läufe mit Status `partial` einbeziehen (Default `true`).
    pub include_partial: Option<bool>,
    /// `changes` (Default) = Phasen gleichen Preises/Status je Apotheke, `all` = eine Zeile je Bucket und Apotheke.
    pub mode: Option<OfferHistoryMode>,
    /// Nur diese Apotheke.
    pub pharmacy_id: Option<i64>,
    /// 1–500, Default 50.
    pub limit: Option<i64>,
    /// ≥ 0, Default 0.
    pub offset: Option<i64>,
}

fn validate_page(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), ApiError> {
    let limit = limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err(ApiError::bad_request(
            "`limit` muss zwischen 1 und 500 liegen",
        ));
    }
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::bad_request("`offset` darf nicht negativ sein"));
    }
    Ok((limit, offset))
}

fn slice<T>(mut rows: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);
    if offset >= rows.len() {
        return Vec::new();
    }
    rows.drain(..offset);
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    rows
}

/// Angebotshistorie einer Sorte (paginiert)
///
/// `rows` sind je nach `mode` `OfferPhaseRow[]` (`changes`) oder `OfferHistoryRow[]` (`all`).
#[utoipa::path(get, path = "/api/v1/strains/{id}/offer-history", tag = "strains",
    params(("id" = i64, Path, description = "Sorten-ID"), OfferHistoryQuery),
    responses(
        (status = 200, description = "Seite der Angebotshistorie", body = OfferHistoryPageDto),
        (status = 400, description = "Ungültige Parameter", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Sorte unbekannt", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn strain_offer_history(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<OfferHistoryQuery>,
) -> Result<Json<OfferHistoryPageDto>, ApiError> {
    let (from, to) = history_window(query.from, query.to)?;
    let (limit, offset) = validate_page(query.limit, query.offset)?;
    let bucket = query.bucket.unwrap_or(HistoryBucket::Run);
    let mode = query.mode.unwrap_or_default();
    let include_partial = query.include_partial.unwrap_or(true);

    if strains::get(&state.pool, id).await?.is_none() {
        return Err(ApiError::not_found(format!("Sorte {id} nicht gefunden")));
    }
    let series = pharmacy_series(&state, id, bucket, from, to, include_partial).await?;
    let (total, rows) = match mode {
        OfferHistoryMode::Changes => {
            // Phases are computed over every pharmacy (the bucket set is the
            // strain's), the filter applies to the resulting rows.
            let mut rows = offer_history::phases(&series);
            if let Some(pharmacy_id) = query.pharmacy_id {
                rows.retain(|r| r.pharmacy_id == pharmacy_id);
            }
            (
                rows.len(),
                OfferHistoryRows::Changes(slice(rows, limit, offset)),
            )
        }
        OfferHistoryMode::All => {
            let mut rows = offer_history::all_rows(&series);
            if let Some(pharmacy_id) = query.pharmacy_id {
                rows.retain(|r| r.pharmacy_id == pharmacy_id);
            }
            (
                rows.len(),
                OfferHistoryRows::All(slice(rows, limit, offset)),
            )
        }
    };
    Ok(Json(OfferHistoryPageDto {
        strain_id: id,
        bucket,
        mode,
        from,
        to,
        total: total as i64,
        limit,
        offset,
        rows,
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReviewsQuery {
    /// 1–500, Default 50.
    pub limit: Option<i64>,
    /// ≥ 0, Default 0.
    pub offset: Option<i64>,
    /// `newest` (Default) | `oldest` | `highest` | `lowest`.
    pub sort: Option<ReviewSort>,
}

/// Bewertungen einer Sorte
///
/// Zusammenfassung (Verteilung, verifizierte Käufe), Verlauf der Durchschnittsbewertung und die gespeicherten Reviews.
#[utoipa::path(get, path = "/api/v1/strains/{id}/reviews", tag = "strains",
    params(("id" = i64, Path, description = "Sorten-ID"), ReviewsQuery),
    responses(
        (status = 200, description = "Bewertungen", body = ReviewsResponseDto),
        (status = 400, description = "Ungültige Parameter", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Sorte unbekannt", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn strain_reviews(
    State(state): State<SharedState>,
    ApiPath(id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<ReviewsQuery>,
) -> Result<Json<ReviewsResponseDto>, ApiError> {
    let (limit, offset) = validate_page(query.limit, query.offset)?;
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RunsQuery {
    /// 1–500, Default 50.
    pub limit: Option<i64>,
    /// ≥ 0, Default 0.
    pub offset: Option<i64>,
    /// Nur Läufe mit diesem Status: `running` | `success` | `partial` | `failed`.
    pub status: Option<String>,
}

/// Scrape-Läufe (neueste zuerst)
#[utoipa::path(get, path = "/api/v1/runs", tag = "runs",
    params(RunsQuery),
    responses(
        (status = 200, description = "Läufe und Gesamtzahl", body = RunsResponseDto),
        (status = 400, description = "Ungültige Parameter", body = crate::api::error::ErrorEnvelopeDto)))]
pub async fn runs_list(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<RunsQuery>,
) -> Result<Json<RunsResponseDto>, ApiError> {
    let (limit, offset) = validate_page(query.limit, query.offset)?;
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

/// Lauf mit Fehlerliste
#[utoipa::path(get, path = "/api/v1/runs/{id}", tag = "runs",
    params(("id" = i64, Path, description = "Lauf-ID")),
    responses(
        (status = 200, description = "Lauf inkl. `errors`", body = RunDetailDto),
        (status = 404, description = "Lauf unbekannt", body = crate::api::error::ErrorEnvelopeDto)))]
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

/// Apotheken
///
/// Alle jemals gesehenen Apotheken mit `offer_count_latest` aus dem letzten usable Lauf (0 ohne Lauf).
#[utoipa::path(get, path = "/api/v1/pharmacies", tag = "pharmacies",
    responses((status = 200, description = "Apotheken", body = PharmaciesResponseDto)))]
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ExportQuery {
    /// Bestimmter Lauf statt des letzten usable Laufs.
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

/// CSV-Export
///
/// Alle Angebote des Laufs in Scrape-Reihenfolge mit den 11 Spalten des ursprünglichen Scrapers
/// (`apotheke, apotheke_plz, apotheke_stadt, name, bezeichnung, genetik, thc, cbd, preis_pro_gramm, verfuegbarkeit, produkt_url`).
#[utoipa::path(get, path = "/api/v1/export.csv", tag = "export",
    params(ExportQuery),
    responses(
        (status = 200, description = "`text/csv; charset=utf-8`, `Content-Disposition: attachment`", content_type = "text/csv", body = String),
        (status = 404, description = "Lauf unbekannt oder noch kein usable Lauf", body = crate::api::error::ErrorEnvelopeDto)))]
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

/// JSON-Export
///
/// Array aller Sorten des Laufs inklusive `offers` und `search` (`Content-Disposition: attachment`).
#[utoipa::path(get, path = "/api/v1/export.json", tag = "export",
    params(ExportQuery),
    responses(
        (status = 200, description = "Sorten inkl. Angebote", body = Vec<StrainDto>),
        (status = 404, description = "Lauf unbekannt oder noch kein usable Lauf", body = crate::api::error::ErrorEnvelopeDto)))]
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

/// Manuellen Scrape-Lauf starten
///
/// Startet einen Lauf im Hintergrund (`trigger: manual`). Nur mit gesetztem `ADMIN_TOKEN`; ohne Token antwortet der Pfad `404`.
#[utoipa::path(post, path = "/api/v1/admin/scrape", tag = "admin",
    security(("admin_token" = [])),
    responses(
        (status = 202, description = "Lauf gestartet", body = ScrapeAcceptedDto),
        (status = 401, description = "Fehlendes oder falsches Bearer-Token", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "`ADMIN_TOKEN` nicht gesetzt", body = crate::api::error::ErrorEnvelopeDto),
        (status = 409, description = "`scrape_in_progress` | `scrape_locked_elsewhere`", body = crate::api::error::ErrorEnvelopeDto)))]
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
