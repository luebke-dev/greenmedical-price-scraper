//! Tracing subscriber and Prometheus metrics recorder.

use std::sync::OnceLock;

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tracing_subscriber::EnvFilter;

use crate::config::LogFormat;
use crate::domain::{RunStatus, RunTrigger};

static METRICS: OnceLock<PrometheusHandle> = OnceLock::new();

/// Initialise the global tracing subscriber (`RUST_LOG`, default `info,sqlx=warn`).
pub fn init_tracing(format: LogFormat) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true);
    match format {
        LogFormat::Json => builder.json().flatten_event(true).init(),
        LogFormat::Pretty => builder.pretty().init(),
    }
}

/// Install (once) and return the Prometheus recorder handle.
pub fn metrics_handle() -> PrometheusHandle {
    METRICS
        .get_or_init(|| {
            let handle = PrometheusBuilder::new()
                .set_buckets_for_metric(
                    Matcher::Full("http_request_duration_seconds".into()),
                    &[
                        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
                    ],
                )
                .expect("non-empty buckets")
                .set_buckets_for_metric(
                    Matcher::Full("scrape_duration_seconds".into()),
                    &[
                        5.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0, 3600.0,
                    ],
                )
                .expect("non-empty buckets")
                .install_recorder()
                .expect("install prometheus recorder");
            describe_metrics();
            init_scrape_series();
            handle
        })
        .clone()
}

fn describe_metrics() {
    metrics::describe_counter!(
        "http_requests_total",
        "HTTP requests by method, route and status"
    );
    metrics::describe_histogram!("http_request_duration_seconds", "HTTP request latency");
    metrics::describe_counter!(
        "scrape_runs_total",
        "Finished scrape runs by status and trigger"
    );
    metrics::describe_histogram!("scrape_duration_seconds", "Duration of scrape runs");
    metrics::describe_gauge!(
        "scrape_last_success_timestamp_seconds",
        "Unix time of the last usable (success/partial) run"
    );
    metrics::describe_gauge!("scrape_last_run_offers", "Offers stored by the last run");
    metrics::describe_counter!(
        "scrape_http_requests_total",
        "Outbound HTTP attempts to the source site"
    );
    metrics::describe_counter!("scrape_http_retries_total", "Outbound HTTP retries");
    metrics::describe_counter!(
        "scrape_reviews_total",
        "Product pages scraped for reviews by result (scraped|failed)"
    );
    metrics::describe_counter!(
        "scrape_lock_skipped_total",
        "Scrape attempts skipped because a lock was held"
    );
    metrics::describe_gauge!(
        "scrape_in_progress",
        "1 while a scrape is running on this instance"
    );
    metrics::describe_gauge!("db_pool_connections", "Open connections in the sqlx pool");
    metrics::describe_counter!(
        "notifications_sent_total",
        "Price-alert digest e-mails by result (sent|error)"
    );
    metrics::describe_gauge!(
        "subscriptions_total",
        "Price-alert subscribers by state (confirmed|unconfirmed)"
    );
}

/// Materialise the scrape series with zero values right away.
///
/// `describe_*` only attaches help text; a series appears in `/metrics` when
/// it is first touched. Dashboards and alerts (`rate()`, `absent()`, "time
/// since last success") behave badly on missing series, so every contract
/// series — including all label combinations of the labelled counters — is
/// initialised at start-up. Counters use `absolute(0)`, which never lowers an
/// existing value.
fn init_scrape_series() {
    metrics::gauge!("scrape_in_progress").set(0.0);
    metrics::gauge!("scrape_last_success_timestamp_seconds").set(0.0);
    metrics::gauge!("scrape_last_run_offers").set(0.0);
    metrics::gauge!("db_pool_connections").set(0.0);
    metrics::counter!("scrape_http_requests_total").absolute(0);
    metrics::counter!("scrape_http_retries_total").absolute(0);
    for result in ["scraped", "failed"] {
        metrics::counter!("scrape_reviews_total", "result" => result).absolute(0);
    }
    for result in ["sent", "error"] {
        metrics::counter!("notifications_sent_total", "result" => result).absolute(0);
    }
    for state in ["confirmed", "unconfirmed"] {
        metrics::gauge!("subscriptions_total", "state" => state).set(0.0);
    }
    for reason in ["lock_held", "in_progress"] {
        metrics::counter!("scrape_lock_skipped_total", "reason" => reason).absolute(0);
    }
    for status in [RunStatus::Success, RunStatus::Partial, RunStatus::Failed] {
        for trigger in [
            RunTrigger::Schedule,
            RunTrigger::Manual,
            RunTrigger::Bootstrap,
        ] {
            metrics::counter!(
                "scrape_runs_total",
                "status" => status.as_str(),
                "trigger" => trigger.as_str()
            )
            .absolute(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_series(text: &str, name: &str, labels: &[&str]) -> bool {
        text.lines().any(|line| {
            (line.starts_with(&format!("{name} ")) || line.starts_with(&format!("{name}{{")))
                && labels.iter().all(|label| line.contains(label))
        })
    }

    #[test]
    fn scrape_series_exist_before_the_first_run() {
        let text = metrics_handle().render();
        for name in [
            "scrape_in_progress",
            "scrape_last_success_timestamp_seconds",
            "scrape_last_run_offers",
            "scrape_http_requests_total",
            "scrape_http_retries_total",
            "db_pool_connections",
        ] {
            assert!(has_series(&text, name, &[]), "missing {name}:\n{text}");
        }
        for result in ["sent", "error"] {
            assert!(has_series(
                &text,
                "notifications_sent_total",
                &[&format!("result=\"{result}\"")]
            ));
        }
        for state in ["confirmed", "unconfirmed"] {
            assert!(has_series(
                &text,
                "subscriptions_total",
                &[&format!("state=\"{state}\"")]
            ));
        }
        for reason in ["lock_held", "in_progress"] {
            assert!(
                has_series(
                    &text,
                    "scrape_lock_skipped_total",
                    &[&format!("reason=\"{reason}\"")]
                ),
                "missing scrape_lock_skipped_total reason={reason}:\n{text}"
            );
        }
        for status in ["success", "partial", "failed"] {
            for trigger in ["schedule", "manual", "bootstrap"] {
                assert!(
                    has_series(
                        &text,
                        "scrape_runs_total",
                        &[
                            &format!("status=\"{status}\""),
                            &format!("trigger=\"{trigger}\"")
                        ]
                    ),
                    "missing scrape_runs_total {status}/{trigger}:\n{text}"
                );
            }
        }
        assert!(
            text.contains("# HELP scrape_in_progress "),
            "help text missing:\n{text}"
        );
    }
}
