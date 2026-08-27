//! In-memory snapshot of the latest usable run with pre-serialised payloads.
//!
//! The cache is invalidated explicitly by the instance that performed a scrape
//! and, for every other replica, revalidated against the database at most
//! once per `SNAPSHOT_REVALIDATE_INTERVAL` with a single indexed query.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use axum::body::Bytes;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::db::{offers, runs, strains};
use crate::domain::{
    self, MetadataDto, OfferRecord, RunDto, RunStatus, StrainDto, StrainsResponseDto, compute_trend,
};

/// Number of days between the latest run and the trend reference run.
pub const TREND_REFERENCE_DAYS: i64 = 7;

/// Snapshots of explicitly requested runs (`?run_id=`) kept in memory.
pub const RUN_CACHE_CAPACITY: usize = 4;

/// Everything the read endpoints need for one run.
#[derive(Debug)]
pub struct Snapshot {
    pub run: RunDto,
    /// `max(strains.reviews_scraped_at)` when the snapshot was built (see `strains::reviews_version`).
    pub reviews_version: Option<chrono::DateTime<chrono::Utc>>,
    pub reference_run: Option<RunDto>,
    pub offers: Vec<OfferRecord>,
    pub strains: Vec<StrainDto>,
    pub metadata: MetadataDto,
    pub etag: String,
    pub metadata_json: Bytes,
    pub strains_json: Bytes,
    pub export_json: Bytes,
    pub csv: Bytes,
}

impl Snapshot {
    /// Load and pre-serialise all payloads for `run`.
    pub async fn build(pool: &PgPool, run: RunDto) -> sqlx::Result<Self> {
        let offers = offers::for_run(pool, run.id).await?;
        let mut strains = domain::group_by_strain(&offers);

        let reference_run = runs::reference_run(
            pool,
            run.started_at - chrono::Duration::days(TREND_REFERENCE_DAYS),
        )
        .await?;
        if let Some(reference) = &reference_run {
            let then = offers::min_prices_for_run(pool, reference.id).await?;
            for strain in &mut strains {
                strain.trend = compute_trend(
                    strain.min_price,
                    then.get(&strain.id).copied(),
                    Some(reference),
                );
            }
        }

        // Ratings live on `strains` (updated by phase 2), not on the run.
        let reviews_version = strains::reviews_version(pool).await?;
        let mut ratings = strains::ratings(pool).await?;
        for strain in &mut strains {
            if let Some(rating) = ratings.remove(&strain.id) {
                strain.sort.rating = rating.rating.as_ref().and_then(|r| r.value);
                strain.rating = rating.rating;
                strain.product_uuid = rating.product_uuid;
            }
        }

        let generated_at = run.finished_at.unwrap_or(run.started_at);
        let metadata = domain::build_metadata(&offers, &strains, generated_at, run.clone());
        let response = StrainsResponseDto {
            run: run.clone(),
            reference_run: reference_run.clone(),
            strains,
        };
        let strains_json = Bytes::from(serde_json::to_vec(&response).expect("serialisable"));
        let export_json = Bytes::from(serde_json::to_vec(&response.strains).expect("serialisable"));
        let metadata_json = Bytes::from(serde_json::to_vec(&metadata).expect("serialisable"));
        let csv = Bytes::from(domain::export::to_csv(&offers));

        Ok(Self {
            etag: match reviews_version {
                Some(version) => format!("\"run-{}-r{}\"", run.id, version.timestamp_millis()),
                None => format!("\"run-{}\"", run.id),
            },
            run,
            reviews_version,
            reference_run,
            offers,
            strains: response.strains,
            metadata,
            metadata_json,
            strains_json,
            export_json,
            csv,
        })
    }
}

