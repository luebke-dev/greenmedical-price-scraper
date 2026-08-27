//! Scheduler behaviour that needs a database: advisory lock, stale cleanup, skip decisions.

mod support;

use chrono::{Duration, Utc};
use greenmedical_backend::db::runs;
use greenmedical_backend::domain::{RunStatus, RunTrigger};
use greenmedical_backend::scheduler::{Attempt, cleanup_stale_runs, should_bootstrap, try_run};
use greenmedical_backend::scrape::run::try_acquire_lock;
use sqlx::PgPool;
use support::{MockSite, default_site, test_state};

#[sqlx::test(migrations = "./migrations")]
async fn advisory_lock_is_exclusive_across_connections(pool: PgPool) {
    let first = try_acquire_lock(&pool).await.unwrap();
    assert!(first.is_some(), "first acquisition succeeds");
    let second = try_acquire_lock(&pool).await.unwrap();
    assert!(second.is_none(), "second connection cannot take the lock");

    first.unwrap().release_now().await.unwrap();
    let third = try_acquire_lock(&pool).await.unwrap();
    assert!(third.is_some(), "lock is free again after release");
}

#[sqlx::test(migrations = "./migrations")]
async fn dropping_the_guard_releases_the_lock(pool: PgPool) {
    {
        let _guard = try_acquire_lock(&pool).await.unwrap().unwrap();
        assert!(try_acquire_lock(&pool).await.unwrap().is_none());
    }
    // The connection returns to the pool with the unlock queued; give it a moment.
    let mut acquired = false;
    for _ in 0..50 {
        if try_acquire_lock(&pool).await.unwrap().is_some() {
            acquired = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(acquired);
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_running_runs_are_marked_failed(pool: PgPool) {
    let stale = runs::insert_running_at(
        &pool,
        RunTrigger::Schedule,
        "old",
        Utc::now() - Duration::hours(3),
    )
    .await
    .unwrap();
    let fresh = runs::insert_running_at(
        &pool,
        RunTrigger::Schedule,
        "new",
        Utc::now() - Duration::minutes(5),
    )
    .await
    .unwrap();
    let state = test_state(pool.clone(), "http://127.0.0.1:1");
    cleanup_stale_runs(&state).await;

    let stale_run = runs::get(&pool, stale).await.unwrap().unwrap();
    assert_eq!(stale_run.status, RunStatus::Failed);
    assert_eq!(stale_run.error.as_deref(), Some("stale"));
    assert!(stale_run.finished_at.is_some());
    let fresh_run = runs::get(&pool, fresh).await.unwrap().unwrap();
    assert_eq!(fresh_run.status, RunStatus::Running);

    let changed = runs::mark_stale(&pool, std::time::Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(changed, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn bootstrap_decision_uses_latest_usable_run(pool: PgPool) {
    let max_age = std::time::Duration::from_secs(8 * 3600);
    assert!(should_bootstrap(
        runs::latest_usable(&pool)
            .await
            .unwrap()
            .map(|r| r.started_at),
        Utc::now(),
        max_age
    ));

    let old = runs::insert_running_at(
        &pool,
        RunTrigger::Schedule,
        "x",
        Utc::now() - Duration::hours(9),
    )
    .await
    .unwrap();
    runs::finish(
        &pool,
        old,
        RunStatus::Success,
        runs::RunCounts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(should_bootstrap(
        runs::latest_usable(&pool)
            .await
            .unwrap()
            .map(|r| r.started_at),
        Utc::now(),
        max_age
    ));

    let failed = runs::insert_running_at(
        &pool,
        RunTrigger::Schedule,
        "x",
        Utc::now() - Duration::hours(1),
    )
    .await
    .unwrap();
    runs::mark_failed(&pool, failed, "x").await.unwrap();
    assert!(
        should_bootstrap(
            runs::latest_usable(&pool)
                .await
                .unwrap()
                .map(|r| r.started_at),
            Utc::now(),
            max_age
        ),
        "failed runs do not count"
    );

    let recent = runs::insert_running_at(
        &pool,
        RunTrigger::Schedule,
        "x",
        Utc::now() - Duration::hours(1),
    )
    .await
    .unwrap();
    runs::finish(
        &pool,
        recent,
        RunStatus::Partial,
        runs::RunCounts::default(),
        None,
    )
    .await
    .unwrap();
    assert!(!should_bootstrap(
        runs::latest_usable(&pool)
            .await
            .unwrap()
            .map(|r| r.started_at),
        Utc::now(),
        max_age
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn try_run_skips_when_lock_is_held_elsewhere(pool: PgPool) {
    let state = test_state(pool.clone(), "http://127.0.0.1:1");
    let guard = try_acquire_lock(&pool).await.unwrap().unwrap();
    assert_eq!(
        try_run(&state, RunTrigger::Schedule).await,
        Attempt::SkippedLockHeld
    );
    guard.release_now().await.unwrap();
    let (_, total) = runs::list(&pool, 10, 0, None).await.unwrap();
    assert_eq!(total, 0, "no run row is created when skipped");
}

#[sqlx::test(migrations = "./migrations")]
async fn try_run_executes_a_schedule_run(pool: PgPool) {
    let site = MockSite::start(default_site()).await;
    let state = test_state(pool.clone(), &site.base_url());
    assert_eq!(try_run(&state, RunTrigger::Schedule).await, Attempt::Ran);
    let latest = runs::latest_usable(&pool).await.unwrap().unwrap();
    assert_eq!(latest.trigger, RunTrigger::Schedule);
    assert_eq!(latest.status, RunStatus::Success);
}
