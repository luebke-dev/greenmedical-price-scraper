//! `GET /api/v1/strains/{id}/offer-history`: phases, flat rows, paging, validation.

mod support;

use std::collections::HashMap;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use greenmedical_backend::api::build_router;
use greenmedical_backend::db::offers;
use greenmedical_backend::domain::RunStatus;
use serde_json::Value;
use sqlx::PgPool;
use support::{SeedOffer, seed_run, test_state};
use tower::ServiceExt;

const APO_A: (&str, &str) = ("aaaa-1", "Apo A");
const APO_B: (&str, &str) = ("bbbb-2", "Apo B");
const SORTE: (&str, &str) = ("Sorte X", "EMK");

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

struct Seeded {
    app: Router,
    strain_id: i64,
    /// r1..r5 run ids.
    runs: [i64; 5],
}

/// r1: A 6.0 | r2 (+1 min): A 6.0, B 7.0 | r3: A 6.5, B 7.0 | r4: B 7.0 | r5: A 6.5, B 7.0
async fn seed(pool: &PgPool) -> Seeded {
    let now = Utc::now();
    let r1 = now - Duration::days(5);
    let starts = [
        r1,
        r1 + Duration::minutes(1),
        now - Duration::days(3),
        now - Duration::days(2),
        now - Duration::days(1),
    ];
    let offers_per_run: [Vec<SeedOffer>; 5] = [
        vec![SeedOffer::new(APO_A, SORTE, 6.0)],
        vec![
            SeedOffer::new(APO_A, SORTE, 6.0),
            SeedOffer::new(APO_B, SORTE, 7.0),
        ],
        vec![
            SeedOffer::new(APO_A, SORTE, 6.5),
            SeedOffer::new(APO_B, SORTE, 7.0),
        ],
        vec![SeedOffer::new(APO_B, SORTE, 7.0)],
        vec![
            SeedOffer::new(APO_A, SORTE, 6.5),
            SeedOffer::new(APO_B, SORTE, 7.0),
        ],
    ];
    let mut runs = [0; 5];
    for (i, (at, seeds)) in starts.iter().zip(offers_per_run.iter()).enumerate() {
        runs[i] = seed_run(pool, *at, RunStatus::Success, seeds).await;
    }
    let strain_id = offers::for_run(pool, runs[0]).await.unwrap()[0].strain_id;
    Seeded {
        app: build_router(test_state(pool.clone(), "http://127.0.0.1:1")),
        strain_id,
        runs,
    }
}

