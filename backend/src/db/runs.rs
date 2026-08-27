//! `scrape_runs` and `scrape_run_errors`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;

use crate::domain::{RunDto, RunErrorDto, RunStatus, RunTrigger};

/// Raw row of `scrape_runs`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunRow {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub trigger: String,
    pub instance: Option<String>,
    pub pharmacies_total: Option<i32>,
    pub pharmacies_scraped: Option<i32>,
    pub pharmacies_failed: Option<i32>,
    pub offer_count: Option<i32>,
    pub http_requests: Option<i32>,
    pub error: Option<String>,
    pub reviews_scraped: Option<i32>,
    pub reviews_failed: Option<i32>,
}

impl From<RunRow> for RunDto {
    fn from(row: RunRow) -> Self {
        RunDto {
            id: row.id,
            started_at: row.started_at,
            finished_at: row.finished_at,
            status: RunStatus::parse(&row.status).unwrap_or(RunStatus::Failed),
            trigger: RunTrigger::parse(&row.trigger).unwrap_or(RunTrigger::Schedule),
            instance: row.instance,
            pharmacies_total: row.pharmacies_total,
            pharmacies_scraped: row.pharmacies_scraped,
            pharmacies_failed: row.pharmacies_failed,
            offer_count: row.offer_count,
            http_requests: row.http_requests,
            error: row.error,
            reviews_scraped: row.reviews_scraped,
            reviews_failed: row.reviews_failed,
        }
    }
}

/// Counters written when a run finishes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunCounts {
    pub pharmacies_total: i32,
    pub pharmacies_scraped: i32,
    pub pharmacies_failed: i32,
    pub offer_count: i32,
    pub http_requests: i32,
}

/// Create a `running` row and return its id.
pub async fn insert_running<'e>(
    exec: impl PgExecutor<'e>,
    trigger: RunTrigger,
    instance: &str,
) -> sqlx::Result<i64> {
    let row = sqlx::query!(
        r#"INSERT INTO scrape_runs (status, trigger, instance) VALUES ('running', $1, $2) RETURNING id"#,
        trigger.as_str(),
        instance
    )
    .fetch_one(exec)
    .await?;
    Ok(row.id)
}

/// Create a `running` row with an explicit start time (tests, seeding).
pub async fn insert_running_at<'e>(
    exec: impl PgExecutor<'e>,
    trigger: RunTrigger,
    instance: &str,
    started_at: DateTime<Utc>,
) -> sqlx::Result<i64> {
    let row = sqlx::query!(
        r#"INSERT INTO scrape_runs (status, trigger, instance, started_at) VALUES ('running', $1, $2, $3) RETURNING id"#,
        trigger.as_str(),
        instance,
        started_at
    )
    .fetch_one(exec)
    .await?;
    Ok(row.id)
}

/// Finish a run with the final status and counters.
pub async fn finish<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    status: RunStatus,
    counts: RunCounts,
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE scrape_runs
           SET finished_at = now(), status = $2, pharmacies_total = $3, pharmacies_scraped = $4,
               pharmacies_failed = $5, offer_count = $6, http_requests = $7, error = $8
           WHERE id = $1"#,
        run_id,
        status.as_str(),
        counts.pharmacies_total,
        counts.pharmacies_scraped,
        counts.pharmacies_failed,
        counts.offer_count,
        counts.http_requests,
        error
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Finish a run with an explicit finish time (tests, seeding).
pub async fn finish_at<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    status: RunStatus,
    counts: RunCounts,
    finished_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE scrape_runs
           SET finished_at = $9, status = $2, pharmacies_total = $3, pharmacies_scraped = $4,
               pharmacies_failed = $5, offer_count = $6, http_requests = $7, error = $8
           WHERE id = $1"#,
        run_id,
        status.as_str(),
        counts.pharmacies_total,
        counts.pharmacies_scraped,
        counts.pharmacies_failed,
        counts.offer_count,
        counts.http_requests,
        None::<String>,
        finished_at
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Store the phase-2 (reviews) counters of a run.
pub async fn set_review_counts<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    scraped: i32,
    failed: i32,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE scrape_runs SET reviews_scraped = $2, reviews_failed = $3 WHERE id = $1"#,
        run_id,
        scraped,
        failed
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Mark a run as failed with an error message (keeps existing counters).
pub async fn mark_failed<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    error: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE scrape_runs SET finished_at = now(), status = 'failed', error = $2 WHERE id = $1"#,
        run_id,
        error
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// Mark `running` rows older than `older_than` as failed. Returns the number of rows changed.
pub async fn mark_stale<'e>(exec: impl PgExecutor<'e>, older_than: Duration) -> sqlx::Result<u64> {
    let cutoff =
        Utc::now() - chrono::Duration::from_std(older_than).unwrap_or(chrono::Duration::hours(2));
    let result = sqlx::query!(
        r#"UPDATE scrape_runs SET finished_at = now(), status = 'failed', error = 'stale'
           WHERE status = 'running' AND started_at < $1"#,
        cutoff
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// Is any run currently `running` (on any replica)? Served from the partial
/// index `scrape_runs_running_idx`.
pub async fn any_running<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM scrape_runs WHERE status = 'running') AS "running!""#
    )
    .fetch_one(exec)
    .await
}

