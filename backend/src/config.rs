//! CLI and environment configuration (see `docs/api-contract.md`).

use std::net::SocketAddr;
use std::time::Duration;

use chrono_tz::Tz;
use clap::builder::BoolishValueParser;
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use url::Url;

/// Default User-Agent, identical to the former Python scraper.
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

fn parse_duration(value: &str) -> Result<Duration, String> {
    humantime::parse_duration(value).map_err(|e| e.to_string())
}

fn parse_timezone(value: &str) -> Result<Tz, String> {
    value.parse::<Tz>().map_err(|e| e.to_string())
}

fn parse_cron(value: &str) -> Result<cron::Schedule, String> {
    value
        .parse::<cron::Schedule>()
        .map_err(|e| format!("invalid cron expression {value:?}: {e}"))
}

/// `SUBSCRIPTION_RATE_LIMIT`: at most `count` requests per `per` and client IP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub count: u32,
    pub per: Duration,
}

impl std::str::FromStr for RateLimit {
    type Err = String;

    /// Parses `"<count>/<duration>"`, e.g. `5/1h` or `20/15m`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (count, per) = value
            .split_once('/')
            .ok_or_else(|| format!("expected <count>/<duration>, got {value:?}"))?;
        let count: u32 = count
            .trim()
            .parse()
            .map_err(|e| format!("invalid count in {value:?}: {e}"))?;
        if count == 0 {
            return Err(format!("count must be at least 1 in {value:?}"));
        }
        let per = humantime::parse_duration(per.trim())
            .map_err(|e| format!("invalid duration in {value:?}: {e}"))?;
        if per.is_zero() {
            return Err(format!("duration must be positive in {value:?}"));
        }
        Ok(Self { count, per })
    }
}

impl std::fmt::Display for RateLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.count, humantime::format_duration(self.per))
    }
}

/// `SMTP_TLS`: how the SMTP connection is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SmtpTls {
    /// Plain connection upgraded with STARTTLS (default, port 587).
    Starttls,
    /// Implicit TLS (port 465).
    Tls,
    /// No encryption (local relays, mailpit).
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Run migrations, the scheduler and the HTTP API (default).
    Serve,
    /// Run exactly one scrape and exit.
    ScrapeOnce {
        /// Only phase 2 (reviews) for every strain of the latest usable run,
        /// ignoring `REVIEWS_MAX_AGE`.
        #[arg(long)]
        reviews_only: bool,
    },
    /// Apply pending database migrations and exit.
    Migrate,
}

#[derive(Parser)]
#[command(name = "greenmedical-backend", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub config: Config,
}

