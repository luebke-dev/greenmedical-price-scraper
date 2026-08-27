//! HTTP API tests via `tower::ServiceExt::oneshot`.

mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::{Duration, Utc};
use greenmedical_backend::api::build_router;
use greenmedical_backend::db::{offers, runs};
use greenmedical_backend::domain::{self, CSV_FIELDNAMES, RunStatus, RunTrigger};
use greenmedical_backend::scrape::run::scrape_now;
use greenmedical_backend::state::SharedState;
use serde_json::Value;
use sqlx::PgPool;
use support::{
    MockSite, SeedOffer, default_site, seed_run, test_config, test_state, test_state_with,
};
use tower::ServiceExt;
use wiremock::ResponseTemplate;

async fn get(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, body)
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
    let (status, _, body) = get(app, uri).await;
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

const PHARMACY_A: (&str, &str) = ("aaaa-1", "Apo A");
const PHARMACY_B: (&str, &str) = ("bbbb-2", "Apo B");
const SORTE_X: (&str, &str) = ("Sorte X", "EMK");
const SORTE_Y: (&str, &str) = ("Sorte Y", "XYZ");

#[sqlx::test(migrations = "./migrations")]
async fn health_and_readiness(pool: PgPool) {
    let state = test_state(pool, "http://127.0.0.1:1");
    let app = build_router(state.clone());
    let (status, body) = get_json(&app, "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, serde_json::json!({ "status": "ok" }));

    let (status, body) = get_json(&app, "/readyz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ready");
    assert_eq!(body["db"], "ok");

    state.shutdown.cancel();
    let (status, body) = get_json(&app, "/readyz").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
}

#[sqlx::test(migrations = "./migrations")]
async fn responses_carry_request_id_and_unknown_paths_are_404(pool: PgPool) {
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, headers, body) = get(&app, "/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(headers.contains_key("x-request-id"));
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["error"]["code"], "not_found");
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_and_strains_return_no_data_without_a_run(pool: PgPool) {
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    for uri in [
        "/api/v1/metadata",
        "/api/v1/strains",
        "/api/v1/export.csv",
        "/api/v1/export.json",
    ] {
        let (status, body) = get_json(&app, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body["error"]["code"], "no_data", "{uri}");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn metadata_equals_pure_function_output(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let app = build_router(state.clone());

    let (status, body) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(status, StatusCode::OK);

    let stored = offers::for_run(&pool, run.id).await.unwrap();
    let strains = domain::group_by_strain(&stored);
    let expected = domain::build_metadata(&stored, &strains, run.finished_at.unwrap(), run.clone());
    assert_eq!(body, serde_json::to_value(&expected).unwrap());
    assert_eq!(body["total"], 4);
    assert_eq!(body["strain_count"], 3);
    assert_eq!(body["pharmacy_count"], 2);
    assert_eq!(body["lowest_price"], 5.49);
    assert_eq!(body["cheapest_gram"]["name"], "Bunatic");
    assert!(body["cheapest_gram"]["strain_id"].as_i64().unwrap() > 0);
    assert!(body["cheapest_gram"]["pharmacy_id"].as_i64().unwrap() > 0);
    assert_eq!(body["run"]["id"], run.id);
    assert_eq!(body["run"]["status"], "success");
    assert_eq!(
        body["generated_at"],
        serde_json::to_value(run.finished_at.unwrap()).unwrap()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn strains_have_etag_and_support_304(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let app = build_router(state.clone());

    let (status, headers, body) = get(&app, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    let etag = format!("\"run-{}\"", run.id);
    assert_eq!(headers[header::ETAG], etag);
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["run"]["id"], run.id);
    assert!(value["reference_run"].is_null());
    let strains = value["strains"].as_array().unwrap();
    assert_eq!(strains.len(), 3);
    let og = strains.iter().find(|s| s["name"] == "OG Kush").unwrap();
    assert_eq!(og["offers"].as_array().unwrap().len(), 2);
    assert_eq!(og["offers"][0]["apotheke"], "Asavita"); // cheapest first
    assert_eq!(og["offers"][0]["preis_eur_pro_gramm"], 5.99);
    assert!(og["offers"][0]["offer_id"].as_i64().unwrap() > 0);
    assert!(og["offers"][0]["pharmacy_id"].as_i64().unwrap() > 0);
    assert_eq!(og["min_price"], 5.99);
    assert_eq!(og["pharmacy_count"], 2);
    assert_eq!(og["thc_value"], 24.0);
    assert!(og["trend"].is_null());
    assert!(og["search"].as_str().unwrap().contains("asavita"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/strains")
                .header(header::IF_NONE_MATCH, &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(response.headers()[header::ETAG], etag);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn strain_detail_and_not_found(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let app = build_router(state.clone());

    let (_, strains) = get_json(&app, "/api/v1/strains").await;
    let id = strains["strains"][0]["id"].as_i64().unwrap();
    let (status, body) = get_json(&app, &format!("/api/v1/strains/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);
    assert_eq!(body["in_latest_run"], true);
    assert_eq!(body["run"]["id"], run.id);
    assert!(body["first_seen_at"].is_string());
    assert!(body["last_seen_at"].is_string());
    assert!(!body["offers"].as_array().unwrap().is_empty());

    let (status, body) = get_json(&app, "/api/v1/strains/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (status, body) = get_json(&app, "/api/v1/strains/abc").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

/// Extractor failures (path/query) must use the JSON error envelope, not axum's plain text.
#[sqlx::test(migrations = "./migrations")]
async fn extractor_rejections_use_the_error_envelope(pool: PgPool) {
    let r = seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let strain_id = offers::for_run(&pool, r).await.unwrap()[0].strain_id;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));

    let history = format!("/api/v1/strains/{strain_id}/history");
    let cases: Vec<(String, &str)> = vec![
        ("/api/v1/strains/abc".into(), "`abc`"),
        (format!("{history}?bucket=week"), "bucket"),
        (format!("{history}?from=2026-01-01"), "from"),
        (format!("{history}?pharmacies=maybe"), "pharmacies"),
        ("/api/v1/runs?limit=abc".into(), "limit"),
        ("/api/v1/runs?offset=-x".into(), "offset"),
        ("/api/v1/runs/abc".into(), "`abc`"),
        ("/api/v1/export.csv?run_id=x".into(), "run_id"),
        ("/api/v1/export.json?run_id=1.5".into(), "run_id"),
    ];
    for (uri, needle) in cases {
        let (status, headers, body) = get(&app, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(headers[header::CONTENT_TYPE], "application/json", "{uri}");
        let value: Value =
            serde_json::from_slice(&body).unwrap_or_else(|_| panic!("{uri}: no JSON body"));
        assert_eq!(value["error"]["code"], "bad_request", "{uri}");
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.contains(needle), "{uri}: {message}");
        assert!(
            !message.contains("Failed to deserialize"),
            "{uri}: {message}"
        );
        assert!(!message.contains("Invalid URL"), "{uri}: {message}");
    }
}

/// A known path with an unsupported method answers 405 with the envelope.
#[sqlx::test(migrations = "./migrations")]
async fn method_mismatch_returns_enveloped_405(pool: PgPool) {
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    for (method, uri) in [
        ("GET", "/api/v1/admin/scrape"),
        ("POST", "/api/v1/metadata"),
        ("DELETE", "/api/v1/strains/1"),
        ("POST", "/healthz"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {uri}"
        );
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/json",
            "{method} {uri}"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "bad_request", "{method} {uri}");
        assert_eq!(value["error"]["message"], "Methode nicht erlaubt");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn strain_not_in_latest_run_is_reported(pool: PgPool) {
    let old_id = seed_run(
        &pool,
        Utc::now() - Duration::days(3),
        RunStatus::Success,
        &[
            SeedOffer::new(PHARMACY_A, SORTE_X, 9.5),
            SeedOffer::new(PHARMACY_A, SORTE_Y, 7.0),
        ],
    )
    .await;
    seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 9.0)],
    )
    .await;
    let old_offers = offers::for_run(&pool, old_id).await.unwrap();
    let y_id = old_offers
        .iter()
        .find(|o| o.name == "Sorte Y")
        .unwrap()
        .strain_id;

    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, body) = get_json(&app, &format!("/api/v1/strains/{y_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["in_latest_run"], false);
    assert_eq!(body["name"], "Sorte Y");
    assert_eq!(body["offers"], serde_json::json!([]));
    assert!(body["min_price"].is_null());
    assert_eq!(body["pharmacy_count"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn trend_is_computed_against_run_eight_days_ago(pool: PgPool) {
    let now = Utc::now();
    let reference = seed_run(
        &pool,
        now - Duration::days(8),
        RunStatus::Success,
        &[
            SeedOffer::new(PHARMACY_A, SORTE_X, 6.49),
            SeedOffer::new(PHARMACY_A, SORTE_Y, 7.0),
        ],
    )
    .await;
    // A run 3 days ago must not be the reference (younger than 7 days).
    seed_run(
        &pool,
        now - Duration::days(3),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let latest = seed_run(
        &pool,
        now,
        RunStatus::Success,
        &[
            SeedOffer::new(PHARMACY_A, SORTE_X, 5.99),
            SeedOffer::new(PHARMACY_B, SORTE_X, 6.49),
            SeedOffer::new(PHARMACY_A, ("Sorte Z", "NEU"), 8.0),
        ],
    )
    .await;

    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, body) = get_json(&app, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["run"]["id"], latest);
    assert_eq!(body["reference_run"]["id"], reference);
    let strains = body["strains"].as_array().unwrap();
    let x = strains.iter().find(|s| s["name"] == "Sorte X").unwrap();
    assert_eq!(x["trend"]["reference_run_id"], reference);
    assert_eq!(x["trend"]["min_price_then"], 6.49);
    assert_eq!(x["trend"]["delta"], -0.5);
    assert_eq!(x["trend"]["direction"], "down");
    assert!((x["trend"]["delta_pct"].as_f64().unwrap() - -7.7).abs() < 0.01);
    let z = strains.iter().find(|s| s["name"] == "Sorte Z").unwrap();
    assert!(z["trend"].is_null(), "new strain has no reference price");
}

#[sqlx::test(migrations = "./migrations")]
async fn history_by_run_and_by_day(pool: PgPool) {
    let now = Utc::now();
    let day1 = now - Duration::days(2);
    let r1 = seed_run(
        &pool,
        day1,
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let r2 = seed_run(
        &pool,
        day1 + Duration::hours(6),
        RunStatus::Partial,
        &[
            SeedOffer::new(PHARMACY_A, SORTE_X, 7.0),
            SeedOffer::new(PHARMACY_B, SORTE_X, 8.0),
        ],
    )
    .await;
    let r3 = seed_run(
        &pool,
        now,
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_B, SORTE_X, 5.0)],
    )
    .await;
    let strain_id = offers::for_run(&pool, r3).await.unwrap()[0].strain_id;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));

    let (status, body) = get_json(&app, &format!("/api/v1/strains/{strain_id}/history")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["strain_id"], strain_id);
    assert_eq!(body["bucket"], "run");
    assert_eq!(body["timezone"], "Europe/Berlin");
    assert!(body.get("pharmacies").is_none());
    let points = body["points"].as_array().unwrap();
    let ids: Vec<i64> = points
        .iter()
        .map(|p| p["run_id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [r1, r2, r3]);
    assert_eq!(points[1]["min"], 7.0);
    assert_eq!(points[1]["avg"], 7.5);
    assert_eq!(points[1]["max"], 8.0);
    assert_eq!(points[1]["offer_count"], 2);
    assert_eq!(points[1]["pharmacy_count"], 2);
    assert_eq!(points[1]["status"], "partial");
    assert_eq!(points[1]["min_per_thc_gram"], 35.0);
    assert!(points[0]["at"].as_str().unwrap().ends_with('Z'));

    let (_, body) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_id}/history?include_partial=false"),
    )
    .await;
    let ids: Vec<i64> = body["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["run_id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [r1, r3]);

    let (status, body) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_id}/history?bucket=day&pharmacies=true"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bucket"], "day");
    let points = body["points"].as_array().unwrap();
    assert_eq!(points.len(), 2);
    assert!(points[0].get("run_id").is_none());
    assert_eq!(points[0]["run_count"], 2);
    assert_eq!(points[0]["min"], 6.0);
    assert_eq!(points[0]["max"], 8.0);
    assert_eq!(points[0]["pharmacy_count"], 2);
    let at = points[0]["at"].as_str().unwrap();
    assert_eq!(at.len(), 10, "{at}");
    assert_eq!(
        at,
        day1.with_timezone(&chrono_tz::Europe::Berlin)
            .format("%Y-%m-%d")
            .to_string()
    );
    let series = body["pharmacies"].as_array().unwrap();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0]["name"], "Apo A");
    assert_eq!(series[0]["points"][0]["price"], 6.0);
    assert_eq!(series[0]["points"][0]["availability"], "Auf Lager");
    assert_eq!(series[1]["name"], "Apo B");
    assert_eq!(series[1]["points"].as_array().unwrap().len(), 2);

    let (status, body) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_id}/history?pharmacies=true"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let series = body["pharmacies"].as_array().unwrap();
    assert_eq!(series[0]["points"][0]["run_id"], r1);
}

#[sqlx::test(migrations = "./migrations")]
async fn history_validation(pool: PgPool) {
    let r = seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let strain_id = offers::for_run(&pool, r).await.unwrap()[0].strain_id;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));

    let (status, body) = get_json(&app, "/api/v1/strains/424242/history").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (status, body) = get_json(
        &app,
        &format!(
            "/api/v1/strains/{strain_id}/history?from=2020-01-01T00:00:00Z&to=2026-01-01T00:00:00Z"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");

    let (status, _) = get_json(
        &app,
        &format!(
            "/api/v1/strains/{strain_id}/history?from=2026-02-01T00:00:00Z&to=2026-01-01T00:00:00Z"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = get_json(
        &app,
        &format!(
            "/api/v1/strains/{strain_id}/history?from=2020-01-01T00:00:00Z&to=2020-03-01T00:00:00Z"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["points"], serde_json::json!([]));
    assert_eq!(body["from"], "2020-01-01T00:00:00Z");

    let (status, body) = get_json(
        &app,
        &format!("/api/v1/strains/{strain_id}/history?bucket=week"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

/// Fractional digits of an RFC 3339 timestamp (`…T08:00:03.123456Z` → 6).
fn fractional_digits(timestamp: &str) -> usize {
    timestamp
        .split_once('.')
        .map(|(_, rest)| rest.trim_end_matches('Z').len())
        .unwrap_or(0)
}

/// Defaulted and user-supplied bounds are echoed with at most microsecond
/// precision, like every timestamp derived from PostgreSQL.
#[sqlx::test(migrations = "./migrations")]
async fn history_bounds_use_microsecond_precision(pool: PgPool) {
    let r = seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let strain_id = offers::for_run(&pool, r).await.unwrap()[0].strain_id;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));

    let (status, body) = get_json(&app, &format!("/api/v1/strains/{strain_id}/history")).await;
    assert_eq!(status, StatusCode::OK);
    for key in ["from", "to"] {
        let value = body[key].as_str().unwrap();
        assert!(
            fractional_digits(value) <= 6,
            "{key} has more than 6 fractional digits: {value}"
        );
        assert!(value.ends_with('Z'), "{key} is not UTC: {value}");
    }

    let (status, body) = get_json(
        &app,
        &format!(
            "/api/v1/strains/{strain_id}/history?from=2026-01-01T00:00:00.123456789Z&to=2026-01-02T00:00:00.999999999Z"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["from"], "2026-01-01T00:00:00.123456Z");
    assert_eq!(body["to"], "2026-01-02T00:00:00.999999Z");
}

#[sqlx::test(migrations = "./migrations")]
async fn runs_list_and_detail(pool: PgPool) {
    let now = Utc::now();
    let r1 = seed_run(
        &pool,
        now - Duration::hours(2),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let r2 = seed_run(
        &pool,
        now - Duration::hours(1),
        RunStatus::Partial,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let failed = runs::insert_running(&pool, RunTrigger::Bootstrap, "x")
        .await
        .unwrap();
    runs::insert_errors(
        &pool,
        failed,
        &[domain::RunErrorDto {
            pharmacy_name: "Apo".into(),
            pharmacy_url: "https://x".into(),
            stage: "pages".into(),
            message: "HTTP 500".into(),
        }],
    )
    .await
    .unwrap();
    runs::mark_failed(&pool, failed, "boom").await.unwrap();

    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, body) = get_json(&app, "/api/v1/runs").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    let ids: Vec<i64> = body["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [failed, r2, r1]);
    assert_eq!(body["runs"][0]["status"], "failed");
    assert_eq!(body["runs"][0]["error"], "boom");
    assert_eq!(body["runs"][0]["trigger"], "bootstrap");

    let (_, body) = get_json(&app, "/api/v1/runs?status=success&limit=1").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["runs"][0]["id"], r1);

    let (_, body) = get_json(&app, "/api/v1/runs?limit=1&offset=1").await;
    assert_eq!(body["runs"].as_array().unwrap().len(), 1);
    assert_eq!(body["runs"][0]["id"], r2);

    let (status, body) = get_json(&app, "/api/v1/runs?limit=501").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
    let (status, _) = get_json(&app, "/api/v1/runs?status=weird").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = get_json(&app, "/api/v1/runs?limit=abc").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");

    let (status, body) = get_json(&app, &format!("/api/v1/runs/{failed}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], failed);
    assert_eq!(body["errors"][0]["stage"], "pages");
    assert_eq!(body["errors"][0]["message"], "HTTP 500");
    let (status, _) = get_json(&app, "/api/v1/runs/999999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn pharmacies_list_offer_count_latest(pool: PgPool) {
    let now = Utc::now();
    seed_run(
        &pool,
        now - Duration::hours(1),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_B, SORTE_X, 6.0)],
    )
    .await;
    seed_run(
        &pool,
        now,
        RunStatus::Success,
        &[
            SeedOffer::new(PHARMACY_A, SORTE_X, 6.0),
            SeedOffer::new(PHARMACY_A, SORTE_Y, 7.0),
        ],
    )
    .await;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, body) = get_json(&app, "/api/v1/pharmacies").await;
    assert_eq!(status, StatusCode::OK);
    let list = body["pharmacies"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["name"], "Apo A");
    assert_eq!(list[0]["offer_count_latest"], 2);
    assert_eq!(list[0]["external_id"], "aaaa-1");
    assert_eq!(list[1]["name"], "Apo B");
    assert_eq!(list[1]["offer_count_latest"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn export_csv_matches_old_format(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let app = build_router(state.clone());

    let (status, headers, body) = get(&app, "/api/v1/export.csv").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"greenmedical_flowers.csv\""
    );
    let text = String::from_utf8(body).unwrap();
    let mut lines = text.split("\r\n");
    assert_eq!(lines.next().unwrap(), CSV_FIELDNAMES.join(","));
    let first = lines.next().unwrap();
    assert!(first.starts_with("Grüne Blüte,04416,Markkleeberg,Bunatic,Luana 27/1 Donny B,Indica,27%,1%,\"5,49 €/g\",Auf Lager,"), "{first}");
    assert_eq!(text.matches("\r\n").count(), 5);

    let (status, _, body_by_id) = get(&app, &format!("/api/v1/export.csv?run_id={}", run.id)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body_by_id, text.as_bytes());
    let (status, body) = get_json(&app, "/api/v1/export.csv?run_id=999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

/// `?run_id=` exports of older runs are built once and then served from a small LRU.
#[sqlx::test(migrations = "./migrations")]
async fn export_by_run_id_is_cached_per_run(pool: PgPool) {
    let now = Utc::now();
    let old = seed_run(
        &pool,
        now - Duration::hours(2),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let latest = seed_run(
        &pool,
        now,
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_B, SORTE_Y, 7.0)],
    )
    .await;
    let running = runs::insert_running(&pool, RunTrigger::Manual, "x")
        .await
        .unwrap();
    let state = test_state(pool.clone(), "http://127.0.0.1:1");
    let app = build_router(state.clone());

    let (status, _, first) = get(&app, &format!("/api/v1/export.csv?run_id={old}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        String::from_utf8(first.clone())
            .unwrap()
            .contains("Sorte X")
    );
    assert_eq!(state.snapshot.cached_run_ids().await, [old]);

    let (status, _, second) = get(&app, &format!("/api/v1/export.json?run_id={old}")).await;
    assert_eq!(status, StatusCode::OK);
    let value: Value = serde_json::from_slice(&second).unwrap();
    assert_eq!(value[0]["name"], "Sorte X");
    assert_eq!(state.snapshot.cached_run_ids().await, [old]);

    // The latest run is served from the main snapshot, not the LRU.
    let (status, _, _) = get(&app, "/api/v1/export.csv").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state.snapshot.peek().unwrap().run.id, latest);
    let (status, _, _) = get(&app, &format!("/api/v1/export.csv?run_id={latest}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state.snapshot.cached_run_ids().await, [old]);

    // A still-running run is never cached (its offers may still arrive).
    let (status, _, body) = get(&app, &format!("/api/v1/export.csv?run_id={running}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        String::from_utf8(body).unwrap(),
        format!("{}\r\n", CSV_FIELDNAMES.join(","))
    );
    assert_eq!(state.snapshot.cached_run_ids().await, [old]);

    // Capacity is bounded: the oldest entry is evicted.
    let mut extra = Vec::new();
    for i in 0..greenmedical_backend::db::snapshot::RUN_CACHE_CAPACITY {
        let id = seed_run(
            &pool,
            now - Duration::days(10 + i as i64),
            RunStatus::Partial,
            &[SeedOffer::new(PHARMACY_A, SORTE_X, 5.0)],
        )
        .await;
        let (status, _, _) = get(&app, &format!("/api/v1/export.csv?run_id={id}")).await;
        assert_eq!(status, StatusCode::OK);
        extra.push(id);
    }
    assert_eq!(state.snapshot.cached_run_ids().await, extra);
}

/// A second replica (own `AppState`, same database) must pick up a run scraped
/// by the first one without a restart.
#[sqlx::test(migrations = "./migrations")]
async fn other_replicas_pick_up_new_runs(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let replica_a = test_state(pool.clone(), &site.base_url());
    let mut config_b = test_config(&site.base_url());
    config_b.snapshot_revalidate_interval = std::time::Duration::ZERO;
    let replica_b = test_state_with(pool.clone(), config_b);
    let app_b = build_router(replica_b.clone());

    let first = scrape_now(&replica_a, RunTrigger::Manual).await.unwrap();
    let (status, headers, _) = get(&app_b, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ETAG], format!("\"run-{}\"", first.id));
    let offer_count_of = |body: &Value, name: &str| -> i64 {
        body["pharmacies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()["offer_count_latest"]
            .as_i64()
            .unwrap()
    };
    let (_, body) = get_json(&app_b, "/api/v1/pharmacies").await;
    assert_eq!(offer_count_of(&body, "Grüne Blüte"), 3);
    assert_eq!(offer_count_of(&body, "Asavita"), 1);

    // Replica A scrapes again; only A's cache is invalidated explicitly.
    site.mount_list(
        ResponseTemplate::new(200).set_body_string(
            "<html><body><table><tr><th>h</th></tr><tr><td><a href=/de/cannabis/pharmacy/asavita>Asavita</a></td><td>10365</td><td>Berlin</td><td>x</td></tr></table></body></html>",
        ),
    )
    .await;
    let second = scrape_now(&replica_a, RunTrigger::Manual).await.unwrap();
    assert_eq!(second.status, RunStatus::Success);
    assert!(second.id > first.id);

    // Replica B revalidates (interval 0) and serves the new run everywhere.
    let (status, headers, body) = get(&app_b, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::ETAG], format!("\"run-{}\"", second.id));
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["run"]["id"], second.id);
    assert_eq!(value["strains"].as_array().unwrap().len(), 1);
    let (_, body) = get_json(&app_b, "/api/v1/metadata").await;
    assert_eq!(body["run"]["id"], second.id);
    assert_eq!(body["total"], 1);
    let (_, body) = get_json(&app_b, "/api/v1/pharmacies").await;
    assert_eq!(offer_count_of(&body, "Grüne Blüte"), 0);
    assert_eq!(offer_count_of(&body, "Asavita"), 1);
    let (status, _, body) = get(&app_b, "/api/v1/export.csv").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(body).unwrap().matches("\r\n").count(), 2);

    // A 304 against the old ETag is no longer possible.
    let response = app_b
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/strains")
                .header(header::IF_NONE_MATCH, format!("\"run-{}\"", first.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// With the default interval a replica serves the cached run until the next
/// revalidation; `mark_stale` forces it (used here instead of waiting 30 s).
#[sqlx::test(migrations = "./migrations")]
async fn revalidation_is_rate_limited(pool: PgPool) {
    let first = seed_run(
        &pool,
        Utc::now() - Duration::hours(1),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 6.0)],
    )
    .await;
    let state = test_state(pool.clone(), "http://127.0.0.1:1");
    assert_eq!(
        state.config.snapshot_revalidate_interval,
        std::time::Duration::from_secs(30)
    );
    let app = build_router(state.clone());
    let (_, body) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(body["run"]["id"], first);

    let second = seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::new(PHARMACY_A, SORTE_X, 5.0)],
    )
    .await;
    let (_, body) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(body["run"]["id"], first, "still cached within the interval");

    state.snapshot.mark_stale();
    let (_, body) = get_json(&app, "/api/v1/metadata").await;
    assert_eq!(body["run"]["id"], second);
    assert_eq!(body["lowest_price"], 5.0);
}

#[sqlx::test(migrations = "./migrations")]
async fn export_json_is_a_bare_array(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let app = build_router(state.clone());
    let (status, headers, body) = get(&app, "/api/v1/export.json").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"flowers.json\""
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    let array = value.as_array().expect("bare array");
    assert_eq!(array.len(), 3);
    assert!(array[0]["offers"].is_array());
    assert!(array[0]["search"].is_string());
}

async fn admin_post(app: &Router, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/admin/scrape");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

async fn wait_for_finish(state: &SharedState, run_id: i64) -> domain::RunDto {
    for _ in 0..200 {
        let run = runs::get(&state.pool, run_id).await.unwrap().unwrap();
        if run.status != RunStatus::Running {
            return run;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("run {run_id} did not finish");
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_scrape_is_hidden_without_token(pool: PgPool) {
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (status, body) = admin_post(&app, Some("anything")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_scrape_401_202_409(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    // Slow list page so the first run is still in progress for the second request.
    site.mount_list(
        ResponseTemplate::new(200)
            .set_body_string("<html><body><table><tr><th>h</th></tr></table></body></html>")
            .set_delay(std::time::Duration::from_millis(800)),
    )
    .await;
    let mut config = test_config(&site.base_url());
    config.admin_token = "s3cret".into();
    let state = test_state_with(pool, config);
    let app = build_router(state.clone());

    let (status, body) = admin_post(&app, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
    let (status, _) = admin_post(&app, Some("wrong")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = admin_post(&app, Some("s3cret")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "running");
    let run_id = body["run_id"].as_i64().unwrap();

    let (status, body) = admin_post(&app, Some("s3cret")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(body["error"]["message"], "scrape_in_progress");

    let run = wait_for_finish(&state, run_id).await;
    assert_eq!(run.trigger, RunTrigger::Manual);
    assert_eq!(run.status, RunStatus::Failed); // empty table → no pharmacies
}

/// A manual run is tracked so shutdown waits for it to be marked `failed`
/// (`shutdown`) before the pool is closed.
#[sqlx::test(migrations = "./migrations")]
async fn manual_run_is_tracked_and_marked_failed_on_shutdown(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    site.mount_list(
        ResponseTemplate::new(200)
            .set_body_string("<html></html>")
            .set_delay(std::time::Duration::from_secs(5)),
    )
    .await;
    let mut config = test_config(&site.base_url());
    config.admin_token = "s3cret".into();
    let state = test_state_with(pool.clone(), config);
    let app = build_router(state.clone());

    let (status, body) = admin_post(&app, Some("s3cret")).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let run_id = body["run_id"].as_i64().unwrap();
    assert_eq!(state.tasks.len(), 1);

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    state.shutdown.cancel();
    state.tasks.close();
    tokio::time::timeout(std::time::Duration::from_secs(20), state.tasks.wait())
        .await
        .expect("manual run finished before the shutdown deadline");

    let run = runs::get(&pool, run_id).await.unwrap().unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("shutdown"));
    assert!(run.finished_at.is_some());
    assert!(state.snapshot.peek().is_none(), "cache invalidated on exit");
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_scrape_reports_lock_held_elsewhere(pool: PgPool) {
    let mut config = test_config("http://127.0.0.1:1");
    config.admin_token = "s3cret".into();
    let state = test_state_with(pool.clone(), config);
    let app = build_router(state.clone());
    // Another "instance" holds the advisory lock on its own connection.
    let guard = greenmedical_backend::scrape::run::try_acquire_lock(&pool)
        .await
        .unwrap()
        .unwrap();
    let (status, body) = admin_post(&app, Some("s3cret")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["message"], "scrape_locked_elsewhere");
    guard.release_now().await.unwrap();
}
