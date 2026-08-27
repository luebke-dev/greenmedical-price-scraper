//! Outbound e-mail: the [`Mailer`] abstraction, its log/SMTP implementations
//! and the German message templates for the price-alert subscriptions.

pub mod templates;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use rand::RngCore;
use tracing::info;

use crate::config::{Config, SmtpTls};

/// One message to a single recipient with a plain-text and an HTML variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

/// Sends e-mails. Implementations must be cheap to share behind an `Arc`.
pub trait Mailer: Send + Sync {
    fn send(&self, email: Email) -> SendFuture<'_>;
}

/// `EMAIL_ENABLED=false`: records only that delivery was skipped and sends nothing.
///
/// Recipient addresses and message bodies can contain personal data and bearer tokens, so
/// they must never be written to production logs.
#[derive(Debug, Default, Clone, Copy)]
pub struct LogMailer;

impl Mailer for LogMailer {
    fn send(&self, _email: Email) -> SendFuture<'_> {
        Box::pin(async move {
            info!("e-mail not sent (EMAIL_ENABLED=false)");
            Ok(())
        })
    }
}

/// SMTP delivery via `lettre` (rustls, no OpenSSL).
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    /// Build the transport from `SMTP_*` / `EMAIL_FROM`; fails on missing host
    /// or an unparsable sender so misconfiguration surfaces at start-up.
    pub fn from_config(config: &Config) -> anyhow::Result<Self> {
        let host = config
            .smtp_host
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .ok_or_else(|| anyhow::anyhow!("EMAIL_ENABLED=true requires SMTP_HOST"))?;
        let from: Mailbox = config
            .email_from
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid EMAIL_FROM {:?}: {e}", config.email_from))?;
        let mut builder = match config.smtp_tls {
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
                .tls(Tls::Required(TlsParameters::new(host.to_owned())?)),
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
                .tls(Tls::Wrapper(TlsParameters::new(host.to_owned())?)),
            SmtpTls::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).tls(Tls::None)
            }
        }
        .port(config.smtp_port);
        if let (Some(user), Some(password)) = (&config.smtp_username, &config.smtp_password)
            && !user.is_empty()
        {
            builder = builder.credentials(Credentials::new(user.clone(), password.clone()));
        }
        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, email: Email) -> SendFuture<'_> {
        Box::pin(async move {
            let to: Mailbox = email
                .to
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid recipient {:?}: {e}", email.to))?;
            let message = Message::builder()
                .from(self.from.clone())
                .to(to)
                .subject(&email.subject)
                .multipart(MultiPart::alternative_plain_html(
                    email.text.clone(),
                    email.html.clone(),
                ))?;
            self.transport.send(message).await?;
            info!(to = %email.to, subject = %email.subject, "e-mail sent");
            Ok(())
        })
    }
}

/// Test double: records every message; `fail_next` makes the next send error.
#[derive(Debug, Default)]
pub struct RecordingMailer {
    sent: Mutex<Vec<Email>>,
    fail: Mutex<bool>,
}

impl RecordingMailer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn sent(&self) -> Vec<Email> {
        self.sent.lock().expect("mailer lock").clone()
    }

    pub fn clear(&self) {
        self.sent.lock().expect("mailer lock").clear();
    }

    /// Make the next `send` fail with an error (the message is not recorded).
    pub fn fail_next(&self) {
        *self.fail.lock().expect("mailer lock") = true;
    }
}

impl Mailer for RecordingMailer {
    fn send(&self, email: Email) -> SendFuture<'_> {
        Box::pin(async move {
            if std::mem::take(&mut *self.fail.lock().expect("mailer lock")) {
                anyhow::bail!("simulated send failure");
            }
            self.sent.lock().expect("mailer lock").push(email);
            Ok(())
        })
    }
}

/// Mailer according to `EMAIL_ENABLED` and the SMTP settings.
pub fn mailer_from_config(config: &Config) -> anyhow::Result<Arc<dyn Mailer>> {
    if config.email_enabled {
        Ok(Arc::new(SmtpMailer::from_config(config)?))
    } else {
        Ok(Arc::new(LogMailer))
    }
}

/// Length in bytes of the random part of every token.
pub const TOKEN_BYTES: usize = 32;

/// 32 random bytes, base64url without padding (43 characters).
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_32_random_bytes_base64url() {
        let token = generate_token();
        assert_eq!(token.len(), 43, "{token}");
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "{token}"
        );
        let decoded = URL_SAFE_NO_PAD.decode(&token).unwrap();
        assert_eq!(decoded.len(), TOKEN_BYTES);
        assert_ne!(token, generate_token());
    }

    #[test]
    fn smtp_mailer_requires_host_and_valid_sender() {
        let base = ["greenmedical-backend", "--database-url", "postgres://x"];
        let cfg = Config::parse_from_args(base).unwrap();
        assert!(SmtpMailer::from_config(&cfg).is_err());
        let mut args = base.to_vec();
        args.extend(["--smtp-host", "mail.test", "--email-from", "not a mailbox"]);
        let cfg = Config::parse_from_args(args).unwrap();
        assert!(SmtpMailer::from_config(&cfg).is_err());
        for tls in ["starttls", "tls", "none"] {
            let mut args = base.to_vec();
            args.extend(["--smtp-host", "mail.test", "--smtp-tls", tls]);
            let cfg = Config::parse_from_args(args).unwrap();
            assert!(SmtpMailer::from_config(&cfg).is_ok(), "{tls}");
        }
    }

    #[tokio::test]
    async fn recording_mailer_records_and_fails_on_demand() {
        let mailer = RecordingMailer::new();
        let email = Email {
            to: "a@b.test".into(),
            subject: "s".into(),
            text: "t".into(),
            html: "<p>t</p>".into(),
        };
        mailer.send(email.clone()).await.unwrap();
        mailer.fail_next();
        assert!(mailer.send(email.clone()).await.is_err());
        mailer.send(email.clone()).await.unwrap();
        assert_eq!(mailer.sent(), vec![email.clone(), email]);
        LogMailer.send(mailer.sent().remove(0)).await.unwrap();
    }
}
