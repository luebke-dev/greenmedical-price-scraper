//! `subscribers`, `subscription_rules` and `notifications`.

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;

use crate::domain::{RuleDto, RuleInputDto, RuleKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriberRow {
    pub id: i64,
    pub email: String,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub confirm_token: String,
    pub manage_token: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_notified_run_id: Option<i64>,
}

impl SubscriberRow {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }
}

/// A rule joined with the strain's display name.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleRow {
    pub id: i64,
    pub subscriber_id: i64,
    pub kind: RuleKind,
    pub strain_id: Option<i64>,
    pub threshold: Option<f64>,
    pub strain_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<RuleRow> for RuleDto {
    fn from(row: RuleRow) -> Self {
        RuleDto {
            id: row.id,
            kind: row.kind,
            strain_id: row.strain_id,
            threshold: row.threshold,
            strain_name: row.strain_name,
            created_at: row.created_at,
        }
    }
}

macro_rules! subscriber_by {
    ($exec:expr, $sql:literal, $value:expr) => {
        sqlx::query_as!(SubscriberRow, $sql, $value)
            .fetch_optional($exec)
            .await
    };
}

/// Case-insensitive lookup (`email` is `citext`).
pub async fn find_by_email<'e>(
    exec: impl PgExecutor<'e>,
    email: &str,
) -> sqlx::Result<Option<SubscriberRow>> {
    subscriber_by!(
        exec,
        r#"SELECT id, email::text AS "email!", confirmed_at, confirm_token, manage_token, created_at, updated_at, last_notified_run_id
           FROM subscribers WHERE email = $1::citext"#,
        email
    )
}

pub async fn find_by_confirm_token<'e>(
    exec: impl PgExecutor<'e>,
    token: &str,
) -> sqlx::Result<Option<SubscriberRow>> {
    subscriber_by!(
        exec,
        r#"SELECT id, email::text AS "email!", confirmed_at, confirm_token, manage_token, created_at, updated_at, last_notified_run_id
           FROM subscribers WHERE confirm_token = $1"#,
        token
    )
}

pub async fn find_by_manage_token<'e>(
    exec: impl PgExecutor<'e>,
    token: &str,
) -> sqlx::Result<Option<SubscriberRow>> {
    subscriber_by!(
        exec,
        r#"SELECT id, email::text AS "email!", confirmed_at, confirm_token, manage_token, created_at, updated_at, last_notified_run_id
           FROM subscribers WHERE manage_token = $1"#,
        token
    )
}

pub async fn get<'e>(exec: impl PgExecutor<'e>, id: i64) -> sqlx::Result<Option<SubscriberRow>> {
    subscriber_by!(
        exec,
        r#"SELECT id, email::text AS "email!", confirmed_at, confirm_token, manage_token, created_at, updated_at, last_notified_run_id
           FROM subscribers WHERE id = $1"#,
        id
    )
}

/// Create an unconfirmed subscriber.
pub async fn insert<'e>(
    exec: impl PgExecutor<'e>,
    email: &str,
    confirm_token: &str,
    manage_token: &str,
) -> sqlx::Result<SubscriberRow> {
    sqlx::query_as!(
        SubscriberRow,
        r#"INSERT INTO subscribers (email, confirm_token, manage_token) VALUES ($1::citext, $2, $3)
           RETURNING id, email::text AS "email!", confirmed_at, confirm_token, manage_token, created_at, updated_at, last_notified_run_id"#,
        email,
        confirm_token,
        manage_token
    )
    .fetch_one(exec)
    .await
}

