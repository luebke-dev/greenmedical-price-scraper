//! `reviews`, `strain_rating_snapshots` and the rating columns on `strains`
//! (phase 2 of a scrape run, see `docs/api-contract.md`).

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgExecutor, PgPool};

use crate::domain::{
    RatingDistributionDto, RatingHistoryPointDto, ReviewDto, ReviewSummaryDto, ReviewsResponseDto,
};
use crate::scrape::reviews::ProductReviews;

/// Newest snapshots returned in `ReviewsResponse.history` (ascending order).
pub const MAX_HISTORY_POINTS: i64 = 400;

/// A strain whose product page should be fetched in phase 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewTarget {
    pub strain_id: i64,
    pub name: String,
    /// First non-empty product URL of the strain in the run (scrape order).
    pub product_url: String,
}

/// Strains of `run_id` whose reviews are missing or older than `older_than`
/// (`None` = every strain of the run), oldest scrape first; `limit` 0 = unlimited.
pub async fn targets_for_run<'e>(
    exec: impl PgExecutor<'e>,
    run_id: i64,
    older_than: Option<DateTime<Utc>>,
    limit: u32,
) -> sqlx::Result<Vec<ReviewTarget>> {
    let limit = if limit == 0 {
        i64::MAX
    } else {
        i64::from(limit)
    };
    let rows = sqlx::query!(
        r#"SELECT s.id AS strain_id, s.name,
                  (array_agg(o.product_url ORDER BY o.position, o.id))[1] AS "product_url!"
           FROM offers o
           JOIN strains s ON s.id = o.strain_id
           WHERE o.run_id = $1 AND o.product_url <> ''
             AND ($2::timestamptz IS NULL OR s.reviews_scraped_at IS NULL OR s.reviews_scraped_at < $2)
           GROUP BY s.id
           ORDER BY s.reviews_scraped_at ASC NULLS FIRST, s.id
           LIMIT $3"#,
        run_id,
        older_than,
        limit
    )
    .fetch_all(exec)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ReviewTarget {
            strain_id: r.strain_id,
            name: r.name,
            product_url: r.product_url,
        })
        .collect())
}

