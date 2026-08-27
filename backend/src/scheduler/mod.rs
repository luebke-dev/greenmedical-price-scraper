//! Cron scheduler: bootstrap, stale-run cleanup and the fire loop.

pub mod lock;

use std::hash::{Hash, Hasher};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use tracing::{error, info, warn};

use crate::db::runs;
use crate::domain::RunTrigger;
use crate::scrape::run::{StartError, execute_run, start_run};
use crate::state::SharedState;

/// Maximum start-up jitter (seconds) to spread replicas.
pub const MAX_JITTER_SECS: u64 = 30;

/// Next fire time strictly after `after`, evaluated in `tz`.
///
/// Shared by the scheduler loop and `Metadata.next_run_at`
/// ([`crate::config::Config::next_scrape_at`]), so the API reports exactly the
/// instant the scheduler will fire at.
pub fn next_fire(schedule: &Schedule, tz: Tz, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let local = after.with_timezone(&tz);
    schedule
        .after(&local)
        .next()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Should the instance scrape right away at start-up?
pub fn should_bootstrap(
    latest_usable_started_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    max_age: Duration,
) -> bool {
    match latest_usable_started_at {
        None => true,
        Some(started) => {
            let age = (now - started).to_std().unwrap_or(Duration::ZERO);
            age > max_age
        }
    }
}

/// Deterministic jitter in `0..=MAX_JITTER_SECS` derived from the instance name.
pub fn jitter_for(instance: &str) -> Duration {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    instance.hash(&mut hasher);
    Duration::from_secs(hasher.finish() % (MAX_JITTER_SECS + 1))
}

/// Outcome of one scheduler attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attempt {
    Ran,
    SkippedInProgress,
    SkippedLockHeld,
    Failed,
}

/// Try to run a scrape, skipping when another run is in progress.
pub async fn try_run(state: &SharedState, trigger: RunTrigger) -> Attempt {
    match start_run(state, trigger).await {
        Ok(handle) => match execute_run(state.clone(), handle).await {
            Ok(_) => Attempt::Ran,
            Err(err) => {
                error!(%err, "scrape run errored");
                Attempt::Failed
            }
        },
        Err(StartError::InProgress) => {
            metrics::counter!("scrape_lock_skipped_total", "reason" => "in_progress").increment(1);
            warn!("scrape already in progress in this process, skipping");
            Attempt::SkippedInProgress
        }
        Err(StartError::LockHeld) => {
            info!("advisory lock held by another instance, skipping");
            Attempt::SkippedLockHeld
        }
        Err(StartError::Db(err)) => {
            error!(%err, "could not start scrape run");
            Attempt::Failed
        }
    }
}

/// Mark stale `running` rows as failed.
pub async fn cleanup_stale_runs(state: &SharedState) {
    match runs::mark_stale(&state.pool, state.config.scrape_stale_run_after).await {
        Ok(0) => {}
        Ok(n) => warn!(count = n, "marked stale running scrape runs as failed"),
        Err(err) => error!(%err, "stale run cleanup failed"),
    }
}

/// Delete unconfirmed subscribers older than 7 days and refresh the gauge.
pub async fn cleanup_subscriptions(state: &SharedState) {
    if let Err(err) = crate::notify::cleanup_unconfirmed(state).await {
        error!(%err, "subscription cleanup failed");
    }
    crate::notify::refresh_gauge(state).await;
}

/// The scheduler loop. Returns when the shutdown token is cancelled.
pub async fn run_scheduler(state: SharedState) {
    let shutdown = state.shutdown.clone();
    let tz = state.config.scrape_timezone;
    let schedule = state.config.scrape_cron.clone();

    cleanup_stale_runs(&state).await;
    cleanup_subscriptions(&state).await;

    let jitter = jitter_for(&state.instance);
    info!(jitter_s = jitter.as_secs(), "scheduler starting");
    tokio::select! {
        _ = tokio::time::sleep(jitter) => {}
        _ = shutdown.cancelled() => return,
    }

    if state.config.scrape_bootstrap {
        let latest = match runs::latest_usable(&state.pool).await {
            Ok(run) => run.map(|r| r.started_at),
            Err(err) => {
                error!(%err, "could not query latest run for bootstrap decision");
                None
            }
        };
        if should_bootstrap(latest, Utc::now(), state.config.scrape_bootstrap_max_age) {
            info!(latest_run = ?latest, "bootstrap scrape");
            try_run(&state, RunTrigger::Bootstrap).await;
        } else {
            info!(latest_run = ?latest, "recent run exists, skipping bootstrap scrape");
        }
    }

    loop {
        let now = Utc::now();
        let Some(next) = next_fire(&schedule, tz, now) else {
            error!(
                cron = schedule.source(),
                "cron schedule has no upcoming fire time, scheduler stops"
            );
            return;
        };
        let wait = (next - now).to_std().unwrap_or(Duration::ZERO);
        info!(
            next_local = %next.with_timezone(&tz).to_rfc3339(),
            next_utc = %next.to_rfc3339(),
            timezone = %tz,
            cron = schedule.source(),
            "next scheduled scrape"
        );
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.cancelled() => {
                info!("scheduler stopped");
                return;
            }
        }
        cleanup_stale_runs(&state).await;
        cleanup_subscriptions(&state).await;
        try_run(&state, RunTrigger::Schedule).await;
    }
}

