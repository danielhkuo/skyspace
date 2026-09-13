//! Configuration from the environment. `main` exits before binding a port
//! when anything is missing or malformed. No `current_term` here: the
//! current term is `terms.is_current` in Postgres, so rolling the term over
//! is not a deploy.

use std::net::SocketAddr;

/// Why configuration could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is unset.
    #[error("missing environment variable {0}")]
    Missing(&'static str),
    /// A variable is set to something unreadable.
    #[error("environment variable {name} is malformed: {detail}")]
    Malformed {
        /// Which variable.
        name: &'static str,
        /// What was wrong.
        detail: String,
    },
    /// `SKYSPACE_SSO_*` is set but the bind address is not loopback.
    #[error("SKYSPACE_BIND must be a loopback address when SSO is configured")]
    SsoNeedsLoopback,
}

/// The proxy-terminated single sign-on. The proxy sends the verified
/// identity in `identity_header`; the request is trusted only when
/// `secret_header` carries `shared_secret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoConfig {
    /// Header carrying the verified email.
    pub identity_header: String,
    /// Header carrying the shared secret.
    pub secret_header: String,
    /// The secret the proxy must send.
    pub shared_secret: String,
}

/// Where sign-in codes are mailed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpConfig {
    /// SMTP host.
    pub host: String,
    /// SMTP port.
    pub port: u16,
    /// Username.
    pub username: String,
    /// Password.
    pub password: String,
    /// The `From:` address.
    pub from: String,
}

/// Everything the server reads at start-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Postgres connection string.
    pub database_url: String,
    /// Default `127.0.0.1:8080`; must be loopback when `sso` is set.
    pub bind: SocketAddr,
    /// Mutating requests must carry a matching `Origin`.
    pub public_origin: String,
    /// Cookie lifetime.
    pub session_ttl_days: u16,
    /// Sign-in code lifetime.
    pub login_code_ttl_minutes: u16,
    /// Ships as `["rice.edu"]`.
    pub allowed_email_domains: Vec<String>,
    /// `None` until a proxy terminates Rice SSO.
    pub sso: Option<SsoConfig>,
    /// `None` in development: the code is logged, not mailed.
    pub smtp: Option<SmtpConfig>,
    /// Shown on the privacy page and in the User-Agent.
    pub contact_email: String,
    /// Log as JSON (production) rather than compact text.
    pub log_json: bool,
}

impl Config {
    /// Read every `SKYSPACE_*` variable, plus `DATABASE_URL`.
    ///
    /// # Errors
    /// Names the first missing or malformed variable.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Read from any name-to-value function; tests pass a map.
    ///
    /// # Errors
    /// Names the first missing or malformed variable.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let required = |name: &'static str| lookup(name).ok_or(ConfigError::Missing(name));
        let parsed = |name: &'static str, default: &str| -> Result<u16, ConfigError> {
            lookup(name)
                .unwrap_or_else(|| default.to_owned())
                .parse()
                .map_err(|e: std::num::ParseIntError| ConfigError::Malformed {
                    name,
                    detail: e.to_string(),
                })
        };
        let bind: SocketAddr = lookup("SKYSPACE_BIND")
            .unwrap_or_else(|| "127.0.0.1:8080".to_owned())
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::Malformed {
                name: "SKYSPACE_BIND",
                detail: e.to_string(),
            })?;
        let sso = lookup("SKYSPACE_SSO_SHARED_SECRET").map(|shared_secret| SsoConfig {
            identity_header: lookup("SKYSPACE_SSO_IDENTITY_HEADER")
                .unwrap_or_else(|| "x-skyspace-identity".to_owned()),
            secret_header: lookup("SKYSPACE_SSO_SECRET_HEADER")
                .unwrap_or_else(|| "x-skyspace-sso-secret".to_owned()),
            shared_secret,
        });
        if sso.is_some() && !bind.ip().is_loopback() {
            return Err(ConfigError::SsoNeedsLoopback);
        }
        let smtp = match lookup("SKYSPACE_SMTP_HOST") {
            Some(host) => Some(SmtpConfig {
                host,
                port: parsed("SKYSPACE_SMTP_PORT", "587")?,
                username: required("SKYSPACE_SMTP_USERNAME")?,
                password: required("SKYSPACE_SMTP_PASSWORD")?,
                from: required("SKYSPACE_SMTP_FROM")?,
            }),
            None => None,
        };
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind,
            public_origin: lookup("SKYSPACE_PUBLIC_ORIGIN")
                .unwrap_or_else(|| "http://127.0.0.1:5173".to_owned()),
            session_ttl_days: parsed("SKYSPACE_SESSION_TTL_DAYS", "30")?,
            login_code_ttl_minutes: parsed("SKYSPACE_LOGIN_CODE_TTL_MINUTES", "10")?,
            allowed_email_domains: lookup("SKYSPACE_ALLOWED_EMAIL_DOMAINS")
                .unwrap_or_else(|| "rice.edu".to_owned())
                .split(',')
                .map(|d| d.trim().to_ascii_lowercase())
                .filter(|d| !d.is_empty())
                .collect(),
            sso,
            smtp,
            contact_email: lookup("SKYSPACE_CONTACT_EMAIL")
                .unwrap_or_else(|| "contact@example.invalid".to_owned()),
            log_json: lookup("SKYSPACE_LOG_JSON").is_some_and(|v| v == "1" || v == "true"),
        })
    }

    /// Defaults for tests: no SMTP, no SSO, a local origin.
    #[must_use]
    pub fn for_test(database_url: &str) -> Self {
        Self {
            database_url: database_url.to_owned(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            public_origin: "http://127.0.0.1:5173".to_owned(),
            session_ttl_days: 30,
            login_code_ttl_minutes: 10,
            allowed_email_domains: vec!["rice.edu".to_owned()],
            sso: None,
            smtp: None,
            contact_email: "test@example.invalid".to_owned(),
            log_json: false,
        }
    }

    /// Whether an address may sign in.
    #[must_use]
    pub fn email_allowed(&self, email: &str) -> bool {
        let Some((local, domain)) = email.rsplit_once('@') else {
            return false;
        };
        !local.is_empty()
            && self
                .allowed_email_domains
                .iter()
                .any(|d| d.eq_ignore_ascii_case(domain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn defaults_fill_in_and_database_url_is_required() {
        let missing = Config::from_lookup(|_| None);
        assert_eq!(missing, Err(ConfigError::Missing("DATABASE_URL")));
        let vars = env(&[("DATABASE_URL", "postgres://x")]);
        let config = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert_eq!(config.bind.port(), 8080);
        assert_eq!(config.allowed_email_domains, vec!["rice.edu"]);
        assert!(config.sso.is_none());
        assert!(config.smtp.is_none());
    }

    #[test]
    fn sso_refuses_a_public_bind() {
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_BIND", "0.0.0.0:8080"),
            ("SKYSPACE_SSO_SHARED_SECRET", "s"),
        ]);
        assert_eq!(
            Config::from_lookup(|k| vars.get(k).cloned()),
            Err(ConfigError::SsoNeedsLoopback)
        );
    }

    #[test]
    fn email_domains_are_checked_case_insensitively() {
        let config = Config::for_test("postgres://x");
        assert!(config.email_allowed("owl@Rice.EDU"));
        assert!(!config.email_allowed("owl@alumni.rice.edu"));
        assert!(!config.email_allowed("@rice.edu"));
        assert!(!config.email_allowed("owl"));
    }
}