/// Persist one parsed product page in a single transaction: strain columns,
/// a rating snapshot and the upserted reviews.
pub async fn persist(
    pool: &PgPool,
    strain_id: i64,
    run_id: Option<i64>,
    parsed: &ProductReviews,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    let rating = parsed.rating_value.unwrap_or(f64::NAN);
    let count = i32::try_from(parsed.review_count).unwrap_or(i32::MAX);
    sqlx::query!(
        r#"UPDATE strains
           SET product_uuid = COALESCE($2, product_uuid),
               rating_value = NULLIF($3::float8, 'NaN'::float8),
               review_count = $4,
               reviews_scraped_at = now()
           WHERE id = $1"#,
        strain_id,
        parsed.product_uuid.as_deref(),
        rating,
        count
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"INSERT INTO strain_rating_snapshots (strain_id, run_id, scraped_at, rating_value, review_count)
           VALUES ($1, $2, now(), NULLIF($3::float8, 'NaN'::float8), $4)"#,
        strain_id,
        run_id,
        rating,
        count
    )
    .execute(&mut *tx)
    .await?;

    if !parsed.reviews.is_empty() {
        let fingerprints: Vec<String> = parsed.reviews.iter().map(|r| r.fingerprint()).collect();
        let authors: Vec<&str> = parsed.reviews.iter().map(|r| r.author.as_str()).collect();
        let dates: Vec<Option<NaiveDate>> = parsed.reviews.iter().map(|r| r.reviewed_on).collect();
        let ratings: Vec<f64> = parsed.reviews.iter().map(|r| r.rating).collect();
        let verified: Vec<bool> = parsed.reviews.iter().map(|r| r.verified).collect();
        let contents: Vec<&str> = parsed.reviews.iter().map(|r| r.content.as_str()).collect();
        sqlx::query!(
            r#"INSERT INTO reviews (strain_id, fingerprint, author, reviewed_on, rating, verified, content, first_seen_at, last_seen_at)
               SELECT $1, d.fingerprint, d.author, d.reviewed_on, d.rating, d.verified, d.content, now(), now()
               FROM (SELECT DISTINCT ON (u.fingerprint) u.*
                     FROM UNNEST($2::text[], $3::text[], $4::date[], $5::float8[], $6::bool[], $7::text[])
                          WITH ORDINALITY AS u(fingerprint, author, reviewed_on, rating, verified, content, position)
                     ORDER BY u.fingerprint, u.position) d
               ORDER BY d.position
               ON CONFLICT (strain_id, fingerprint) DO UPDATE SET last_seen_at = now(), verified = EXCLUDED.verified"#,
            strain_id,
            &fingerprints,
            &authors as &[&str],
            &dates as &[Option<NaiveDate>],
            &ratings,
            &verified,
            &contents as &[&str]
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Sort orders of `GET /strains/{id}/reviews`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReviewSort {
    #[default]
    Newest,
    Oldest,
    Highest,
    Lowest,
}

impl ReviewSort {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewSort::Newest => "newest",
            ReviewSort::Oldest => "oldest",
            ReviewSort::Highest => "highest",
            ReviewSort::Lowest => "lowest",
        }
    }
}

/// Build the whole `ReviewsResponse` for a strain (caller checks existence).
pub async fn response(
    pool: &PgPool,
    strain_id: i64,
    sort: ReviewSort,
    limit: i64,
    offset: i64,
) -> sqlx::Result<ReviewsResponseDto> {
    let strain = sqlx::query!(
        r#"SELECT rating_value::float8 AS "rating_value?: f64", review_count, reviews_scraped_at
           FROM strains WHERE id = $1"#,
        strain_id
    )
    .fetch_optional(pool)
    .await?;
    let (value, count, scraped_at) = match strain {
        Some(s) => (
            s.rating_value,
            s.review_count.unwrap_or(0),
            s.reviews_scraped_at,
        ),
        None => (None, 0, None),
    };

    let stats = sqlx::query!(
        r#"SELECT COUNT(*) AS "stored!", COUNT(*) FILTER (WHERE verified) AS "verified!",
                  COUNT(*) FILTER (WHERE round(rating) <= 1) AS "one!",
                  COUNT(*) FILTER (WHERE round(rating) = 2) AS "two!",
                  COUNT(*) FILTER (WHERE round(rating) = 3) AS "three!",
                  COUNT(*) FILTER (WHERE round(rating) = 4) AS "four!",
                  COUNT(*) FILTER (WHERE round(rating) >= 5) AS "five!"
           FROM reviews WHERE strain_id = $1"#,
        strain_id
    )
    .fetch_one(pool)
    .await?;

    let history = sqlx::query!(
        r#"SELECT scraped_at, rating_value::float8 AS "rating_value?: f64", review_count
           FROM (SELECT scraped_at, rating_value, review_count, id FROM strain_rating_snapshots
                 WHERE strain_id = $1 ORDER BY scraped_at DESC, id DESC LIMIT $2) newest
           ORDER BY scraped_at ASC, id ASC"#,
        strain_id,
        MAX_HISTORY_POINTS
    )
    .fetch_all(pool)
    .await?;

    let reviews = sqlx::query!(
        r#"SELECT id, author, reviewed_on, rating::float8 AS "rating!: f64", verified, content, first_seen_at
           FROM reviews WHERE strain_id = $1
           ORDER BY CASE WHEN $2 = 'highest' THEN rating END DESC,
                    CASE WHEN $2 = 'lowest' THEN rating END ASC,
                    CASE WHEN $2 = 'oldest' THEN reviewed_on END ASC NULLS LAST,
                    CASE WHEN $2 <> 'oldest' THEN reviewed_on END DESC NULLS LAST,
                    CASE WHEN $2 = 'oldest' THEN id END ASC,
                    id DESC
           LIMIT $3 OFFSET $4"#,
        strain_id,
        sort.as_str(),
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    Ok(ReviewsResponseDto {
        strain_id,
        summary: ReviewSummaryDto {
            value,
            count,
            scraped_at,
            distribution: RatingDistributionDto {
                one: stats.one,
                two: stats.two,
                three: stats.three,
                four: stats.four,
                five: stats.five,
            },
            verified_count: stats.verified,
            stored_count: stats.stored,
        },
        history: history
            .into_iter()
            .map(|h| RatingHistoryPointDto {
                at: h.scraped_at,
                value: h.rating_value,
                count: h.review_count,
            })
            .collect(),
        reviews: reviews
            .into_iter()
            .map(|r| ReviewDto {
                id: r.id,
                author: r.author,
                reviewed_on: r.reviewed_on,
                rating: r.rating,
                verified: r.verified,
                content: r.content,
                first_seen_at: r.first_seen_at,
            })
            .collect(),
        total: stats.stored,
    })
}