/// All runtime configuration. Every field is an environment variable.
///
/// `Debug` is implemented manually so that neither the database password nor
/// the admin token can leak into logs.
#[derive(Clone, Parser)]
#[command(name = "greenmedical-backend")]
pub struct Config {
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,

    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 10)]
    pub database_max_connections: u32,

    #[arg(long, env = "HTTP_BIND", default_value = "0.0.0.0:8080")]
    pub http_bind: SocketAddr,

    #[arg(long, env = "METRICS_BIND", default_value = "0.0.0.0:9090")]
    pub metrics_bind: SocketAddr,

    #[arg(long, env = "HTTP_REQUEST_TIMEOUT", default_value = "30s", value_parser = parse_duration)]
    pub http_request_timeout: Duration,

    /// How often the cached snapshot re-checks the database for a newer usable
    /// run. Keeps replicas that did not perform the scrape from serving stale data.
    #[arg(long, env = "SNAPSHOT_REVALIDATE_INTERVAL", default_value = "30s", value_parser = parse_duration)]
    pub snapshot_revalidate_interval: Duration,

    /// Comma separated list of allowed origins; empty disables CORS headers.
    #[arg(
        long,
        env = "CORS_ALLOWED_ORIGINS",
        default_value = "",
        value_delimiter = ','
    )]
    pub cors_allowed_origins: Vec<String>,

    #[arg(long, env = "LOG_FORMAT", default_value = "json", value_enum)]
    pub log_format: LogFormat,

    #[arg(long, env = "MIGRATE_ON_STARTUP", default_value = "true", value_parser = BoolishValueParser::new(), action = ArgAction::Set)]
    pub migrate_on_startup: bool,

    #[arg(long, env = "SCRAPE_ENABLED", default_value = "true", value_parser = BoolishValueParser::new(), action = ArgAction::Set)]
    pub scrape_enabled: bool,

    /// `cron` crate format: sec min hour day-of-month month day-of-week.
    /// Default: every full hour (in `SCRAPE_TIMEZONE`).
    #[arg(long, env = "SCRAPE_CRON", default_value = "0 0 * * * *", value_parser = parse_cron)]
    pub scrape_cron: cron::Schedule,

    #[arg(long, env = "SCRAPE_TIMEZONE", default_value = "Europe/Berlin", value_parser = parse_timezone)]
    pub scrape_timezone: Tz,

    #[arg(long, env = "SCRAPE_BOOTSTRAP", default_value = "true", value_parser = BoolishValueParser::new(), action = ArgAction::Set)]
    pub scrape_bootstrap: bool,

    #[arg(long, env = "SCRAPE_BOOTSTRAP_MAX_AGE", default_value = "2h", value_parser = parse_duration)]
    pub scrape_bootstrap_max_age: Duration,

    #[arg(long, env = "SCRAPE_STALE_RUN_AFTER", default_value = "2h", value_parser = parse_duration)]
    pub scrape_stale_run_after: Duration,

    #[arg(
        long,
        env = "SCRAPE_BASE_URL",
        default_value = "https://greenmedical.health"
    )]
    pub scrape_base_url: Url,

    #[arg(long, env = "SCRAPE_USER_AGENT", default_value = DEFAULT_USER_AGENT)]
    pub scrape_user_agent: String,

    #[arg(long, env = "SCRAPE_REQUEST_TIMEOUT", default_value = "30s", value_parser = parse_duration)]
    pub scrape_request_timeout: Duration,

    #[arg(long, env = "SCRAPE_RETRY_TOTAL", default_value_t = 4)]
    pub scrape_retry_total: u32,

    #[arg(long, env = "SCRAPE_BACKOFF_FACTOR", default_value_t = 1.0)]
    pub scrape_backoff_factor: f64,

    #[arg(long, env = "SCRAPE_PHARMACY_DELAY", default_value = "300ms", value_parser = parse_duration)]
    pub scrape_pharmacy_delay: Duration,

    #[arg(long, env = "SCRAPE_PAGE_DELAY", default_value = "500ms", value_parser = parse_duration)]
    pub scrape_page_delay: Duration,

    #[arg(long, env = "SCRAPE_MIN_SUCCESS_RATIO", default_value_t = 0.5)]
    pub scrape_min_success_ratio: f64,

    /// Phase 2 of a run: scrape the product pages for reviews.
    #[arg(long, env = "REVIEWS_ENABLED", default_value = "true", value_parser = BoolishValueParser::new(), action = ArgAction::Set)]
    pub reviews_enabled: bool,

    /// Strains whose reviews are younger than this are skipped in phase 2.
    #[arg(long, env = "REVIEWS_MAX_AGE", default_value = "24h", value_parser = parse_duration)]
    pub reviews_max_age: Duration,

    /// Upper bound of product pages fetched per run (0 = unlimited).
    #[arg(long, env = "REVIEWS_MAX_PER_RUN", default_value_t = 0)]
    pub reviews_max_per_run: u32,

    /// Bearer token for `POST /api/v1/admin/scrape`; empty disables the endpoint.
    #[arg(long, env = "ADMIN_TOKEN", default_value = "", hide_env_values = true)]
    pub admin_token: String,

    /// Instance label stored on runs; defaults to `$HOSTNAME`.
    #[arg(long, env = "INSTANCE_NAME")]
    pub instance_name: Option<String>,

    /// Base URL of the frontend for links in e-mails (`/sorte/{id}`, `/abo/...`).
    #[arg(long, env = "PUBLIC_URL", default_value = "http://localhost:9000")]
    pub public_url: Url,

    /// `false` disables subscription creation and outbound e-mail delivery.
    #[arg(long, env = "EMAIL_ENABLED", default_value = "false", value_parser = BoolishValueParser::new(), action = ArgAction::Set)]
    pub email_enabled: bool,

    #[arg(long, env = "SMTP_HOST")]
    pub smtp_host: Option<String>,

    #[arg(long, env = "SMTP_PORT", default_value_t = 587)]
    pub smtp_port: u16,

    #[arg(long, env = "SMTP_USERNAME")]
    pub smtp_username: Option<String>,

    #[arg(long, env = "SMTP_PASSWORD", hide_env_values = true)]
    pub smtp_password: Option<String>,

    #[arg(long, env = "SMTP_TLS", default_value = "starttls", value_enum)]
    pub smtp_tls: SmtpTls,

    /// Sender of every e-mail (`Name <address>` or bare address).
    #[arg(
        long,
        env = "EMAIL_FROM",
        default_value = "GreenMedical Livebestand <noreply@localhost>"
    )]
    pub email_from: String,

    /// Maximum subscription creations (= confirmation mails) per client IP, in memory.
    #[arg(long, env = "SUBSCRIPTION_RATE_LIMIT", default_value = "5/1h")]
    pub subscription_rate_limit: RateLimit,
}

