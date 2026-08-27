//! End-to-end scrape runs against a wiremock site, persisted in a temporary database.

mod support;

use std::collections::BTreeSet;

use greenmedical_backend::db::{offers, runs};
use greenmedical_backend::domain::{RunStatus, RunTrigger};
use greenmedical_backend::scrape::run::scrape_now;
use greenmedical_backend::telemetry;
use sqlx::PgPool;
use support::{
    GENERIC_STRAIN, MockPharmacy, MockSite, MockTile, SESSION_COOKIE, default_site, test_state,
};
use wiremock::ResponseTemplate;

/// Regression for the session bug: the site stores the selected pharmacy in
/// the PHP session cookie set by the 302 on `deliveryTarget` requests. A
/// client without a cookie jar gets the generic catalogue for every pharmacy
/// (identical rows everywhere), which the old code happily stored as
/// `success`.
#[sqlx::test(migrations = "./migrations")]
async fn pharmacy_selection_survives_the_session_redirect(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    let stored = offers::for_run(&pool, run.id).await.unwrap();
    assert!(
        stored.iter().all(|o| o.name != GENERIC_STRAIN),
        "generic catalogue leaked into the run: {stored:?}"
    );
    let per_pharmacy: Vec<Vec<&str>> = ["Grüne Blüte", "Asavita"]
        .iter()
        .map(|pharmacy| {
            stored
                .iter()
                .filter(|o| o.pharmacy == *pharmacy)
                .map(|o| o.name.as_str())
                .collect()
        })
        .collect();
    assert_eq!(per_pharmacy[0], ["Bunatic", "OG Kush", "Cosmic Cream"]);
    assert_eq!(per_pharmacy[1], ["OG Kush"]);
    assert_ne!(per_pharmacy[0], per_pharmacy[1]);

    // Every flowers page was served from the pharmacy's own session.
    let flowers: Vec<_> = site
        .requests()
        .await
        .into_iter()
        .filter(|r| r.url.path() == "/de/cannabis/flowers")
        .collect();
    let (selects, pages): (Vec<_>, Vec<_>) = flowers
        .iter()
        .partition(|r| r.url.query_pairs().any(|(k, _)| k == "deliveryTarget"));
    assert_eq!(selects.len(), 3, "one select per page fetch");
    assert_eq!(pages.len(), 3, "one redirected page fetch per select");
    let sessions: BTreeSet<String> = pages
        .iter()
        .map(|r| {
            r.headers
                .get("cookie")
                .expect("redirected request carries the session cookie")
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        sessions,
        BTreeSet::from([
            format!("{SESSION_COOKIE}=sess-gr_ne_bl_te"),
            format!("{SESSION_COOKIE}=sess-asavita"),
        ])
    );
}

/// Each run gets a fresh cookie jar (`with create_session()` parity): the
/// first request of a run never carries the previous run's session.
#[sqlx::test(migrations = "./migrations")]
async fn each_run_starts_with_a_fresh_session(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());

    let first = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let second = scrape_now(&state, RunTrigger::Schedule).await.unwrap();
    assert_eq!(first.status, RunStatus::Success);
    assert_eq!(second.status, RunStatus::Success);
    assert_eq!(first.offer_count, Some(4));
    assert_eq!(second.offer_count, Some(4));

    let list_requests: Vec<_> = site
        .requests()
        .await
        .into_iter()
        .filter(|r| r.url.path() == "/de/cannabis/pharmacy/")
        .collect();
    assert_eq!(list_requests.len(), 2);
    for request in &list_requests {
        assert!(
            request.headers.get("cookie").is_none(),
            "session leaked into a new run: {:?}",
            request.headers.get("cookie")
        );
    }
    // Within a run the session is kept (the detail pages are fetched after the list).
    let detail_with_cookie = site
        .requests()
        .await
        .iter()
        .filter(|r| r.url.path().starts_with("/de/cannabis/pharmacy/") && r.url.path().len() > 22)
        .all(|r| r.headers.get("cookie").is_some());
    assert!(detail_with_cookie, "session cookie dropped within a run");
}

