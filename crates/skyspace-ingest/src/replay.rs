//! `replay`: read archived bodies and call the same parse-and-store path
//! the live job called, with no network. Only the newest body of each URL
//! from a run that ended `ok` is replayed (a quarantined search form or a
//! superseded page is not), oldest first, so the tables end on what Rice
//! last published. Guards and withdrawal are not re-run: a replay repairs
//! what a parser bug dropped, it does not re-decide what Rice published,
//! and it cannot undo a withdrawal because a listing clears
//! `withdrawn_at` only when its body is newer than the withdrawal. A page
//! that does not parse is an issue and the run continues; only the store
//! stops it.

use skyspace_core::Timestamp;
use skyspace_core::code::Crn;
use skyspace_core::program::CatalogYear;
use skyspace_core::term::TermCode;
use skyspace_parse::{
    ParseError, RefKind, parse_associated_sections, parse_catalog_subject, parse_program_index,
    parse_program_page, parse_reference_list, parse_section_detail, parse_subject_listing,
};
use skyspace_store::{DraftInput, IssueSeverity, TermInput};
use time::Date;
use tracing::Instrument;

use crate::ctx::{JobCtx, RawRow};
use crate::error::JobError;
use crate::fetch::{Source, decode};
use crate::jobs::{Run, RunOutcome, Summary, job_names};
use crate::urls::{catalog_year_of_ga_url, query_param};