/// `database_url` with the password replaced by `***` (or `***` when unparsable).
fn redact_database_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(mut url) => {
            if url.password().is_some() {
                let _ = url.set_password(Some("***"));
            }
            url.to_string()
        }
        Err(_) => "***".to_owned(),
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &redact_database_url(&self.database_url))
            .field("database_max_connections", &self.database_max_connections)
            .field("http_bind", &self.http_bind)
            .field("metrics_bind", &self.metrics_bind)
            .field("http_request_timeout", &self.http_request_timeout)
            .field(
                "snapshot_revalidate_interval",
                &self.snapshot_revalidate_interval,
            )
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("log_format", &self.log_format)
            .field("migrate_on_startup", &self.migrate_on_startup)
            .field("scrape_enabled", &self.scrape_enabled)
            .field("scrape_cron", &self.scrape_cron.to_string())
            .field("scrape_timezone", &self.scrape_timezone)
            .field("scrape_bootstrap", &self.scrape_bootstrap)
            .field("scrape_bootstrap_max_age", &self.scrape_bootstrap_max_age)
            .field("scrape_stale_run_after", &self.scrape_stale_run_after)
            .field("scrape_base_url", &self.scrape_base_url)
            .field("scrape_user_agent", &self.scrape_user_agent)
            .field("scrape_request_timeout", &self.scrape_request_timeout)
            .field("scrape_retry_total", &self.scrape_retry_total)
            .field("scrape_backoff_factor", &self.scrape_backoff_factor)
            .field("scrape_pharmacy_delay", &self.scrape_pharmacy_delay)
            .field("scrape_page_delay", &self.scrape_page_delay)
            .field("scrape_min_success_ratio", &self.scrape_min_success_ratio)
            .field("reviews_enabled", &self.reviews_enabled)
            .field("reviews_max_age", &self.reviews_max_age)
            .field("reviews_max_per_run", &self.reviews_max_per_run)
            .field(
                "admin_token",
                &if self.admin_token().is_some() {
                    "***"
                } else {
                    ""
                },
            )
            .field("instance_name", &self.instance_name)
            .field("public_url", &self.public_url)
            .field("email_enabled", &self.email_enabled)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_username", &self.smtp_username)
            .field("smtp_password", &self.smtp_password.as_ref().map(|_| "***"))
            .field("smtp_tls", &self.smtp_tls)
            .field("email_from", &self.email_from)
            .field(
                "subscription_rate_limit",
                &self.subscription_rate_limit.to_string(),
            )
            .finish()
    }
}

impl std::fmt::Debug for Cli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cli")
            .field("command", &self.command)
            .field("config", &self.config)
            .finish()
    }
}

