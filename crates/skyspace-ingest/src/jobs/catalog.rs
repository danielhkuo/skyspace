//! `pull catalog`: one `CATALIST` page per subject for one academic year.
//! Records are keyed by year, not term, so the run carries no term code.
//! The volume and fill-rate guards run over the whole year before any
//! course is written.

use skyspace_core::catalog::Course;
use skyspace_core::code::Subject;
use skyspace_core::program::CatalogYear;
use skyspace_parse::{ParseError, RefKind, parse_catalog_subject, parse_reference_list};
use skyspace_store::IssueSeverity;
use tracing::Instrument;

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::{Fetch, Source};
use crate::guards::{drifting_fields, volume_drifts};
use crate::jobs::{Run, RunOutcome, Summary, job_names, subject_codes};
use crate::urls;

/// Pull every subject's course records for `year`.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_catalog<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    year: CatalogYear,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::CATALOG, catalog_year = year.0);
    async move {
        let mut run = Run::begin(ctx, job_names::CATALOG, None).await?;
        let result = body(&mut run, f, year).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    year: CatalogYear,
) -> Result<(RunOutcome, Option<String>), JobError> {
    run.set_targets(1);
    let subjects_url = urls::subjects_for_year(year)?;
    let Some(page) = run.fetch(f, &subjects_url, Source::Reference).await? else {
        return Err(JobError::Corrupt(
            "SUBJECTS list could not be fetched".to_owned(),
        ));
    };
    let entries = parse_reference_list(&page.text, RefKind::Subjects)?;
    let subjects = subject_codes(run, subjects_url.as_str(), &entries).await?;
    run.add_targets(subjects.len());

    let previous_total: u64 = run
        .ctx()
        .store
        .last_ok_run(job_names::CATALOG, None)
        .await?
        .and_then(|r| u64::try_from(r.rows_written).ok())
        .unwrap_or(0);
    let history = run
        .ctx()
        .fill_history(job_names::CATALOG, None, Source::Catalog.as_str())
        .await?;

    let mut courses: Vec<Course> = Vec::new();
    for subject in subjects {
        let span = tracing::info_span!("page", subject = %subject);
        one_subject(run, f, year, &subject, &mut courses)
            .instrument(span)
            .await?;
    }
    run.write_stats(Source::Catalog.as_str()).await?;

    let mut quarantine = Vec::new();
    let total = u64::from(run.rows_kept());
    if volume_drifts(previous_total, total) {
        quarantine.push(format!(
            "course count {total} is more than 20% from the last good run's {previous_total}"
        ));
    }
    for (field, median, now) in drifting_fields(&history, &run.fill_rates()) {
        quarantine.push(format!(
            "field {field} filled {:.0}% of rows, trailing median {:.0}%",
            now * 100.0,
            median * 100.0
        ));
    }
    if !quarantine.is_empty() {
        tracing::warn!(reasons = ?quarantine, "quarantined");
        return Ok((RunOutcome::Quarantined, Some(quarantine.join("; "))));
    }
    let written = run.ctx().store.upsert_courses(year, &courses).await?;
    run.add_rows(written);
    Ok((RunOutcome::Ok, None))
}

async fn one_subject<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    year: CatalogYear,
    subject: &Subject,
    courses: &mut Vec<Course>,
) -> Result<(), JobError> {
    let url = urls::catalist(year, subject)?;
    let Some(page) = run.fetch(f, &url, Source::Catalog).await? else {
        return Ok(());
    };
    match parse_catalog_subject(&page.text, year) {
        Ok(result) => {
            run.report(url.as_str(), &result.report).await?;
            courses.extend(result.value);
        }
        Err(ParseError::SelectorMissing { selector, document }) => {
            // An unknown subject or year returns a page with no records.
            run.issue(
                url.as_str(),
                IssueSeverity::Warn,
                "no_rows",
                serde_json::json!({ "subject": subject.as_str(), "selector": selector, "document": document }),
            )
            .await?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