/// Helper for tests and logs: interpret a local wall-clock time in `tz`.
pub fn local(tz: Tz, y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
    tz.with_ymd_and_hms(y, m, d, h, min, 0)
        .single()
        .expect("unambiguous local time")
        .with_timezone(&Utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    fn schedule() -> Schedule {
        "0 0 4,10,16,22 * * *".parse().unwrap()
    }

    #[test]
    fn next_fire_across_dst_end_2026_10_25() {
        // Europe/Berlin leaves DST on 2026-10-25 at 03:00 CEST -> 02:00 CET.
        let before = local(Berlin, 2026, 10, 24, 22, 30); // 20:30 UTC (CEST)
        let next = next_fire(&schedule(), Berlin, before).unwrap();
        assert_eq!(
            next.with_timezone(&Berlin).to_rfc3339(),
            "2026-10-25T04:00:00+01:00"
        );
        assert_eq!(next.to_rfc3339(), "2026-10-25T03:00:00+00:00");
        // The night is one hour longer: 22:00 CEST -> 04:00 CET is 7 hours.
        let previous = local(Berlin, 2026, 10, 24, 22, 0);
        assert_eq!((next - previous).num_hours(), 7);
        // The day before, 04:00 local was 02:00 UTC.
        let day_before =
            next_fire(&schedule(), Berlin, local(Berlin, 2026, 10, 23, 23, 0)).unwrap();
        assert_eq!(day_before.to_rfc3339(), "2026-10-24T02:00:00+00:00");
    }

    fn hourly() -> Schedule {
        "0 0 * * * *".parse().unwrap()
    }

    #[test]
    fn hourly_next_fire_across_dst_end_2026_10_25() {
        // 03:00 CEST -> 02:00 CET on 2026-10-25, i.e. at 01:00 UTC the local
        // hour 02:00 repeats. The `cron` crate fires 02:00 CEST (00:00 UTC) and
        // skips the ambiguous second 02:00 CET (01:00 UTC), so there is one
        // two-hour gap in UTC; `Metadata.next_run_at` reports exactly what the
        // scheduler will do, which is what matters for the countdown.
        let mut at = "2026-10-24T22:30:00Z".parse::<DateTime<Utc>>().unwrap();
        let mut fires = Vec::new();
        for _ in 0..6 {
            let next = next_fire(&hourly(), Berlin, at).unwrap();
            assert!(next > at);
            fires.push(next.to_rfc3339());
            at = next;
        }
        assert_eq!(
            fires,
            [
                "2026-10-24T23:00:00+00:00",
                "2026-10-25T00:00:00+00:00",
                "2026-10-25T02:00:00+00:00",
                "2026-10-25T03:00:00+00:00",
                "2026-10-25T04:00:00+00:00",
                "2026-10-25T05:00:00+00:00",
            ]
        );
        // Every fire is on a full hour in both zones and strictly increasing.
        for fire in &fires {
            let utc = fire.parse::<DateTime<Utc>>().unwrap();
            assert_eq!(
                utc.with_timezone(&Berlin).format("%M:%S").to_string(),
                "00:00"
            );
        }
    }

    #[test]
    fn hourly_next_fire_around_hour_boundary() {
        let just_before = "2026-08-27T09:59:59Z".parse::<DateTime<Utc>>().unwrap();
        let on_the_hour = "2026-08-27T10:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let just_after = "2026-08-27T10:00:01Z".parse::<DateTime<Utc>>().unwrap();
        assert_eq!(
            next_fire(&hourly(), Berlin, just_before)
                .unwrap()
                .to_rfc3339(),
            "2026-08-27T10:00:00+00:00"
        );
        // Strictly after: exactly on the hour rolls over to the next one.
        assert_eq!(
            next_fire(&hourly(), Berlin, on_the_hour)
                .unwrap()
                .to_rfc3339(),
            "2026-08-27T11:00:00+00:00"
        );
        assert_eq!(
            next_fire(&hourly(), Berlin, just_after)
                .unwrap()
                .to_rfc3339(),
            "2026-08-27T11:00:00+00:00"
        );
    }

    #[test]
    fn next_fire_is_strictly_after() {
        let at = local(Berlin, 2026, 8, 27, 10, 0);
        let next = next_fire(&schedule(), Berlin, at).unwrap();
        assert_eq!(
            next.with_timezone(&Berlin).to_rfc3339(),
            "2026-08-27T16:00:00+02:00"
        );
    }

    #[test]
    fn bootstrap_decision() {
        let now = Utc::now();
        let max_age = Duration::from_secs(8 * 3600);
        assert!(should_bootstrap(None, now, max_age));
        assert!(!should_bootstrap(
            Some(now - chrono::Duration::hours(1)),
            now,
            max_age
        ));
        assert!(should_bootstrap(
            Some(now - chrono::Duration::hours(9)),
            now,
            max_age
        ));
        assert!(!should_bootstrap(
            Some(now + chrono::Duration::hours(1)),
            now,
            max_age
        ));
    }

    #[test]
    fn jitter_is_bounded_and_deterministic() {
        let a = jitter_for("backend-0");
        assert_eq!(a, jitter_for("backend-0"));
        assert!(a <= Duration::from_secs(MAX_JITTER_SECS));
    }
}
