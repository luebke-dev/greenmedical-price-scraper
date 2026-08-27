//! Run lifecycle: locking, executing a scrape, persisting the result.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sqlx::postgres::{PgAdvisoryLock, PgAdvisoryLockGuard};
use sqlx::{Either, PgPool, Postgres, pool::PoolConnection};
use tokio::sync::OwnedMutexGuard;
use tracing::{error, info, warn};
use url::Url;

use crate::db::{SCRAPE_LOCK_KEY, offers, pharmacies, reviews, runs, strains};
use crate::domain::{
    RunDto, RunStatus, RunTrigger, calculate_thc_price, clean_text, parse_decimal, parse_percent,
    strain_key,
};
use crate::scrape::client::ScrapeClient;
use crate::scrape::reviews::parse_product_reviews;
use crate::scrape::{ScrapeError, SiteScrape, scrape_site};
use crate::state::SharedState;

pub type LockGuard = PgAdvisoryLockGuard<PoolConnection<Postgres>>;

/// Why a run could not be started.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("scrape_in_progress")]
    InProgress,
    #[error("scrape_locked_elsewhere")]
    LockHeld,
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// A started run: holds the in-process gate and the advisory lock until dropped.
pub struct RunHandle {
    pub run_id: i64,
    pub trigger: RunTrigger,
    _gate: OwnedMutexGuard<()>,
    lock: Option<LockGuard>,
}

/// Try to take the cluster-wide advisory lock on a dedicated pool connection.
pub async fn try_acquire_lock(pool: &PgPool) -> sqlx::Result<Option<LockGuard>> {
    let conn = pool.acquire().await?;
    match PgAdvisoryLock::new(SCRAPE_LOCK_KEY)
        .try_acquire(conn)
        .await?
    {
        Either::Left(guard) => Ok(Some(guard)),
        Either::Right(_conn) => Ok(None),
    }
}

/// Acquire the in-process gate and the advisory lock, then create the `running` row.
pub async fn start_run(state: &SharedState, trigger: RunTrigger) -> Result<RunHandle, StartError> {
    let gate = state
        .scrape_gate
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::InProgress)?;
    let Some(lock) = try_acquire_lock(&state.pool).await? else {
        metrics::counter!("scrape_lock_skipped_total", "reason" => "lock_held").increment(1);
        return Err(StartError::LockHeld);
    };
    let run_id = runs::insert_running(&state.pool, trigger, &state.instance).await?;
    info!(run_id, trigger = trigger.as_str(), "scrape run started");
    Ok(RunHandle {
        run_id,
        trigger,
        _gate: gate,
        lock: Some(lock),
    })
}

/// Final status of a run and the reason when it failed.
pub fn decide_status(scrape: &SiteScrape, min_success_ratio: f64) -> (RunStatus, Option<String>) {
    if scrape.pharmacies_total == 0 {
        return (
            RunStatus::Failed,
            Some("no pharmacies found (layout change?)".into()),
        );
    }
    if scrape.pharmacies_resolved == 0 {
        return (RunStatus::Failed, Some("no pharmacy UUIDs resolved".into()));
    }
    // Pharmacies without a Livebestand UUID cannot be scraped at all and are
    // skipped (not failed), so they do not count against the success ratio.
    let attempted = scrape
        .pharmacies_total
        .saturating_sub(scrape.pharmacies_skipped)
        .max(1);
    let ratio = f64::from(scrape.pharmacies_scraped) / f64::from(attempted);
    if ratio < min_success_ratio {
        return (
            RunStatus::Failed,
            Some(format!(
                "success ratio {ratio:.2} below minimum {min_success_ratio:.2} ({} of {} pharmacies)",
                scrape.pharmacies_scraped, attempted
            )),
        );
    }
    if scrape.offers.is_empty() {
        return (
            RunStatus::Failed,
            Some("no offers scraped (layout change?)".into()),
        );
    }
    if scrape.pharmacies_failed > 0 {
        return (RunStatus::Partial, None);
    }
    (RunStatus::Success, None)
}

