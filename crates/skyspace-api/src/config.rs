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
    /// The bind address is reachable from outside but the origin is plain
    /// `http://`, so the session cookie would go out without `Secure`.
    #[error(
        "SKYSPACE_PUBLIC_ORIGIN must be https:// when SKYSPACE_BIND is not a loopback address \
         (got {origin} on {bind})"
    )]
    InsecureOrigin {
        /// The origin as configured.
        origin: String,
        /// The bind address as configured.
        bind: SocketAddr,
    },
}

/// The origin `from_env` falls back to: the Vite dev server. `main` warns
/// when it is in use, because a deploy that forgot `SKYSPACE_PUBLIC_ORIGIN`
/// would otherwise fail its CSRF check silently.
pub const DEFAULT_PUBLIC_ORIGIN: &str = "http://127.0.0.1:5173";

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
    /// Mutating requests must carry a matching `Origin`. Must be `https://`
    /// unless `bind` is loopback; `DEFAULT_PUBLIC_ORIGIN` otherwise.
    pub public_origin: String,
    /// `SKYSPACE_TRUSTED_PROXY=1`: the server sits behind one proxy that
    /// appends the peer to `X-Forwarded-For`, so the last hop is the client.
    /// Off, the header is ignored and the socket peer is the client.
    pub trusted_proxy: bool,
    /// Cookie lifetime.
    pub session_ttl_days: u16,
    /// Sign-in code lifetime.
    pub login_code_ttl_minutes: u16,
    /// Ships as `["rice.edu"]`.
    pub allowed_email_domains: Vec<String>,
    /// `None` until a proxy terminates Rice SSO.
    pub sso: Option<SsoConfig>,
    /// `None` in development: nothing is mailed. The code reaches the log
    /// only with `log_login_codes` (see `reveals_login_codes`).
    pub smtp: Option<SmtpConfig>,
    /// Shown on the privacy page and in the User-Agent.
    pub contact_email: String,
    /// Log as JSON (production) rather than compact text.
    pub log_json: bool,
    /// `SKYSPACE_LOG_LOGIN_CODES=1`: without SMTP, log each sign-in code at
    /// `info` with a redacted address. Ignored when `log_json` is set, so a
    /// production log can never carry a code.
    pub log_login_codes: bool,
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
        let public_origin =
            lookup("SKYSPACE_PUBLIC_ORIGIN").unwrap_or_else(|| DEFAULT_PUBLIC_ORIGIN.to_owned());
        if !public_origin.starts_with("https://") && !bind.ip().is_loopback() {
            return Err(ConfigError::InsecureOrigin {
                origin: public_origin,
                bind,
            });
        }
        let flag = |name: &str| lookup(name).is_some_and(|v| v == "1" || v == "true");
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
            public_origin,
            trusted_proxy: flag("SKYSPACE_TRUSTED_PROXY"),
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
            log_json: flag("SKYSPACE_LOG_JSON"),
            log_login_codes: flag("SKYSPACE_LOG_LOGIN_CODES"),
        })
    }

    /// Whether `public_origin` is the built-in development default.
    #[must_use]
    pub fn uses_default_origin(&self) -> bool {
        self.public_origin == DEFAULT_PUBLIC_ORIGIN
    }

    /// Whether the log mailer may print codes: asked for, and not JSON logs.
    #[must_use]
    pub const fn reveals_login_codes(&self) -> bool {
        self.log_login_codes && !self.log_json
    }

    /// Defaults for tests: no SMTP, no SSO, a local origin.
    #[must_use]
    pub fn for_test(database_url: &str) -> Self {
        Self {
            database_url: database_url.to_owned(),
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            public_origin: DEFAULT_PUBLIC_ORIGIN.to_owned(),
            trusted_proxy: false,
            session_ttl_days: 30,
            login_code_ttl_minutes: 10,
            allowed_email_domains: vec!["rice.edu".to_owned()],
            sso: None,
            smtp: None,
            contact_email: "test@example.invalid".to_owned(),
            log_json: false,
            log_login_codes: false,
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
        assert!(!config.trusted_proxy);
        assert!(config.uses_default_origin());
        assert!(!config.reveals_login_codes());
    }

    #[test]
    fn a_public_bind_needs_an_https_origin() {
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_BIND", "0.0.0.0:8080"),
        ]);
        let refused = Config::from_lookup(|k| vars.get(k).cloned());
        assert!(
            matches!(refused, Err(ConfigError::InsecureOrigin { .. })),
            "{refused:?}"
        );
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_BIND", "0.0.0.0:8080"),
            ("SKYSPACE_PUBLIC_ORIGIN", "http://plans.example.edu"),
        ]);
        let refused = Config::from_lookup(|k| vars.get(k).cloned()).unwrap_err();
        assert!(refused.to_string().contains("https://"), "{refused}");
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_BIND", "0.0.0.0:8080"),
            ("SKYSPACE_PUBLIC_ORIGIN", "https://plans.example.edu"),
            ("SKYSPACE_TRUSTED_PROXY", "1"),
        ]);
        let config = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert!(!config.uses_default_origin());
        assert!(config.trusted_proxy);
        // Loopback may stay on plain http: the dev server.
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_PUBLIC_ORIGIN", "http://localhost:5173"),
        ]);
        assert!(Config::from_lookup(|k| vars.get(k).cloned()).is_ok());
    }

    #[test]
    fn login_codes_are_never_revealed_in_json_logs() {
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_LOG_LOGIN_CODES", "1"),
        ]);
        let dev = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert!(dev.reveals_login_codes());
        let vars = env(&[
            ("DATABASE_URL", "postgres://x"),
            ("SKYSPACE_LOG_LOGIN_CODES", "1"),
            ("SKYSPACE_LOG_JSON", "1"),
        ]);
        let prod = Config::from_lookup(|k| vars.get(k).cloned()).unwrap();
        assert!(prod.log_login_codes);
        assert!(!prod.reveals_login_codes());
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
