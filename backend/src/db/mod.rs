//! Database access. All SQL is static and checked by `sqlx::query!` (offline data in `.sqlx/`).

pub mod offers;
pub mod pharmacies;
pub mod reviews;
pub mod runs;
pub mod snapshot;
pub mod strains;
pub mod subscriptions;

/// Advisory lock key shared by all instances for the scrape run.
pub const SCRAPE_LOCK_KEY: &str = "greenmedical:scrape";