fn rows(body: &Value) -> &Vec<Value> {
    body["rows"].as_array().unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn all_rows_per_bucket_and_pharmacy(pool: PgPool) {
    let s = seed(&pool).await;
    let uri = format!("/api/v1/strains/{}/offer-history?mode=all", s.strain_id);
    let (status, body) = get(&s.app, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["strain_id"], s.strain_id);
    assert_eq!(body["bucket"], "run");
    assert_eq!(body["mode"], "all");
    assert_eq!(body["total"], 8);
    assert_eq!(body["limit"], 50);
    assert_eq!(body["offset"], 0);
    assert!(body["from"].as_str().unwrap().ends_with('Z'));
    let [r1, r2, r3, r4, r5] = s.runs;
    let got: Vec<(i64, &str, f64)> = rows(&body)
        .iter()
        .map(|r| {
            (
                r["run_id"].as_i64().unwrap(),
                r["pharmacy"].as_str().unwrap(),
                r["price"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (r5, "Apo A", 6.5),
            (r5, "Apo B", 7.0),
            (r4, "Apo B", 7.0),
            (r3, "Apo A", 6.5),
            (r3, "Apo B", 7.0),
            (r2, "Apo A", 6.0),
            (r2, "Apo B", 7.0),
            (r1, "Apo A", 6.0),
        ]
    );
    let first = &rows(&body)[0];
    assert_eq!(first["availability"], "Auf Lager");
    assert_eq!(first["city"], "Berlin");
    assert!(first["pharmacy_id"].as_i64().unwrap() > 0);
    assert_eq!(first["price_per_thc_gram"], 32.5);
}

#[sqlx::test(migrations = "./migrations")]
async fn phases_merge_unchanged_runs_and_track_delisting(pool: PgPool) {
    let s = seed(&pool).await;
    let (_, all) = get(
        &s.app,
        &format!("/api/v1/strains/{}/offer-history?mode=all", s.strain_id),
    )
    .await;
    let at_of: HashMap<i64, String> = rows(&all)
        .iter()
        .map(|r| {
            (
                r["run_id"].as_i64().unwrap(),
                r["at"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let [r1, r2, r3, r4, r5] = s.runs;

    let (status, body) = get(
        &s.app,
        &format!("/api/v1/strains/{}/offer-history", s.strain_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["mode"], "changes");
    assert_eq!(body["total"], 5);
    let phases = rows(&body);
    type Phase<'a> = (&'a str, &'a str, Option<&'a str>, i64, bool, Option<f64>);
    let summary: Vec<Phase> = phases
        .iter()
        .map(|r| {
            (
                r["pharmacy"].as_str().unwrap(),
                r["from"].as_str().unwrap(),
                r["to"].as_str(),
                r["runs"].as_i64().unwrap(),
                r["delisted"].as_bool().unwrap(),
                r["price"].as_f64(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Apo A", at_of[&r5].as_str(), None, 1, false, Some(6.5)),
            (
                "Apo A",
                at_of[&r4].as_str(),
                Some(at_of[&r4].as_str()),
                1,
                true,
                None
            ),
            (
                "Apo A",
                at_of[&r3].as_str(),
                Some(at_of[&r3].as_str()),
                1,
                false,
                Some(6.5)
            ),
            // B appears in r2 only; r1 is ignored for B (no leading delisted phase).
            ("Apo B", at_of[&r2].as_str(), None, 4, false, Some(7.0)),
            (
                "Apo A",
                at_of[&r1].as_str(),
                Some(at_of[&r2].as_str()),
                2,
                false,
                Some(6.0)
            ),
        ]
    );
    assert_eq!(phases[1]["availability"], "");
    assert!(phases[1]["price_per_thc_gram"].is_null());
    assert_eq!(phases[0]["availability"], "Auf Lager");
}

#[sqlx::test(migrations = "./migrations")]
async fn day_bucket_merges_runs_of_a_day(pool: PgPool) {
    let s = seed(&pool).await;
    let (status, body) = get(
        &s.app,
        &format!("/api/v1/strains/{}/offer-history?bucket=day", s.strain_id),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["bucket"], "day");
    // r1 and r2 fall on the same day: A's first phase spans one day bucket.
    let a_first = rows(&body)
        .iter()
        .rfind(|r| r["pharmacy"] == "Apo A")
        .unwrap();
    assert_eq!(a_first["from"].as_str().unwrap().len(), 10);
    assert_eq!(a_first["runs"], 1);
    assert_eq!(a_first["price"], 6.0);
    let b = rows(&body)
        .iter()
        .find(|r| r["pharmacy"] == "Apo B")
        .unwrap();
    assert_eq!(b["runs"], 4);
    assert!(b["to"].is_null());
    let (_, all) = get(
        &s.app,
        &format!(
            "/api/v1/strains/{}/offer-history?bucket=day&mode=all",
            s.strain_id
        ),
    )
    .await;
    assert_eq!(all["total"], 7);
    assert!(rows(&all)[0].get("run_id").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_filter_and_validation(pool: PgPool) {
    let s = seed(&pool).await;
    let base = format!("/api/v1/strains/{}/offer-history", s.strain_id);
    let (_, body) = get(&s.app, &format!("{base}?mode=all&limit=2&offset=1")).await;
    assert_eq!(body["total"], 8);
    assert_eq!(body["limit"], 2);
    assert_eq!(body["offset"], 1);
    let [_, _, _, r4, r5] = s.runs;
    assert_eq!(rows(&body).len(), 2);
    assert_eq!(rows(&body)[0]["run_id"], r5);
    assert_eq!(rows(&body)[0]["pharmacy"], "Apo B");
    assert_eq!(rows(&body)[1]["run_id"], r4);
    let (_, body) = get(&s.app, &format!("{base}?offset=100")).await;
    assert_eq!(body["total"], 5);
    assert!(rows(&body).is_empty());

    let (_, all) = get(&s.app, &format!("{base}?mode=all")).await;
    let b_id = rows(&all)
        .iter()
        .find(|r| r["pharmacy"] == "Apo B")
        .unwrap()["pharmacy_id"]
        .as_i64()
        .unwrap();
    let (_, body) = get(&s.app, &format!("{base}?pharmacy_id={b_id}")).await;
    assert_eq!(body["total"], 1);
    assert_eq!(rows(&body)[0]["runs"], 4);
    let (_, body) = get(&s.app, &format!("{base}?pharmacy_id={b_id}&mode=all")).await;
    assert_eq!(body["total"], 4);

    // Window: only the last two runs. A is absent in r4, which is now the
    // first bucket, so A starts at r5 without a leading delisted phase.
    let from = (Utc::now() - Duration::days(2) - Duration::hours(1)).to_rfc3339();
    let (_, body) = get(&s.app, &format!("{base}?from={}", from.replace('+', "%2B"))).await;
    assert_eq!(body["total"], 2, "A 6.5 (r5), B (r4-r5)");
    assert_eq!(rows(&body)[1]["runs"], 2);

    for uri in [
        format!("{base}?limit=0"),
        format!("{base}?limit=501"),
        format!("{base}?offset=-1"),
        format!("{base}?mode=some"),
        format!("{base}?bucket=week"),
        format!("{base}?from=2026-02-01T00:00:00Z&to=2026-01-01T00:00:00Z"),
        format!("{base}?from=2020-01-01T00:00:00Z&to=2026-01-01T00:00:00Z"),
        format!("{base}?pharmacy_id=abc"),
    ] {
        let (status, body) = get(&s.app, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "bad_request", "{uri}");
    }
    let (status, body) = get(&s.app, "/api/v1/strains/424242/offer-history").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}
