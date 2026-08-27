//! HTTP client with the retry/backoff policy of the former `requests` session.

use std::error::Error as _;
use std::fmt;
use std::time::Duration;

use chrono::Utc;
use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue, ORIGIN, REFERER, RETRY_AFTER,
};
use tracing::{debug, warn};
use url::Url;

use crate::config::Config;

/// Status codes that are retried (urllib3 `status_forcelist`).
pub const RETRY_STATUS: [u16; 5] = [429, 500, 502, 503, 504];
/// Upper bound honoured for `Retry-After` so a hostile header cannot stall a run.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// A fetched page plus the number of HTTP attempts it took.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub body: String,
    pub attempts: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpErrorKind {
    #[error("HTTP {0}")]
    Status(StatusCode),
    #[error("{0}")]
    Transport(String),
}

/// Terminal fetch failure (after retries were exhausted or for non-retryable errors).
///
/// Displays as `<kind> for <url> after <n> attempt(s)`; the URL is left out
/// when the kind already names it (reqwest transport errors do).
#[derive(Debug, thiserror::Error)]
pub struct HttpError {
    pub url: String,
    pub attempts: u32,
    pub kind: HttpErrorKind,
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = self.kind.to_string();
        if kind.contains(&self.url) {
            write!(f, "{kind} after {} attempt(s)", self.attempts)
        } else {
            write!(
                f,
                "{kind} for {} after {} attempt(s)",
                self.url, self.attempts
            )
        }
    }
}

/// reqwest's `Display` names only the outermost error (`error sending request
/// for url (…)`); the useful part (`Connection refused`, `timed out`) lives
/// in the source chain.
fn transport_message(err: &reqwest::Error) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

/// One scraping session: a reqwest client with the site's headers, the retry
/// policy and its own cookie jar.
///
/// The cookie jar matters: the site answers a flowers request carrying
/// `deliveryTarget` with `302 Location: /de/cannabis/flowers?page=N` plus
/// `Set-Cookie: PHPSESSID=…` and keeps the pharmacy selection in that PHP
/// session. Without the cookie the redirected request yields the generic
/// catalogue for every pharmacy. Build one client per scrape run (like the
/// former `with create_session() as session`) so sessions never leak between
/// runs.
#[derive(Debug, Clone)]
pub struct ScrapeClient {
    http: reqwest::Client,
    retry_total: u32,
    backoff_factor: f64,
}

impl ScrapeClient {
    pub fn new(config: &Config) -> reqwest::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("de,en-US;q=0.7,en;q=0.3"),
        );
        let http = reqwest::Client::builder()
            .user_agent(config.scrape_user_agent.clone())
            .default_headers(headers)
            .timeout(config.scrape_request_timeout)
            .gzip(true)
            // Fresh jar per client: the pharmacy selection lives in the PHP
            // session cookie that the 302 on `deliveryTarget` requests sets.
            .cookie_store(true)
            .build()?;
        Ok(Self {
            http,
            retry_total: config.scrape_retry_total,
            backoff_factor: config.scrape_backoff_factor,
        })
    }

    /// urllib3 backoff: 0 for the first retry, then `factor * 2^(n-1)`.
    pub fn backoff(&self, consecutive_errors: u32) -> Duration {
        if consecutive_errors <= 1 {
            return Duration::ZERO;
        }
        let secs = self.backoff_factor * 2f64.powi(consecutive_errors as i32 - 1);
        Duration::from_secs_f64(secs.max(0.0))
    }

    /// GET a page as text, retrying transient failures.
    pub async fn get_text(&self, url: Url) -> Result<Fetched, HttpError> {
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            metrics::counter!("scrape_http_requests_total").increment(1);
            let response = self.http.get(url.clone()).send().await;
            let retries_left = self.retry_total.saturating_sub(attempts - 1) > 0;

            let (delay, kind) = match response {
                Ok(resp) if resp.status().is_success() => match resp.text().await {
                    Ok(body) => return Ok(Fetched { body, attempts }),
                    // A connection dropped mid-body is a read error; urllib3
                    // retries those for GET as well.
                    Err(err) if retries_left => (
                        self.backoff(attempts),
                        HttpErrorKind::Transport(transport_message(&err)),
                    ),
                    Err(err) => {
                        return Err(HttpError {
                            url: url.to_string(),
                            attempts,
                            kind: HttpErrorKind::Transport(transport_message(&err)),
                        });
                    }
                },
                Ok(resp) => {
                    let status = resp.status();
                    if retries_left && RETRY_STATUS.contains(&status.as_u16()) {
                        let retry_after = retry_after_delay(resp.headers());
                        (
                            retry_after.unwrap_or_else(|| self.backoff(attempts)),
                            HttpErrorKind::Status(status),
                        )
                    } else {
                        return Err(HttpError {
                            url: url.to_string(),
                            attempts,
                            kind: HttpErrorKind::Status(status),
                        });
                    }
                }
                Err(err) => {
                    let retryable = err.is_connect() || err.is_timeout() || err.is_request();
                    if retries_left && retryable {
                        (
                            self.backoff(attempts),
                            HttpErrorKind::Transport(transport_message(&err)),
                        )
                    } else {
                        return Err(HttpError {
                            url: url.to_string(),
                            attempts,
                            kind: HttpErrorKind::Transport(transport_message(&err)),
                        });
                    }
                }
            };

            metrics::counter!("scrape_http_retries_total").increment(1);
            warn!(%url, attempt = attempts, error = %kind, delay_ms = delay.as_millis() as u64, "retrying request");
            debug!(retries_left = self.retry_total - attempts, "retry budget");
            tokio::time::sleep(delay).await;
        }
    }

    /// POST a JSON document to a read-only source endpoint.
    pub async fn post_json_text(
        &self,
        url: Url,
        referer: &Url,
        body: &serde_json::Value,
    ) -> Result<Fetched, HttpError> {
        let origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
        metrics::counter!("scrape_http_requests_total").increment(1);
        let response = self
            .http
            .post(url.clone())
            .header(CONTENT_TYPE, "application/json")
            .header(ORIGIN, origin)
            .header(REFERER, referer.as_str())
            .body(serde_json::to_vec(body).expect("JSON value serializes"))
            .send()
            .await;
        match response {
            Ok(resp) if resp.status().is_success() => resp
                .text()
                .await
                .map(|body| Fetched { body, attempts: 1 })
                .map_err(|err| HttpError {
                    url: url.to_string(),
                    attempts: 1,
                    kind: HttpErrorKind::Transport(transport_message(&err)),
                }),
            Ok(resp) => Err(HttpError {
                url: url.to_string(),
                attempts: 1,
                kind: HttpErrorKind::Status(resp.status()),
            }),
            Err(err) => Err(HttpError {
                url: url.to_string(),
                attempts: 1,
                kind: HttpErrorKind::Transport(transport_message(&err)),
            }),
        }
    }
}