impl Config {
    /// Parse only the configuration part from an argument iterator (tests).
    pub fn parse_from_args<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        Self::try_parse_from(args)
    }

    /// Admin token, `None` when empty (endpoint disabled).
    pub fn admin_token(&self) -> Option<&str> {
        let token = self.admin_token.trim();
        (!token.is_empty()).then_some(token)
    }

    /// Resolved instance name: `INSTANCE_NAME`, then `$HOSTNAME`, then `/etc/hostname`.
    pub fn instance_name(&self) -> String {
        if let Some(name) = self.instance_name.as_deref().map(str::trim)
            && !name.is_empty()
        {
            return name.to_owned();
        }
        if let Ok(host) = std::env::var("HOSTNAME")
            && !host.trim().is_empty()
        {
            return host.trim().to_owned();
        }
        std::fs::read_to_string("/etc/hostname")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_owned())
    }

    /// Next scheduled scrape strictly after `now`, or `None` when the
    /// scheduler is disabled. Pure function of the configuration, so every
    /// replica reports the same value (`Metadata.next_run_at`).
    pub fn next_scrape_at(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        if !self.scrape_enabled {
            return None;
        }
        crate::scheduler::next_fire(&self.scrape_cron, self.scrape_timezone, now)
    }

    /// `Metadata.schedule`: active cron + timezone, `None` when disabled.
    pub fn schedule_dto(&self) -> Option<crate::domain::ScheduleDto> {
        self.scrape_enabled.then(|| crate::domain::ScheduleDto {
            cron: self.scrape_cron.source().to_owned(),
            timezone: self.scrape_timezone.name().to_owned(),
        })
    }

    /// Origins list without empty entries.
    pub fn cors_origins(&self) -> Vec<String> {
        self.cors_allowed_origins
            .iter()
            .map(|o| o.trim().to_owned())
            .filter(|o| !o.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &[&str]) -> Config {
        let mut args = vec!["greenmedical-backend", "--database-url", "postgres://x"];
        args.extend_from_slice(extra);
        Config::parse_from_args(args).expect("valid config")
    }

    #[test]
    fn defaults_match_contract() {
        let cfg = parse(&[]);
        assert_eq!(cfg.database_max_connections, 10);
        assert_eq!(cfg.http_bind.to_string(), "0.0.0.0:8080");
        assert_eq!(cfg.metrics_bind.to_string(), "0.0.0.0:9090");
        assert_eq!(cfg.http_request_timeout, Duration::from_secs(30));
        assert_eq!(cfg.snapshot_revalidate_interval, Duration::from_secs(30));
        assert!(cfg.cors_origins().is_empty());
        assert_eq!(cfg.log_format, LogFormat::Json);
        assert!(cfg.migrate_on_startup);
        assert!(cfg.scrape_enabled);
        assert_eq!(cfg.scrape_cron.source(), "0 0 * * * *");
        assert_eq!(cfg.scrape_timezone, chrono_tz::Europe::Berlin);
        assert!(cfg.scrape_bootstrap);
        assert_eq!(cfg.scrape_bootstrap_max_age, Duration::from_secs(2 * 3600));
        assert_eq!(cfg.scrape_stale_run_after, Duration::from_secs(2 * 3600));
        assert_eq!(cfg.scrape_base_url.as_str(), "https://greenmedical.health/");
        assert_eq!(cfg.scrape_user_agent, DEFAULT_USER_AGENT);
        assert_eq!(cfg.scrape_request_timeout, Duration::from_secs(30));
        assert_eq!(cfg.scrape_retry_total, 4);
        assert_eq!(cfg.scrape_backoff_factor, 1.0);
        assert_eq!(cfg.scrape_pharmacy_delay, Duration::from_millis(300));
        assert_eq!(cfg.scrape_page_delay, Duration::from_millis(500));
        assert_eq!(cfg.scrape_min_success_ratio, 0.5);
        assert!(cfg.reviews_enabled);
        assert_eq!(cfg.reviews_max_age, Duration::from_secs(24 * 3600));
        assert_eq!(cfg.reviews_max_per_run, 0);
        assert_eq!(cfg.admin_token(), None);
        assert_eq!(cfg.public_url.as_str(), "http://localhost:9000/");
        assert!(!cfg.email_enabled);
        assert_eq!(cfg.smtp_host, None);
        assert_eq!(cfg.smtp_port, 587);
        assert_eq!(cfg.smtp_tls, SmtpTls::Starttls);
        assert_eq!(
            cfg.email_from,
            "GreenMedical Livebestand <noreply@localhost>"
        );
        assert_eq!(
            cfg.subscription_rate_limit,
            RateLimit {
                count: 5,
                per: Duration::from_secs(3600)
            }
        );
    }

    #[test]
    fn rate_limit_parsing() {
        assert_eq!(
            "20/15m".parse::<RateLimit>().unwrap(),
            RateLimit {
                count: 20,
                per: Duration::from_secs(900)
            }
        );
        assert_eq!(
            " 1 / 30s ".trim().parse::<RateLimit>().unwrap(),
            RateLimit {
                count: 1,
                per: Duration::from_secs(30)
            }
        );
        for bad in ["5", "/1h", "0/1h", "5/0s", "x/1h", "5/never", "5/1h/2"] {
            assert!(
                bad.parse::<RateLimit>().is_err(),
                "{bad} should be rejected"
            );
        }
        assert_eq!(
            RateLimit {
                count: 5,
                per: Duration::from_secs(3600)
            }
            .to_string(),
            "5/1h"
        );
        let cfg = parse(&["--subscription-rate-limit", "3/10m"]);
        assert_eq!(cfg.subscription_rate_limit.count, 3);
        assert!(
            Config::parse_from_args([
                "greenmedical-backend",
                "--database-url",
                "postgres://x",
                "--subscription-rate-limit",
                "lots"
            ])
            .is_err()
        );
    }

    #[test]
    fn smtp_settings_and_redaction() {
        let cfg = parse(&[
            "--email-enabled",
            "true",
            "--smtp-host",
            "mail.test",
            "--smtp-port",
            "1025",
            "--smtp-tls",
            "none",
            "--smtp-username",
            "u",
            "--smtp-password",
            "pw-secret",
        ]);
        assert!(cfg.email_enabled);
        assert_eq!(cfg.smtp_host.as_deref(), Some("mail.test"));
        assert_eq!(cfg.smtp_port, 1025);
        assert_eq!(cfg.smtp_tls, SmtpTls::None);
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("pw-secret"), "{dbg}");
        assert!(dbg.contains("smtp_password: Some(\"***\")"), "{dbg}");
    }

    #[test]
    fn boolean_flags_accept_boolish_values() {
        let cfg = parse(&["--scrape-enabled", "false", "--migrate-on-startup", "0"]);
        assert!(!cfg.scrape_enabled);
        assert!(!cfg.migrate_on_startup);
    }

    #[test]
    fn cors_origins_are_split_and_trimmed() {
        let cfg = parse(&["--cors-allowed-origins", "https://a.test, https://b.test"]);
        assert_eq!(cfg.cors_origins(), ["https://a.test", "https://b.test"]);
    }

    #[test]
    fn debug_redacts_secrets() {
        let cfg = Config::parse_from_args([
            "greenmedical-backend",
            "--database-url",
            "postgres://user:hunter2@db:5432/gm",
            "--admin-token",
            "topsecret",
        ])
        .unwrap();
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("hunter2"), "{dbg}");
        assert!(!dbg.contains("topsecret"), "{dbg}");
        assert!(dbg.contains("postgres://user:***@db:5432/gm"), "{dbg}");
        assert!(dbg.contains("admin_token: \"***\""), "{dbg}");
        assert_eq!(redact_database_url("not a url"), "***");
        let cli = Cli::try_parse_from([
            "greenmedical-backend",
            "--database-url",
            "postgres://u:pw@h/d",
        ])
        .unwrap();
        assert!(!format!("{cli:?}").contains("pw@"));
    }

    #[test]
    fn admin_token_blank_is_disabled() {
        assert_eq!(parse(&["--admin-token", "  "]).admin_token(), None);
        assert_eq!(
            parse(&["--admin-token", "s3cret"]).admin_token(),
            Some("s3cret")
        );
    }

    #[test]
    fn scrape_once_accepts_reviews_only() {
        let cli = Cli::try_parse_from([
            "greenmedical-backend",
            "--database-url",
            "postgres://x",
            "scrape-once",
            "--reviews-only",
        ])
        .unwrap();
        assert_eq!(
            cli.command,
            Some(Command::ScrapeOnce { reviews_only: true })
        );
    }

    #[test]
    fn next_scrape_at_is_none_when_disabled() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-27T10:20:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let cfg = parse(&[]);
        assert_eq!(
            cfg.next_scrape_at(now).unwrap().to_rfc3339(),
            "2026-08-27T11:00:00+00:00"
        );
        let schedule = cfg.schedule_dto().unwrap();
        assert_eq!(
            (schedule.cron.as_str(), schedule.timezone.as_str()),
            ("0 0 * * * *", "Europe/Berlin")
        );
        let off = parse(&["--scrape-enabled", "false"]);
        assert_eq!(off.next_scrape_at(now), None);
        assert_eq!(off.schedule_dto(), None);
    }

    #[test]
    fn invalid_cron_is_rejected() {
        let args = [
            "greenmedical-backend",
            "--database-url",
            "postgres://x",
            "--scrape-cron",
            "nope",
        ];
        assert!(Config::parse_from_args(args).is_err());
    }
}
