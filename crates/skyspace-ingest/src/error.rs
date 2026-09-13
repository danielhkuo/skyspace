//! The two error types callers see: one for a single fetch, one for a job.

use skyspace_core::TermCode;
use skyspace_parse::ParseError;
use skyspace_store::StoreError;

/// Why one fetch failed after its retries.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The client could not connect, timed out, or the body did not arrive.
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    /// A status we do not retry, such as 404, or a retried status that
    /// never cleared.
    #[error("status {0}")]
    Status(u16),
    /// The bytes could not be written to the archive.
    #[error("archive: {0}")]
    Archive(#[from] std::io::Error),
    /// The body is larger than `fetch::MAX_BODY_BYTES`; nothing was
    /// archived. Not retried.
    #[error("body of {0} bytes is over the cap")]
    TooLarge(u64),
}

impl FetchError {
    /// Whether this failure counts toward the consecutive-failure stop.
    /// Transport errors and statuses we retried are "Rice is down";
    /// a page that is gone (404, 410) or too large is that page's problem
    /// and the run moves on, with the failure still counted.
    #[must_use]
    pub const fn is_outage(&self) -> bool {
        match self {
            // A disk that cannot take the bytes stops the run the same way.
            Self::Transport(_) | Self::Archive(_) => true,
            Self::Status(status) => !matches!(*status, 404 | 410),
            Self::TooLarge(_) => false,
        }
    }
}

/// Why a job could not run to a recorded end. A run that the guards
/// quarantined or that stopped after repeated fetch failures is not an
/// error: it returns a `Summary` whose outcome says so.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Another process holds this job's advisory lock; the run was skipped.
    #[error("another run of this job holds the lock")]
    Locked,
    /// The database refused.
    #[error("store: {0}")]
    Store(#[from] StoreError),
    /// The archive could not be read or written.
    #[error("archive: {0}")]
    Archive(#[from] std::io::Error),
    /// A fetch failed after its retries.
    #[error("fetch: {0}")]
    Fetch(#[from] FetchError),
    /// A page could not be parsed at all.
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
    /// A value could not be serialised for a `jsonb` column.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A URL could not be built.
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
    /// The requested term is not in Rice's `TERMS` list, so a listing pull
    /// would silently receive the current term's rows.
    #[error("term {0} is not in Rice's TERMS list")]
    UnknownTerm(TermCode),
    /// Five requests in a row failed: Rice is down, stop and alert.
    #[error("{0} consecutive fetch failures; stopping")]
    ConsecutiveFailures(u32),
    /// A value in an archived row or URL could not be read back.
    #[error("archived row: {0}")]
    Corrupt(String),
}