/// Re-parse every archived document of `source` fetched since `since`.
///
/// # Errors
/// `JobError::Locked` when another replay is running; `Store` when the
/// run row cannot be opened or closed.
pub async fn replay(ctx: &JobCtx, source: Source, since: Date) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::REPLAY, source = %source);
    async move {
        let mut run = Run::begin(ctx, job_names::REPLAY, None).await?;
        let result = body(&mut run, source, since).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body(
    run: &mut Run<'_>,
    source: Source,
    since: Date,
) -> Result<(RunOutcome, Option<String>), JobError> {
    let rows = run.ctx().raw_responses_since(source, since).await?;
    run.set_targets(rows.len());
    let mut skipped = 0u32;
    for row in rows {
        let span = tracing::info_span!("page", url = %row.url);
        match one_row(run, source, &row).instrument(span).await {
            Ok(()) => {}
            Err(JobError::Store(e)) => return Err(JobError::Store(e)),
            Err(e) => {
                // One page a parser (or the archive) cannot read is an
                // issue on that page; the rest of the replay goes on.
                skipped += 1;
                tracing::warn!(url = %row.url, error = %e, "page skipped");
                run.issue(
                    &row.url,
                    IssueSeverity::Error,
                    "replay_skipped",
                    serde_json::json!({ "raw_response": row.id, "error": e.to_string() }),
                )
                .await?;
            }
        }
    }
    run.write_stats(source.as_str()).await?;
    let note = (skipped > 0).then(|| format!("{skipped} pages could not be replayed"));
    Ok((RunOutcome::Ok, note))
}

/// Record a subject page with no records the way the live jobs do: a
/// `no_rows` warning, not a failure.
async fn no_rows(
    run: &mut Run<'_>,
    row: &RawRow,
    selector: &str,
    document: &str,
) -> Result<(), JobError> {
    run.issue(
        &row.url,
        IssueSeverity::Warn,
        "no_rows",
        serde_json::json!({ "selector": selector, "document": document }),
    )
    .await
}

fn term_of(row: &RawRow) -> Result<TermCode, JobError> {
    let code = match &row.term_code {
        Some(code) => code.clone(),
        None => query_param(&row.url, "p_term")
            .or_else(|| query_param(&row.url, "term"))
            .ok_or_else(|| JobError::Corrupt(format!("raw_responses {} has no term", row.id)))?,
    };
    TermCode::parse(&code).map_err(|e| JobError::Corrupt(format!("raw_responses {}: {e}", row.id)))
}

fn crn_of(row: &RawRow) -> Result<Crn, JobError> {
    query_param(&row.url, "p_crn")
        .or_else(|| query_param(&row.url, "crn"))
        .and_then(|c| c.parse::<u32>().ok())
        .map(Crn)
        .ok_or_else(|| JobError::Corrupt(format!("raw_responses {} has no crn", row.id)))
}

async fn one_row(run: &mut Run<'_>, source: Source, row: &RawRow) -> Result<(), JobError> {
    let bytes = run.ctx().archive.get(&row.key()?)?;
    let text = decode(&bytes, row.content_type.as_deref());
    match source {
        Source::Listing => replay_listing(run, row, &text).await,
        Source::Catalog => replay_catalog(run, row, &text).await,
        Source::SectionXml => replay_section_xml(run, row, &text).await,
        Source::Detail => replay_detail(run, row, &text).await,
        Source::Reference => replay_reference(run, row, &text).await,
        Source::ProgramIndex => {
            let parsed = parse_program_index(&text)?;
            run.report(&row.url, &parsed.report).await
        }
        Source::Program => replay_program(run, row, &text).await,
    }
}

async fn replay_listing(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    let term = term_of(row)?;
    let parsed = match parse_subject_listing(text, term) {
        Ok(parsed) => parsed,
        Err(ParseError::SelectorMissing { selector, document }) => {
            // A subject with no sections this term, as the live job reads it.
            return no_rows(run, row, selector, document).await;
        }
        Err(e) => return Err(e.into()),
    };
    run.report(&row.url, &parsed.report).await?;
    let written = run
        .ctx()
        .store
        .upsert_sections_fetched_at(term, &parsed.value, row.fetched_at)
        .await?;
    run.add_rows(written);
    Ok(())
}

async fn replay_catalog(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    let year = query_param(&row.url, "p_acyr_code")
        .and_then(|y| y.parse::<u16>().ok())
        .map(CatalogYear)
        .ok_or_else(|| JobError::Corrupt(format!("raw_responses {} has no year", row.id)))?;
    let parsed = match parse_catalog_subject(text, year) {
        Ok(parsed) => parsed,
        Err(ParseError::SelectorMissing { selector, document }) => {
            return no_rows(run, row, selector, document).await;
        }
        Err(e) => return Err(e.into()),
    };
    run.report(&row.url, &parsed.report).await?;
    let written = run.ctx().store.upsert_courses(year, &parsed.value).await?;
    run.add_rows(written);
    Ok(())
}

async fn replay_section_xml(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    let term = term_of(row)?;
    let parsed = parse_associated_sections(text, term)?;
    run.report(&row.url, &parsed.report).await?;
    for section in parsed.value {
        let xml = crate::jobs::xml_row(section);
        if run.ctx().store.upsert_section_xml(term, &xml).await? {
            run.add_rows(1);
        }
    }
    Ok(())
}

async fn replay_detail(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    let term = term_of(row)?;
    let crn = crn_of(row)?;
    let fetched_at = Timestamp(row.fetched_at.unix_timestamp());
    let parsed = parse_section_detail(text, term, crn, fetched_at)?;
    run.report(&row.url, &parsed.report).await?;
    if run
        .ctx()
        .store
        .upsert_section_detail(term, crn, &parsed.value)
        .await?
    {
        run.add_rows(1);
    }
    Ok(())
}

/// Only `TERMS` has a table; the other lists are re-read for their count.
async fn replay_reference(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    if query_param(&row.url, "action").as_deref() != Some("TERMS") {
        return Ok(());
    }
    let entries = parse_reference_list(text, RefKind::Terms)?;
    let terms: Vec<TermInput> = entries
        .iter()
        .filter_map(|e| {
            TermCode::parse(&e.code).ok().map(|code| TermInput {
                code,
                label: e.label.clone(),
            })
        })
        .collect();
    let written = run.ctx().store.upsert_terms(&terms).await?;
    run.add_rows(written);
    Ok(())
}

async fn replay_program(run: &mut Run<'_>, row: &RawRow, text: &str) -> Result<(), JobError> {
    let url = row.url.as_str();
    let parsed = parse_program_page(text, url)?;
    run.report(url, &parsed.report).await?;
    let slug = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    if slug.is_empty() {
        run.issue(url, IssueSeverity::Error, "no_slug", serde_json::json!({}))
            .await?;
        return Ok(());
    }
    let put = run
        .ctx()
        .store
        .put_draft(&DraftInput {
            run_id: run.id(),
            slug,
            catalog_year: catalog_year_of_ga_url(url),
            source_url: url.to_owned(),
            source_sha256: row.key()?,
            body: serde_json::to_value(&parsed.value)?,
        })
        .await?;
    if put.created() {
        run.add_rows(1);
    }
    Ok(())
}
