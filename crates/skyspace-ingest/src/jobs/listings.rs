//! `pull listings`: one `QUERY` page per subject, the catalog spine.
//!
//! Order of operations matters. The term is checked against `TERMS` before
//! any listing is fetched, because Rice answers an unknown code with the
//! current term's rows. Every page is archived and parsed, then the three
//! statistical guards run over the whole term, and only a run that passes
//! writes rows and withdraws sections. A quarantined run leaves the bytes
//! on disk and the catalog untouched.

use std::collections::BTreeMap;

use skyspace_core::catalog::SectionListing;
use skyspace_core::code::Subject;
use skyspace_core::term::TermCode;
use skyspace_parse::{ParseError, RefKind, parse_subject_listing};
use skyspace_store::{IssueSeverity, RunKey};
use tracing::Instrument;

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::{Fetch, Source};
use crate::guards::{drifting_fields, volume_drifts, zero_rows};
use crate::jobs::{Run, RunOutcome, Summary, fetch_reference, job_names, subject_codes};
use crate::urls;

/// Pull every subject listing for `term`.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_section_listings<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    term: TermCode,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::LISTINGS, term_code = %term);
    async move {
        let mut run = Run::begin(ctx, job_names::LISTINGS, Some(term)).await?;
        let result = body(&mut run, f, term).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

/// One subject's parsed page, held until the guards have run.
struct SubjectRows {
    subject: Subject,
    rows: Vec<SectionListing>,
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
) -> Result<(RunOutcome, Option<String>), JobError> {
    run.set_targets(2);
    let Some(terms) = fetch_reference(run, f, RefKind::Terms, term).await? else {
        return Err(JobError::UnknownTerm(term));
    };
    if !terms.iter().any(|t| t.code == term.to_string()) {
        return Err(JobError::UnknownTerm(term));
    }
    let subjects_url = urls::reference(RefKind::Subjects, term)?;
    let Some(entries) = fetch_reference(run, f, RefKind::Subjects, term).await? else {
        return Err(JobError::Corrupt(
            "SUBJECTS list could not be fetched".to_owned(),
        ));
    };
    let subjects = subject_codes(run, subjects_url.as_str(), &entries).await?;
    run.add_targets(subjects.len());

    let previous_counts = run.ctx().live_section_counts(term).await?;
    let previous_total: u64 = run
        .ctx()
        .store
        .last_ok_run(job_names::LISTINGS, Some(term))
        .await?
        .and_then(|r| u64::try_from(r.rows_written).ok())
        .unwrap_or(0);
    let history = run
        .ctx()
        .fill_history(
            job_names::LISTINGS,
            Some(RunKey::Term(term)),
            Source::Listing.as_str(),
        )
        .await?;

    let mut parsed: Vec<SubjectRows> = Vec::with_capacity(subjects.len());
    let mut quarantine: Vec<String> = Vec::new();
    for subject in subjects {
        let previous = previous_counts.get(subject.as_str()).copied().unwrap_or(0);
        let span = tracing::info_span!("page", subject = %subject);
        one_subject(
            run,
            f,
            term,
            subject,
            previous,
            &mut parsed,
            &mut quarantine,
        )
        .instrument(span)
        .await?;
    }
    run.write_stats(Source::Listing.as_str()).await?;

    let total = u64::from(run.rows_kept());
    if volume_drifts(previous_total, total) {
        quarantine.push(format!(
            "section count {total} is more than 20% from the last good run's {previous_total}"
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

    for SubjectRows { subject, rows } in &parsed {
        let written = run.ctx().store.upsert_sections(term, rows).await?;
        run.add_rows(written);
        let seen: Vec<_> = rows.iter().map(|r| r.crn).collect();
        let withdrawn = run
            .ctx()
            .store
            .withdraw_missing(term, subject, &seen)
            .await?;
        if withdrawn > 0 {
            tracing::info!(subject = %subject, withdrawn, "sections withdrawn");
        }
    }
    check_canaries(run, term, &parsed).await?;
    Ok((RunOutcome::Ok, None))
}

/// Fetch and parse one subject. A zero-rows result for a subject that had
/// rows joins `quarantine`; a page that parsed joins `parsed`.
async fn one_subject<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    subject: Subject,
    previous: u64,
    parsed: &mut Vec<SubjectRows>,
    quarantine: &mut Vec<String>,
) -> Result<(), JobError> {
    let url = urls::listing(term, &subject)?;
    let Some(page) = run.fetch(f, &url, Source::Listing).await? else {
        return Ok(());
    };
    match parse_subject_listing(&page.text, term) {
        Ok(result) => {
            run.report(url.as_str(), &result.report).await?;
            if zero_rows(previous, u64::from(result.report.rows_kept)) {
                quarantine.push(format!("{subject} had {previous} rows, now none"));
                run.issue(
                    url.as_str(),
                    IssueSeverity::Error,
                    "zero_rows",
                    serde_json::json!({ "subject": subject.as_str(), "previous": previous }),
                )
                .await?;
            }
            parsed.push(SubjectRows {
                subject,
                rows: result.value,
            });
        }
        Err(ParseError::SelectorMissing { selector, document }) if previous == 0 => {
            // A subject with no sections this term is normal; a subject
            // that had rows and now shows none is the guard below.
            run.issue(
                url.as_str(),
                IssueSeverity::Warn,
                "no_rows",
                serde_json::json!({ "subject": subject.as_str(), "selector": selector, "document": document }),
            )
            .await?;
        }
        Err(ParseError::SelectorMissing { selector, .. }) => {
            quarantine.push(format!(
                "{subject} had {previous} rows, now `{selector}` matches nothing"
            ));
            run.issue(
                url.as_str(),
                IssueSeverity::Error,
                "zero_rows",
                serde_json::json!({ "subject": subject.as_str(), "previous": previous, "selector": selector }),
            )
            .await?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// Compare each pinned listing canary with what this run parsed for its
/// URL. A vanished canary is an issue, not a crash.
async fn check_canaries(
    run: &mut Run<'_>,
    term: TermCode,
    parsed: &[SubjectRows],
) -> Result<(), JobError> {
    let canaries = run.ctx().store.canaries(term).await?;
    let by_url: BTreeMap<String, usize> = parsed
        .iter()
        .filter_map(|p| {
            urls::listing(term, &p.subject)
                .ok()
                .map(|u| (u.to_string(), p.rows.len()))
        })
        .collect();
    for canary in canaries
        .iter()
        .filter(|c| c.source == Source::Listing.as_str())
    {
        let rows = by_url.get(&canary.url).copied();
        let expected = canary.expect_rows.and_then(|n| usize::try_from(n).ok());
        let mismatch = match (rows, expected) {
            (None, _) => Some("canary URL was not fetched this run".to_owned()),
            (Some(got), Some(want)) if got != want => {
                Some(format!("expected {want} rows, got {got}"))
            }
            _ => None,
        };
        if let Some(detail) = mismatch {
            run.issue(
                &canary.url,
                IssueSeverity::Warn,
                "canary_mismatch",
                serde_json::json!({ "canary": canary.id, "note": canary.note, "detail": detail }),
            )
            .await?;
        }
    }
    Ok(())
}
