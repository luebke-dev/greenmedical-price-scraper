//! Price-alert evaluation: diff the finished run against the previous usable
//! run, match the confirmed subscribers' rules, store `notifications` rows and
//! send one digest e-mail per subscriber.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::db::subscriptions::{self, RuleRow, StrainState};
use crate::db::{runs, subscriptions as subs};
use crate::domain::RuleKind;
use crate::mail::templates;
use crate::state::SharedState;

/// Number of days an unconfirmed subscriber is kept.
pub const UNCONFIRMED_MAX_AGE_DAYS: i64 = 7;

/// One triggered event (also the JSON `payload` of a `notifications` row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub kind: RuleKind,
    pub strain_id: i64,
    pub strain_name: String,
    pub designation: String,
    /// `min_price` in the evaluated run.
    pub price: Option<f64>,
    /// `min_price` in the previous run (`None` when not listed or unpriced).
    pub previous_price: Option<f64>,
    pub thc_value: Option<f64>,
    /// Pharmacy of the cheapest offer in the evaluated run.
    pub pharmacy: Option<String>,
    pub threshold: Option<f64>,
}

/// Events of one rule.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleEvents {
    pub rule: RuleRow,
    pub events: Vec<Event>,
}

/// Everything one subscriber's e-mail for a run contains.
#[derive(Debug, Clone, PartialEq)]
pub struct Digest {
    pub run_id: i64,
    pub run_at: DateTime<Utc>,
    pub manage_token: String,
    pub groups: Vec<RuleEvents>,
}

/// The comparison of a run with its predecessor, indexed by strain id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunDiff {
    pub latest: BTreeMap<i64, StrainState>,
    pub previous: BTreeMap<i64, StrainState>,
}

impl RunDiff {
    pub fn new(latest: Vec<StrainState>, previous: Vec<StrainState>) -> Self {
        Self {
            latest: latest.into_iter().map(|s| (s.strain_id, s)).collect(),
            previous: previous.into_iter().map(|s| (s.strain_id, s)).collect(),
        }
    }

    fn event(&self, kind: RuleKind, state: &StrainState, threshold: Option<f64>) -> Event {
        Event {
            kind,
            strain_id: state.strain_id,
            strain_name: state.name.clone(),
            designation: state.designation.clone(),
            price: state.min_price,
            previous_price: self
                .previous
                .get(&state.strain_id)
                .and_then(|p| p.min_price),
            thc_value: state.thc_value,
            pharmacy: Some(state.pharmacy.clone()),
            threshold,
        }
    }

    /// Prices are compared at cent precision.
    fn cents(value: f64) -> i64 {
        (value * 100.0).round() as i64
    }

    /// „Unterschreiten“: below the threshold now and not below it before
    /// (previously unlisted, unpriced or at/above the threshold).
    fn crossed_below(&self, state: &StrainState, threshold: f64) -> bool {
        let Some(price) = state.min_price else {
            return false;
        };
        if Self::cents(price) >= Self::cents(threshold) {
            return false;
        }
        match self
            .previous
            .get(&state.strain_id)
            .and_then(|p| p.min_price)
        {
            Some(before) => Self::cents(before) >= Self::cents(threshold),
            None => true,
        }
    }

    fn is_new(&self, strain_id: i64) -> bool {
        !self.previous.contains_key(&strain_id)
    }

