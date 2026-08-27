//! Price-alert subscriptions: API lifecycle, validation, rate limit, evaluation, cleanup.

mod support;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use chrono::{Duration, Utc};
use greenmedical_backend::api::build_router;
use greenmedical_backend::db::subscriptions as subs;
use greenmedical_backend::domain::{RunStatus, RunTrigger};
use greenmedical_backend::notify;
use greenmedical_backend::scrape::run::scrape_now;
use greenmedical_backend::state::SharedState;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    MockPharmacy, MockSite, MockTile, SeedOffer, seed_run, test_config, test_state_with_mailer,
};
use tower::ServiceExt;

const APO_A: (&str, &str) = ("aaaa-1", "Apo A");
const APO_B: (&str, &str) = ("bbbb-2", "Apo B");
const SORTE_X: (&str, &str) = ("Sorte X", "EMK");
const SORTE_Y: (&str, &str) = ("Sorte Y", "XYZ");
const SORTE_Z: (&str, &str) = ("Sorte Z", "ZZZ");

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    ip: Option<&str>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(ip) = ip {
        builder = builder.header("x-forwarded-for", ip);
    }
    let request = match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, headers, value)
}

async fn post(app: &Router, uri: &str, body: Value) -> (StatusCode, HeaderMap, Value) {
    call(app, Method::POST, uri, Some(body), None).await
}

async fn strain_id(pool: &PgPool, name: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM strains WHERE name = $1")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn manage_token(pool: &PgPool, email: &str) -> String {
    subs::find_by_email(pool, email)
        .await
        .unwrap()
        .expect("subscriber exists")
        .manage_token
}

