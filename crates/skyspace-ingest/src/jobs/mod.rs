//! The timer jobs, and the harness they share: take the advisory lock,
//! open an `ingest_runs` row, count every request and failure, stop after
//! five failures in a row, close the row with an outcome.

mod catalog;
mod detail;
mod listings;
mod reference;
mod requirements;
mod seats;
mod sections;

use std::collections::BTreeMap;
use std::time::Instant;

pub use skyspace_store::RunOutcome;

use skyspace_core::term::TermCode;
use skyspace_parse::{ParseReport, RefKind, ReferenceEntry, parse_reference_list};
use skyspace_store::{IssueSeverity, JobLock, RawResponseInput, RunKey, RunSummaryRow};
use time::OffsetDateTime;
use url::Url;

pub use catalog::pull_catalog;
pub use detail::pull_course_detail;
pub use listings::pull_section_listings;
pub use reference::pull_reference_lists;
pub use requirements::pull_requirements;
pub use seats::poll_seats;
pub use sections::{pull_section_xml, xml_row};

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::{Fetch, FetchOutcome, MAX_CONSECUTIVE_FAILURES, Source};
use crate::urls;

/// Printed by the CLI, stored in `ingest_runs`. `requests < targets` means
/// the run stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Requests made.
    pub requests: u32,
    /// Requests planned.
    pub targets: u32,
    /// Requests that failed after retries.
    pub failures: u32,
    /// Bytes fetched.
    pub bytes: u64,
    /// Rows written to layer-2 tables.
    pub rows_written: u64,
    /// Parse issues recorded.
    pub issues: u32,
    /// How it ended.
    pub outcome: RunOutcome,
    /// When it started.
    pub started: OffsetDateTime,
    /// When it finished.
    pub finished: OffsetDateTime,
    /// The failure text, the quarantine reason, or why the run did nothing.
    pub note: Option<String>,
}

impl Summary {
    /// A run that did nothing, with the reason: outside a poll window, or
    /// the last good run is still fresh.
    #[must_use]
    pub fn nothing_to_do(note: impl Into<String>) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            requests: 0,
            targets: 0,
            failures: 0,
            bytes: 0,
            rows_written: 0,
            issues: 0,
            outcome: RunOutcome::Ok,
            started: now,
            finished: now,
            note: Some(note.into()),
        }
    }

    /// One line of plain English with the numbers.
    #[must_use]
    pub fn line(&self, job: &str) -> String {
        let seconds = (self.finished - self.started).as_seconds_f64();
        let mut text = format!(
            "{job}: {} of {} requests, {} failed, {} bytes, {} rows written, {} issues, {} in {seconds:.1} s",
            self.requests,
            self.targets,
            self.failures,
            self.bytes,
            self.rows_written,
            self.issues,
            outcome_text(self.outcome),
        );
        if let Some(note) = &self.note {
            text.push_str("; ");
            text.push_str(note);
        }
        text
    }
}

/// The stored text of an outcome.
#[must_use]
pub const fn outcome_text(outcome: RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Ok => "ok",
        RunOutcome::Failed => "failed",
        RunOutcome::Quarantined => "quarantined",
    }
}

/// The process exit code for an outcome: 0, 1, 2.
#[must_use]
pub const fn exit_code(outcome: RunOutcome) -> i32 {
    match outcome {
        RunOutcome::Ok => 0,
        RunOutcome::Failed => 1,
        RunOutcome::Quarantined => 2,
    }
}

/// Job names as `ingest_runs.job` records them. The store's
/// `DATA_VERSION_JOBS` names the ones whose good run moves the catalog
/// `ETag`; a test below keeps the two lists in step.
pub mod job_names {
    /// `pull reference`.
    pub const REFERENCE: &str = "reference";
    /// `pull catalog`.
    pub const CATALOG: &str = "catalog";
    /// `pull listings`.
    pub const LISTINGS: &str = "listings";
    /// `pull sections`.
    pub const SECTIONS: &str = "sections";
    /// `pull detail`.
    pub const DETAIL: &str = "detail";
    /// `pull seats`.
    pub const SEATS: &str = "seats";
    /// `pull requirements`.
    pub const REQUIREMENTS: &str = "requirements";
    /// `replay`.
    pub const REPLAY: &str = "replay";
    /// `import university`.
    pub const IMPORT: &str = "import";
}

