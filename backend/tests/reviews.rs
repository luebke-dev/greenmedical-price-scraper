//! Phase 2 (reviews) end-to-end against wiremock, plus the reviews API.

mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Duration, NaiveDate, Utc};
use greenmedical_backend::api::build_router;
use greenmedical_backend::db::reviews;
use greenmedical_backend::domain::{RunStatus, RunTrigger};
use greenmedical_backend::scrape::reviews::{ParsedReview, ProductReviews};
use greenmedical_backend::scrape::run::{scrape_now, scrape_reviews_only};
use greenmedical_backend::telemetry;
use serde_json::Value;
use sqlx::PgPool;
use support::{
    MockReview, MockSite, SeedOffer, default_site, product_page_html, seed_run, test_config,
    test_state, test_state_with,
};
use tower::ServiceExt;
use wiremock::ResponseTemplate;

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

const UUID_A: &str = "c822b844-1925-11ef-b5f0-0242ac170003";

fn five_reviews() -> Vec<MockReview> {
    vec![
        MockReview::new("Carlos S.", "25.08.2026", 4, "Bom material"),
        MockReview::new("Andre G.", "22.08.2026", 5, "Sehr gut").unverified(),
        MockReview::new("Ivan Z.", "16.08.2026", 5, ""),
        MockReview::new("Kim L.", "01.08.2026", 2, "Naja").half(),
        MockReview::new("Ohne Datum", "", 1, "Schlecht"),
    ]
}

/// Mount review pages for the three strains of `default_site`.
async fn mount_default_products(site: &MockSite) {
    let tiles: Vec<_> = site.pharmacies[0].pages.iter().flatten().cloned().collect();
    // Bunatic: 124 reviews per JSON-LD, 5 on the page.
    site.mount_product(
        &tiles[0],
        ResponseTemplate::new(200).set_body_string(product_page_html(
            UUID_A,
            Some(4.3),
            124,
            &five_reviews(),
        )),
    )
    .await;
    // OG Kush: no reviews at all.
    site.mount_product(
        &tiles[1],
        ResponseTemplate::new(200).set_body_string(product_page_html(
            "f1de4982-e28e-4af4-b3c1-1e4107421385",
            None,
            0,
            &[],
        )),
    )
    .await;
    // Cosmic Cream: 3 reviews (below the best_rated threshold).
    site.mount_product(
        &tiles[2],
        ResponseTemplate::new(200).set_body_string(product_page_html(
            "11111111-1111-1111-1111-111111111111",
            Some(5.0),
            3,
            &[MockReview::new("A", "01.01.2026", 5, "top")],
        )),
    )
    .await;
}

