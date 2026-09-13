//! `pull sections`: the `ASSOCIATED-SECTIONS` XML for every CRN whose
//! copy is missing or older than `stale_after`. The feed lists the
//! sections associated with a CRN, so every row it carries is merged and
//! the requested CRN is stamped as fetched even when the feed omits it.

use std::time::Duration;

use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
use skyspace_parse::{SectionXml, parse_associated_sections};
use skyspace_store::SectionXmlRow;
use tracing::Instrument;

use crate::ctx::{DueColumn, JobCtx};
use crate::error::JobError;
use crate::fetch::{Fetch, Source};
use crate::jobs::{Run, RunOutcome, Summary, job_names};
use crate::urls;

/// Pull the weekly XML for every CRN due in `term`.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_section_xml<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    term: TermCode,
    stale_after: Duration,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::SECTIONS, term_code = %term);
    async move {
        let mut run = Run::begin(ctx, job_names::SECTIONS, Some(term)).await?;
        let result = body(&mut run, f, term, stale_after).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

/// The store's row for one `<COURSE>` of the feed. The feed carries no
/// part-of-term label and no exam slot; the store keeps what it had.
#[must_use]
pub fn xml_row(section: SectionXml) -> SectionXmlRow {
    SectionXmlRow {
        crn: section.crn,
        part_of_term: section.part_of_term,
        part_of_term_label: None,
        final_exam: section.final_exam,
        credits: Some(section.credits),
        attributes: section.attributes,
        school: section.school,
        meetings: section.meetings,
        final_exam_meeting: None,
        instructors: section.instructors,
    }
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    stale_after: Duration,
) -> Result<(RunOutcome, Option<String>), JobError> {
    let due = run
        .ctx()
        .crns_due(term, DueColumn::Xml, stale_after)
        .await?;
    run.set_targets(due.len());
    for crn in due {
        let span = tracing::info_span!("page", crn = %crn);
        one_crn(run, f, term, crn).instrument(span).await?;
    }
    run.write_stats(Source::SectionXml.as_str()).await?;
    Ok((RunOutcome::Ok, None))
}

async fn one_crn<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    crn: Crn,
) -> Result<(), JobError> {
    let url = urls::associated_sections(term, crn)?;
    let Some(page) = run.fetch(f, &url, Source::SectionXml).await? else {
        return Ok(());
    };
    let parsed = parse_associated_sections(&page.text, term)?;
    run.report(url.as_str(), &parsed.report).await?;
    for section in parsed.value {
        let row = xml_row(section);
        if run.ctx().store.upsert_section_xml(term, &row).await? {
            run.add_rows(1);
        }
    }
    run.ctx().mark_xml_fetched(term, crn).await?;
    Ok(())
}