/// The advisory lock key for a job: FNV-1a over `skyspace-ingest/<job>`,
/// spelled out here so the key is the same in every build and Rust
/// release (`DefaultHasher` promises neither), and an old and a new binary
/// in a rolling deploy still serialise.
#[must_use]
pub fn lock_key(job: &str) -> i64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in b"skyspace-ingest/".iter().chain(job.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    i64::from_ne_bytes(hash.to_ne_bytes())
}

/// The sums a run accumulates, plus the per-field fill counts that become
/// `parse_stats`.
#[derive(Debug, Default)]
pub(crate) struct Counters {
    requests: u32,
    targets: u32,
    failures: u32,
    consecutive_failures: u32,
    bytes: u64,
    rows_written: u64,
    issues: u32,
    /// Rows kept over every page parsed.
    rows_kept: u32,
    /// `field -> rows filled` summed over every page.
    fills: BTreeMap<String, u32>,
}

/// One open run: the lock, the `ingest_runs` id and the counters. Every
/// job builds one with [`Run::begin`] and ends it with [`Run::finish`].
pub(crate) struct Run<'a> {
    ctx: &'a JobCtx,
    id: i64,
    job: &'static str,
    key: Option<RunKey>,
    started: OffsetDateTime,
    clock: Instant,
    counters: Counters,
    lock: Option<JobLock>,
}

impl<'a> Run<'a> {
    /// Take the lock and open the run.
    ///
    /// # Errors
    /// `JobError::Locked` when another process holds the job; `Store` when
    /// the row cannot be opened.
    pub(crate) async fn begin(
        ctx: &'a JobCtx,
        job: &'static str,
        term: Option<TermCode>,
    ) -> Result<Self, JobError> {
        Self::begin_keyed(ctx, job, term.map(RunKey::Term)).await
    }

    /// `begin` for a run keyed by a term, a catalog year, or nothing.
    ///
    /// # Errors
    /// As `begin`.
    pub(crate) async fn begin_keyed(
        ctx: &'a JobCtx,
        job: &'static str,
        key: Option<RunKey>,
    ) -> Result<Self, JobError> {
        let lock = ctx
            .store
            .try_advisory_lock(lock_key(job))
            .await?
            .ok_or(JobError::Locked)?;
        let id = ctx.store.start_run_keyed(job, key).await?;
        tracing::info!(job, key = key.map(RunKey::as_db), run = id, "run started");
        Ok(Self {
            ctx,
            id,
            job,
            key,
            started: OffsetDateTime::now_utc(),
            clock: Instant::now(),
            counters: Counters::default(),
            lock: Some(lock),
        })
    }

    /// The term this run is keyed by, when it is.
    fn term(&self) -> Option<TermCode> {
        match self.key {
            Some(RunKey::Term(term)) => Some(term),
            Some(RunKey::Year(_)) | None => None,
        }
    }