#[sqlx::test(migrations = "./migrations")]
async fn phase_two_stores_ratings_snapshots_and_reviews(pool: PgPool) {
    let handle = telemetry::metrics_handle();
    let site = MockSite::start(default_site()).await;
    mount_default_products(&site).await;
    let state = test_state(pool.clone(), &site.base_url());

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.reviews_scraped, Some(3));
    assert_eq!(run.reviews_failed, Some(0));
    assert!(run.finished_at.is_some());

    // Product pages are fetched without query/fragment.
    let product_requests = site.product_requests().await;
    assert_eq!(product_requests.len(), 3);
    assert!(product_requests.iter().all(|r| r.url.query().is_none()));

    let app = build_router(state.clone());
    let (status, body) = get_json(&app, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    let strains = body["strains"].as_array().unwrap();
    let bunatic = strains.iter().find(|s| s["name"] == "Bunatic").unwrap();
    assert_eq!(bunatic["rating"]["value"], 4.3);
    assert_eq!(bunatic["rating"]["count"], 124);
    assert!(bunatic["rating"]["scraped_at"].is_string());
    assert_eq!(bunatic["sort"]["rating"], 4.3);
    assert_eq!(bunatic["product_uuid"], UUID_A);
    let og = strains.iter().find(|s| s["name"] == "OG Kush").unwrap();
    assert_eq!(og["rating"]["value"], Value::Null);
    assert_eq!(og["rating"]["count"], 0);
    assert_eq!(og["sort"]["rating"], Value::Null);
    assert_eq!(body["run"]["reviews_scraped"], 3);

    let (_, metadata) = get_json(&app, "/api/v1/metadata").await;
    // Cosmic Cream is rated 5.0 but has only 3 reviews: Bunatic wins.
    assert_eq!(metadata["best_rated"]["name"], "Bunatic");
    assert_eq!(metadata["best_rated"]["rating_value"], 4.3);
    assert_eq!(metadata["best_rated"]["review_count"], 124);
    assert_eq!(metadata["best_rated"]["price"], 5.49);
    assert_eq!(metadata["best_rated"]["apotheke"], "Grüne Blüte");
    assert!(metadata["cheapest_gram"].get("rating_value").is_none());

    // Strain detail carries the rating too.
    let id = bunatic["id"].as_i64().unwrap();
    let (_, detail) = get_json(&app, &format!("/api/v1/strains/{id}")).await;
    assert_eq!(detail["rating"]["count"], 124);
    assert_eq!(detail["product_uuid"], UUID_A);

    // Snapshot row references the run; five reviews stored with fingerprints.
    let snapshot: Vec<(Option<i64>, Option<f64>, i32)> = sqlx::query_as(
        "SELECT run_id, rating_value::float8, review_count FROM strain_rating_snapshots WHERE strain_id = $1",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(snapshot, vec![(Some(run.id), Some(4.3), 124)]);
    let (_, reviews) = get_json(&app, &format!("/api/v1/strains/{id}/reviews")).await;
    assert_eq!(reviews["summary"]["count"], 124);
    assert_eq!(reviews["summary"]["stored_count"], 5);
    assert_eq!(reviews["summary"]["verified_count"], 4);
    assert_eq!(reviews["history"].as_array().unwrap().len(), 1);
    let half = reviews["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["author"] == "Kim L.")
        .unwrap();
    assert_eq!(half["rating"], 2.5);
    let undated = reviews["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["author"] == "Ohne Datum")
        .unwrap();
    assert_eq!(undated["reviewed_on"], Value::Null);

    let text = handle.render();
    assert!(
        text.contains("scrape_reviews_total{result=\"scraped\"}"),
        "{text}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn fresh_ratings_are_skipped_and_reviews_only_refreshes_everything(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    mount_default_products(&site).await;
    let state = test_state(pool.clone(), &site.base_url());

    let first = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(first.reviews_scraped, Some(3));
    assert_eq!(site.product_requests().await.len(), 3);

    // Second run within REVIEWS_MAX_AGE: nothing to refresh.
    let second = scrape_now(&state, RunTrigger::Schedule).await.unwrap();
    assert_eq!(second.status, RunStatus::Success);
    assert_eq!(second.reviews_scraped, Some(0));
    assert_eq!(second.reviews_failed, Some(0));
    assert_eq!(site.product_requests().await.len(), 3);

    // --reviews-only ignores the max age and refreshes every strain of the latest run.
    let outcome = scrape_reviews_only(&state).await.unwrap();
    assert_eq!(outcome.run_id, second.id);
    assert_eq!((outcome.scraped, outcome.failed), (3, 0));
    assert_eq!(site.product_requests().await.len(), 6);

    // Reviews are deduplicated by fingerprint; snapshots accumulate.
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM reviews),
                (SELECT COUNT(*) FROM strain_rating_snapshots),
                (SELECT COUNT(*) FROM reviews WHERE last_seen_at > first_seen_at)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (6, 6, 6), "(reviews, snapshots, refreshed)");
    let run = greenmedical_backend::db::runs::get(&pool, second.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.reviews_scraped, Some(3));

    // A strain older than the max age is refreshed by the next run.
    sqlx::query(
        "UPDATE strains SET reviews_scraped_at = now() - interval '2 days' WHERE name = 'Bunatic'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let third = scrape_now(&state, RunTrigger::Schedule).await.unwrap();
    assert_eq!(third.reviews_scraped, Some(1));
    assert_eq!(site.product_requests().await.len(), 7);
}

#[sqlx::test(migrations = "./migrations")]
async fn review_failures_are_counted_but_never_fail_the_run(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let tiles: Vec<_> = site.pharmacies[0].pages.iter().flatten().cloned().collect();
    site.mount_product(
        &tiles[0],
        ResponseTemplate::new(200).set_body_string(product_page_html(
            UUID_A,
            Some(4.0),
            6,
            &five_reviews(),
        )),
    )
    .await;
    site.mount_product(&tiles[1], ResponseTemplate::new(500))
        .await;
    // tiles[2] has no mock at all → 404.
    let state = test_state(pool.clone(), &site.base_url());

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.reviews_scraped, Some(1));
    assert_eq!(run.reviews_failed, Some(2));
    let rated: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM strains WHERE reviews_scraped_at IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rated, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn phase_two_can_be_disabled(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    mount_default_products(&site).await;
    let mut config = test_config(&site.base_url());
    config.reviews_enabled = false;
    let state = test_state_with(pool.clone(), config);

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.reviews_scraped, None);
    assert_eq!(run.reviews_failed, None);
    assert!(site.product_requests().await.is_empty());
    let app = build_router(state);
    let (_, body) = get_json(&app, "/api/v1/strains").await;
    assert_eq!(body["strains"][0]["rating"], Value::Null);
    let (_, metadata) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(metadata["best_rated"], Value::Null);
}

#[sqlx::test(migrations = "./migrations")]
async fn max_per_run_limits_phase_two_oldest_first(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    mount_default_products(&site).await;
    let mut config = test_config(&site.base_url());
    config.reviews_max_per_run = 2;
    let state = test_state_with(pool.clone(), config);

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.reviews_scraped, Some(2));
    // The remaining (never scraped) strain comes first in the next run.
    sqlx::query("UPDATE strains SET reviews_scraped_at = now() - interval '2 days' WHERE reviews_scraped_at IS NOT NULL")
        .execute(&pool)
        .await
        .unwrap();
    let outcome = scrape_reviews_only(&state).await.unwrap();
    assert_eq!(outcome.scraped, 2);
    let never: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM strains WHERE reviews_scraped_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(never, 0);
}

// ---------------------------------------------------------------------------
// Reviews API on seeded data
// ---------------------------------------------------------------------------

fn review(author: &str, date: Option<(i32, u32, u32)>, rating: f64, content: &str) -> ParsedReview {
    ParsedReview {
        author: author.into(),
        reviewed_on: date.map(|(y, m, d)| NaiveDate::from_ymd_opt(y, m, d).unwrap()),
        rating,
        verified: rating >= 3.0,
        content: content.into(),
    }
}

async fn seed_reviews(pool: &PgPool) -> (i64, i64) {
    let run_id = seed_run(
        pool,
        Utc::now() - Duration::hours(1),
        RunStatus::Success,
        &[
            SeedOffer::new(("aaaa-1", "Apo A"), ("Sorte X", "EMK"), 9.5),
            SeedOffer::new(("bbbb-2", "Apo B"), ("Sorte X", "EMK"), 8.0),
            SeedOffer::new(("aaaa-1", "Apo A"), ("Sorte Y", "XYZ"), 7.0),
        ],
    )
    .await;
    let strain_x: i64 = sqlx::query_scalar("SELECT id FROM strains WHERE name = 'Sorte X'")
        .fetch_one(pool)
        .await
        .unwrap();
    let parsed = ProductReviews {
        product_uuid: Some(UUID_A.into()),
        rating_value: Some(3.8),
        review_count: 6,
        reviews: vec![
            review("A", Some((2026, 8, 1)), 5.0, "fünf"),
            review("B", Some((2026, 8, 3)), 4.5, "viereinhalb"),
            review("C", Some((2026, 8, 2)), 2.5, "zweieinhalb"),
            review("D", None, 1.0, "eins"),
            review("E", Some((2026, 7, 1)), 3.0, "drei"),
            review("F", Some((2026, 8, 3)), 4.0, "vier"),
        ],
    };
    reviews::persist(pool, strain_x, Some(run_id), &parsed)
        .await
        .unwrap();
    (run_id, strain_x)
}

#[sqlx::test(migrations = "./migrations")]
async fn reviews_endpoint_summary_sorting_and_pagination(pool: PgPool) {
    let (_, strain_x) = seed_reviews(&pool).await;
    let app = build_router(test_state(pool.clone(), "http://127.0.0.1:1"));

    let (status, body) = get_json(&app, &format!("/api/v1/strains/{strain_x}/reviews")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["strain_id"], strain_x);
    assert_eq!(body["summary"]["value"], 3.8);
    assert_eq!(body["summary"]["count"], 6);
    assert!(body["summary"]["scraped_at"].is_string());
    assert_eq!(body["summary"]["stored_count"], 6);
    assert_eq!(body["summary"]["verified_count"], 4);
    // 5.0→5, 4.5→5 (half up), 2.5→3, 1.0→1, 3.0→3, 4.0→4
    assert_eq!(
        body["summary"]["distribution"],
        serde_json::json!({ "1": 1, "2": 0, "3": 2, "4": 1, "5": 2 })
    );
    assert_eq!(body["total"], 6);
    assert_eq!(body["history"].as_array().unwrap().len(), 1);
    assert_eq!(body["history"][0]["value"], 3.8);
    assert_eq!(body["history"][0]["count"], 6);

    let authors = |body: &Value| -> Vec<String> {
        body["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["author"].as_str().unwrap().to_owned())
            .collect()
    };
    // newest: date desc, undated last, ties by newest id first.
    assert_eq!(authors(&body), ["F", "B", "C", "A", "E", "D"]);
    let (_, oldest) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?sort=oldest"),
    )
    .await;
    assert_eq!(authors(&oldest), ["E", "A", "C", "B", "F", "D"]);
    let (_, highest) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?sort=highest"),
    )
    .await;
    assert_eq!(authors(&highest), ["A", "B", "F", "E", "C", "D"]);
    let (_, lowest) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?sort=lowest"),
    )
    .await;
    assert_eq!(authors(&lowest), ["D", "C", "E", "F", "B", "A"]);

    // Pagination.
    let (_, page) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?limit=2&offset=2"),
    )
    .await;
    assert_eq!(authors(&page), ["C", "A"]);
    assert_eq!(page["total"], 6);
    let (_, beyond) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?limit=2&offset=10"),
    )
    .await;
    assert!(beyond["reviews"].as_array().unwrap().is_empty());

    // Review shape.
    let first = &body["reviews"][0];
    assert_eq!(first["reviewed_on"], "2026-08-03");
    assert_eq!(first["rating"], 4.0);
    assert_eq!(first["verified"], true);
    assert_eq!(first["content"], "vier");
    assert!(first["id"].is_number() && first["first_seen_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn reviews_endpoint_validation_and_empty_strain(pool: PgPool) {
    let (_, strain_x) = seed_reviews(&pool).await;
    let app = build_router(test_state(pool.clone(), "http://127.0.0.1:1"));

    let (status, body) = get_json(&app, "/api/v1/strains/999999/reviews").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    for uri in [
        format!("/api/v1/strains/{strain_x}/reviews?limit=501"),
        format!("/api/v1/strains/{strain_x}/reviews?limit=0"),
        format!("/api/v1/strains/{strain_x}/reviews?offset=-1"),
        format!("/api/v1/strains/{strain_x}/reviews?sort=best"),
    ] {
        let (status, body) = get_json(&app, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "bad_request", "{uri}");
    }
    let (status, _) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_x}/reviews?limit=500"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Never scraped strain: empty lists, null summary values.
    let strain_y: i64 = sqlx::query_scalar("SELECT id FROM strains WHERE name = 'Sorte Y'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, body) = get_json(&app, &format!("/api/v1/strains/{strain_y}/reviews")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({
            "strain_id": strain_y,
            "summary": { "value": null, "count": 0, "scraped_at": null,
                         "distribution": { "1": 0, "2": 0, "3": 0, "4": 0, "5": 0 },
                         "verified_count": 0, "stored_count": 0 },
            "history": [],
            "reviews": [],
            "total": 0
        })
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn strains_and_metadata_expose_ratings_with_best_rated_threshold(pool: PgPool) {
    let (run_id, strain_x) = seed_reviews(&pool).await;
    let strain_y: i64 = sqlx::query_scalar("SELECT id FROM strains WHERE name = 'Sorte Y'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Sorte Y: better rating but only 4 reviews.
    let mut parsed = ProductReviews {
        product_uuid: None,
        rating_value: Some(4.9),
        review_count: 4,
        reviews: vec![],
    };
    reviews::persist(&pool, strain_y, Some(run_id), &parsed)
        .await
        .unwrap();
    let state = test_state(pool.clone(), "http://127.0.0.1:1");
    let app = build_router(state.clone());

    let (_, body) = get_json(&app, "/api/v1/strains?sort=name").await;
    let x = &body["strains"][0];
    assert_eq!(x["name"], "Sorte X");
    assert_eq!(x["rating"]["value"], 3.8);
    assert_eq!(x["rating"]["count"], 6);
    assert_eq!(x["sort"]["rating"], 3.8);
    assert_eq!(x["product_uuid"], UUID_A);
    let y = &body["strains"][1];
    assert_eq!(y["rating"]["value"], 4.9);
    assert_eq!(y["product_uuid"], Value::Null);
    assert!(x.get("search").is_none(), "list items carry no search text");
    let (_, detail) = get_json(&app, &format!("/api/v1/strains/{strain_x}")).await;
    assert!(
        !detail["search"].as_str().unwrap().contains("fünf"),
        "reviews are not searchable"
    );

    let (_, metadata) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(metadata["best_rated"]["strain_id"], strain_x);
    assert_eq!(metadata["best_rated"]["price"], 8.0);
    assert_eq!(metadata["best_rated"]["apotheke"], "Apo B");
    assert_eq!(metadata["best_rated"]["review_count"], 6);

    // Reaching the threshold flips best_rated. Only mark the cache stale (as the
    // revalidation timer would): the rebuild must be triggered by the changed
    // reviews version, e.g. after `scrape-once --reviews-only` in another process.
    parsed.review_count = 5;
    reviews::persist(&pool, strain_y, None, &parsed)
        .await
        .unwrap();
    state.snapshot.mark_stale();
    let (_, metadata) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(metadata["best_rated"]["strain_id"], strain_y);
    assert_eq!(metadata["best_rated"]["rating_value"], 4.9);
    assert_eq!(metadata["best_rated"]["review_count"], 5);
    // Export keeps the same Strain shape.
    let (_, export) = get_json(&app, "/api/v1/export.json").await;
    assert_eq!(export[1]["rating"]["count"], 5);
}
