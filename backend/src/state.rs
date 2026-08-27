//! Shared application state.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sqlx::PgPool;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::db::snapshot::SnapshotCache;

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
}

pub type SharedState = Arc<AppState>;

impl AppState {
    pub fn new(config: Config, pool: PgPool, shutdown: CancellationToken) -> SharedState {
        let instance = config.instance_name();
        let snapshot = SnapshotCache::new(config.snapshot_revalidate_interval);
        Arc::new(Self {
            config: Arc::new(config),
            pool,
            snapshot,
            scrape_gate: Arc::new(Mutex::new(())),
            ready: AtomicBool::new(false),
            shutdown,
            tasks: TaskTracker::new(),
            instance,
        })
    }
}
