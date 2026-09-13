//! What every handler shares: the store, the configuration and the mailer.

use std::sync::{Arc, Mutex};

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sha2::{Digest, Sha256};

use crate::config::{Config, SmtpConfig};

/// Cloned into every handler. Cheap: the store is a pool handle.
#[derive(Clone)]
pub struct AppState {
    /// Every SQL statement runs through this.
    pub store: skyspace_store::Store,
    /// Read at start-up, never changed.
    pub config: Arc<Config>,
    /// Where sign-in codes go.
    pub mailer: Mailer,
}

impl AppState {
    /// Assemble the state `main` serves.
    #[must_use]
    pub fn new(store: skyspace_store::Store, config: Config, mailer: Mailer) -> Self {
        Self {
            store,
            config: Arc::new(config),
            mailer,
        }
    }

    /// Default configuration and a capturing mailer, so no test needs SMTP.
    #[must_use]
    pub fn for_test(store: skyspace_store::Store) -> Self {
        Self::new(
            store,
            Config::for_test("postgres://test"),
            Mailer::Capture(Arc::default()),
        )
    }
}

/// A code the capturing mailer kept, for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentCode {
    /// The address.
    pub to: String,
    /// The six digits.
    pub code: String,
}

/// Why a code could not be mailed.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// SMTP refused the message.
    #[error("smtp transport failed")]
    Transport(#[from] lettre::transport::smtp::Error),
    /// The message could not be assembled.
    #[error("could not build the message")]
    Message(#[from] lettre::error::Error),
    /// An address is not a mailbox.
    #[error("address is not a mailbox")]
    Address(#[from] lettre::address::AddressError),
}

/// Enum, not a trait object: three cases and no plan for a fourth.
#[derive(Clone)]
pub enum Mailer {
    /// Production: a real SMTP relay.
    Smtp {
        /// The connection; boxed because the other variants are a pointer.
        transport: Box<AsyncSmtpTransport<Tokio1Executor>>,
        /// The `From:` address.
        from: Mailbox,
    },
    /// Tests: every code is kept in memory.
    Capture(Arc<Mutex<Vec<SentCode>>>),
    /// Development without SMTP: a log line says a code was issued, and
    /// never says to whom or which.
    Log,
}

impl Mailer {
    /// `Smtp` when the configuration names a relay, otherwise `Log`.
    ///
    /// # Errors
    /// `MailError::Address` when the `From:` address is not a mailbox;
    /// `Transport` when the relay name is unusable.
    pub fn from_config(smtp: Option<&SmtpConfig>) -> Result<Self, MailError> {
        let Some(smtp) = smtp else {
            return Ok(Self::Log);
        };
        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)?
            .port(smtp.port)
            .credentials(Credentials::new(
                smtp.username.clone(),
                smtp.password.clone(),
            ))
            .build();
        Ok(Self::Smtp {
            transport: Box::new(transport),
            from: smtp.from.parse()?,
        })
    }

    /// Deliver a sign-in code.
    ///
    /// # Errors
    /// `MailError` when the address is malformed or SMTP refuses the message.
    pub async fn send_login_code(&self, to: &str, code: &str) -> Result<(), MailError> {
        match self {
            Self::Smtp { transport, from } => {
                let message = Message::builder()
                    .from(from.clone())
                    .to(to.parse()?)
                    .subject("Your Skyspace sign-in code")
                    .body(format!(
                        "Your Skyspace sign-in code is {code}. It expires in ten minutes.\n\
                         If you did not ask for it, ignore this message."
                    ))?;
                transport.send(message).await?;
                Ok(())
            }
            Self::Capture(sent) => {
                if let Ok(mut sent) = sent.lock() {
                    sent.push(SentCode {
                        to: to.to_owned(),
                        code: code.to_owned(),
                    });
                }
                Ok(())
            }
            Self::Log => {
                tracing::info!(to_hash = %address_hash(to), "login code issued");
                Ok(())
            }
        }
    }

    /// Every code captured so far; empty for the other variants.
    #[must_use]
    pub fn captured(&self) -> Vec<SentCode> {
        match self {
            Self::Capture(sent) => sent.lock().map(|s| s.clone()).unwrap_or_default(),
            Self::Smtp { .. } | Self::Log => Vec::new(),
        }
    }
}

/// The first eight hex digits of the address's SHA-256: enough to match
/// two log lines, not enough to name a person.
fn address_hash(address: &str) -> String {
    let digest = Sha256::digest(address.trim().to_ascii_lowercase().as_bytes());
    crate::auth::hex(&digest[..4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capture_keeps_codes_and_log_keeps_nothing() {
        let mailer = Mailer::Capture(Arc::default());
        mailer
            .send_login_code("owl@rice.edu", "123456")
            .await
            .unwrap();
        assert_eq!(
            mailer.captured(),
            vec![SentCode {
                to: "owl@rice.edu".into(),
                code: "123456".into()
            }]
        );
        Mailer::Log
            .send_login_code("owl@rice.edu", "1")
            .await
            .unwrap();
        assert!(Mailer::Log.captured().is_empty());
        assert_eq!(address_hash("Owl@Rice.edu"), address_hash("owl@rice.edu"));
        assert_eq!(address_hash("x").len(), 8);
    }

    #[test]
    fn log_mailer_without_smtp() {
        assert!(matches!(Mailer::from_config(None), Ok(Mailer::Log)));
        let smtp = SmtpConfig {
            host: "smtp.example.invalid".into(),
            port: 587,
            username: "u".into(),
            password: "p".into(),
            from: "not a mailbox".into(),
        };
        assert!(matches!(
            Mailer::from_config(Some(&smtp)),
            Err(MailError::Address(_))
        ));
    }
}