    /// Events of one rule against this diff, in strain id order.
    pub fn evaluate(&self, rule: &RuleRow) -> Vec<Event> {
        let kind = rule.kind;
        match kind {
            RuleKind::StrainAvailable => {
                let Some(state) = rule.strain_id.and_then(|id| self.latest.get(&id)) else {
                    return Vec::new();
                };
                if self.is_new(state.strain_id) {
                    vec![self.event(kind, state, None)]
                } else {
                    Vec::new()
                }
            }
            RuleKind::StrainPriceBelow => {
                let (Some(state), Some(threshold)) = (
                    rule.strain_id.and_then(|id| self.latest.get(&id)),
                    rule.threshold,
                ) else {
                    return Vec::new();
                };
                if self.crossed_below(state, threshold) {
                    vec![self.event(kind, state, Some(threshold))]
                } else {
                    Vec::new()
                }
            }
            RuleKind::AnyPriceBelow => {
                let Some(threshold) = rule.threshold else {
                    return Vec::new();
                };
                self.latest
                    .values()
                    .filter(|s| self.crossed_below(s, threshold))
                    .map(|s| self.event(kind, s, Some(threshold)))
                    .collect()
            }
            RuleKind::ThcAbove => {
                let Some(threshold) = rule.threshold else {
                    return Vec::new();
                };
                self.latest
                    .values()
                    .filter(|s| {
                        self.is_new(s.strain_id) && s.thc_value.is_some_and(|thc| thc > threshold)
                    })
                    .map(|s| self.event(kind, s, Some(threshold)))
                    .collect()
            }
            RuleKind::NewStrain => self
                .latest
                .values()
                .filter(|s| self.is_new(s.strain_id))
                .map(|s| self.event(kind, s, None))
                .collect(),
            RuleKind::StrainPriceChange => {
                let Some(state) = rule.strain_id.and_then(|id| self.latest.get(&id)) else {
                    return Vec::new();
                };
                let Some(previous) = self.previous.get(&state.strain_id) else {
                    return Vec::new();
                };
                let changed = match (state.min_price, previous.min_price) {
                    (Some(now), Some(then)) => Self::cents(now) != Self::cents(then),
                    (None, None) => false,
                    _ => true,
                };
                if changed {
                    vec![self.event(kind, state, None)]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// Result of [`evaluate_run`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluationOutcome {
    pub run_id: i64,
    /// Subscribers that received (or should have received) a digest.
    pub digests: u32,
    /// Newly stored notification rows.
    pub notifications: u32,
    /// Digests whose e-mail failed.
    pub failed: u32,
}

/// Evaluate all confirmed subscriptions for the finished usable run `run_id`.
///
/// Notifications are stored first (deduplicated by the unique key), the e-mail
/// is sent afterwards; delivery errors are recorded on the rows and never
/// retried. Without a previous usable run nothing is compared.
pub async fn evaluate_run(state: &SharedState, run_id: i64) -> anyhow::Result<EvaluationOutcome> {
    let pool = &state.pool;
    let mut outcome = EvaluationOutcome {
        run_id,
        ..EvaluationOutcome::default()
    };
    let Some(run) = runs::get(pool, run_id).await? else {
        anyhow::bail!("run {run_id} not found");
    };
    let Some(previous_id) = subs::previous_usable_run_id(pool, run_id).await? else {
        info!(
            run_id,
            "no previous usable run, skipping subscription evaluation"
        );
        return Ok(outcome);
    };
    let rules = subs::rules_of_confirmed(pool).await?;
    if rules.is_empty() {
        info!(run_id, "no confirmed subscriptions");
        return Ok(outcome);
    }
    let diff = RunDiff::new(
        subs::strain_states(pool, run_id).await?,
        subs::strain_states(pool, previous_id).await?,
    );

    let mut by_subscriber: BTreeMap<i64, Vec<RuleEvents>> = BTreeMap::new();
    for rule in rules {
        let events = diff.evaluate(&rule);
        if events.is_empty() {
            continue;
        }
        by_subscriber
            .entry(rule.subscriber_id)
            .or_default()
            .push(RuleEvents { rule, events });
    }

    let run_at = run.finished_at.unwrap_or(run.started_at);
    for (subscriber_id, groups) in by_subscriber {
        let Some(subscriber) = subscriptions::get(pool, subscriber_id).await? else {
            continue;
        };
        // Store first; only events that were not stored before go into the mail.
        let mut ids: Vec<i64> = Vec::new();
        let mut fresh: Vec<RuleEvents> = Vec::new();
        for group in groups {
            let mut events = Vec::with_capacity(group.events.len());
            for event in group.events {
                let payload = serde_json::to_value(&event)?;
                if let Some(id) = subs::insert_notification(
                    pool,
                    subscriber_id,
                    run_id,
                    group.rule.id,
                    Some(event.strain_id),
                    &payload,
                )
                .await?
                {
                    ids.push(id);
                    events.push(event);
                }
            }
            if !events.is_empty() {
                fresh.push(RuleEvents {
                    rule: group.rule,
                    events,
                });
            }
        }
        if fresh.is_empty() {
            continue;
        }
        outcome.notifications += ids.len() as u32;
        outcome.digests += 1;
        let digest = Digest {
            run_id,
            run_at,
            manage_token: subscriber.manage_token.clone(),
            groups: fresh,
        };
        let email = templates::digest(
            &state.config.public_url,
            state.config.scrape_timezone,
            &subscriber.email,
            &digest,
        );
        match state.mailer.send(email).await {
            Ok(()) => {
                metrics::counter!("notifications_sent_total", "result" => "sent").increment(1);
                subs::mark_sent(pool, &ids, None).await?;
                subs::set_last_notified(pool, subscriber_id, run_id).await?;
            }
            Err(err) => {
                outcome.failed += 1;
                metrics::counter!("notifications_sent_total", "result" => "error").increment(1);
                warn!(run_id, subscriber_id, %err, "notification e-mail failed");
                subs::mark_sent(pool, &ids, Some(&err.to_string())).await?;
            }
        }
    }
    info!(
        run_id,
        previous_run_id = previous_id,
        digests = outcome.digests,
        notifications = outcome.notifications,
        failed = outcome.failed,
        "subscription evaluation finished"
    );
    Ok(outcome)
}

/// [`evaluate_run`] for the run hook: logs instead of failing the caller.
pub async fn evaluate_run_logged(state: &SharedState, run_id: i64) {
    if let Err(err) = evaluate_run(state, run_id).await {
        error!(run_id, %err, "subscription evaluation errored");
    }
}

/// Delete unconfirmed subscribers older than [`UNCONFIRMED_MAX_AGE_DAYS`].
pub async fn cleanup_unconfirmed(state: &SharedState) -> sqlx::Result<u64> {
    let cutoff = Utc::now() - chrono::Duration::days(UNCONFIRMED_MAX_AGE_DAYS);
    let deleted = subs::delete_unconfirmed_before(&state.pool, cutoff).await?;
    if deleted > 0 {
        info!(deleted, "removed unconfirmed subscribers");
    }
    Ok(deleted)
}

/// Refresh the `subscriptions_total{state}` gauge from the database.
pub async fn refresh_gauge(state: &SharedState) {
    match subs::counts(&state.pool).await {
        Ok((confirmed, unconfirmed)) => {
            metrics::gauge!("subscriptions_total", "state" => "confirmed").set(confirmed as f64);
            metrics::gauge!("subscriptions_total", "state" => "unconfirmed")
                .set(unconfirmed as f64);
        }
        Err(err) => warn!(%err, "could not count subscriptions"),
    }
}

/// Helper for tests: index states by id.
pub fn states_by_id(states: &[StrainState]) -> HashMap<i64, &StrainState> {
    states.iter().map(|s| (s.strain_id, s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(id: i64, price: Option<f64>, thc: Option<f64>) -> StrainState {
        StrainState {
            strain_id: id,
            name: format!("Sorte {id}"),
            designation: String::new(),
            min_price: price,
            thc_value: thc,
            pharmacy: "Apo".into(),
        }
    }

    fn rule(kind: RuleKind, strain_id: Option<i64>, threshold: Option<f64>) -> RuleRow {
        RuleRow {
            id: 1,
            subscriber_id: 1,
            kind,
            strain_id,
            threshold,
            strain_name: None,
            created_at: Utc::now(),
        }
    }

    fn ids(events: &[Event]) -> Vec<i64> {
        events.iter().map(|e| e.strain_id).collect()
    }

    /// previous: 1 @ 6.49, 2 @ 5.00, 3 unpriced, 4 @ 4.00 (delisted now)
    /// latest:   1 @ 5.49, 2 @ 5.00, 3 @ 7.00, 5 @ 4.50 (new, THC 25), 6 unpriced (new, THC 10)
    fn diff() -> RunDiff {
        RunDiff::new(
            vec![
                state(1, Some(5.49), Some(20.0)),
                state(2, Some(5.00), Some(18.0)),
                state(3, Some(7.00), None),
                state(5, Some(4.50), Some(25.0)),
                state(6, None, Some(10.0)),
            ],
            vec![
                state(1, Some(6.49), Some(20.0)),
                state(2, Some(5.00), Some(18.0)),
                state(3, None, None),
                state(4, Some(4.00), Some(30.0)),
            ],
        )
    }

    #[test]
    fn strain_available_fires_only_for_relisted_strains() {
        let d = diff();
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::StrainAvailable, Some(5), None))),
            [5]
        );
        assert!(
            d.evaluate(&rule(RuleKind::StrainAvailable, Some(1), None))
                .is_empty()
        );
        // Delisted or unknown strains never fire.
        assert!(
            d.evaluate(&rule(RuleKind::StrainAvailable, Some(4), None))
                .is_empty()
        );
        assert!(
            d.evaluate(&rule(RuleKind::StrainAvailable, Some(99), None))
                .is_empty()
        );
        assert!(
            d.evaluate(&rule(RuleKind::StrainAvailable, None, None))
                .is_empty()
        );
    }

    #[test]
    fn strain_price_below_fires_only_on_crossing() {
        let d = diff();
        // 6.49 -> 5.49 crosses 6.00.
        let events = d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(1), Some(6.0)));
        assert_eq!(ids(&events), [1]);
        assert_eq!(events[0].price, Some(5.49));
        assert_eq!(events[0].previous_price, Some(6.49));
        assert_eq!(events[0].threshold, Some(6.0));
        assert_eq!(events[0].pharmacy.as_deref(), Some("Apo"));
        // Already below before: no event ("nur beim Unterschreiten").
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(1), Some(7.0)))
                .is_empty()
        );
        // Equal to the threshold is not below.
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(1), Some(5.49)))
                .is_empty()
        );
        // Unchanged price: no crossing.
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(2), Some(6.0)))
                .is_empty()
        );
        // Newly listed below the threshold counts as crossing.
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(5), Some(5.0)))),
            [5]
        );
        // Previously unpriced, now above the threshold: nothing.
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(3), Some(6.0)))
                .is_empty()
        );
        // Previously unpriced, now below: crossing.
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(3), Some(8.0)))),
            [3]
        );
        // Unpriced now: never.
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(6), Some(8.0)))
                .is_empty()
        );
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceBelow, Some(1), None))
                .is_empty()
        );
    }

    #[test]
    fn any_price_below_yields_one_event_per_crossing_strain() {
        let d = diff();
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::AnyPriceBelow, None, Some(6.0)))),
            [1, 5]
        );
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::AnyPriceBelow, None, Some(8.0)))),
            [3, 5]
        );
        assert!(
            d.evaluate(&rule(RuleKind::AnyPriceBelow, None, Some(4.0)))
                .is_empty()
        );
    }

    #[test]
    fn thc_above_only_for_newly_listed_strains() {
        let d = diff();
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::ThcAbove, None, Some(20.0)))),
            [5]
        );
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::ThcAbove, None, Some(5.0)))),
            [5, 6]
        );
        // Equal is not above; strain 1 (THC 20, not new) never fires.
        assert!(
            d.evaluate(&rule(RuleKind::ThcAbove, None, Some(25.0)))
                .is_empty()
        );
        assert!(d.evaluate(&rule(RuleKind::ThcAbove, None, None)).is_empty());
    }

    #[test]
    fn new_strain_lists_every_newly_listed_strain() {
        let d = diff();
        let events = d.evaluate(&rule(RuleKind::NewStrain, None, None));
        assert_eq!(ids(&events), [5, 6]);
        assert_eq!(events[0].price, Some(4.5));
        assert_eq!(events[0].previous_price, None);
        assert_eq!(events[1].price, None);
        // Identical runs: nothing is new.
        let same = RunDiff::new(
            vec![state(1, Some(1.0), None)],
            vec![state(1, Some(1.0), None)],
        );
        assert!(
            same.evaluate(&rule(RuleKind::NewStrain, None, None))
                .is_empty()
        );
    }

    #[test]
    fn strain_price_change_compares_listed_strains_only() {
        let d = diff();
        let events = d.evaluate(&rule(RuleKind::StrainPriceChange, Some(1), None));
        assert_eq!(ids(&events), [1]);
        assert_eq!(events[0].previous_price, Some(6.49));
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceChange, Some(2), None))
                .is_empty()
        );
        // Unpriced -> priced counts as a change.
        assert_eq!(
            ids(&d.evaluate(&rule(RuleKind::StrainPriceChange, Some(3), None))),
            [3]
        );
        // New or delisted strains are not "changes".
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceChange, Some(5), None))
                .is_empty()
        );
        assert!(
            d.evaluate(&rule(RuleKind::StrainPriceChange, Some(4), None))
                .is_empty()
        );
        // Sub-cent noise is not a change.
        let noise = RunDiff::new(
            vec![state(1, Some(5.491), None)],
            vec![state(1, Some(5.49), None)],
        );
        assert!(
            noise
                .evaluate(&rule(RuleKind::StrainPriceChange, Some(1), None))
                .is_empty()
        );
    }

    #[test]
    fn event_payload_round_trips_as_json() {
        let d = diff();
        let event = d
            .evaluate(&rule(RuleKind::AnyPriceBelow, None, Some(6.0)))
            .remove(0);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "any_price_below");
        assert_eq!(json["strain_id"], 1);
        assert_eq!(serde_json::from_value::<Event>(json).unwrap(), event);
        assert_eq!(states_by_id(&[state(9, None, None)]).len(), 1);
    }
}