/// A run that ends `failed` via `decide_status` must be counted once, not
/// twice (`scrape_runs_total` and `scrape_duration_seconds` used to be
/// recorded in the success branch and again at the end).
#[sqlx::test(migrations = "./migrations")]
async fn failed_run_records_metrics_exactly_once(pool: PgPool) {
    let handle = telemetry::metrics_handle();
    let site = MockSite::start(default_site()).await;
    // No table at all → 0 pharmacies → `failed` after a successful scrape_site().
    site.mount_list(ResponseTemplate::new(200).set_body_string("<html><body></body></html>"))
        .await;
    let state = test_state(pool.clone(), &site.base_url());

    // The bootstrap trigger is used by no other test in this binary, so the
    // labelled counter is not shared with tests running in parallel.
    let run = scrape_now(&state, RunTrigger::Bootstrap).await.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert!(run.error.as_deref().unwrap().contains("no pharmacies"));

    let text = handle.render();
    let counted: Vec<f64> = text
        .lines()
        .filter(|l| {
            l.starts_with("scrape_runs_total{")
                && l.contains("status=\"failed\"")
                && l.contains("trigger=\"bootstrap\"")
        })
        .map(|l| l.rsplit(' ').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(counted, vec![1.0], "{text}");
    assert!(text.lines().any(|l| l == "scrape_in_progress 0"), "{text}");
}

#[sqlx::test(migrations = "./migrations")]
async fn successful_run_stores_offers_pharmacies_and_strains(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());

    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.trigger, RunTrigger::Manual);
    assert_eq!(run.instance.as_deref(), Some("test-instance"));
    assert_eq!(run.pharmacies_total, Some(2));
    assert_eq!(run.pharmacies_scraped, Some(2));
    assert_eq!(run.pharmacies_failed, Some(0));
    assert_eq!(run.offer_count, Some(4));
    // list + 2 details + 2 pages + 1 page
    assert_eq!(run.http_requests, Some(6));
    assert!(run.finished_at.is_some());
    assert_eq!(run.error, None);

    let stored = offers::for_run(&pool, run.id).await.unwrap();
    assert_eq!(stored.len(), 4);
    // Scrape order is preserved.
    let names: Vec<_> = stored.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Bunatic", "OG Kush", "Cosmic Cream", "OG Kush"]);
    assert_eq!(stored[0].pharmacy, "Grüne Blüte");
    assert_eq!(stored[0].pharmacy_postal_code, "04416");
    assert_eq!(stored[0].pharmacy_city, "Markkleeberg");
    assert_eq!(stored[0].price_per_gram, "5,49 €/g");
    assert_eq!(stored[0].price_eur_per_gram, Some(5.49));
    assert_eq!(stored[0].thc_value, Some(27.0));
    assert_eq!(stored[0].price_eur_per_thc_gram, Some(20.33));
    assert!(stored[0].product_url.contains(
        "deliveryTarget=cGhhcm1hY3k6fDpiNGJkZGNjNS1kYzQxLTQ5ZDgtODdkZi0xNGEwM2Q1NjFiMzI%3D"
    ));
    assert!(
        !stored[0].product_url.contains('#'),
        "fragment must be stripped"
    );
    // "<1%" is parsed to 0.99.
    assert_eq!(stored[2].cbd, "<1%");
    assert_eq!(stored[2].cbd_value, Some(0.99));
    // The shared strain has the same id at both pharmacies.
    assert_eq!(stored[1].strain_id, stored[3].strain_id);
    assert_ne!(stored[1].pharmacy_id, stored[3].pharmacy_id);
    assert_eq!(stored[3].availability, "NEU");

    let latest = runs::latest_usable(&pool).await.unwrap().unwrap();
    assert_eq!(latest.id, run.id);
    let snapshot = state.snapshot.get_or_load(&pool).await.unwrap().unwrap();
    assert_eq!(snapshot.run.id, run.id);
    assert_eq!(snapshot.strains.len(), 3);
    assert_eq!(snapshot.metadata.total, 4);
    assert_eq!(snapshot.metadata.pharmacy_count, 2);
    assert_eq!(snapshot.metadata.lowest_price, Some(5.49));
}