/// `Retry-After` in seconds or as HTTP date. Like urllib3 (`respect_retry_after_header`)
/// it is honoured for every retried status, capped at `MAX_RETRY_AFTER`.
fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let delay = if let Ok(secs) = raw.parse::<u64>() {
        Duration::from_secs(secs)
    } else {
        let date = chrono::DateTime::parse_from_rfc2822(raw).ok()?;
        let diff = date.with_timezone(&Utc) - Utc::now();
        diff.to_std().unwrap_or(Duration::ZERO)
    };
    Some(delay.min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(factor: f64) -> ScrapeClient {
        let cfg = Config::parse_from_args([
            "x",
            "--database-url",
            "postgres://x",
            "--scrape-backoff-factor",
            &factor.to_string(),
        ])
        .unwrap();
        ScrapeClient::new(&cfg).unwrap()
    }

    #[test]
    fn backoff_sequence_matches_urllib3() {
        let c = client(1.0);
        assert_eq!(c.backoff(1), Duration::ZERO);
        assert_eq!(c.backoff(2), Duration::from_secs(2));
        assert_eq!(c.backoff(3), Duration::from_secs(4));
        assert_eq!(c.backoff(4), Duration::from_secs(8));
        assert_eq!(client(0.0).backoff(4), Duration::ZERO);
    }

    #[test]
    fn retry_after_seconds_and_cap() {
        let mut headers = HeaderMap::new();
        assert_eq!(retry_after_delay(&headers), None);
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(retry_after_delay(&headers), Some(Duration::from_secs(7)));
        headers.insert(RETRY_AFTER, HeaderValue::from_static("99999"));
        assert_eq!(retry_after_delay(&headers), Some(MAX_RETRY_AFTER));
        headers.insert(RETRY_AFTER, HeaderValue::from_static("soon"));
        assert_eq!(retry_after_delay(&headers), None);
    }

    #[test]
    fn retry_after_http_date_in_the_past_is_zero() {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(retry_after_delay(&headers), Some(Duration::ZERO));
    }

    #[test]
    fn status_error_names_url_once() {
        let err = HttpError {
            url: "http://h/de/cannabis/pharmacy/".into(),
            attempts: 5,
            kind: HttpErrorKind::Status(StatusCode::SERVICE_UNAVAILABLE),
        };
        assert_eq!(
            err.to_string(),
            "HTTP 503 Service Unavailable for http://h/de/cannabis/pharmacy/ after 5 attempt(s)"
        );
    }

    #[test]
    fn transport_error_does_not_repeat_url_already_in_message() {
        let url = "http://h/de/cannabis/pharmacy/";
        let err = HttpError {
            url: url.into(),
            attempts: 5,
            kind: HttpErrorKind::Transport(format!(
                "error sending request for url ({url}): connection refused"
            )),
        };
        let text = err.to_string();
        assert_eq!(
            text,
            "error sending request for url (http://h/de/cannabis/pharmacy/): connection refused after 5 attempt(s)"
        );
        assert_eq!(text.matches(url).count(), 1);
    }

    #[test]
    fn transport_error_without_url_gets_it_appended() {
        let err = HttpError {
            url: "http://h/x".into(),
            attempts: 1,
            kind: HttpErrorKind::Transport("body read failed".into()),
        };
        assert_eq!(
            err.to_string(),
            "body read failed for http://h/x after 1 attempt(s)"
        );
    }

    #[tokio::test]
    async fn transport_message_includes_the_cause_chain() {
        // Port 1 is never listening: connection refused, no retries.
        let mut cfg = Config::parse_from_args(["x", "--database-url", "postgres://x"]).unwrap();
        cfg.scrape_retry_total = 0;
        let err = ScrapeClient::new(&cfg)
            .unwrap()
            .get_text(Url::parse("http://127.0.0.1:1/de/cannabis/pharmacy/").unwrap())
            .await
            .unwrap_err();
        let text = err.to_string();
        assert_eq!(err.attempts, 1);
        assert!(text.starts_with("error sending request"), "{text}");
        assert!(text.ends_with("after 1 attempt(s)"), "{text}");
        assert_eq!(
            text.matches("http://127.0.0.1:1/de/cannabis/pharmacy/")
                .count(),
            1,
            "{text}"
        );
        assert!(
            text.to_lowercase().contains("connect"),
            "cause chain missing: {text}"
        );
    }
}
