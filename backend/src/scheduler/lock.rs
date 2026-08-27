//! Cluster-wide advisory lock (re-exported from the run module for discoverability).

pub use crate::db::SCRAPE_LOCK_KEY;
pub use crate::scrape::run::{LockGuard, try_acquire_lock};