    pub(crate) fn ctx(&self) -> &'a JobCtx {
        self.ctx
    }

    pub(crate) fn id(&self) -> i64 {
        self.id
    }

    pub(crate) fn started(&self) -> OffsetDateTime {
        self.started
    }

    pub(crate) fn set_targets(&mut self, targets: usize) {
        self.counters.targets = u32::try_from(targets).unwrap_or(u32::MAX);
    }

    pub(crate) fn add_targets(&mut self, more: usize) {
        self.counters.targets = self
            .counters
            .targets
            .saturating_add(u32::try_from(more).unwrap_or(u32::MAX));
    }

    /// Rows kept by every parse so far.
    pub(crate) fn rows_kept(&self) -> u32 {
        self.counters.rows_kept
    }

    pub(crate) fn add_rows(&mut self, rows: u64) {
        self.counters.rows_written = self.counters.rows_written.saturating_add(rows);
    }

    /// Fetch one archived document and index it in `raw_responses`.
    /// `Ok(None)` when the fetch failed after its retries: the failure is
    /// counted and the job moves on. Five outages in a row (transport
    /// errors, retried statuses; not a 404 or an oversize body) end the
    /// run.
    ///
    /// # Errors
    /// `JobError::ConsecutiveFailures` at the fifth outage in a row;
    /// `Store` when the index row cannot be written.
    pub(crate) async fn fetch<F: Fetch>(
        &mut self,
        f: &F,
        url: &Url,
        source: Source,
    ) -> Result<Option<FetchOutcome>, JobError> {
        let outcome = self.count_fetch(url, f.get(url, source).await)?;
        let Some(outcome) = outcome else {
            return Ok(None);
        };
        self.ctx
            .store
            .record_raw_response(&RawResponseInput {
                run_id: self.id,
                source,
                url: url.to_string(),
                term: self.term(),
                fetched_at: outcome.fetched_at,
                status: outcome.status,
                content_type: outcome.content_type.clone(),
                byte_len: u32::try_from(outcome.body.len()).unwrap_or(u32::MAX),
                sha256: outcome.sha256,
                headers: serde_json::to_value(&outcome.headers)?,
            })
            .await?;
        Ok(Some(outcome))
    }

    /// Fetch one document that is never archived: a seats reading.
    ///
    /// # Errors
    /// `JobError::ConsecutiveFailures` at the fifth failure in a row.
    pub(crate) async fn fetch_live<F: Fetch>(
        &mut self,
        f: &F,
        url: &Url,
    ) -> Result<Option<FetchOutcome>, JobError> {
        self.count_fetch(url, f.get_live(url).await)
    }

    fn count_fetch(
        &mut self,
        url: &Url,
        result: Result<FetchOutcome, crate::error::FetchError>,
    ) -> Result<Option<FetchOutcome>, JobError> {
        self.counters.requests = self.counters.requests.saturating_add(1);
        match result {
            Ok(outcome) => {
                self.counters.consecutive_failures = 0;
                self.counters.bytes = self
                    .counters
                    .bytes
                    .saturating_add(outcome.body.len() as u64);
                Ok(Some(outcome))
            }
            Err(e) => {
                self.counters.failures = self.counters.failures.saturating_add(1);
                if e.is_outage() {
                    self.counters.consecutive_failures += 1;
                } else {
                    self.counters.consecutive_failures = 0;
                }
                tracing::warn!(url = %url, error = %e, consecutive = self.counters.consecutive_failures, "fetch failed");
                if self.counters.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    return Err(JobError::ConsecutiveFailures(
                        self.counters.consecutive_failures,
                    ));
                }
                Ok(None)
            }
        }
    }

    /// Record one issue against a URL.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub(crate) async fn issue(
        &mut self,
        url: &str,
        severity: IssueSeverity,
        code: &str,
        detail: serde_json::Value,
    ) -> Result<(), JobError> {
        self.counters.issues = self.counters.issues.saturating_add(1);
        self.ctx
            .store
            .record_parse_issue(self.id, url, severity, code, detail)
            .await?;
        Ok(())
    }

    /// Record a page's parse report: every issue as a row, every field's
    /// fill count into the run's totals. Logged with the report's names.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub(crate) async fn report(&mut self, url: &str, report: &ParseReport) -> Result<(), JobError> {
        tracing::info!(
            url,
            rows_seen = report.rows_seen,
            rows_kept = report.rows_kept,
            issues = report.issues.len(),
            "parsed"
        );
        for issue in &report.issues {
            let code = serde_json::to_value(issue.code)?
                .as_str()
                .unwrap_or("issue")
                .to_owned();
            self.issue(
                url,
                IssueSeverity::Warn,
                &code,
                serde_json::json!({ "detail": issue.detail }),
            )
            .await?;
        }
        self.counters.rows_kept = self.counters.rows_kept.saturating_add(report.rows_kept);
        for (field, filled) in &report.field_filled {
            let entry = self.counters.fills.entry((*field).to_owned()).or_default();
            *entry = entry.saturating_add(*filled);
        }
        Ok(())
    }

    /// This run's fill rate per field, for the drift guard.
    pub(crate) fn fill_rates(&self) -> BTreeMap<String, f64> {
        let kept = self.counters.rows_kept;
        self.counters
            .fills
            .iter()
            .map(|(field, filled)| (field.clone(), crate::guards::fill_rate(kept, *filled)))
            .collect()
    }

    /// Write the accumulated fill counts as `parse_stats` for `source`.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub(crate) async fn write_stats(&self, source: &str) -> Result<(), JobError> {
        let kept = self.counters.rows_kept;
        let fields: Vec<(String, u32, u32)> = self
            .counters
            .fills
            .iter()
            .map(|(field, filled)| (field.clone(), kept, *filled))
            .collect();
        if !fields.is_empty() {
            self.ctx
                .store
                .record_parse_stats(self.id, source, &fields)
                .await?;
        }
        Ok(())
    }

    /// Close the run and release the lock.
    ///
    /// # Errors
    /// `JobError::Store` when the row cannot be closed.
    pub(crate) async fn finish(
        mut self,
        outcome: RunOutcome,
        note: Option<String>,
    ) -> Result<Summary, JobError> {
        let c = &self.counters;
        let row = RunSummaryRow {
            outcome,
            requests: c.requests,
            targets: c.targets,
            failures: c.failures,
            bytes: c.bytes,
            rows_written: c.rows_written,
            error: note.clone(),
        };
        let closed = self.ctx.store.finish_run(self.id, &row).await;
        if let Some(lock) = self.lock.take() {
            lock.release().await?;
        }
        closed?;
        let finished = OffsetDateTime::now_utc();
        tracing::info!(
            job = self.job,
            key = self.key.map(RunKey::as_db),
            run = self.id,
            outcome = outcome_text(outcome),
            requests = c.requests,
            targets = c.targets,
            failures = c.failures,
            rows_written = c.rows_written,
            issues = c.issues,
            duration_ms = u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX),
            "run finished"
        );
        Ok(Summary {
            requests: c.requests,
            targets: c.targets,
            failures: c.failures,
            bytes: c.bytes,
            rows_written: c.rows_written,
            issues: c.issues,
            outcome,
            started: self.started,
            finished,
            note,
        })
    }

    /// End a run whose body returned `Err`: record it as failed with the
    /// error text and hand back the summary, so the CLI exits 1 with the
    /// numbers. A store failure while closing propagates.
    ///
    /// # Errors
    /// `JobError::Store`.
    pub(crate) async fn conclude(
        self,
        result: Result<(RunOutcome, Option<String>), JobError>,
    ) -> Result<Summary, JobError> {
        match result {
            Ok((outcome, note)) => self.finish(outcome, note).await,
            Err(e) => {
                tracing::error!(error = %e, "run failed");
                self.finish(RunOutcome::Failed, Some(e.to_string())).await
            }
        }
    }
}

