//! Shared application state.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sqlx::PgPool;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::api::rate_limit::RateLimiter;
use crate::config::Config;
use crate::db::snapshot::SnapshotCache;
use crate::mail::{Mailer, mailer_from_config};

/// Process-wide state shared by the API, the scheduler and scrape runs.
///
/// There is deliberately no shared HTTP client here: every scrape run builds
/// its own [`crate::scrape::client::ScrapeClient`] so the site's session
/// cookie (which stores the selected pharmacy) never leaks between runs.
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub snapshot: SnapshotCache,
    /// In-process gate: only one scrape per process.
    pub scrape_gate: Arc<Mutex<()>>,
    /// `false` until startup finished and again once shutdown started.
    pub ready: AtomicBool,
    pub shutdown: CancellationToken,
    /// Background work (manual scrape runs) that `serve()` waits for before
    /// closing the pool, so a run cancelled by SIGTERM can still be marked failed.
    pub tasks: TaskTracker,
    pub instance: String,
    /// Outbound e-mail (log-only unless `EMAIL_ENABLED=true`).
    pub mailer: Arc<dyn Mailer>,
    /// Per-IP limit of `POST /api/v1/subscriptions`.
    pub rate_limiter: RateLimiter,
}

pub type SharedState = Arc<AppState>;

impl AppState {
    /// State with the mailer derived from the configuration; panics on an
    /// unusable SMTP configuration (use [`Self::try_new`] to handle it).
    pub fn new(config: Config, pool: PgPool, shutdown: CancellationToken) -> SharedState {
        Self::try_new(config, pool, shutdown).expect("valid e-mail configuration")
    }

    pub fn try_new(
        config: Config,
        pool: PgPool,
        shutdown: CancellationToken,
    ) -> anyhow::Result<SharedState> {
        let mailer = mailer_from_config(&config)?;
        Ok(Self::with_mailer(config, pool, shutdown, mailer))
    }

    /// State with an explicit mailer (tests).
    pub fn with_mailer(
        config: Config,
        pool: PgPool,
        shutdown: CancellationToken,
        mailer: Arc<dyn Mailer>,
    ) -> SharedState {
        let instance = config.instance_name();
        let snapshot = SnapshotCache::new(config.snapshot_revalidate_interval);
        let rate_limiter = RateLimiter::new(config.subscription_rate_limit);
        Arc::new(Self {
            config: Arc::new(config),
            pool,
            snapshot,
            scrape_gate: Arc::new(Mutex::new(())),
            ready: AtomicBool::new(false),
            shutdown,
            tasks: TaskTracker::new(),
            instance,
            mailer,
            rate_limiter,
        })
    }
}