/// Set `confirmed_at` (idempotent).
pub async fn confirm<'e>(exec: impl PgExecutor<'e>, id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE subscribers SET confirmed_at = COALESCE(confirmed_at, now()), updated_at = now() WHERE id = $1"#,
        id
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn touch<'e>(exec: impl PgExecutor<'e>, id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE subscribers SET updated_at = now() WHERE id = $1"#,
        id
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn delete<'e>(exec: impl PgExecutor<'e>, id: i64) -> sqlx::Result<()> {
    sqlx::query!(r#"DELETE FROM subscribers WHERE id = $1"#, id)
        .execute(exec)
        .await?;
    Ok(())
}

/// Delete unconfirmed subscribers created before `cutoff`; returns the count.
pub async fn delete_unconfirmed_before<'e>(
    exec: impl PgExecutor<'e>,
    cutoff: DateTime<Utc>,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"DELETE FROM subscribers WHERE confirmed_at IS NULL AND created_at < $1"#,
        cutoff
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// `(confirmed, unconfirmed)` subscriber counts for the gauge.
pub async fn counts<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<(i64, i64)> {
    let row = sqlx::query!(
        r#"SELECT COUNT(*) FILTER (WHERE confirmed_at IS NOT NULL) AS "confirmed!",
                  COUNT(*) FILTER (WHERE confirmed_at IS NULL) AS "unconfirmed!"
           FROM subscribers"#
    )
    .fetch_one(exec)
    .await?;
    Ok((row.confirmed, row.unconfirmed))
}

/// Add rules; duplicates (same kind/strain/threshold) are ignored.
pub async fn add_rules<'e>(
    exec: impl PgExecutor<'e>,
    subscriber_id: i64,
    rules: &[RuleInputDto],
) -> sqlx::Result<()> {
    if rules.is_empty() {
        return Ok(());
    }
    let kinds: Vec<&str> = rules.iter().map(|r| r.kind.as_str()).collect();
    let strain_ids: Vec<Option<i64>> = rules.iter().map(|r| r.strain_id).collect();
    let thresholds: Vec<Option<f64>> = rules.iter().map(|r| r.threshold).collect();
    sqlx::query!(
        r#"INSERT INTO subscription_rules (subscriber_id, kind, strain_id, threshold)
           SELECT $1, kind, strain_id, threshold::numeric(8, 2)
           FROM UNNEST($2::text[], $3::int8[], $4::float8[]) AS r (kind, strain_id, threshold)
           ON CONFLICT DO NOTHING"#,
        subscriber_id,
        &kinds as &[&str],
        &strain_ids as &[Option<i64>],
        &thresholds as &[Option<f64>]
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn delete_rules<'e>(exec: impl PgExecutor<'e>, subscriber_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        r#"DELETE FROM subscription_rules WHERE subscriber_id = $1"#,
        subscriber_id
    )
    .execute(exec)
    .await?;
    Ok(())
}

struct RawRule {
    id: i64,
    subscriber_id: i64,
    kind: String,
    strain_id: Option<i64>,
    threshold: Option<f64>,
    strain_name: Option<String>,
    created_at: DateTime<Utc>,
}

fn rule_rows(raw: Vec<RawRule>) -> Vec<RuleRow> {
    raw.into_iter()
        .filter_map(|r| {
            let kind = RuleKind::parse(&r.kind)?;
            Some(RuleRow {
                id: r.id,
                subscriber_id: r.subscriber_id,
                kind,
                strain_id: r.strain_id,
                threshold: r.threshold,
                strain_name: r.strain_name,
                created_at: r.created_at,
            })
        })
        .collect()
}

/// Rules of one subscriber in creation order.
pub async fn rules_for<'e>(
    exec: impl PgExecutor<'e>,
    subscriber_id: i64,
) -> sqlx::Result<Vec<RuleRow>> {
    let raw = sqlx::query_as!(
        RawRule,
        r#"SELECT r.id, r.subscriber_id, r.kind, r.strain_id, r.threshold::float8 AS "threshold?: f64",
                  s.name AS "strain_name?", r.created_at
           FROM subscription_rules r LEFT JOIN strains s ON s.id = r.strain_id
           WHERE r.subscriber_id = $1 ORDER BY r.id"#,
        subscriber_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rule_rows(raw))
}

/// Rules of every confirmed subscriber, ordered by subscriber then rule id.
pub async fn rules_of_confirmed<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<Vec<RuleRow>> {
    let raw = sqlx::query_as!(
        RawRule,
        r#"SELECT r.id, r.subscriber_id, r.kind, r.strain_id, r.threshold::float8 AS "threshold?: f64",
                  s.name AS "strain_name?", r.created_at
           FROM subscription_rules r
           JOIN subscribers sub ON sub.id = r.subscriber_id
           LEFT JOIN strains s ON s.id = r.strain_id
           WHERE sub.confirmed_at IS NOT NULL ORDER BY r.subscriber_id, r.id"#
    )
    .fetch_all(exec)
    .await?;
    Ok(rule_rows(raw))
}

