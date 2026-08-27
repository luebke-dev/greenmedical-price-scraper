//! `GET /api/v1/strains`: server-side pagination, filtering, sorting, facets and ETag.

mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use greenmedical_backend::api::build_router;
use greenmedical_backend::domain::RunStatus;
use serde_json::Value;
use sqlx::PgPool;
use support::{SeedOffer, seed_run, set_rating, test_state};
use tower::ServiceExt;

const APO_A: (&str, &str) = ("aaaa-1", "Apo A");
const APO_B: (&str, &str) = ("bbbb-2", "Apo B");

async fn get(app: &Router, uri: &str) -> (StatusCode, axum::http::HeaderMap, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, headers, value)
}

fn names(body: &Value) -> Vec<String> {
    body["strains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect()
}

fn ids(body: &Value) -> Vec<i64> {
    body["strains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_i64().unwrap())
        .collect()
}

/// Five strains: prices 5/7/6(+6.5)/8/null, one without THC value and genetik.
async fn seed(pool: &PgPool) -> Router {
    seed_run(
        pool,
        Utc::now(),
        RunStatus::Success,
        &[
            SeedOffer::new(APO_A, ("Äpfel", "AAA"), 5.0)
                .thc("20%")
                .genetik("Sativa"),
            SeedOffer::new(APO_A, ("apfel", "BBB"), 7.0)
                .thc("25%")
                .cbd("0,5%")
                .genetik("Indica"),
            SeedOffer::new(APO_A, ("Zebra", "ZZZ"), 6.0)
                .thc("18%")
                .genetik("Hybrid"),
            SeedOffer::new(APO_B, ("Zebra", "ZZZ"), 6.5)
                .thc("18%")
                .genetik("Hybrid"),
            SeedOffer::new(APO_B, ("Sorte 9", "S9"), 8.0)
                .thc("20%")
                .genetik("Sativa"),
            SeedOffer::unpriced(APO_A, ("Sorte 10", "S10"))
                .thc("")
                .genetik(""),
        ],
    )
    .await;
    let ids: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM strains")
        .fetch_all(pool)
        .await
        .unwrap();
    for (id, name) in ids {
        match name.as_str() {
            "Äpfel" => set_rating(pool, id, Some(4.5), 10).await,
            "Zebra" => set_rating(pool, id, Some(3.0), 2).await,
            _ => {}
        }
    }
    build_router(test_state(pool.clone(), "http://127.0.0.1:1"))
}

#[sqlx::test(migrations = "./migrations")]
async fn defaults_price_asc_nulls_last_limit_50(pool: PgPool) {
    let app = seed(&pool).await;
    let (status, _, body) = get(&app, "/api/v1/strains").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 5);
    assert_eq!(body["limit"], 50);
    assert_eq!(body["offset"], 0);
    assert_eq!(
        names(&body),
        ["Äpfel", "Zebra", "apfel", "Sorte 9", "Sorte 10"]
    );
    let last = &body["strains"][4];
    assert!(last["sort"]["price"].is_null());
    for item in body["strains"].as_array().unwrap() {
        assert!(item.get("offers").is_none());
        assert!(item.get("search").is_none());
        for key in [
            "id",
            "name",
            "bezeichnung",
            "genetik",
            "thc",
            "cbd",
            "thc_value",
            "cbd_value",
            "min_price",
            "min_price_per_thc_gram",
            "pharmacy_count",
            "sort",
            "trend",
            "rating",
            "product_uuid",
        ] {
            assert!(item.get(key).is_some(), "missing {key}");
        }
    }
    assert_eq!(body["strains"][0]["rating"]["value"], 4.5);
    assert_eq!(body["strains"][0]["sort"]["rating"], 4.5);
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_and_validation(pool: PgPool) {
    let app = seed(&pool).await;
    let (_, _, body) = get(&app, "/api/v1/strains?limit=2&offset=1").await;
    assert_eq!(body["total"], 5);
    assert_eq!(body["limit"], 2);
    assert_eq!(body["offset"], 1);
    assert_eq!(names(&body), ["Zebra", "apfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?limit=2&offset=4").await;
    assert_eq!(names(&body), ["Sorte 10"]);
    let (_, _, body) = get(&app, "/api/v1/strains?offset=99").await;
    assert_eq!(body["total"], 5);
    assert!(body["strains"].as_array().unwrap().is_empty());

    for uri in [
        "/api/v1/strains?limit=0",
        "/api/v1/strains?limit=501",
        "/api/v1/strains?offset=-1",
        "/api/v1/strains?sort=foo",
        "/api/v1/strains?dir=up",
        "/api/v1/strains?limit=abc",
        "/api/v1/strains?price_min=abc",
    ] {
        let (status, _, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"]["code"], "bad_request", "{uri}");
    }
    // Unknown parameters are ignored.
    let (status, _, body) = get(&app, "/api/v1/strains?foo=bar&limit=500").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["limit"], 500);
}

#[sqlx::test(migrations = "./migrations")]
async fn filters(pool: PgPool) {
    let app = seed(&pool).await;
    let (_, _, body) = get(&app, "/api/v1/strains?q=ZEBRA").await;
    assert_eq!(names(&body), ["Zebra"]);
    let (_, _, body) = get(&app, "/api/v1/strains?q=apo%20b").await;
    assert_eq!(body["total"], 2, "search text includes pharmacies");
    assert_eq!(names(&body), ["Zebra", "Sorte 9"]);

    let (_, _, body) = get(&app, "/api/v1/strains?genetik=SATIVA,%20hybrid").await;
    assert_eq!(names(&body), ["Äpfel", "Zebra", "Sorte 9"]);
    let (_, _, body) = get(&app, "/api/v1/strains?genetik=indica").await;
    assert_eq!(names(&body), ["apfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?genetik=,").await;
    assert_eq!(body["total"], 5, "empty genetik list is ignored");

    // Ranges are inclusive; strains without a value pass only without bounds.
    let (_, _, body) = get(&app, "/api/v1/strains?price_max=6").await;
    assert_eq!(names(&body), ["Äpfel", "Zebra"]);
    let (_, _, body) = get(&app, "/api/v1/strains?price_min=6&price_max=7").await;
    assert_eq!(names(&body), ["Zebra", "apfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?thc_min=20").await;
    assert_eq!(names(&body), ["Äpfel", "apfel", "Sorte 9"]);
    let (_, _, body) = get(&app, "/api/v1/strains?thc_min=19&thc_max=21").await;
    assert_eq!(names(&body), ["Äpfel", "Sorte 9"]);
    let (_, _, body) = get(&app, "/api/v1/strains?thc_max=100").await;
    assert_eq!(body["total"], 4, "null THC excluded with one bound");
    let (_, _, body) = get(&app, "/api/v1/strains?cbd_max=0.5").await;
    assert_eq!(names(&body), ["apfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?cbd_min=0").await;
    assert_eq!(body["total"], 5);

    let (_, _, body) = get(&app, "/api/v1/strains?rating_min=4").await;
    assert_eq!(names(&body), ["Äpfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?rating_min=0").await;
    assert_eq!(names(&body), ["Äpfel", "Zebra"], "unrated strains excluded");

    let (_, _, body) = get(&app, "/api/v1/strains?genetik=sativa&price_min=6&q=apo").await;
    assert_eq!(names(&body), ["Sorte 9"]);
    assert_eq!(body["total"], 1);
    assert_eq!(
        body["facets"]["genetik"].as_array().unwrap().len(),
        3,
        "facets ignore filters"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn sorting(pool: PgPool) {
    let app = seed(&pool).await;
    let (_, _, by_id) = get(&app, "/api/v1/strains?sort=name").await;
    let (apfel_ids, rest): (Vec<_>, Vec<_>) = by_id["strains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_i64().unwrap(),
                s["name"].as_str().unwrap().to_owned(),
            )
        })
        .partition(|(_, name)| name == "Äpfel" || name == "apfel");
    assert_eq!(apfel_ids.len(), 2);
    assert!(
        apfel_ids[0].0 < apfel_ids[1].0,
        "equal collation keys tie-break by id asc"
    );
    assert_eq!(
        rest.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>(),
        ["Sorte 9", "Sorte 10", "Zebra"]
    );

    let (_, _, body) = get(&app, "/api/v1/strains?sort=name&dir=desc").await;
    let n = names(&body);
    assert_eq!(&n[..3], ["Zebra", "Sorte 10", "Sorte 9"]);
    assert!(
        ids(&body)[3] < ids(&body)[4],
        "tie-break stays id asc in desc"
    );

    let (_, _, body) = get(&app, "/api/v1/strains?sort=price&dir=desc").await;
    assert_eq!(
        names(&body),
        ["Sorte 9", "apfel", "Zebra", "Äpfel", "Sorte 10"]
    );

    let (_, _, body) = get(&app, "/api/v1/strains?sort=thc").await;
    assert_eq!(
        names(&body),
        ["Zebra", "Äpfel", "Sorte 9", "apfel", "Sorte 10"]
    );
    let (_, _, body) = get(&app, "/api/v1/strains?sort=thc&dir=desc").await;
    assert_eq!(
        names(&body),
        ["apfel", "Äpfel", "Sorte 9", "Zebra", "Sorte 10"]
    );

    let (_, _, body) = get(&app, "/api/v1/strains?sort=cbd").await;
    assert_eq!(names(&body)[0], "apfel");
    let (_, _, body) = get(&app, "/api/v1/strains?sort=cbd&dir=desc").await;
    assert_eq!(names(&body)[4], "apfel");

    let (_, _, body) = get(&app, "/api/v1/strains?sort=price_per_thc_gram").await;
    // 5/0.20=25, 6/0.18=33.3, 7/0.25=28, 8/0.20=40, null
    assert_eq!(
        names(&body),
        ["Äpfel", "apfel", "Zebra", "Sorte 9", "Sorte 10"]
    );
    let (_, _, body) = get(&app, "/api/v1/strains?sort=price_per_thc_gram&dir=desc").await;
    assert_eq!(
        names(&body),
        ["Sorte 9", "Zebra", "apfel", "Äpfel", "Sorte 10"]
    );

    let (_, _, body) = get(&app, "/api/v1/strains?sort=pharmacy_count&dir=desc").await;
    assert_eq!(names(&body)[0], "Zebra");
    assert_eq!(body["strains"][0]["pharmacy_count"], 2);
    let (_, _, body) = get(&app, "/api/v1/strains?sort=pharmacy_count").await;
    assert_eq!(names(&body)[4], "Zebra");

    let (_, _, body) = get(&app, "/api/v1/strains?sort=rating").await;
    assert_eq!(&names(&body)[..2], ["Zebra", "Äpfel"]);
    let (_, _, body) = get(&app, "/api/v1/strains?sort=rating&dir=desc").await;
    assert_eq!(&names(&body)[..2], ["Äpfel", "Zebra"]);
    assert!(body["strains"][4]["sort"]["rating"].is_null(), "nulls last");

    let (_, _, body) = get(&app, "/api/v1/strains?sort=bezeichnung").await;
    assert_eq!(
        names(&body),
        ["Äpfel", "apfel", "Sorte 9", "Sorte 10", "Zebra"]
    );
    let (_, _, body) = get(&app, "/api/v1/strains?sort=genetik&dir=desc").await;
    assert_eq!(
        names(&body),
        ["Äpfel", "Sorte 9", "apfel", "Zebra", "Sorte 10"]
    );
    let (_, _, body) = get(&app, "/api/v1/strains?sort=genetik").await;
    assert_eq!(
        names(&body)[0],
        "Sorte 10",
        "empty genetik sorts first ascending"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn facets(pool: PgPool) {
    let app = seed(&pool).await;
    let (_, _, body) = get(&app, "/api/v1/strains?limit=1").await;
    assert_eq!(
        body["facets"]["genetik"],
        serde_json::json!([
            { "value": "Hybrid", "count": 1 },
            { "value": "Indica", "count": 1 },
            { "value": "Sativa", "count": 2 }
        ])
    );
    assert_eq!(
        body["facets"]["price"],
        serde_json::json!({ "min": 5.0, "max": 8.0 })
    );
    assert_eq!(
        body["facets"]["thc"],
        serde_json::json!({ "min": 18.0, "max": 25.0 })
    );
    assert_eq!(
        body["facets"]["cbd"],
        serde_json::json!({ "min": 0.5, "max": 1.0 })
    );
    assert_eq!(
        body["facets"]["rating"],
        serde_json::json!({ "min": 3.0, "max": 4.5 })
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn facets_are_null_without_values(pool: PgPool) {
    seed_run(
        &pool,
        Utc::now(),
        RunStatus::Success,
        &[SeedOffer::unpriced(APO_A, ("Nur Name", "X"))
            .thc("")
            .cbd("")
            .genetik("")],
    )
    .await;
    let app = build_router(test_state(pool, "http://127.0.0.1:1"));
    let (_, _, body) = get(&app, "/api/v1/strains").await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["facets"]["genetik"], serde_json::json!([]));
    assert!(body["facets"]["price"].is_null());
    assert!(body["facets"]["thc"].is_null());
    assert!(body["facets"]["cbd"].is_null());
    assert!(body["facets"]["rating"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn etag_varies_with_query_and_supports_304(pool: PgPool) {
    let app = seed(&pool).await;
    let (_, headers, _) = get(&app, "/api/v1/strains").await;
    let default_etag = headers[header::ETAG].to_str().unwrap().to_owned();
    assert_eq!(headers[header::CACHE_CONTROL], "public, max-age=300");
    let (_, headers, _) = get(&app, "/api/v1/strains?sort=price&dir=asc&limit=50&offset=0").await;
    assert_eq!(
        headers[header::ETAG],
        default_etag,
        "explicit defaults hash alike"
    );
    let (_, headers, _) = get(&app, "/api/v1/strains?sort=name").await;
    let name_etag = headers[header::ETAG].to_str().unwrap().to_owned();
    assert_ne!(name_etag, default_etag);
    let (_, headers, _) = get(&app, "/api/v1/strains?genetik=Sativa,Indica").await;
    let genetik_etag = headers[header::ETAG].to_str().unwrap().to_owned();
    let (_, headers, _) = get(&app, "/api/v1/strains?genetik=indica,SATIVA").await;
    assert_eq!(
        headers[header::ETAG],
        genetik_etag,
        "normalised genetik set"
    );
    // Ratings were scraped: the ETag carries the reviews version.
    assert!(default_etag.contains("-r"), "{default_etag}");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/strains?sort=name")
                .header(header::IF_NONE_MATCH, &name_etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(response.headers()[header::ETAG], name_etag);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/strains")
                .header(header::IF_NONE_MATCH, &name_etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "other query's ETag does not match"
    );
}