/// Newest usable (`success`/`partial`) run.
pub async fn latest_usable<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<Option<RunDto>> {
    let row = sqlx::query_as!(
        RunRow,
        r#"SELECT id, started_at, finished_at, status, trigger, instance, pharmacies_total, pharmacies_scraped,
                  pharmacies_failed, offer_count, http_requests, error, reviews_scraped, reviews_failed
           FROM scrape_runs WHERE status IN ('success', 'partial')
           ORDER BY started_at DESC, id DESC LIMIT 1"#
    )
    .fetch_optional(exec)
    .await?;
    Ok(row.map(Into::into))
}

/// Id of the newest usable run only (cheap revalidation of the snapshot cache;
/// served from the partial index on `started_at`).
pub async fn latest_usable_id<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!(
        r#"SELECT id FROM scrape_runs WHERE status IN ('success', 'partial')
           ORDER BY started_at DESC, id DESC LIMIT 1"#
    )
    .fetch_optional(exec)
    .await
}

/// Newest usable run started at or before `before` (trend reference).
pub async fn reference_run<'e>(
    exec: impl PgExecutor<'e>,
    before: DateTime<Utc>,
) -> sqlx::Result<Option<RunDto>> {
    let row = sqlx::query_as!(
        RunRow,
        r#"SELECT id, started_at, finished_at, status, trigger, instance, pharmacies_total, pharmacies_scraped,
                  pharmacies_failed, offer_count, http_requests, error, reviews_scraped, reviews_failed
           FROM scrape_runs WHERE status IN ('success', 'partial') AND started_at <= $1
           ORDER BY started_at DESC, id DESC LIMIT 1"#,
        before
    )
    .fetch_optional(exec)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn get<'e>(exec: impl PgExecutor<'e>, run_id: i64) -> sqlx::Result<Option<RunDto>> {
    let row = sqlx::query_as!(
        RunRow,
        r#"SELECT id, started_at, finished_at, status, trigger, instance, pharmacies_total, pharmacies_scraped,
                  pharmacies_failed, offer_count, http_requests, error, reviews_scraped, reviews_failed
           FROM scrape_runs WHERE id = $1"#,
        run_id
    )
    .fetch_optional(exec)
    .await?;
    Ok(row.map(Into::into))
}

/// Paginated run list, newest first, optionally filtered by status.
pub async fn list<'e>(
    exec: impl PgExecutor<'e> + Copy,
    limit: i64,
    offset: i64,
    status: Option<&str>,
) -> sqlx::Result<(Vec<RunDto>, i64)> {
    let rows = sqlx::query_as!(
        RunRow,
        r#"SELECT id, started_at, finished_at, status, trigger, instance, pharmacies_total, pharmacies_scraped,
                  pharmacies_failed, offer_count, http_requests, error, reviews_scraped, reviews_failed
           FROM scrape_runs WHERE ($3::text IS NULL OR status = $3)
           ORDER BY started_at DESC, id DESC LIMIT $1 OFFSET $2"#,
        limit,
        offset,
        status
    )
    .fetch_all(exec)
    .await?;
    let total = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "total!" FROM scrape_runs WHERE ($1::text IS NULL OR status = $1)"#,
        status
    )
    .fetch_one(exec)
    .await?;
    Ok((rows.into_iter().map(Into::into).collect(), total))
}

pub async fn errors<'e>(exec: impl PgExecutor<'e>, run_id: i64) -> sqlx::Result<Vec<RunErrorDto>> {
    let rows = sqlx::query_as!(
        RunErrorDto,
        r#"SELECT pharmacy_name, pharmacy_url, stage, message FROM scrape_run_errors WHERE run_id = $1 ORDER BY id"#,
        run_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows)
}

pub async fn insert_errors<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    errors: &[RunErrorDto],
) -> sqlx::Result<()> {
    if errors.is_empty() {
        return Ok(());
    }
    let names: Vec<&str> = errors.iter().map(|e| e.pharmacy_name.as_str()).collect();
    let urls: Vec<&str> = errors.iter().map(|e| e.pharmacy_url.as_str()).collect();
    let stages: Vec<&str> = errors.iter().map(|e| e.stage.as_str()).collect();
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    sqlx::query!(
        r#"INSERT INTO scrape_run_errors (run_id, pharmacy_name, pharmacy_url, stage, message)
           SELECT $1, * FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[])"#,
        run_id,
        &names as &[&str],
        &urls as &[&str],
        &stages as &[&str],
        &messages as &[&str]
    )
    .execute(exec)
    .await?;
    Ok(())
}