fn first_nonempty(current: &mut String, candidate: &str) {
    if current.is_empty() && !candidate.is_empty() {
        *current = candidate.to_owned();
    }
}

/// Persist a finished scrape in one transaction.
async fn persist(
    pool: &PgPool,
    run_id: i64,
    scrape: &SiteScrape,
    status: RunStatus,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    if status != RunStatus::Failed {
        let pharmacy_inputs: Vec<pharmacies::PharmacyInput> = scrape
            .offers
            .iter()
            .map(|o| pharmacies::PharmacyInput {
                external_id: o.pharmacy_uuid.clone(),
                provider: o.provider,
                name: clean_text(Some(&o.pharmacy.name)),
                plz: clean_text(Some(&o.pharmacy.plz)),
                city: clean_text(Some(&o.pharmacy.stadt)),
                address: clean_text(Some(&o.pharmacy.adresse)),
                url: o.pharmacy.url.trim().to_owned(),
            })
            .collect();
        let pharmacy_ids = pharmacies::upsert_many(&mut *tx, &pharmacy_inputs).await?;

        // Strain inputs: first non-empty display value per key, in scrape order.
        let mut strain_inputs: Vec<strains::StrainInput> = Vec::new();
        let mut strain_index: HashMap<(String, String), usize> = HashMap::new();
        for offer in &scrape.offers {
            let name = clean_text(Some(&offer.product.name));
            let bezeichnung = clean_text(Some(&offer.product.bezeichnung));
            let key = (strain_key(&name), strain_key(&bezeichnung));
            let genetik = clean_text(Some(&offer.product.genetik));
            let thc = clean_text(Some(&offer.product.thc));
            let cbd = clean_text(Some(&offer.product.cbd));
            match strain_index.get(&key) {
                Some(&i) => {
                    let entry = &mut strain_inputs[i];
                    first_nonempty(&mut entry.name, &name);
                    first_nonempty(&mut entry.bezeichnung, &bezeichnung);
                    first_nonempty(&mut entry.genetik, &genetik);
                    first_nonempty(&mut entry.thc_label, &thc);
                    first_nonempty(&mut entry.cbd_label, &cbd);
                }
                None => {
                    strain_index.insert(key.clone(), strain_inputs.len());
                    strain_inputs.push(strains::StrainInput {
                        name_key: key.0,
                        bezeichnung_key: key.1,
                        name,
                        bezeichnung,
                        genetik,
                        thc_label: thc,
                        cbd_label: cbd,
                    });
                }
            }
        }
        let strain_ids = strains::upsert_many(&mut *tx, &strain_inputs).await?;

        let mut inserts = Vec::with_capacity(scrape.offers.len());
        for (position, offer) in scrape.offers.iter().enumerate() {
            let name = clean_text(Some(&offer.product.name));
            let bezeichnung = clean_text(Some(&offer.product.bezeichnung));
            let key = (strain_key(&name), strain_key(&bezeichnung));
            let price_label = clean_text(Some(&offer.product.preis_pro_gramm));
            let thc_label = clean_text(Some(&offer.product.thc));
            let cbd_label = clean_text(Some(&offer.product.cbd));
            let price = parse_decimal(&price_label);
            let thc_pct = parse_percent(&thc_label);
            let cbd_pct = parse_percent(&cbd_label);
            inserts.push(offers::OfferInsert {
                pharmacy_id: *pharmacy_ids
                    .get(&offer.pharmacy_uuid)
                    .expect("pharmacy upserted"),
                strain_id: *strain_ids.get(&key).expect("strain upserted"),
                position: position as i32,
                genetik: clean_text(Some(&offer.product.genetik)),
                thc_label,
                cbd_label,
                price_label,
                availability: clean_text(Some(&offer.product.verfuegbarkeit)),
                product_url: offer.product.produkt_url.trim().to_owned(),
                price_eur: price,
                thc_pct,
                cbd_pct,
                price_per_thc_g: calculate_thc_price(price, thc_pct),
                price_per_cbd_g: calculate_thc_price(price, cbd_pct),
            });
        }
        offers::insert_many(&mut *tx, run_id, &inserts).await?;
    }

    runs::insert_errors(&mut *tx, run_id, &scrape.errors).await?;
    runs::finish(
        &mut *tx,
        run_id,
        status,
        runs::RunCounts {
            pharmacies_total: scrape.pharmacies_total as i32,
            pharmacies_scraped: scrape.pharmacies_scraped as i32,
            pharmacies_failed: scrape.pharmacies_failed as i32,
            offer_count: if status == RunStatus::Failed {
                0
            } else {
                scrape.offers.len() as i32
            },
            http_requests: scrape.http_requests as i32,
        },
        error,
    )
    .await?;
    tx.commit().await
}

