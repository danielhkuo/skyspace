//! Configuration from the environment, loaded through `dotenvy` first.

use std::path::PathBuf;

use skyspace_ingest::{Archive, Fetcher, JobCtx};
use skyspace_store::Store;

/// Everything a command reads at start-up.
#[derive(Debug, Clone)]
pub struct Config {
    /// `DATABASE_URL`.
    pub database_url: String,
    /// `SKYSPACE_ARCHIVE_DIR`, default `./archive`.
    pub archive_dir: PathBuf,
    /// `SKYSPACE_CONTACT_EMAIL`: the address in the User-Agent. Required
    /// before any request leaves for Rice.
    pub contact_email: Option<String>,
    /// `SKYSPACE_USER_AGENT_SITE`: the site named in the User-Agent.
    pub user_agent_site: String,
    /// `SKYSPACE_LOG_JSON=1`.
    pub log_json: bool,
}

/// Why configuration could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is unset.
    #[error("missing environment variable {0}")]
    Missing(&'static str),
}

impl Config {
    /// Read the environment. Only `DATABASE_URL` is required to start;
    /// the contact address is checked when a pull is about to run.
    ///
    /// # Errors
    /// `ConfigError::Missing` for `DATABASE_URL`.
    pub fn from_env() -> Result<Self, ConfigError> {
        let get = |name: &'static str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        Ok(Self {
            database_url: get("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?,
            archive_dir: get("SKYSPACE_ARCHIVE_DIR")
                .map_or_else(|| PathBuf::from("./archive"), PathBuf::from),
            contact_email: get("SKYSPACE_CONTACT_EMAIL"),
            user_agent_site: get("SKYSPACE_USER_AGENT_SITE")
                .unwrap_or_else(|| "skyspace.rice.edu".to_owned()),
            log_json: get("SKYSPACE_LOG_JSON").is_some_and(|v| v == "1" || v == "true"),
        })
    }

    /// Open the pool. Small: one operator process at a time.
    ///
    /// # Errors
    /// The store's connection error.
    pub async fn store(&self) -> anyhow::Result<Store> {
        Ok(Store::connect(&self.database_url, 4).await?)
    }

    /// The archive handle.
    #[must_use]
    pub fn archive(&self) -> Archive {
        Archive::new(&self.archive_dir)
    }

    /// Store plus archive.
    ///
    /// # Errors
    /// The store's connection error.
    pub async fn job_ctx(&self) -> anyhow::Result<JobCtx> {
        Ok(JobCtx::new(self.store().await?, self.archive()))
    }

    /// The identified User-Agent, or an error when no contact address is
    /// set: a pull without one is not polite.
    ///
    /// # Errors
    /// When `SKYSPACE_CONTACT_EMAIL` is unset.
    pub fn user_agent(&self) -> anyhow::Result<String> {
        let contact = self.contact_email.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "SKYSPACE_CONTACT_EMAIL is not set; Rice must be able to reach us before we fetch"
            )
        })?;
        Ok(skyspace_ingest::fetch::user_agent(
            &self.user_agent_site,
            contact,
        ))
    }

    /// The real fetcher, identified.
    ///
    /// # Errors
    /// A missing contact address, or a TLS backend that cannot start.
    pub fn fetcher(&self) -> anyhow::Result<Fetcher> {
        Ok(Fetcher::new(self.archive(), self.user_agent()?)?)
    }
}
