//! `offers`: bulk insert per run and the read/history queries.

use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::PgExecutor;

use crate::domain::{
    HistoryPointDto, OfferRecord, PharmacySeriesDto, PharmacySeriesPointDto, RunStatus,
};

/// One offer to store. `None` prices are sent as NaN and turned into NULL in SQL.
#[derive(Debug, Clone, PartialEq)]
pub struct OfferInsert {
    pub pharmacy_id: i64,
    pub strain_id: i64,
    pub position: i32,
    pub genetics: String,
    pub thc_label: String,
    pub cbd_label: String,
    pub price_label: String,
    pub availability: String,
    pub product_url: String,
    pub price_eur: Option<f64>,
    pub thc_pct: Option<f64>,
    pub cbd_pct: Option<f64>,
    pub price_per_thc_g: Option<f64>,
    pub price_per_cbd_g: Option<f64>,
}

fn nan_for_null(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

/// Bulk insert all offers of a run with a single `UNNEST` statement.
pub async fn insert_many<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    offers: &[OfferInsert],
) -> sqlx::Result<u64> {
    if offers.is_empty() {
        return Ok(0);
    }
    let pharmacy_ids: Vec<i64> = offers.iter().map(|o| o.pharmacy_id).collect();
    let strain_ids: Vec<i64> = offers.iter().map(|o| o.strain_id).collect();
    let positions: Vec<i32> = offers.iter().map(|o| o.position).collect();
    let genetics_values: Vec<&str> = offers.iter().map(|o| o.genetics.as_str()).collect();
    let thc_labels: Vec<&str> = offers.iter().map(|o| o.thc_label.as_str()).collect();
    let cbd_labels: Vec<&str> = offers.iter().map(|o| o.cbd_label.as_str()).collect();
    let price_labels: Vec<&str> = offers.iter().map(|o| o.price_label.as_str()).collect();
    let availabilities: Vec<&str> = offers.iter().map(|o| o.availability.as_str()).collect();
    let product_urls: Vec<&str> = offers.iter().map(|o| o.product_url.as_str()).collect();
    let prices: Vec<f64> = offers.iter().map(|o| nan_for_null(o.price_eur)).collect();
    let thc_pcts: Vec<f64> = offers.iter().map(|o| nan_for_null(o.thc_pct)).collect();
    let cbd_pcts: Vec<f64> = offers.iter().map(|o| nan_for_null(o.cbd_pct)).collect();
    let thc_prices: Vec<f64> = offers
        .iter()
        .map(|o| nan_for_null(o.price_per_thc_g))
        .collect();
    let cbd_prices: Vec<f64> = offers
        .iter()
        .map(|o| nan_for_null(o.price_per_cbd_g))
        .collect();

    let result = sqlx::query!(
        r#"INSERT INTO offers (run_id, pharmacy_id, strain_id, position, genetics, thc_label, cbd_label, price_label,
                               availability, product_url, price_eur, thc_pct, cbd_pct, price_per_thc_g, price_per_cbd_g)
           SELECT $1, u.pharmacy_id, u.strain_id, u.position, u.genetics, u.thc_label, u.cbd_label, u.price_label,
                  u.availability, u.product_url,
                  NULLIF(u.price_eur, 'NaN'::float8), NULLIF(u.thc_pct, 'NaN'::float8), NULLIF(u.cbd_pct, 'NaN'::float8),
                  NULLIF(u.price_per_thc_g, 'NaN'::float8), NULLIF(u.price_per_cbd_g, 'NaN'::float8)
           FROM UNNEST($2::bigint[], $3::bigint[], $4::int[], $5::text[], $6::text[], $7::text[], $8::text[],
                       $9::text[], $10::text[], $11::float8[], $12::float8[], $13::float8[], $14::float8[], $15::float8[])
                AS u(pharmacy_id, strain_id, position, genetics, thc_label, cbd_label, price_label,
                     availability, product_url, price_eur, thc_pct, cbd_pct, price_per_thc_g, price_per_cbd_g)"#,
        run_id,
        &pharmacy_ids,
        &strain_ids,
        &positions,
        &genetics_values as &[&str],
        &thc_labels as &[&str],
        &cbd_labels as &[&str],
        &price_labels as &[&str],
        &availabilities as &[&str],
        &product_urls as &[&str],
        &prices,
        &thc_pcts,
        &cbd_pcts,
        &thc_prices,
        &cbd_prices
    )
    .execute(exec)
    .await?;
    Ok(result.rows_affected())
}