/// Monotonic clock base for `checked_at` (milliseconds since first use).
fn now_millis() -> u64 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    u64::try_from(EPOCH.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// `ArcSwap`-backed cache of the latest usable run plus a tiny LRU of
/// explicitly requested (finished) runs for the export endpoints.
#[derive(Debug)]
pub struct SnapshotCache {
    current: ArcSwapOption<Snapshot>,
    /// When the cached run id was last confirmed to be the latest (see `now_millis`).
    checked_at: AtomicU64,
    /// Bumped by `invalidate()`; a build that started before an invalidation
    /// must not be stored (it may belong to the previous run).
    generation: AtomicU64,
    revalidate_interval: Duration,
    build_lock: Mutex<()>,
    /// Most recently used last. Only finished runs are cached (their offers never change).
    by_run: Mutex<VecDeque<Arc<Snapshot>>>,
}

impl Default for SnapshotCache {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl SnapshotCache {
    /// A cache that revalidates the latest run id at most once per `revalidate_interval`.
    pub fn new(revalidate_interval: Duration) -> Self {
        Self {
            current: ArcSwapOption::empty(),
            checked_at: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            revalidate_interval,
            build_lock: Mutex::new(()),
            by_run: Mutex::new(VecDeque::with_capacity(RUN_CACHE_CAPACITY)),
        }
    }

    /// `0` means "never checked" and always triggers a revalidation.
    fn mark_checked(&self) {
        self.checked_at
            .store(now_millis().max(1), Ordering::Release);
    }

    fn is_fresh(&self) -> bool {
        let checked = self.checked_at.load(Ordering::Acquire);
        if checked == 0 {
            return false;
        }
        let interval = u64::try_from(self.revalidate_interval.as_millis()).unwrap_or(u64::MAX);
        now_millis().saturating_sub(checked) < interval
    }

    /// Cached snapshot, or load the latest usable run from the database.
    /// Returns `None` when no usable run exists yet.
    ///
    /// A cached snapshot older than the revalidation interval is checked
    /// against the newest usable run id and the rating data version; when a
    /// newer run or newer reviews exist (scraped by another replica or a
    /// `scrape-once --reviews-only` process) the snapshot is rebuilt. A failing revalidation query
    /// keeps serving the cached snapshot.
    pub async fn get_or_load(&self, pool: &PgPool) -> sqlx::Result<Option<Arc<Snapshot>>> {
        if let Some(snapshot) = self.current.load_full()
            && self.is_fresh()
        {
            return Ok(Some(snapshot));
        }
        let _guard = self.build_lock.lock().await;
        // Another task may have loaded or revalidated while we waited.
        if let Some(snapshot) = self.current.load_full()
            && self.is_fresh()
        {
            return Ok(Some(snapshot));
        }

        if let Some(cached) = self.current.load_full() {
            let check = async {
                let id = runs::latest_usable_id(pool).await?;
                let version = strains::reviews_version(pool).await?;
                Ok::<_, sqlx::Error>((id, version))
            };
            match check.await {
                Ok((Some(id), version))
                    if id == cached.run.id && version == cached.reviews_version =>
                {
                    self.mark_checked();
                    return Ok(Some(cached));
                }
                Ok((Some(id), _)) => {
                    tracing::info!(
                        cached_run = cached.run.id,
                        latest_run = id,
                        "newer scrape run or review data found, rebuilding snapshot"
                    );
                }
                Ok((None, _)) => {
                    // The cached run vanished (deleted); fall through to a full reload.
                    tracing::warn!(cached_run = cached.run.id, "cached run is no longer usable");
                }
                Err(err) => {
                    tracing::warn!(%err, "snapshot revalidation failed, serving cached run");
                    self.mark_checked();
                    return Ok(Some(cached));
                }
            }
        }

        loop {
            let generation = self.generation.load(Ordering::Acquire);
            let Some(run) = runs::latest_usable(pool).await? else {
                self.current.store(None);
                self.mark_checked();
                return Ok(None);
            };
            let snapshot = Arc::new(Snapshot::build(pool, run).await?);
            if self.generation.load(Ordering::Acquire) != generation {
                // A scrape committed and invalidated while we were building:
                // the run we just loaded may already be stale, so reload
                // instead of caching it.
                tracing::debug!(
                    run = snapshot.run.id,
                    "snapshot invalidated during build, reloading"
                );
                continue;
            }
            self.current.store(Some(snapshot.clone()));
            self.mark_checked();
            return Ok(Some(snapshot));
        }
    }

    /// Snapshot of a specific run for `?run_id=` exports: the current snapshot
    /// when it matches, otherwise a small LRU of previously built runs, otherwise
    /// a fresh build (serialised through one lock to bound the load).
    /// Returns `None` when the run does not exist.
    pub async fn get_run(&self, pool: &PgPool, run_id: i64) -> sqlx::Result<Option<Arc<Snapshot>>> {
        if let Some(current) = self.current.load_full()
            && current.run.id == run_id
        {
            return Ok(Some(current));
        }
        let mut by_run = self.by_run.lock().await;
        if let Some(index) = by_run.iter().position(|s| s.run.id == run_id) {
            let snapshot = by_run.remove(index).expect("index from position");
            by_run.push_back(snapshot.clone());
            return Ok(Some(snapshot));
        }
        let Some(run) = runs::get(pool, run_id).await? else {
            return Ok(None);
        };
        let finished = run.status != RunStatus::Running;
        let snapshot = Arc::new(Snapshot::build(pool, run).await?);
        if finished {
            if by_run.len() >= RUN_CACHE_CAPACITY {
                by_run.pop_front();
            }
            by_run.push_back(snapshot.clone());
        }
        Ok(Some(snapshot))
    }

    /// Drop the cached snapshot; the next read rebuilds it.
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.current.store(None);
        self.checked_at.store(0, Ordering::Release);
    }

    /// Number of invalidations so far (tests, diagnostics).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Force the next `get_or_load` to revalidate against the database.
    pub fn mark_stale(&self) {
        self.checked_at.store(0, Ordering::Release);
    }

    /// Currently cached snapshot without touching the database.
    pub fn peek(&self) -> Option<Arc<Snapshot>> {
        self.current.load_full()
    }

    /// Run ids currently held in the per-run LRU (tests, diagnostics).
    pub async fn cached_run_ids(&self) -> Vec<i64> {
        self.by_run.lock().await.iter().map(|s| s.run.id).collect()
    }
}
