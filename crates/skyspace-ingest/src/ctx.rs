//! What every job shares: the store handle, the archive, and the handful
//! of ingest-only queries the store does not wrap. Each query here is
//! plain SQL over `ingest_runs`, `raw_responses`, `parse_stats` and
//! `sections`, documented at the call so a new member can paste it into
//! `psql`.

use std::collections::BTreeMap;
use std::time::Duration;

use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
use skyspace_store::Store;
use time::{Date, OffsetDateTime};

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

/// One `raw_responses` row, as the replay and diff commands read it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RawRow {
    /// Row id.
    pub id: i64,
    /// The run that fetched it.
    pub run_id: i64,
    /// The URL fetched.
    pub url: String,
    /// The term, for per-term documents.
    pub term_code: Option<String>,
    /// When.
    pub fetched_at: OffsetDateTime,
    /// The `Content-Type` header, for decoding.
    pub content_type: Option<String>,
    /// The archive key.
    pub sha256: Vec<u8>,
    /// The fetching run's outcome; null while it runs.
    pub run_outcome: Option<String>,
}

impl RawRow {
    /// The archive key as an array.
    ///
    /// # Errors
    /// `JobError::Corrupt` when the column is not 32 bytes.
    pub fn key(&self) -> Result<[u8; 32], JobError> {
        <[u8; 32]>::try_from(self.sha256.as_slice())
            .map_err(|_| JobError::Corrupt(format!("raw_responses {} sha256 length", self.id)))
    }
}