fn token_from(text: &str, param: &str) -> String {
    let start = text.find(&format!("{param}=")).expect("token link") + param.len() + 1;
    text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

async fn seed_first_run(pool: &PgPool) -> i64 {
    seed_run(
        pool,
        Utc::now() - Duration::hours(6),
        RunStatus::Success,
        &[
            SeedOffer::new(APO_A, SORTE_X, 6.49),
            SeedOffer::new(APO_B, SORTE_X, 6.99),
            SeedOffer::new(APO_A, SORTE_Y, 5.00),
        ],
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
async fn create_confirm_manage_and_unsubscribe(pool: PgPool) {
    seed_first_run(&pool).await;
    let x = strain_id(&pool, "Sorte X").await;
    let (state, mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));
    let app = build_router(state.clone());

    let (status, headers, body) = post(
        &app,
        "/api/v1/subscriptions",
        json!({
            "email": "Max@Example.org",
            "rules": [
                {"kind": "strain_price_below", "strain_id": x, "threshold": 6},
                {"kind": "new_strain"}
            ]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(body, json!({"status": "confirmation_sent"}));

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    let mail = &sent[0];
    assert_eq!(mail.to, "Max@Example.org");
    assert_eq!(mail.subject, "Bitte bestätige deinen Preisalarm");
    assert!(
        mail.text
            .contains("http://localhost:9000/abo/bestaetigen?token="),
        "{}",
        mail.text
    );
    let confirm = token_from(&mail.text, "token");
    assert_eq!(confirm.len(), 43);

    // Unknown and empty tokens are 404 (enveloped, no-store).
    let (status, headers, body) = post(
        &app,
        "/api/v1/subscriptions/confirm",
        json!({"token": "nope"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(body["error"]["code"], "not_found");
    let (status, _, _) = post(&app, "/api/v1/subscriptions/confirm", json!({"token": ""})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _, body) = post(
        &app,
        "/api/v1/subscriptions/confirm",
        json!({"token": confirm}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["email"], "Max@Example.org");
    assert_eq!(body["confirmed"], true);
    let rules = body["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0]["kind"], "strain_price_below");
    assert_eq!(rules[0]["strain_id"], x);
    assert_eq!(rules[0]["threshold"], 6.0);
    assert_eq!(rules[0]["strain_name"], "Sorte X");
    assert!(rules[0]["created_at"].is_string());
    assert_eq!(rules[1]["kind"], "new_strain");
    assert!(rules[1].get("strain_id").is_none());
    assert!(rules[1].get("threshold").is_none());
    assert_eq!(rules[1]["strain_name"], Value::Null);
    // Confirming twice is idempotent.
    let (status, _, _) = post(
        &app,
        "/api/v1/subscriptions/confirm",
        json!({"token": confirm}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let manage = manage_token(&pool, "max@example.org").await;
    let uri = format!("/api/v1/subscriptions/manage?token={manage}");
    let (status, headers, body) = call(&app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(body["rules"].as_array().unwrap().len(), 2);
    let (status, _, body) = call(
        &app,
        Method::GET,
        "/api/v1/subscriptions/manage?token=unknown",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    let (status, _, body) = call(
        &app,
        Method::GET,
        "/api/v1/subscriptions/manage",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // PUT replaces all rules.
    let (status, _, body) = call(
        &app,
        Method::PUT,
        &uri,
        Some(json!({"rules": [{"kind": "strain_price_change", "strain_id": x}]})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let rules = body["rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["kind"], "strain_price_change");
    assert_eq!(rules[0]["strain_name"], "Sorte X");
    let (status, _, body) = call(&app, Method::PUT, &uri, Some(json!({"rules": []})), None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "bad_request");
    let (status, _, _) = call(
        &app,
        Method::PUT,
        "/api/v1/subscriptions/manage?token=unknown",
        Some(json!({"rules": [{"kind": "new_strain"}]})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // DELETE unsubscribes; everything is gone afterwards.
    let (status, headers, _) = call(&app, Method::DELETE, &uri, None, None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let (status, _, _) = call(&app, Method::GET, &uri, None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = call(&app, Method::DELETE, &uri, None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        subs::find_by_email(&pool, "max@example.org")
            .await
            .unwrap()
            .is_none()
    );
    let rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscription_rules")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rules, 0);
    // Only the single confirmation mail was sent during the whole lifecycle.
    assert_eq!(mailer.sent().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_email_adds_rules_and_resends_confirmation_only_while_unconfirmed(pool: PgPool) {
    seed_first_run(&pool).await;
    let x = strain_id(&pool, "Sorte X").await;
    let y = strain_id(&pool, "Sorte Y").await;
    let (state, mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));
    let app = build_router(state);

    let create = |email: &str, rules: Value| {
        let app = app.clone();
        let email = email.to_owned();
        async move {
            post(
                &app,
                "/api/v1/subscriptions",
                json!({"email": email, "rules": rules}),
            )
            .await
            .0
        }
    };
    assert_eq!(
        create(
            "max@example.org",
            json!([{"kind": "strain_available", "strain_id": x}])
        )
        .await,
        StatusCode::ACCEPTED
    );
    // Same address in different case: rules are added, a duplicate rule is ignored.
    assert_eq!(
        create(
            "MAX@example.org",
            json!([
                {"kind": "strain_available", "strain_id": x},
                {"kind": "strain_available", "strain_id": y},
                {"kind": "any_price_below", "threshold": 5.5}
            ])
        )
        .await,
        StatusCode::ACCEPTED
    );
    let subscriber = subs::find_by_email(&pool, "max@example.org")
        .await
        .unwrap()
        .unwrap();
    let rules = subs::rules_for(&pool, subscriber.id).await.unwrap();
    assert_eq!(rules.len(), 3);
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscribers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);
    // Two confirmation mails while unconfirmed, both with the same token.
    let sent = mailer.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        token_from(&sent[0].text, "token"),
        token_from(&sent[1].text, "token")
    );

    subs::confirm(&pool, subscriber.id).await.unwrap();
    assert_eq!(
        create("max@example.org", json!([{"kind": "new_strain"}])).await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        subs::rules_for(&pool, subscriber.id).await.unwrap().len(),
        4
    );
    // Confirmed subscribers get no further confirmation mail.
    assert_eq!(mailer.sent().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn honeypot_is_accepted_without_action(pool: PgPool) {
    let (state, mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));
    let app = build_router(state);
    let (status, _, body) = post(
        &app,
        "/api/v1/subscriptions",
        json!({"email": "bot@example.org", "rules": [{"kind": "new_strain"}], "website": "http://spam"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["status"], "confirmation_sent");
    assert!(mailer.sent().is_empty());
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscribers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 0);
    // An empty honeypot is fine.
    let (status, _, _) = post(
        &app,
        "/api/v1/subscriptions",
        json!({"email": "human@example.org", "rules": [{"kind": "new_strain"}], "website": ""}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(mailer.sent().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn rate_limit_is_enforced_per_client_ip(pool: PgPool) {
    let mut config = test_config("http://127.0.0.1:1");
    config.subscription_rate_limit = "2/1h".parse().unwrap();
    let (state, mailer) = test_state_with_mailer(pool, config);
    let app = build_router(state);
    let body =
        |i: usize| json!({"email": format!("u{i}@example.org"), "rules": [{"kind": "new_strain"}]});
    for i in 0..2 {
        let (status, _, _) = call(
            &app,
            Method::POST,
            "/api/v1/subscriptions",
            Some(body(i)),
            Some("203.0.113.9, 10.0.0.1"),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
    }
    let (status, headers, value) = call(
        &app,
        Method::POST,
        "/api/v1/subscriptions",
        Some(body(2)),
        Some("203.0.113.9"),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(value["error"]["code"], "bad_request");
    assert_eq!(mailer.sent().len(), 2);
    // Another client is not affected; invalid requests do not count.
    let (status, _, _) = call(
        &app,
        Method::POST,
        "/api/v1/subscriptions",
        Some(json!({"email": "nope", "rules": []})),
        Some("198.51.100.1"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = call(
        &app,
        Method::POST,
        "/api/v1/subscriptions",
        Some(body(3)),
        Some("198.51.100.1"),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_validates_email_and_rules(pool: PgPool) {
    seed_first_run(&pool).await;
    let x = strain_id(&pool, "Sorte X").await;
    let (state, mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));
    let app = build_router(state);
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"email": "not-an-email", "rules": [{"kind": "new_strain"}]}),
            "email",
        ),
        (json!({"email": "a@b.test", "rules": []}), "rules"),
        (
            json!({"email": "a@b.test", "rules": (0..21).map(|_| json!({"kind": "new_strain"})).collect::<Vec<_>>()}),
            "rules",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "strain_available"}]}),
            "strain_id",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "strain_available", "strain_id": 999999}]}),
            "nicht gefunden",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "new_strain", "strain_id": x}]}),
            "strain_id",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "strain_price_below", "strain_id": x}]}),
            "threshold",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "strain_price_below", "strain_id": x, "threshold": -1}]}),
            "threshold",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "any_price_below", "threshold": 0}]}),
            "threshold",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "any_price_below", "threshold": 5, "strain_id": x}]}),
            "strain_id",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "thc_above", "threshold": 120}]}),
            "100",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "new_strain", "threshold": 1}]}),
            "threshold",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "strain_price_change"}]}),
            "strain_id",
        ),
        (
            json!({"email": "a@b.test", "rules": [{"kind": "unknown_kind"}]}),
            "JSON",
        ),
        (json!({"rules": [{"kind": "new_strain"}]}), "JSON"),
    ];
    for (body, expected) in cases {
        let (status, headers, value) = post(&app, "/api/v1/subscriptions", body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {value}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(value["error"]["code"], "bad_request", "{body}");
        let message = value["error"]["message"].as_str().unwrap();
        assert!(message.contains(expected), "{body}: {message}");
    }
    // Wrong content type / broken JSON are enveloped too.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/subscriptions")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(mailer.sent().is_empty());
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM subscribers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 0);

    // Thresholds are stored at cent precision.
    let (status, _, _) = post(
        &app,
        "/api/v1/subscriptions",
        json!({"email": "a@b.test", "rules": [{"kind": "strain_price_below", "strain_id": x, "threshold": 5.999}]}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let subscriber = subs::find_by_email(&pool, "a@b.test")
        .await
        .unwrap()
        .unwrap();
    let rules = subs::rules_for(&pool, subscriber.id).await.unwrap();
    assert_eq!(rules[0].threshold, Some(6.0));
}

/// Confirmed subscriber with `rules`, created directly in the database.
async fn confirmed_subscriber(pool: &PgPool, email: &str, rules: Value) -> i64 {
    let subscriber = subs::insert(
        pool,
        email,
        &format!("confirm-{email}"),
        &format!("manage-{email}"),
    )
    .await
    .unwrap();
    let rules: Vec<greenmedical_backend::domain::RuleInputDto> =
        serde_json::from_value(rules).unwrap();
    subs::add_rules(pool, subscriber.id, &rules).await.unwrap();
    subs::confirm(pool, subscriber.id).await.unwrap();
    subscriber.id
}

#[sqlx::test(migrations = "./migrations")]
async fn evaluation_stores_notifications_and_sends_one_digest_per_subscriber(pool: PgPool) {
    let run1 = seed_first_run(&pool).await;
    let x = strain_id(&pool, "Sorte X").await;
    let y = strain_id(&pool, "Sorte Y").await;
    let (state, mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));

    // The first run has no predecessor: nothing to compare.
    let outcome = notify::evaluate_run(&state, run1).await.unwrap();
    assert_eq!(outcome.digests, 0);

    let max = confirmed_subscriber(
        &pool,
        "max@example.org",
        json!([
            {"kind": "strain_price_below", "strain_id": x, "threshold": 6},
            {"kind": "new_strain"},
            {"kind": "any_price_below", "threshold": 5.5},
            {"kind": "strain_price_change", "strain_id": x},
            {"kind": "strain_price_change", "strain_id": y},
            {"kind": "thc_above", "threshold": 20}
        ]),
    )
    .await;
    // Quiet rules only: no mail.
    let quiet = confirmed_subscriber(
        &pool,
        "quiet@example.org",
        json!([{"kind": "strain_price_change", "strain_id": y}]),
    )
    .await;
    // Unconfirmed: never notified.
    let pending = subs::insert(&pool, "pending@example.org", "c-p", "m-p")
        .await
        .unwrap();
    let pending_rules: Vec<greenmedical_backend::domain::RuleInputDto> =
        serde_json::from_value(json!([{"kind": "new_strain"}])).unwrap();
    subs::add_rules(&pool, pending.id, &pending_rules)
        .await
        .unwrap();

    let run2 = seed_run(
        &pool,
        Utc::now() - Duration::hours(1),
        RunStatus::Partial,
        &[
            SeedOffer::new(APO_A, SORTE_X, 5.49),
            SeedOffer::new(APO_B, SORTE_X, 5.99),
            SeedOffer::new(APO_A, SORTE_Y, 5.00),
            SeedOffer::new(APO_B, SORTE_Z, 4.99).thc("25%"),
        ],
    )
    .await;
    let z = strain_id(&pool, "Sorte Z").await;

    let outcome = notify::evaluate_run(&state, run2).await.unwrap();
    assert_eq!(outcome.digests, 1);
    assert_eq!(outcome.notifications, 6);
    assert_eq!(outcome.failed, 0);

    let rows = subs::notifications_for_run(&pool, run2).await.unwrap();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|r| r.subscriber_id == max));
    assert!(
        rows.iter()
            .all(|r| r.sent_at.is_some() && r.error.is_none())
    );
    let mut kinds: Vec<(String, i64)> = rows
        .iter()
        .map(|r| {
            (
                r.payload["kind"].as_str().unwrap().to_owned(),
                r.strain_id.unwrap(),
            )
        })
        .collect();
    kinds.sort();
    assert_eq!(
        kinds,
        vec![
            ("any_price_below".to_owned(), x),
            ("any_price_below".to_owned(), z),
            ("new_strain".to_owned(), z),
            ("strain_price_below".to_owned(), x),
            ("strain_price_change".to_owned(), x),
            ("thc_above".to_owned(), z),
        ]
    );
    let below = rows
        .iter()
        .find(|r| r.payload["kind"] == "strain_price_below")
        .unwrap();
    assert_eq!(below.payload["price"], 5.49);
    assert_eq!(below.payload["previous_price"], 6.49);
    assert_eq!(below.payload["pharmacy"], "Apo A");
    assert_eq!(
        subs::get(&pool, max)
            .await
            .unwrap()
            .unwrap()
            .last_notified_run_id,
        Some(run2)
    );
    assert_eq!(
        subs::get(&pool, quiet)
            .await
            .unwrap()
            .unwrap()
            .last_notified_run_id,
        None
    );

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    let mail = &sent[0];
    assert_eq!(mail.to, "max@example.org");
    assert!(
        mail.subject.starts_with("Preisalarm: 6 Ereignisse ("),
        "{}",
        mail.subject
    );
    for expected in [
        "Preis der Sorte unter Schwellwert (6,00 €/g): Sorte X",
        "Neue Sorte\n",
        "Preis unter Schwellwert (5,50 €/g)",
        "Preisänderung: Sorte X",
        "Neue Sorte mit THC über Schwellwert (20,00%)",
        &format!("http://localhost:9000/sorte/{x}"),
        &format!("http://localhost:9000/sorte/{z}"),
        "5,49 €/g (vorher 6,49 €/g)",
        "Apotheke Apo B",
        "http://localhost:9000/abo/verwalten?token=manage-max@example.org",
    ] {
        assert!(
            mail.text.contains(expected),
            "missing {expected:?} in:\n{}",
            mail.text
        );
    }
    assert!(!mail.text.contains("Sorte Y"), "{}", mail.text);
    assert!(mail.html.contains("<h3>Neue Sorte</h3>"));

    // Re-evaluation: everything is deduplicated, no second mail.
    let outcome = notify::evaluate_run(&state, run2).await.unwrap();
    assert_eq!(outcome.digests, 0);
    assert_eq!(outcome.notifications, 0);
    assert_eq!(
        subs::notifications_for_run(&pool, run2)
            .await
            .unwrap()
            .len(),
        6
    );
    assert_eq!(mailer.sent().len(), 1);

    // A failed send is recorded on the rows and not retried.
    let run3 = seed_run(
        &pool,
        Utc::now() - Duration::minutes(10),
        RunStatus::Success,
        &[
            SeedOffer::new(APO_A, SORTE_X, 5.49),
            SeedOffer::new(APO_A, SORTE_Y, 4.50),
            SeedOffer::new(APO_B, SORTE_Z, 4.99).thc("25%"),
        ],
    )
    .await;
    mailer.fail_next();
    let outcome = notify::evaluate_run(&state, run3).await.unwrap();
    // max: any_price_below (Y 5.00 -> 4.50 crosses 5.5? no, was already below) and price change Y;
    // quiet: price change Y. Two digests, one of them failing.
    assert_eq!(outcome.digests, 2);
    assert_eq!(outcome.failed, 1);
    let rows = subs::notifications_for_run(&pool, run3).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.payload["kind"] == "strain_price_change")
    );
    let failed: Vec<_> = rows.iter().filter(|r| r.error.is_some()).collect();
    assert_eq!(failed.len(), 1);
    assert!(failed[0].sent_at.is_none());
    assert_eq!(mailer.sent().len(), 2);
    let again = notify::evaluate_run(&state, run3).await.unwrap();
    assert_eq!(again.digests, 0);
    assert_eq!(mailer.sent().len(), 2);
    // The pending subscriber never got anything.
    let pending_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE subscriber_id = $1")
            .bind(pending.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending_rows, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_scrape_run_triggers_the_evaluation(pool: PgPool) {
    // Previous run knows only Sorte X; the mock site lists a different strain.
    seed_run(
        &pool,
        Utc::now() - Duration::hours(6),
        RunStatus::Success,
        &[SeedOffer::new(APO_A, SORTE_X, 6.49)],
    )
    .await;
    confirmed_subscriber(
        &pool,
        "max@example.org",
        json!([{"kind": "new_strain"}, {"kind": "thc_above", "threshold": 20}]),
    )
    .await;
    let site = MockSite::start(vec![MockPharmacy::new(
        "Apo Live",
        "b4bddcc5-dc41-49d8-87df-14a03d561b32",
        vec![vec![
            MockTile::new("Live Sorte", "LS 22/1", "7,49 €/g").thc("22%"),
        ]],
    )])
    .await;
    let mut config = test_config(&site.base_url());
    config.reviews_enabled = false;
    let (state, mailer) = test_state_with_mailer(pool.clone(), config);
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    let rows = subs::notifications_for_run(&pool, run.id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.sent_at.is_some()));
    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].subject.starts_with("Preisalarm: 2 Ereignisse"));
    assert!(
        sent[0]
            .text
            .contains("Live Sorte (LS 22/1) – 7,49 €/g, THC 22 %, Apotheke Apo Live"),
        "{}",
        sent[0].text
    );
    // The strain endpoints are untouched by subscriptions.
    let app = build_router(state);
    let (status, _, body) = call(&app, Method::GET, "/api/v1/strains", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn unconfirmed_subscribers_are_cleaned_up_after_seven_days(pool: PgPool) {
    let (state, _mailer) = test_state_with_mailer(pool.clone(), test_config("http://127.0.0.1:1"));
    let old_pending = subs::insert(&pool, "old@example.org", "c1", "m1")
        .await
        .unwrap();
    let old_confirmed = subs::insert(&pool, "oldc@example.org", "c2", "m2")
        .await
        .unwrap();
    subs::confirm(&pool, old_confirmed.id).await.unwrap();
    let fresh = subs::insert(&pool, "fresh@example.org", "c3", "m3")
        .await
        .unwrap();
    sqlx::query("UPDATE subscribers SET created_at = now() - interval '8 days' WHERE id = ANY($1)")
        .bind(vec![old_pending.id, old_confirmed.id])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE subscribers SET created_at = now() - interval '6 days' WHERE id = $1")
        .bind(fresh.id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(notify::cleanup_unconfirmed(&state).await.unwrap(), 1);
    assert!(subs::get(&pool, old_pending.id).await.unwrap().is_none());
    assert!(subs::get(&pool, old_confirmed.id).await.unwrap().is_some());
    assert!(subs::get(&pool, fresh.id).await.unwrap().is_some());
    assert_eq!(subs::counts(&pool).await.unwrap(), (1, 1));
    greenmedical_backend::scheduler::cleanup_subscriptions(&state).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn method_not_allowed_on_subscription_paths_is_enveloped(pool: PgPool) {
    let (state, _mailer) = test_state_with_mailer(pool, test_config("http://127.0.0.1:1"));
    let app: Router = build_router(state);
    let (status, _, body) = call(&app, Method::GET, "/api/v1/subscriptions", None, None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{body}");
    assert_eq!(body["error"]["code"], "bad_request");
}

#[allow(dead_code)]
fn _state_type_check(_: SharedState) {}