fn record_run_metrics(
    status: RunStatus,
    trigger: RunTrigger,
    started: Instant,
    offer_count: usize,
) {
    metrics::counter!("scrape_runs_total", "status" => status.as_str(), "trigger" => trigger.as_str())
        .increment(1);
    metrics::histogram!("scrape_duration_seconds").record(started.elapsed().as_secs_f64());
    if matches!(status, RunStatus::Success | RunStatus::Partial) {
        metrics::gauge!("scrape_last_success_timestamp_seconds")
            .set(chrono::Utc::now().timestamp() as f64);
        metrics::gauge!("scrape_last_run_offers").set(offer_count as f64);
    }
}

/// Resets the in-progress gauge and drops the cached snapshot on every exit
/// path of `execute_run`, including early `?` returns.
struct RunFinalizer<'a> {
    state: &'a SharedState,
}

impl Drop for RunFinalizer<'_> {
    fn drop(&mut self) {
        metrics::gauge!("scrape_in_progress").set(0.0);
        self.state.snapshot.invalidate();
    }
}

/// Outcome of phase 2 (product pages scraped for reviews).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewsOutcome {
    pub run_id: i64,
    pub scraped: u32,
    pub failed: u32,
}

/// `produkt_url` without query and fragment (the product page itself).
pub fn product_page_url(product_url: &str) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(product_url.trim())?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// Phase 2: fetch the product page of every strain in `targets`, parse the
/// reviews and commit per strain. Failures are logged and counted, never
/// propagated; cancellation stops the loop after the current strain.
pub async fn scrape_reviews(
    state: &SharedState,
    client: &ScrapeClient,
    run_id: i64,
    targets: &[reviews::ReviewTarget],
) -> ReviewsOutcome {
    let mut outcome = ReviewsOutcome {
        run_id,
        ..ReviewsOutcome::default()
    };
    let total = targets.len();
    for (index, target) in targets.iter().enumerate() {
        if state.shutdown.is_cancelled() {
            warn!(
                run_id,
                done = index,
                total,
                "review scrape cancelled by shutdown"
            );
            break;
        }
        if index > 0 {
            tokio::select! {
                _ = tokio::time::sleep(state.config.scrape_page_delay) => {}
                _ = state.shutdown.cancelled() => {
                    warn!(run_id, done = index, total, "review scrape cancelled by shutdown");
                    break;
                }
            }
        }
        let result = async {
            let url = product_page_url(&target.product_url)
                .map_err(|err| anyhow::anyhow!("invalid product url: {err}"))?;
            let fetched = tokio::select! {
                fetched = client.get_text(url) => fetched?,
                _ = state.shutdown.cancelled() => anyhow::bail!("shutdown"),
            };
            let parsed = parse_product_reviews(&fetched.body);
            reviews::persist(&state.pool, target.strain_id, Some(run_id), &parsed).await?;
            Ok::<_, anyhow::Error>(parsed)
        }
        .await;
        match result {
            Ok(parsed) => {
                outcome.scraped += 1;
                metrics::counter!("scrape_reviews_total", "result" => "scraped").increment(1);
                info!(
                    run_id, index = index + 1, total, strain_id = target.strain_id, strain = %target.name,
                    rating = ?parsed.rating_value, count = parsed.review_count, stored = parsed.reviews.len(),
                    "reviews scraped"
                );
            }
            Err(err) => {
                outcome.failed += 1;
                metrics::counter!("scrape_reviews_total", "result" => "failed").increment(1);
                warn!(
                    run_id, index = index + 1, total, strain_id = target.strain_id, strain = %target.name,
                    url = %target.product_url, %err, "review scrape failed"
                );
            }
        }
    }
    outcome
}