/// Fetch one reference list and parse it. `Ok(None)` when the fetch failed.
///
/// # Errors
/// `JobError::Parse` when the list is not the shape asked for, or the
/// harness errors.
pub(crate) async fn fetch_reference<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    kind: RefKind,
    term: TermCode,
) -> Result<Option<Vec<ReferenceEntry>>, JobError> {
    let url = urls::reference(kind, term)?;
    let Some(page) = run.fetch(f, &url, Source::Reference).await? else {
        return Ok(None);
    };
    let entries = parse_reference_list(&page.text, kind)?;
    Ok(Some(entries))
}

/// The subjects Rice lists, as core `Subject` values; a code the core type
/// rejects is recorded as an issue and skipped.
pub(crate) async fn subject_codes(
    run: &mut Run<'_>,
    url: &str,
    entries: &[ReferenceEntry],
) -> Result<Vec<skyspace_core::code::Subject>, JobError> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        match skyspace_core::code::Subject::new(&entry.code) {
            Ok(subject) => out.push(subject),
            Err(e) => {
                run.issue(
                    url,
                    IssueSeverity::Warn,
                    "unknown_subject",
                    serde_json::json!({ "code": entry.code, "error": e.to_string() }),
                )
                .await?;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{job_names as j, lock_key};

    /// The key is a fixed function of the name, not of the toolchain.
    #[test]
    fn lock_keys_are_stable() {
        assert_eq!(lock_key(j::LISTINGS), lock_key("listings"));
        assert_ne!(lock_key(j::LISTINGS), lock_key(j::SEATS));
        // FNV-1a of "skyspace-ingest/listings", pinned.
        assert_eq!(lock_key(j::LISTINGS), -4_827_120_266_633_811_147_i64);
    }

    /// Every job whose good run changes what the catalog serves is in the
    /// store's `ETag` list, and nothing in that list is a name we do not
    /// run.
    #[test]
    fn data_version_jobs_match_job_names() {
        let ours = [
            j::REFERENCE,
            j::CATALOG,
            j::LISTINGS,
            j::SECTIONS,
            j::DETAIL,
            j::SEATS,
            j::REQUIREMENTS,
            j::REPLAY,
            j::IMPORT,
        ];
        for job in skyspace_store::DATA_VERSION_JOBS {
            assert!(ours.contains(&job), "{job} is not a job name");
        }
        for job in [
            j::LISTINGS,
            j::DETAIL,
            j::REFERENCE,
            j::SECTIONS,
            j::CATALOG,
        ] {
            assert!(
                skyspace_store::DATA_VERSION_JOBS.contains(&job),
                "{job} writes catalog rows but does not move the ETag"
            );
        }
        assert_eq!(skyspace_store::CATALOG_JOB, j::CATALOG);
    }
}
