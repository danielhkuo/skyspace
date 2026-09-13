//! What every handler shares: the store, the configuration and the mailer.

use std::sync::{Arc, Mutex};

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sha2::{Digest, Sha256};

use crate::config::Config;

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
    /// Development without SMTP. A log line says a code was issued for a
    /// hashed address; with `reveal_codes` (`Config::reveals_login_codes`:
    /// `SKYSPACE_LOG_LOGIN_CODES=1` and not JSON logs) the line also carries
    /// the code and a redacted address, so a developer can sign in. The
    /// full address is never logged.
    Log {
        /// Print the code and a redacted address.
        reveal_codes: bool,
    },
}

impl Mailer {
    /// `Smtp` when the configuration names a relay, otherwise `Log`, which
    /// prints codes only when `config.reveals_login_codes()`.
    ///
    /// # Errors
    /// `MailError::Address` when the `From:` address is not a mailbox;
    /// `Transport` when the relay name is unusable.
    pub fn from_config(config: &Config) -> Result<Self, MailError> {
        let Some(smtp) = config.smtp.as_ref() else {
            return Ok(Self::Log {
                reveal_codes: config.reveals_login_codes(),
            });
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
            Self::Log { reveal_codes: true } => {
                tracing::info!(
                    to_hash = %address_hash(to),
                    to = %redact_address(to),
                    code,
                    "login code issued (not mailed: no SMTP configured)"
                );
                Ok(())
            }
            Self::Log {
                reveal_codes: false,
            } => {
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
            Self::Smtp { .. } | Self::Log { .. } => Vec::new(),
        }
    }
}

/// The first eight hex digits of the address's SHA-256: enough to match
/// two log lines, not enough to name a person.
fn address_hash(address: &str) -> String {
    let digest = Sha256::digest(address.trim().to_ascii_lowercase().as_bytes());
    crate::auth::hex(&digest[..4])
}

/// `ow…@rice.edu`: the first two characters of the local part and the
/// domain. A local part of one or two characters is dropped entirely, so
/// at least one character is always hidden and the line never spells out
/// a whole address.
fn redact_address(address: &str) -> String {
    let address = address.trim();
    let (local, domain) = address.rsplit_once('@').unwrap_or((address, ""));
    let shown: String = if local.chars().count() >= 3 {
        local.chars().take(2).collect()
    } else {
        String::new()
    };
    format!("{shown}…@{domain}")
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
        let quiet = Mailer::Log {
            reveal_codes: false,
        };
        quiet.send_login_code("owl@rice.edu", "1").await.unwrap();
        assert!(quiet.captured().is_empty());
        assert_eq!(address_hash("Owl@Rice.edu"), address_hash("owl@rice.edu"));
        assert_eq!(address_hash("x").len(), 8);
    }

    #[test]
    fn addresses_are_redacted_to_two_characters_and_the_domain() {
        assert_eq!(redact_address("owl@rice.edu"), "ow…@rice.edu");
        assert_eq!(redact_address(" Owlet@rice.edu "), "Ow…@rice.edu");
        assert_eq!(redact_address("abc@rice.edu"), "ab…@rice.edu");
        assert_eq!(redact_address("ab@rice.edu"), "…@rice.edu");
        assert_eq!(redact_address("nonsense"), "no…@");
    }

    /// A subscriber whose output the test can read back.
    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
        type Writer = Self;
        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }

    async fn logged(mailer: Mailer) -> String {
        let sink = Sink::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(sink.clone())
            .with_ansi(false)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        mailer
            .send_login_code("owl@rice.edu", "424242")
            .await
            .unwrap();
        String::from_utf8(sink.0.lock().unwrap().clone()).unwrap()
    }

    #[tokio::test]
    async fn the_log_mailer_prints_the_code_only_when_asked() {
        let quiet = logged(Mailer::Log {
            reveal_codes: false,
        })
        .await;
        assert!(quiet.contains("login code issued"), "{quiet}");
        assert!(!quiet.contains("424242"), "{quiet}");
        assert!(!quiet.contains("rice.edu"), "{quiet}");
        let loud = logged(Mailer::Log { reveal_codes: true }).await;
        assert!(loud.contains("424242"), "{loud}");
        assert!(loud.contains("ow…@rice.edu"), "{loud}");
        assert!(!loud.contains("owl@rice.edu"), "{loud}");
    }

    #[test]
    fn log_mailer_without_smtp() {
        let mut config = Config::for_test("postgres://x");
        assert!(matches!(
            Mailer::from_config(&config),
            Ok(Mailer::Log {
                reveal_codes: false
            })
        ));
        config.log_login_codes = true;
        assert!(matches!(
            Mailer::from_config(&config),
            Ok(Mailer::Log { reveal_codes: true })
        ));
        config.log_json = true;
        assert!(matches!(
            Mailer::from_config(&config),
            Ok(Mailer::Log {
                reveal_codes: false
            })
        ));
        config.smtp = Some(crate::config::SmtpConfig {
            host: "smtp.example.invalid".into(),
            port: 587,
            username: "u".into(),
            password: "p".into(),
            from: "not a mailbox".into(),
        });
        assert!(matches!(
            Mailer::from_config(&config),
            Err(MailError::Address(_))
        ));
    }
}