/// Run phase 2 for `run_id` (`older_than = None` refreshes every strain of
/// the run) and store the counters on the run. Never fails the run.
async fn run_review_phase(
    state: &SharedState,
    client: &ScrapeClient,
    run_id: i64,
    older_than: Option<chrono::DateTime<chrono::Utc>>,
) -> anyhow::Result<ReviewsOutcome> {
    let targets = reviews::targets_for_run(
        &state.pool,
        run_id,
        older_than,
        state.config.reviews_max_per_run,
    )
    .await?;
    info!(
        run_id,
        strains = targets.len(),
        "review scrape (phase 2) started"
    );
    let started = Instant::now();
    let outcome = scrape_reviews(state, client, run_id, &targets).await;
    runs::set_review_counts(
        &state.pool,
        run_id,
        outcome.scraped as i32,
        outcome.failed as i32,
    )
    .await?;
    info!(
        run_id,
        scraped = outcome.scraped,
        failed = outcome.failed,
        elapsed_s = started.elapsed().as_secs(),
        "review scrape (phase 2) finished"
    );
    // Ratings live on `strains`, so the cached snapshot of this run is stale now.
    state.snapshot.invalidate();
    Ok(outcome)
}

/// `scrape-once --reviews-only`: phase 2 for every strain of the latest
/// usable run, ignoring `REVIEWS_MAX_AGE` (but honouring
/// `REVIEWS_MAX_PER_RUN`). Takes the same in-process gate and advisory lock
/// as a full run so it never overlaps with one. The counters of that run are
/// overwritten with this pass's results.
pub async fn scrape_reviews_only(state: &SharedState) -> anyhow::Result<ReviewsOutcome> {
    let _gate = state
        .scrape_gate
        .clone()
        .try_lock_owned()
        .map_err(|_| StartError::InProgress)?;
    let Some(lock) = try_acquire_lock(&state.pool).await? else {
        return Err(StartError::LockHeld.into());
    };
    let run = runs::latest_usable(&state.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no usable scrape run yet"))?;
    let client = ScrapeClient::new(&state.config)?;
    let outcome = run_review_phase(state, &client, run.id, None).await;
    if let Err(err) = lock.release_now().await {
        warn!(%err, "releasing advisory lock failed");
    }
    outcome
}

/// Execute the scrape for a started run and persist the outcome.
///
/// Every run gets its own [`ScrapeClient`] (and therefore its own cookie
/// jar): the site keeps the selected pharmacy in a PHP session cookie, so a
/// jar shared between runs would carry one run's session into the next.
///
/// Cancellation via the shutdown token marks the run `failed` with error
/// `shutdown`. Returns the final run row.
///
/// Phase 2 (reviews) runs after the run has been finished (status and
/// `finished_at` set, offers committed): the run is already the "latest
/// usable run" for readers while product pages are being fetched, and the
/// stale-run cleanup cannot mistake a long review pass for a hung run. The
/// gate and advisory lock stay held until phase 2 ends. Phase 2 only updates
/// `reviews_scraped`/`reviews_failed` on the run; it never changes its status.
pub async fn execute_run(state: SharedState, mut handle: RunHandle) -> anyhow::Result<RunDto> {
    let run_id = handle.run_id;
    let trigger = handle.trigger;
    let started = Instant::now();
    metrics::gauge!("scrape_in_progress").set(1.0);
    let _finalizer = RunFinalizer { state: &state };

    let client = ScrapeClient::new(&state.config);
    let outcome = match &client {
        Ok(client) => tokio::select! {
            result = scrape_site(client, &state.config) => Some(result),
            _ = state.shutdown.cancelled() => None,
        },
        Err(_) => None,
    };
    let outcome = match client {
        Err(err) => Some(Err(ScrapeError::Client(err))),
        Ok(_) => outcome,
    };

    // Final status plus the offer count that made it into the database.
    let (final_status, offer_count) = match outcome {
        None => {
            warn!(run_id, "scrape cancelled by shutdown");
            runs::mark_failed(&state.pool, run_id, "shutdown").await?;
            (RunStatus::Failed, 0)
        }
        Some(Err(err)) => {
            error!(run_id, %err, "scrape failed");
            let scrape = SiteScrape {
                http_requests: err.attempts(),
                ..SiteScrape::default()
            };
            persist(
                &state.pool,
                run_id,
                &scrape,
                RunStatus::Failed,
                Some(&err.to_string()),
            )
            .await?;
            (RunStatus::Failed, 0)
        }
        Some(Ok(scrape)) => {
            let (status, error) = decide_status(&scrape, state.config.scrape_min_success_ratio);
            match persist(&state.pool, run_id, &scrape, status, error.as_deref()).await {
                Ok(()) => {
                    info!(
                        run_id,
                        status = status.as_str(),
                        offers = scrape.offers.len(),
                        pharmacies_total = scrape.pharmacies_total,
                        pharmacies_scraped = scrape.pharmacies_scraped,
                        pharmacies_failed = scrape.pharmacies_failed,
                        http_requests = scrape.http_requests,
                        elapsed_s = started.elapsed().as_secs(),
                        "scrape run finished"
                    );
                    let stored = if status == RunStatus::Failed {
                        0
                    } else {
                        scrape.offers.len()
                    };
                    (status, stored)
                }
                Err(err) => {
                    error!(run_id, %err, "persisting scrape failed");
                    runs::mark_failed(&state.pool, run_id, &format!("persist: {err}")).await?;
                    (RunStatus::Failed, 0)
                }
            }
        }
    };
    // Exactly once per run, whatever the outcome.
    record_run_metrics(final_status, trigger, started, offer_count);
    // Drop the cached snapshot now (before the lock is released) so a reader
    // never caches the old run after a new one became visible (a build that
    // raced with this commit is discarded via the cache generation counter);
    // the finalizer repeats this on all other exit paths.
    state.snapshot.invalidate();

    // Price alerts: compare this (now usable) run with its predecessor. Runs
    // before phase 2 and never fails the run.
    if matches!(final_status, RunStatus::Success | RunStatus::Partial) {
        crate::notify::evaluate_run_logged(&state, run_id).await;
    }

    // Phase 2: reviews for the strains of this (now usable) run.
    if state.config.reviews_enabled
        && final_status != RunStatus::Failed
        && !state.shutdown.is_cancelled()
        && let Ok(client) = ScrapeClient::new(&state.config)
    {
        let max_age = chrono::Duration::from_std(state.config.reviews_max_age)
            .unwrap_or_else(|_| chrono::Duration::hours(24));
        let older_than = chrono::Utc::now() - max_age;
        if let Err(err) = run_review_phase(&state, &client, run_id, Some(older_than)).await {
            error!(run_id, %err, "review scrape (phase 2) errored");
        }
    }

    if let Some(lock) = handle.lock.take()
        && let Err(err) = lock.release_now().await
    {
        warn!(%err, "releasing advisory lock failed");
    }

    let run = runs::get(&state.pool, run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("run {run_id} vanished"))?;
    Ok(run)
}

/// Convenience: start and execute a run in one call.
pub async fn scrape_now(state: &SharedState, trigger: RunTrigger) -> anyhow::Result<RunDto> {
    let handle = start_run(state, trigger).await?;
    execute_run(Arc::clone(state), handle).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrape::ScrapedOffer;
    use crate::scrape::parse::{PharmacyRow, Product};

    fn offer() -> ScrapedOffer {
        ScrapedOffer {
            provider: crate::domain::Provider::Greenmedical,
            pharmacy: PharmacyRow {
                name: "Apo".into(),
                url: "https://x".into(),
                plz: "1".into(),
                stadt: "B".into(),
                adresse: "S".into(),
            },
            pharmacy_uuid: "u".into(),
            product: Product::default(),
        }
    }

    fn scrape(total: u32, resolved: u32, scraped: u32, failed: u32, offers: usize) -> SiteScrape {
        SiteScrape {
            pharmacies_total: total,
            pharmacies_resolved: resolved,
            pharmacies_scraped: scraped,
            pharmacies_failed: failed,
            offers: (0..offers).map(|_| offer()).collect(),
            ..SiteScrape::default()
        }
    }

    #[test]
    fn product_page_url_strips_query_and_fragment() {
        let url = product_page_url(
            " https://greenmedical.health/de/cannabis/flower/x-y?deliveryTarget=abc#reviews ",
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://greenmedical.health/de/cannabis/flower/x-y"
        );
        assert!(product_page_url("not a url").is_err());
    }

    #[test]
    fn status_success_partial_failed() {
        assert_eq!(
            decide_status(&scrape(10, 10, 10, 0, 100), 0.5).0,
            RunStatus::Success
        );
        assert_eq!(
            decide_status(&scrape(10, 9, 9, 1, 100), 0.5).0,
            RunStatus::Partial
        );
        assert_eq!(
            decide_status(&scrape(10, 4, 4, 6, 100), 0.5).0,
            RunStatus::Failed
        );
        assert_eq!(
            decide_status(&scrape(10, 5, 5, 5, 100), 0.5).0,
            RunStatus::Partial
        );
        assert_eq!(
            decide_status(&scrape(0, 0, 0, 0, 0), 0.5).0,
            RunStatus::Failed
        );
        assert_eq!(
            decide_status(&scrape(10, 0, 0, 10, 0), 0.5).0,
            RunStatus::Failed
        );
        let (status, error) = decide_status(&scrape(10, 10, 10, 0, 0), 0.5);
        assert_eq!(status, RunStatus::Failed);
        assert!(error.unwrap().contains("no offers"));
    }

    #[test]
    fn skipped_pharmacies_do_not_count_as_failures() {
        // 10 listed, 6 without UUID, 4 attempted and scraped: success, not partial.
        let s = SiteScrape {
            pharmacies_skipped: 6,
            ..scrape(10, 4, 4, 0, 100)
        };
        assert_eq!(decide_status(&s, 0.5), (RunStatus::Success, None));
        // 10 listed, 6 skipped, 4 attempted, 2 scraped, 2 failed: ratio 0.5 -> partial.
        let s = SiteScrape {
            pharmacies_skipped: 6,
            ..scrape(10, 4, 2, 2, 100)
        };
        assert_eq!(decide_status(&s, 0.5).0, RunStatus::Partial);
        // 1 scraped of 4 attempted -> below ratio -> failed.
        let s = SiteScrape {
            pharmacies_skipped: 6,
            ..scrape(10, 4, 1, 3, 100)
        };
        let (status, error) = decide_status(&s, 0.5);
        assert_eq!(status, RunStatus::Failed);
        assert!(error.unwrap().contains("1 of 4 pharmacies"));
    }
}