/// Which weekly work queue a CRN is due on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueColumn {
    /// `sections.xml_fetched_at`.
    Xml,
    /// `sections.detail_fetched_at`.
    Detail,
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
    /// ```sql
    /// select c.subject, count(*) from sections s join courses c on c.id = s.course_id
    /// where s.term_code = $1 and s.withdrawn_at is null group by c.subject;
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn live_section_counts(
        &self,
        term: TermCode,
    ) -> Result<BTreeMap<String, u64>, JobError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "select c.subject, count(*) from sections s join courses c on c.id = s.course_id \
             where s.term_code = $1 and s.withdrawn_at is null group by c.subject",
        )
        .bind(term.to_string())
        .fetch_all(self.store.pool())
        .await
        .map_err(skyspace_store::StoreError::from)?;
        Ok(rows
            .into_iter()
            .map(|(subject, n)| (subject, u64::try_from(n).unwrap_or(0)))
            .collect())
    }

    /// Fill rates per field over the last `FILL_HISTORY_RUNS` good runs of
    /// `job` (for the term when given) and `source`, newest first.
    ///
    /// ```sql
    /// select ps.field, ps.rows_seen, ps.rows_filled from parse_stats ps
    /// join ingest_runs r on r.id = ps.run_id
    /// where ps.source = $3 and r.id in (
    ///   select id from ingest_runs where job = $1 and outcome = 'ok'
    ///     and ($2::text is null or term_code = $2)
    ///   order by finished_at desc limit 7);
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn fill_history(
        &self,
        job: &str,
        term: Option<TermCode>,
        source: &str,
    ) -> Result<BTreeMap<String, Vec<f64>>, JobError> {
        let rows: Vec<(String, i32, i32)> = sqlx::query_as(
            "select ps.field, ps.rows_seen, ps.rows_filled from parse_stats ps \
             join ingest_runs r on r.id = ps.run_id \
             where ps.source = $3 and r.id in ( \
                select id from ingest_runs where job = $1 and outcome = 'ok' \
                  and ($2::text is null or term_code = $2) \
                order by finished_at desc limit $4)",
        )
        .bind(job)
        .bind(term.map(|t| t.to_string()))
        .bind(source)
        .bind(i64::try_from(FILL_HISTORY_RUNS).unwrap_or(7))
        .fetch_all(self.store.pool())
        .await
        .map_err(skyspace_store::StoreError::from)?;
        let mut out: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for (field, seen, filled) in rows {
            let rate = crate::guards::fill_rate(
                u32::try_from(seen).unwrap_or(0),
                u32::try_from(filled).unwrap_or(0),
            );
            out.entry(field).or_default().push(rate);
        }
        Ok(out)
    }

    /// CRNs whose weekly document is missing or older than `stale_after`,
    /// never-fetched first.
    ///
    /// ```sql
    /// select crn from sections where term_code = $1 and withdrawn_at is null
    ///   and (xml_fetched_at is null or xml_fetched_at < $2)
    /// order by xml_fetched_at nulls first, id;
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn crns_due(
        &self,
        term: TermCode,
        column: DueColumn,
        stale_after: Duration,
    ) -> Result<Vec<Crn>, JobError> {
        let cutoff = OffsetDateTime::now_utc() - stale_after;
        let sql = match column {
            DueColumn::Xml => {
                "select crn from sections where term_code = $1 and withdrawn_at is null \
                 and (xml_fetched_at is null or xml_fetched_at < $2) \
                 order by xml_fetched_at nulls first, id"
            }
            DueColumn::Detail => {
                "select crn from sections where term_code = $1 and withdrawn_at is null \
                 and (detail_fetched_at is null or detail_fetched_at < $2) \
                 order by detail_fetched_at nulls first, id"
            }
        };
        let rows: Vec<(i32,)> = sqlx::query_as(sql)
            .bind(term.to_string())
            .bind(cutoff)
            .fetch_all(self.store.pool())
            .await
            .map_err(skyspace_store::StoreError::from)?;
        rows.into_iter()
            .map(|(crn,)| {
                u32::try_from(crn)
                    .map(Crn)
                    .map_err(|_| JobError::Corrupt(format!("sections.crn {crn}")))
            })
            .collect()
    }

    /// Stamp `xml_fetched_at` on the CRN a feed was requested for. The
    /// `ASSOCIATED-SECTIONS` feed lists the sections associated with a
    /// CRN and may omit the CRN itself, which would otherwise stay due.
    ///
    /// ```sql
    /// update sections set xml_fetched_at = now() where term_code = $1 and crn = $2;
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn mark_xml_fetched(&self, term: TermCode, crn: Crn) -> Result<(), JobError> {
        sqlx::query("update sections set xml_fetched_at = now() where term_code = $1 and crn = $2")
            .bind(term.to_string())
            .bind(i32::try_from(crn.0).map_err(|_| JobError::Corrupt(format!("crn {crn}")))?)
            .execute(self.store.pool())
            .await
            .map_err(skyspace_store::StoreError::from)?;
        Ok(())
    }

    /// Every archived response of one source fetched on or after `since`
    /// (UTC midnight), oldest first, so a replay ends on the newest body.
    ///
    /// ```sql
    /// select rr.id, rr.run_id, rr.url, rr.term_code, rr.fetched_at, rr.content_type,
    ///        rr.sha256, r.outcome as run_outcome
    /// from raw_responses rr join ingest_runs r on r.id = rr.run_id
    /// where rr.source = $1 and rr.fetched_at >= $2 order by rr.fetched_at, rr.id;
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn raw_responses_since(
        &self,
        source: Source,
        since: Date,
    ) -> Result<Vec<RawRow>, JobError> {
        let rows: Vec<RawRow> = sqlx::query_as(
            "select rr.id, rr.run_id, rr.url, rr.term_code, rr.fetched_at, rr.content_type, \
                    rr.sha256, r.outcome as run_outcome \
             from raw_responses rr join ingest_runs r on r.id = rr.run_id \
             where rr.source = $1 and rr.fetched_at >= $2 order by rr.fetched_at, rr.id",
        )
        .bind(source.as_str())
        .bind(since.midnight().assume_utc())
        .fetch_all(self.store.pool())
        .await
        .map_err(skyspace_store::StoreError::from)?;
        Ok(rows)
    }

    /// The archived responses for one URL, newest first, with the outcome
    /// of the run that fetched each: what `skyspace archive diff` reads.
    ///
    /// ```sql
    /// select ... from raw_responses rr join ingest_runs r on r.id = rr.run_id
    /// where rr.url = $1 order by rr.fetched_at desc, rr.id desc limit $2;
    /// ```
    ///
    /// # Errors
    /// `JobError::Store`.
    pub async fn raw_history(&self, url: &str, limit: u32) -> Result<Vec<RawRow>, JobError> {
        let rows: Vec<RawRow> = sqlx::query_as(
            "select rr.id, rr.run_id, rr.url, rr.term_code, rr.fetched_at, rr.content_type, \
                    rr.sha256, r.outcome as run_outcome \
             from raw_responses rr join ingest_runs r on r.id = rr.run_id \
             where rr.url = $1 order by rr.fetched_at desc, rr.id desc limit $2",
        )
        .bind(url)
        .bind(i64::from(limit))
        .fetch_all(self.store.pool())
        .await
        .map_err(skyspace_store::StoreError::from)?;
        Ok(rows)
    }
}
