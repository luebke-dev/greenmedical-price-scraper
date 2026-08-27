//! GreenMedical price scraper backend: scheduler, scraper, PostgreSQL storage and JSON API.

pub mod api;
pub mod config;
pub mod db;
pub mod domain;
pub mod mail;
pub mod notify;
pub mod scheduler;
pub mod scrape;
pub mod shutdown;
pub mod state;
pub mod telemetry;

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tracing::{error, info, warn};

use crate::config::{Cli, Command, Config};
use crate::domain::RunTrigger;
use crate::state::{AppState, SharedState};

/// Embedded migrations (`backend/migrations`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Connect the pool according to the configuration.
pub async fn connect_pool(config: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections.max(4))
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await
        .context("connecting to PostgreSQL")?;
    Ok(pool)
}

/// Apply pending migrations (sqlx takes its own advisory lock).
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await.context("running migrations")?;
    info!("database migrations applied");
    Ok(())
}

/// CLI entry point.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let command = cli.command.unwrap_or(Command::Serve);
    match command {
        Command::Serve => serve(cli.config).await,
        Command::ScrapeOnce { reviews_only } => scrape_once(cli.config, reviews_only).await,
        Command::Migrate => {
            let pool = connect_pool(&cli.config).await?;
            migrate(&pool).await
        }
    }
}

/// Run one scrape and exit with an error when the run did not succeed.
///
/// With `reviews_only` only phase 2 runs: every strain of the latest usable
/// run is refreshed regardless of `REVIEWS_MAX_AGE`.
pub async fn scrape_once(config: Config, reviews_only: bool) -> anyhow::Result<()> {
    let pool = connect_pool(&config).await?;
    if config.migrate_on_startup {
        migrate(&pool).await?;
    }
    let shutdown = shutdown::install_signal_handler();
    let state = AppState::try_new(config, pool, shutdown)?;
    scheduler::cleanup_stale_runs(&state).await;
    if reviews_only {
        let outcome = scrape::run::scrape_reviews_only(&state).await?;
        info!(
            run_id = outcome.run_id,
            scraped = outcome.scraped,
            failed = outcome.failed,
            "scrape-once --reviews-only finished"
        );
        return Ok(());
    }
    let run = scrape::run::scrape_now(&state, RunTrigger::Manual).await?;
    info!(
        run_id = run.id,
        status = run.status.as_str(),
        offers = run.offer_count,
        "scrape-once finished"
    );
    if run.status == domain::RunStatus::Failed {
        anyhow::bail!(
            "scrape run {} failed: {}",
            run.id,
            run.error.unwrap_or_default()
        );
    }
    Ok(())
}

async fn pool_metrics_task(state: SharedState) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                metrics::gauge!("db_pool_connections").set(f64::from(state.pool.size()));
            }
            _ = state.shutdown.cancelled() => return,
        }
    }
}

/// Serve the API, metrics endpoint and scheduler until SIGTERM/SIGINT.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let metrics_handle = telemetry::metrics_handle();
    let pool = connect_pool(&config).await?;
    if config.migrate_on_startup {
        migrate(&pool).await?;
    }
    let shutdown = shutdown::install_signal_handler();
    let state = AppState::try_new(config, pool, shutdown.clone())?;
    if state.config.email_enabled {
        info!(
            host = ?state.config.smtp_host,
            port = state.config.smtp_port,
            tls = ?state.config.smtp_tls,
            "e-mail delivery via SMTP"
        );
    } else {
        info!("e-mail delivery and subscription creation disabled (EMAIL_ENABLED=false)");
    }
    notify::refresh_gauge(&state).await;

    // Warm the snapshot cache; a missing run is fine.
    match state.snapshot.get_or_load(&state.pool).await {
        Ok(Some(snapshot)) => {
            info!(
                run_id = snapshot.run.id,
                offers = snapshot.offers.len(),
                "snapshot loaded"
            );
            // Restore the last-success gauges from the database so a restart
            // does not report "never succeeded" until the next run.
            if let Some(finished) = snapshot.run.finished_at {
                metrics::gauge!("scrape_last_success_timestamp_seconds")
                    .set(finished.timestamp() as f64);
            }
            metrics::gauge!("scrape_last_run_offers").set(snapshot.offers.len() as f64);
        }
        Ok(None) => info!("no usable scrape run yet"),
        Err(err) => warn!(%err, "could not warm the snapshot cache"),
    }

    let api_listener = tokio::net::TcpListener::bind(state.config.http_bind)
        .await
        .with_context(|| format!("binding {}", state.config.http_bind))?;
    let metrics_listener = tokio::net::TcpListener::bind(state.config.metrics_bind)
        .await
        .with_context(|| format!("binding {}", state.config.metrics_bind))?;
    info!(http = %state.config.http_bind, metrics = %state.config.metrics_bind, instance = %state.instance, "listening");

    // Connect info provides the peer address for the subscription rate limit
    // (used when no `X-Forwarded-For` header is present).
    let api_server = axum::serve(
        api_listener,
        api::build_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown.clone().cancelled_owned());
    let metrics_server = axum::serve(metrics_listener, api::metrics_router(metrics_handle))
        .with_graceful_shutdown(shutdown.clone().cancelled_owned());

    let scheduler_task = if state.config.scrape_enabled {
        Some(tokio::spawn(scheduler::run_scheduler(state.clone())))
    } else {
        info!("scheduler disabled (SCRAPE_ENABLED=false)");
        None
    };
    let pool_task = tokio::spawn(pool_metrics_task(state.clone()));

    state.ready.store(true, Ordering::SeqCst);

    let (api_result, metrics_result) = tokio::join!(api_server, metrics_server);
    state.ready.store(false, Ordering::SeqCst);
    if let Err(err) = api_result {
        error!(%err, "api server error");
    }
    if let Err(err) = metrics_result {
        error!(%err, "metrics server error");
    }
    shutdown.cancel();
    if let Some(task) = scheduler_task {
        let _ = task.await;
    }
    let _ = pool_task.await;
    // Manual (admin-triggered) runs: let them observe the cancellation and
    // mark their row `failed` while the pool is still open.
    state.tasks.close();
    state.tasks.wait().await;
    state.pool.close().await;
    info!("shutdown complete");
    Ok(())
}