/// All offers of a run in scrape order, joined with pharmacy and strain data.
pub async fn for_run<'e>(exec: impl PgExecutor<'e>, run_id: i64) -> sqlx::Result<Vec<OfferRecord>> {
    let rows = sqlx::query!(
        r#"SELECT o.id AS offer_id, o.pharmacy_id, o.strain_id,
                  p.provider, p.name AS pharmacy, p.postal_code AS pharmacy_postal_code, p.city AS pharmacy_city,
                  s.name, s.designation,
                  o.genetics, o.thc_label, o.cbd_label, o.price_label, o.availability, o.product_url,
                  o.price_eur::float8 AS "price_eur?: f64", o.price_per_thc_g::float8 AS "price_per_thc_g?: f64",
                  o.price_per_cbd_g::float8 AS "price_per_cbd_g?: f64",
                  o.thc_pct::float8 AS "thc_pct?: f64", o.cbd_pct::float8 AS "cbd_pct?: f64"
           FROM offers o
           JOIN pharmacies p ON p.id = o.pharmacy_id
           JOIN strains s ON s.id = o.strain_id
           WHERE o.run_id = $1
           ORDER BY o.position, o.id"#,
        run_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| OfferRecord {
            offer_id: r.offer_id,
            pharmacy_id: r.pharmacy_id,
            provider: match r.provider.as_str() {
                "ansay" => crate::domain::Provider::Ansay,
                _ => crate::domain::Provider::GreenMedical,
            },
            strain_id: r.strain_id,
            pharmacy: r.pharmacy,
            pharmacy_postal_code: r.pharmacy_postal_code,
            pharmacy_city: r.pharmacy_city,
            name: r.name,
            designation: r.designation,
            genetics: r.genetics,
            thc: r.thc_label,
            cbd: r.cbd_label,
            price_per_gram: r.price_label,
            availability: r.availability,
            product_url: r.product_url,
            price_eur_per_gram: r.price_eur,
            price_eur_per_thc_gram: r.price_per_thc_g,
            price_eur_per_cbd_gram: r.price_per_cbd_g,
            thc_value: r.thc_pct,
            cbd_value: r.cbd_pct,
        })
        .collect())
}

/// Minimum price per strain in a run (trend reference).
pub async fn min_prices_for_run<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
) -> sqlx::Result<HashMap<i64, f64>> {
    let rows = sqlx::query!(
        r#"SELECT strain_id, MIN(price_eur)::float8 AS "min_price?: f64"
           FROM offers WHERE run_id = $1 AND price_eur IS NOT NULL GROUP BY strain_id"#,
        run_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| r.min_price.map(|p| (r.strain_id, p)))
        .collect())
}

fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// Price history per run.
pub async fn history_by_run<'e>(
    exec: impl PgExecutor<'e>,
    strain_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    include_partial: bool,
) -> sqlx::Result<Vec<HistoryPointDto>> {
    let rows = sqlx::query!(
        r#"SELECT r.id AS run_id, r.started_at, r.status,
                  MIN(o.price_eur)::float8 AS "min?: f64", AVG(o.price_eur)::float8 AS "avg?: f64",
                  MAX(o.price_eur)::float8 AS "max?: f64",
                  MIN(o.price_per_thc_g)::float8 AS "min_thc?: f64", AVG(o.price_per_thc_g)::float8 AS "avg_thc?: f64",
                  MAX(o.price_per_thc_g)::float8 AS "max_thc?: f64",
                  COUNT(*) AS "offer_count!", COUNT(DISTINCT o.pharmacy_id) AS "pharmacy_count!"
           FROM scrape_runs r JOIN offers o ON o.run_id = r.id
           WHERE o.strain_id = $1 AND r.started_at >= $2 AND r.started_at <= $3
             AND (r.status = 'success' OR ($4::bool AND r.status = 'partial'))
           GROUP BY r.id ORDER BY r.started_at, r.id"#,
        strain_id,
        from,
        to,
        include_partial
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| HistoryPointDto {
            run_id: Some(r.run_id),
            run_count: None,
            at: rfc3339(r.started_at),
            status: RunStatus::parse(&r.status),
            min: r.min.map(crate::domain::round2),
            avg: r.avg.map(crate::domain::round2),
            max: r.max.map(crate::domain::round2),
            min_per_thc_gram: r.min_thc.map(crate::domain::round2),
            avg_per_thc_gram: r.avg_thc.map(crate::domain::round2),
            max_per_thc_gram: r.max_thc.map(crate::domain::round2),
            offer_count: r.offer_count,
            pharmacy_count: r.pharmacy_count,
        })
        .collect())
}

/// Price history per calendar day in `timezone`. `offer_count` is the average
/// number of offers per run of that day, `pharmacy_count` the distinct pharmacies.
pub async fn history_by_day<'e>(
    exec: impl PgExecutor<'e>,
    strain_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    include_partial: bool,
    timezone: &str,
) -> sqlx::Result<Vec<HistoryPointDto>> {
    let rows = sqlx::query!(
        r#"SELECT (r.started_at AT TIME ZONE $5)::date AS "day!",
                  COUNT(DISTINCT r.id) AS "run_count!",
                  MIN(o.price_eur)::float8 AS "min?: f64", AVG(o.price_eur)::float8 AS "avg?: f64",
                  MAX(o.price_eur)::float8 AS "max?: f64",
                  MIN(o.price_per_thc_g)::float8 AS "min_thc?: f64", AVG(o.price_per_thc_g)::float8 AS "avg_thc?: f64",
                  MAX(o.price_per_thc_g)::float8 AS "max_thc?: f64",
                  ROUND(COUNT(*)::numeric / COUNT(DISTINCT r.id))::bigint AS "offer_count!",
                  COUNT(DISTINCT o.pharmacy_id) AS "pharmacy_count!"
           FROM scrape_runs r JOIN offers o ON o.run_id = r.id
           WHERE o.strain_id = $1 AND r.started_at >= $2 AND r.started_at <= $3
             AND (r.status = 'success' OR ($4::bool AND r.status = 'partial'))
           GROUP BY 1 ORDER BY 1"#,
        strain_id,
        from,
        to,
        include_partial,
        timezone
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| HistoryPointDto {
            run_id: None,
            run_count: Some(r.run_count),
            at: r.day.format("%Y-%m-%d").to_string(),
            status: None,
            min: r.min.map(crate::domain::round2),
            avg: r.avg.map(crate::domain::round2),
            max: r.max.map(crate::domain::round2),
            min_per_thc_gram: r.min_thc.map(crate::domain::round2),
            avg_per_thc_gram: r.avg_thc.map(crate::domain::round2),
            max_per_thc_gram: r.max_thc.map(crate::domain::round2),
            offer_count: r.offer_count,
            pharmacy_count: r.pharmacy_count,
        })
        .collect())
}

struct SeriesRow {
    pharmacy_id: i64,
    name: String,
    city: String,
    run_id: Option<i64>,
    at: String,
    price: Option<f64>,
    price_per_thc_gram: Option<f64>,
    availability: String,
}