#[sqlx::test(migrations = "./migrations")]
async fn pharmacy_failure_yields_partial_run_with_error_rows(pool: PgPool) {
    let mut pharmacies = default_site();
    pharmacies.push(
        MockPharmacy::new(
            "Kaputt",
            "99999999-0000-0000-0000-000000000000",
            vec![vec![]],
        )
        .detail_status(500),
    );
    pharmacies.push(MockPharmacy::new("Ohne UUID", "unused", vec![vec![]]).without_uuid());
    let site = MockSite::start(pharmacies).await;
    let state = test_state(pool.clone(), &site.base_url());

    let run = scrape_now(&state, RunTrigger::Schedule).await.unwrap();
    assert_eq!(run.status, RunStatus::Partial);
    assert_eq!(run.pharmacies_total, Some(4));
    assert_eq!(run.pharmacies_scraped, Some(2));
    // "Ohne UUID" is skipped like in the Python scraper, not counted as failed.
    assert_eq!(run.pharmacies_failed, Some(1));
    assert_eq!(run.offer_count, Some(4));
    // 1 list + 2 ok details + 5 attempts for the 500 detail + 1 detail without uuid + 3 pages
    assert_eq!(run.http_requests, Some(12));

    let errors = runs::errors(&pool, run.id).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].pharmacy_name, "Kaputt");
    assert_eq!(errors[0].stage, "uuid");
    assert!(
        errors[0].message.contains("HTTP 500"),
        "{}",
        errors[0].message
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn pages_failure_is_recorded_with_pages_stage(pool: PgPool) {
    let mut pharmacies = default_site();
    pharmacies.push(
        MockPharmacy::new(
            "Seitenfehler",
            "77777777-0000-0000-0000-000000000000",
            vec![vec![]],
        )
        .pages_status(502),
    );
    let site = MockSite::start(pharmacies).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Partial);
    let errors = runs::errors(&pool, run.id).await.unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].stage, "pages");
    assert!(
        errors[0].message.starts_with("HTTP 502"),
        "{}",
        errors[0].message
    );
    assert!(
        !errors[0].message.contains("pharmacy list"),
        "{}",
        errors[0].message
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn unreachable_pharmacy_list_fails_the_run(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    site.mount_list(ResponseTemplate::new(500)).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert!(
        run.error.as_deref().unwrap().contains("pharmacy list"),
        "{:?}",
        run.error
    );
    assert_eq!(run.http_requests, Some(5));
    assert_eq!(run.offer_count, Some(0));
    assert!(runs::latest_usable(&pool).await.unwrap().is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn low_success_ratio_fails_the_run_and_stores_no_offers(pool: PgPool) {
    let mut pharmacies = default_site();
    for i in 0..3 {
        pharmacies.push(
            MockPharmacy::new(
                &format!("Down {i}"),
                &format!("aaaaaaa{i}-0000-0000-0000-000000000000"),
                vec![vec![]],
            )
            .detail_status(503),
        );
    }
    let site = MockSite::start(pharmacies).await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert!(
        run.error.as_deref().unwrap().contains("success ratio"),
        "{:?}",
        run.error
    );
    assert_eq!(run.pharmacies_scraped, Some(2));
    assert_eq!(run.pharmacies_failed, Some(3));
    assert_eq!(run.offer_count, Some(0));
    assert!(offers::for_run(&pool, run.id).await.unwrap().is_empty());
    assert_eq!(runs::errors(&pool, run.id).await.unwrap().len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn zero_offers_guard_keeps_previous_run_latest(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let first = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(first.status, RunStatus::Success);

    // Same pharmacies, but every flowers page is empty now (layout change).
    let mut empty = default_site();
    for pharmacy in &mut empty {
        pharmacy.pages = vec![vec![]];
    }
    let empty_site = MockSite::start(empty).await;
    let state2 = test_state(pool.clone(), &empty_site.base_url());
    let second = scrape_now(&state2, RunTrigger::Schedule).await.unwrap();
    assert_eq!(second.status, RunStatus::Failed);
    assert!(
        second.error.as_deref().unwrap().contains("no offers"),
        "{:?}",
        second.error
    );

    let latest = runs::latest_usable(&pool).await.unwrap().unwrap();
    assert_eq!(latest.id, first.id);
    let snapshot = state2.snapshot.get_or_load(&pool).await.unwrap().unwrap();
    assert_eq!(snapshot.run.id, first.id);
    assert_eq!(snapshot.offers.len(), 4);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_tile_in_one_pharmacy_yields_two_offers(pool: PgPool) {
    let site = MockSite::start(vec![MockPharmacy::new(
        "Doppelt",
        "12121212-0000-0000-0000-000000000000",
        vec![vec![
            MockTile::new("Amnesia Haze", "ARX 25/1", "7,49 €/g").thc("25%"),
            MockTile::new("Amnesia Haze", "ARX 25/1", "8,49 €/g").thc("28%"),
        ]],
    )])
    .await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    assert_eq!(run.offer_count, Some(2));
    let stored = offers::for_run(&pool, run.id).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].strain_id, stored[1].strain_id);
    assert_eq!(stored[0].pharmacy_id, stored[1].pharmacy_id);
    assert_eq!(stored[0].thc, "25%");
    assert_eq!(stored[1].thc, "28%");
    let snapshot = state.snapshot.get_or_load(&pool).await.unwrap().unwrap();
    assert_eq!(snapshot.strains.len(), 1);
    assert_eq!(snapshot.strains[0].offers.len(), 2);
    assert_eq!(snapshot.strains[0].pharmacy_count, 1);
    assert_eq!(snapshot.strains[0].min_price, Some(7.49));
}

#[sqlx::test(migrations = "./migrations")]
async fn second_run_reuses_pharmacy_and_strain_ids(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    let first = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    let second = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_ne!(first.id, second.id);
    let a = offers::for_run(&pool, first.id).await.unwrap();
    let b = offers::for_run(&pool, second.id).await.unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.strain_id, y.strain_id);
        assert_eq!(x.pharmacy_id, y.pharmacy_id);
        assert_ne!(x.offer_id, y.offer_id);
    }
    let pharmacy_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pharmacies")
        .fetch_one(&pool)
        .await
        .unwrap();
    let strain_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM strains")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pharmacy_count, 2);
    assert_eq!(strain_count, 3);
    let latest = runs::latest_usable(&pool).await.unwrap().unwrap();
    assert_eq!(latest.id, second.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_price_is_stored_as_null(pool: PgPool) {
    let site = MockSite::start(vec![MockPharmacy::new(
        "Preislos",
        "34343434-0000-0000-0000-000000000000",
        vec![vec![
            MockTile::new("Ohne Preis", "OP 1", ""),
            MockTile::new("Mit Preis", "MP 1", "9,00 €/g"),
        ]],
    )])
    .await;
    let state = test_state(pool.clone(), &site.base_url());
    let run = scrape_now(&state, RunTrigger::Manual).await.unwrap();
    assert_eq!(run.status, RunStatus::Success);
    let stored = offers::for_run(&pool, run.id).await.unwrap();
    assert_eq!(stored[0].price_per_gram, "");
    assert_eq!(stored[0].price_eur_per_gram, None);
    assert_eq!(stored[0].price_eur_per_thc_gram, None);
    assert_eq!(stored[1].price_eur_per_gram, Some(9.0));
    let snapshot = state.snapshot.get_or_load(&pool).await.unwrap().unwrap();
    // Unpriced strain sorts by key ("mit preis" < "ohne preis") and has null price.
    let ohne = snapshot
        .strains
        .iter()
        .find(|s| s.name == "Ohne Preis")
        .unwrap();
    assert_eq!(ohne.min_price, None);
    assert_eq!(snapshot.metadata.lowest_price, Some(9.0));
}

#[sqlx::test(migrations = "./migrations")]
async fn cancelled_run_is_marked_failed_with_shutdown(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    site.mount_list(
        ResponseTemplate::new(200)
            .set_body_string("<html></html>")
            .set_delay(std::time::Duration::from_secs(5)),
    )
    .await;
    let state = test_state(pool.clone(), &site.base_url());
    let handle = greenmedical_backend::scrape::start_run(&state, RunTrigger::Manual)
        .await
        .unwrap();
    let run_id = handle.run_id;
    let worker = tokio::spawn(greenmedical_backend::scrape::execute_run(
        state.clone(),
        handle,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    state.shutdown.cancel();
    let run = worker.await.unwrap().unwrap();
    assert_eq!(run.id, run_id);
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.error.as_deref(), Some("shutdown"));
    assert!(run.finished_at.is_some());
}
