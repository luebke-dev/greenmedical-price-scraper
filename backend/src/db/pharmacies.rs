//! `pharmacies`: stable identities keyed by the site's UUID.

use std::collections::HashMap;

use sqlx::PgExecutor;

use crate::domain::{PharmacyDto, Provider};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PharmacyInput {
    pub external_id: String,
    pub provider: Provider,
    pub name: String,
    pub plz: String,
    pub city: String,
    pub address: String,
    pub url: String,
}

/// Upsert pharmacies (deduplicated by `external_id`, first occurrence wins)
/// and return `external_id → id`.
pub async fn upsert_many<'e>(
    exec: impl PgExecutor<'e>,
    inputs: &[PharmacyInput],
) -> sqlx::Result<HashMap<String, i64>> {
    let mut seen = HashMap::new();
    let mut unique: Vec<&PharmacyInput> = Vec::new();
    for input in inputs {
        if seen.insert(input.external_id.as_str(), ()).is_none() {
            unique.push(input);
        }
    }
    if unique.is_empty() {
        return Ok(HashMap::new());
    }
    let external_ids: Vec<&str> = unique.iter().map(|p| p.external_id.as_str()).collect();
    let providers: Vec<&str> = unique.iter().map(|p| p.provider.as_str()).collect();
    let names: Vec<&str> = unique.iter().map(|p| p.name.as_str()).collect();
    let plzs: Vec<&str> = unique.iter().map(|p| p.plz.as_str()).collect();
    let cities: Vec<&str> = unique.iter().map(|p| p.city.as_str()).collect();
    let addresses: Vec<&str> = unique.iter().map(|p| p.address.as_str()).collect();
    let urls: Vec<&str> = unique.iter().map(|p| p.url.as_str()).collect();

    let rows = sqlx::query!(
        r#"INSERT INTO pharmacies (external_id, provider, name, plz, city, address, url, first_seen_at, last_seen_at)
           SELECT u.external_id, u.provider, u.name, u.plz, u.city, u.address, u.url, now(), now()
           FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[], $7::text[])
                AS u(external_id, provider, name, plz, city, address, url)
           ON CONFLICT (external_id) DO UPDATE SET
               provider = EXCLUDED.provider, name = EXCLUDED.name, plz = EXCLUDED.plz, city = EXCLUDED.city,
               address = EXCLUDED.address, url = EXCLUDED.url, last_seen_at = EXCLUDED.last_seen_at
           RETURNING id, external_id"#,
        &external_ids as &[&str],
        &providers as &[&str],
        &names as &[&str],
        &plzs as &[&str],
        &cities as &[&str],
        &addresses as &[&str],
        &urls as &[&str]
    )
    .fetch_all(exec)
    .await?;
    Ok(rows.into_iter().map(|r| (r.external_id, r.id)).collect())
}

/// All pharmacies with the number of offers in `latest_run_id` (0 without a run).
pub async fn list<'e>(
    exec: impl PgExecutor<'e>,
    latest_run_id: Option<i64>,
) -> sqlx::Result<Vec<PharmacyDto>> {
    let rows = sqlx::query!(
        r#"SELECT p.id, p.external_id, p.provider, p.name, p.plz, p.city, p.address, p.url, p.first_seen_at, p.last_seen_at,
                  (SELECT COUNT(*) FROM offers o WHERE o.pharmacy_id = p.id AND o.run_id = $1) AS "offer_count_latest!"
           FROM pharmacies p ORDER BY p.name, p.id"#,
        latest_run_id
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PharmacyDto {
            id: r.id,
            external_id: r.external_id,
            provider: match r.provider.as_str() {
                "ansay" => Provider::Ansay,
                _ => Provider::Greenmedical,
            },
            name: r.name,
            plz: r.plz,
            city: r.city,
            address: r.address,
            url: r.url,
            first_seen_at: r.first_seen_at,
            last_seen_at: r.last_seen_at,
            offer_count_latest: r.offer_count_latest,
        })
        .collect())
}
