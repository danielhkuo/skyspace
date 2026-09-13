//! `pull detail`: the 1.4 s section page, for reserved seats and fees
//! only, for every CRN whose copy is missing or older than `stale_after`.

use std::time::Duration;

use skyspace_core::Timestamp;
use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
use skyspace_parse::parse_section_detail;
use skyspace_store::IssueSeverity;
use tracing::Instrument;

use crate::ctx::{DueColumn, JobCtx};
use crate::error::JobError;
use crate::fetch::{Fetch, Source};
use crate::jobs::{Run, RunOutcome, Summary, job_names};
use crate::urls;

/// Pull the detail page for every CRN due in `term`.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_course_detail<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    term: TermCode,
    stale_after: Duration,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::DETAIL, term_code = %term);
    async move {
        let mut run = Run::begin(ctx, job_names::DETAIL, Some(term)).await?;
        let result = body(&mut run, f, term, stale_after).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    stale_after: Duration,
) -> Result<(RunOutcome, Option<String>), JobError> {
    let due = run
        .ctx()
        .crns_due(term, DueColumn::Detail, stale_after)
        .await?;
    run.set_targets(due.len());
    for crn in due {
        let span = tracing::info_span!("page", crn = %crn);
        one_crn(run, f, term, crn).instrument(span).await?;
    }
    run.write_stats(Source::Detail.as_str()).await?;
    Ok((RunOutcome::Ok, None))
}

async fn one_crn<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    crn: Crn,
) -> Result<(), JobError> {
    let url = urls::detail(term, crn)?;
    let Some(page) = run.fetch(f, &url, Source::Detail).await? else {
        return Ok(());
    };
    let fetched_at = Timestamp(page.fetched_at.unix_timestamp());
    let parsed = parse_section_detail(&page.text, term, crn, fetched_at)?;
    run.report(url.as_str(), &parsed.report).await?;
    if run
        .ctx()
        .store
        .upsert_section_detail(term, crn, &parsed.value)
        .await?
    {
        run.add_rows(1);
    } else {
        run.issue(
            url.as_str(),
            IssueSeverity::Warn,
            "crn_not_held",
            serde_json::json!({ "crn": crn.to_string() }),
        )
        .await?;
    }
    Ok(())
}