fn group_series(rows: Vec<SeriesRow>) -> Vec<PharmacySeriesDto> {
    let mut series: Vec<PharmacySeriesDto> = Vec::new();
    for row in rows {
        let point = PharmacySeriesPointDto {
            run_id: row.run_id,
            at: row.at,
            price: row.price.map(crate::domain::round2),
            price_per_thc_gram: row.price_per_thc_gram.map(crate::domain::round2),
            availability: row.availability,
        };
        match series.last_mut() {
            Some(last) if last.pharmacy_id == row.pharmacy_id => last.points.push(point),
            _ => series.push(PharmacySeriesDto {
                pharmacy_id: row.pharmacy_id,
                name: row.name,
                city: row.city,
                points: vec![point],
            }),
        }
    }
    series
}

/// Per-pharmacy price series per run (cheapest offer of the pharmacy in that run).
pub async fn pharmacy_series_by_run<'e>(
    exec: impl PgExecutor<'e>,
    strain_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    include_partial: bool,
) -> sqlx::Result<Vec<PharmacySeriesDto>> {
    let rows = sqlx::query!(
        r#"SELECT o.pharmacy_id, p.name, p.city, r.id AS run_id, r.started_at,
                  MIN(o.price_eur)::float8 AS "price?: f64", MIN(o.price_per_thc_g)::float8 AS "price_thc?: f64",
                  (ARRAY_AGG(o.availability ORDER BY o.price_eur ASC NULLS LAST, o.id))[1] AS "availability!"
           FROM offers o JOIN scrape_runs r ON r.id = o.run_id JOIN pharmacies p ON p.id = o.pharmacy_id
           WHERE o.strain_id = $1 AND r.started_at >= $2 AND r.started_at <= $3
             AND (r.status = 'success' OR ($4::bool AND r.status = 'partial'))
           GROUP BY o.pharmacy_id, p.name, p.city, r.id, r.started_at
           ORDER BY p.name, o.pharmacy_id, r.started_at, r.id"#,
        strain_id,
        from,
        to,
        include_partial
    )
    .fetch_all(exec)
    .await?;
    Ok(group_series(
        rows.into_iter()
            .map(|r| SeriesRow {
                pharmacy_id: r.pharmacy_id,
                name: r.name,
                city: r.city,
                run_id: Some(r.run_id),
                at: rfc3339(r.started_at),
                price: r.price,
                price_per_thc_gram: r.price_thc,
                availability: r.availability,
            })
            .collect(),
    ))
}

/// Per-pharmacy price series per day (cheapest offer of the pharmacy that day).
pub async fn pharmacy_series_by_day<'e>(
    exec: impl PgExecutor<'e>,
    strain_id: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    include_partial: bool,
    timezone: &str,
) -> sqlx::Result<Vec<PharmacySeriesDto>> {
    let rows = sqlx::query!(
        r#"SELECT o.pharmacy_id, p.name, p.city, (r.started_at AT TIME ZONE $5)::date AS "day!",
                  MIN(o.price_eur)::float8 AS "price?: f64", MIN(o.price_per_thc_g)::float8 AS "price_thc?: f64",
                  (ARRAY_AGG(o.availability ORDER BY r.started_at DESC, o.price_eur ASC NULLS LAST, o.id))[1] AS "availability!"
           FROM offers o JOIN scrape_runs r ON r.id = o.run_id JOIN pharmacies p ON p.id = o.pharmacy_id
           WHERE o.strain_id = $1 AND r.started_at >= $2 AND r.started_at <= $3
             AND (r.status = 'success' OR ($4::bool AND r.status = 'partial'))
           GROUP BY o.pharmacy_id, p.name, p.city, 4
           ORDER BY p.name, o.pharmacy_id, 4"#,
        strain_id,
        from,
        to,
        include_partial,
        timezone
    )
    .fetch_all(exec)
    .await?;
    Ok(group_series(
        rows.into_iter()
            .map(|r| SeriesRow {
                pharmacy_id: r.pharmacy_id,
                name: r.name,
                city: r.city,
                run_id: None,
                at: r.day.format("%Y-%m-%d").to_string(),
                price: r.price,
                price_per_thc_gram: r.price_thc,
                availability: r.availability,
            })
            .collect(),
    ))
}
