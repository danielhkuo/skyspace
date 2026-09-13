//! What every job shares: the store handle and the archive. Every SQL
//! statement lives in `skyspace-store`; the handful of ingest-shaped reads
//! here are thin wrappers that turn the store's rows into the guards'
//! shapes.

use std::collections::BTreeMap;
use std::time::Duration;

use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
pub use skyspace_store::DueColumn;
pub use skyspace_store::RawResponseRow as RawRow;
use skyspace_store::{RunKey, Store};
use time::Date;

use crate::archive::Archive;
use crate::error::JobError;
use crate::fetch::Source;
use crate::guards::FILL_HISTORY_RUNS;

/// The store and archive a job runs against.
#[derive(Clone)]
pub struct JobCtx {
    /// Every SQL statement goes through this.
    pub store: Store,
    /// Where the bytes go before anything parses them.
    pub archive: Archive,
}

impl std::fmt::Debug for JobCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobCtx")
            .field("archive", &self.archive)
            .finish_non_exhaustive()
    }
}

impl JobCtx {
    /// Wrap a store and an archive.
    #[must_use]
    pub fn new(store: Store, archive: Archive) -> Self {
        Self { store, archive }
    }

    /// Live section count per subject for the term: what "rows in the
    /// last good run" means for the zero-rows guard.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn live_section_counts(
        &self,
        term: TermCode,
    ) -> Result<BTreeMap<String, u64>, JobError> {
        Ok(self.store.live_section_counts(term).await?)
    }

    /// Fill rates per field over the last `FILL_HISTORY_RUNS` good runs of
    /// `job` under `key` and `source`, newest first.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn fill_history(
        &self,
        job: &str,
        key: Option<RunKey>,
        source: &str,
    ) -> Result<BTreeMap<String, Vec<f64>>, JobError> {
        let runs = u32::try_from(FILL_HISTORY_RUNS).unwrap_or(7);
        let rows = self.store.fill_history(job, key, source, runs).await?;
        let mut out: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (field, seen, filled) in rows {
            out.entry(field)
                .or_default()
                .push(crate::guards::fill_rate(seen, filled));
        }
        Ok(out)
    }

    /// CRNs whose weekly document is missing or older than `stale_after`,
    /// never-fetched first.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn crns_due(
        &self,
        term: TermCode,
        column: DueColumn,
        stale_after: Duration,
    ) -> Result<Vec<Crn>, JobError> {
        Ok(self.store.crns_due(term, column, stale_after).await?)
    }

    /// Stamp `xml_fetched_at` on the CRN a feed was requested for. The
    /// `ASSOCIATED-SECTIONS` feed lists the sections associated with a
    /// CRN and may omit the CRN itself, which would otherwise stay due.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn mark_xml_fetched(&self, term: TermCode, crn: Crn) -> Result<(), JobError> {
        self.store.mark_xml_fetched(term, crn).await?;
        Ok(())
    }

    /// What a replay reads: for every URL of `source` fetched on or after
    /// `since` (UTC midnight), the newest body a run ending `ok` fetched,
    /// oldest first.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn raw_responses_since(
        &self,
        source: Source,
        since: Date,
    ) -> Result<Vec<RawRow>, JobError> {
        Ok(self.store.raw_responses_since(source, since).await?)
    }

    /// The archived responses for one URL, newest first, with the outcome
    /// of the run that fetched each: what `skyspace archive diff` reads.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn raw_history(&self, url: &str, limit: u32) -> Result<Vec<RawRow>, JobError> {
        Ok(self.store.raw_history(url, limit).await?)
    }
}
