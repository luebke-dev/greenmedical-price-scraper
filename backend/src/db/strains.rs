//! `strains`: stable identities keyed by `(name_key, designation_key)`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgExecutor;

use crate::domain::RatingDto;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrainInput {
    pub name_key: String,
    pub designation_key: String,
    pub name: String,
    pub designation: String,
    pub genetics: String,
    pub thc_label: String,
    pub cbd_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrainRow {
    pub id: i64,
    pub name: String,
    pub designation: String,
    pub genetics: String,
    pub thc_label: String,
    pub cbd_label: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub product_uuid: Option<String>,
    /// `None` until the product page was scraped for reviews.
    pub rating: Option<RatingDto>,
}

/// Rating columns of one strain, as stored by the last review scrape.
#[derive(Debug, Clone, PartialEq)]
pub struct StrainRating {
    pub product_uuid: Option<String>,
    pub rating: Option<RatingDto>,
}

fn rating_dto(
    value: Option<f64>,
    count: Option<i32>,
    scraped_at: Option<DateTime<Utc>>,
) -> Option<RatingDto> {
    scraped_at.map(|scraped_at| RatingDto {
        value,
        count: count.unwrap_or(0),
        scraped_at,
    })
}

/// Rating data of all strains that have been scraped for reviews at least once.
/// Newest `reviews_scraped_at` over all strains: the "version" of the rating
/// data, used by the snapshot cache to notice review updates made by another
/// process or replica (they do not create a new run).
pub async fn reviews_version<'e>(
    exec: impl PgExecutor<'e>,
) -> sqlx::Result<Option<chrono::DateTime<chrono::Utc>>> {
    sqlx::query_scalar!(r#"SELECT max(reviews_scraped_at) AS "version?" FROM strains"#)
        .fetch_one(exec)
        .await
}

pub async fn ratings<'e>(exec: impl PgExecutor<'e>) -> sqlx::Result<HashMap<i64, StrainRating>> {
    let rows = sqlx::query!(
        r#"SELECT id, product_uuid, rating_value::float8 AS "rating_value?: f64", review_count, reviews_scraped_at
           FROM strains WHERE reviews_scraped_at IS NOT NULL"#
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                StrainRating {
                    product_uuid: r.product_uuid,
                    rating: rating_dto(r.rating_value, r.review_count, r.reviews_scraped_at),
                },
            )
        })
        .collect())
}

/// Upsert strains (deduplicated by key, first occurrence wins) and return
/// `(name_key, designation_key) → id`.
pub async fn upsert_many<'e>(
    exec: impl PgExecutor<'e>,
    inputs: &[StrainInput],
) -> sqlx::Result<HashMap<(String, String), i64>> {
    let mut seen = HashMap::new();
    let mut unique: Vec<&StrainInput> = Vec::new();
    for input in inputs {
        let key = (input.name_key.as_str(), input.designation_key.as_str());
        if seen.insert(key, ()).is_none() {
            unique.push(input);
        }
    }
    if unique.is_empty() {
        return Ok(HashMap::new());
    }
    let name_keys: Vec<&str> = unique.iter().map(|s| s.name_key.as_str()).collect();
    let bez_keys: Vec<&str> = unique.iter().map(|s| s.designation_key.as_str()).collect();
    let names: Vec<&str> = unique.iter().map(|s| s.name.as_str()).collect();
    let bezs: Vec<&str> = unique.iter().map(|s| s.designation.as_str()).collect();
    let genetics_values: Vec<&str> = unique.iter().map(|s| s.genetics.as_str()).collect();
    let thcs: Vec<&str> = unique.iter().map(|s| s.thc_label.as_str()).collect();
    let cbds: Vec<&str> = unique.iter().map(|s| s.cbd_label.as_str()).collect();

    let rows = sqlx::query!(
        r#"INSERT INTO strains (name_key, designation_key, name, designation, genetics, thc_label, cbd_label, first_seen_at, last_seen_at)
           SELECT u.name_key, u.designation_key, u.name, u.designation, u.genetics, u.thc_label, u.cbd_label, now(), now()
           FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[])
                AS u(name_key, designation_key, name, designation, genetics, thc_label, cbd_label)
           ON CONFLICT (name_key, designation_key) DO UPDATE SET
               name = EXCLUDED.name, designation = EXCLUDED.designation, genetics = EXCLUDED.genetics,
               thc_label = EXCLUDED.thc_label, cbd_label = EXCLUDED.cbd_label, last_seen_at = EXCLUDED.last_seen_at
           RETURNING id, name_key, designation_key"#,
        &name_keys as &[&str],
        &bez_keys as &[&str],
        &names as &[&str],
        &bezs as &[&str],
        &genetics_values as &[&str],
        &thcs as &[&str],
        &cbds as &[&str]
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ((r.name_key, r.designation_key), r.id))
        .collect())
}

pub async fn get<'e>(exec: impl PgExecutor<'e>, id: i64) -> sqlx::Result<Option<StrainRow>> {
    let row = sqlx::query!(
        r#"SELECT id, name, designation, genetics, thc_label, cbd_label, first_seen_at, last_seen_at,
                  product_uuid, rating_value::float8 AS "rating_value?: f64", review_count, reviews_scraped_at
           FROM strains WHERE id = $1"#,
        id
    )
    .fetch_optional(exec)
    .await?;
    Ok(row.map(|r| StrainRow {
        id: r.id,
        name: r.name,
        designation: r.designation,
        genetics: r.genetics,
        thc_label: r.thc_label,
        cbd_label: r.cbd_label,
        first_seen_at: r.first_seen_at,
        last_seen_at: r.last_seen_at,
        product_uuid: r.product_uuid,
        rating: rating_dto(r.rating_value, r.review_count, r.reviews_scraped_at),
    }))
}