/// Per-strain state of one run: the values the rules compare.
#[derive(Debug, Clone, PartialEq)]
pub struct StrainState {
    pub strain_id: i64,
    pub name: String,
    pub bezeichnung: String,
    /// Cheapest parsed price of the run (`None` when no offer has a price).
    pub min_price: Option<f64>,
    /// Highest parsed THC value among the run's offers.
    pub thc_value: Option<f64>,
    /// Pharmacy of the cheapest offer.
    pub pharmacy: String,
}

/// State of every strain listed (≥ 1 offer) in `run_id`.
pub async fn strain_states<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
) -> sqlx::Result<Vec<StrainState>> {
    let rows = sqlx::query!(
        r#"SELECT o.strain_id, s.name, s.bezeichnung,
                  MIN(o.price_eur)::float8 AS "min_price?: f64",
                  MAX(o.thc_pct)::float8 AS "thc_value?: f64",
                  (ARRAY_AGG(p.name ORDER BY o.price_eur ASC NULLS LAST, o.id))[1] AS "pharmacy!"
           FROM offers o JOIN strains s ON s.id = o.strain_id JOIN pharmacies p ON p.id = o.pharmacy_id
           WHERE o.run_id = $1
           GROUP BY o.strain_id, s.name, s.bezeichnung
           ORDER BY o.strain_id"#,
        run_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| StrainState {
            strain_id: r.strain_id,
            name: r.name,
            bezeichnung: r.bezeichnung,
            min_price: r.min_price.map(crate::domain::round2),
            thc_value: r.thc_value,
            pharmacy: r.pharmacy,
        })
        .collect())
}

/// The usable run that precedes `run_id` in `(started_at, id)` order.
pub async fn previous_usable_run_id<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!(
        r#"SELECT r.id FROM scrape_runs r, scrape_runs cur
           WHERE cur.id = $1 AND r.status IN ('success', 'partial')
             AND (r.started_at, r.id) < (cur.started_at, cur.id)
           ORDER BY r.started_at DESC, r.id DESC LIMIT 1"#,
        run_id
    )
    .fetch_optional(exec)
    .await
}

/// Insert a notification; `None` when `(rule_id, strain_id, run_id)` already exists.
pub async fn insert_notification<'e>(
    exec: impl PgExecutor<'e>,
    subscriber_id: i64,
    run_id: i64,
    rule_id: i64,
    strain_id: Option<i64>,
    payload: &serde_json::Value,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!(
        r#"INSERT INTO notifications (subscriber_id, run_id, rule_id, strain_id, payload)
           VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING RETURNING id"#,
        subscriber_id,
        run_id,
        rule_id,
        strain_id,
        payload
    )
    .fetch_optional(exec)
    .await
}

/// Record the delivery result of the notifications of one digest.
pub async fn mark_sent<'e>(
    exec: impl PgExecutor<'e>,
    ids: &[i64],
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE notifications
           SET sent_at = CASE WHEN $2::text IS NULL THEN now() ELSE NULL END, error = $2
           WHERE id = ANY($1)"#,
        ids,
        error
    )
    .execute(exec)
    .await?;
    Ok(())
}

pub async fn set_last_notified<'e>(
    exec: impl PgExecutor<'e>,
    subscriber_id: i64,
    run_id: i64,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"UPDATE subscribers SET last_notified_run_id = $2 WHERE id = $1"#,
        subscriber_id,
        run_id
    )
    .execute(exec)
    .await?;
    Ok(())
}

/// A stored notification (tests, diagnostics).
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationRow {
    pub id: i64,
    pub subscriber_id: i64,
    pub run_id: i64,
    pub rule_id: i64,
    pub strain_id: Option<i64>,
    pub payload: serde_json::Value,
    pub sent_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

pub async fn notifications_for_run<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
) -> sqlx::Result<Vec<NotificationRow>> {
    sqlx::query_as!(
        NotificationRow,
        r#"SELECT id, subscriber_id, run_id, rule_id, strain_id, payload, sent_at, error
           FROM notifications WHERE run_id = $1 ORDER BY id"#,
        run_id
    )
    .fetch_all(exec)
    .await
}
