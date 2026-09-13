//! `pull seats`: the live enrollment XML for the poll set, every 15
//! minutes inside a registration window. Exits at once outside a window,
//! or when the last good run is younger than the window's interval. The
//! XML is never archived: `parse_enrollment` is lossless and
//! `seat_snapshots` is the archive for this source.

use std::time::Duration;

use skyspace_core::catalog::Seats;
use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
use skyspace_parse::parse_enrollment;
use skyspace_store::IssueSeverity;
use time::OffsetDateTime;
use tracing::Instrument;

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::Fetch;
use crate::jobs::{Run, RunOutcome, Summary, job_names};
use crate::urls;

/// Readings are written in batches this size, so a deadline stop keeps
/// what was polled.
const BATCH: usize = 200;

/// Poll the term's poll set until `deadline` has passed.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// window or run row cannot be read or written.
pub async fn poll_seats<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    term: TermCode,
    deadline: Duration,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::SEATS, term_code = %term);
    async move {
        let now = OffsetDateTime::now_utc();
        let Some(window) = ctx.store.poll_windows_active(term, now).await? else {
            tracing::info!("outside a poll window");
            return Ok(Summary::nothing_to_do(format!(
                "no poll window is open for {term}"
            )));
        };
        let interval =
            Duration::from_secs(60 * u64::try_from(window.interval_minutes).unwrap_or(15));
        if let Some(last) = ctx.store.last_ok_run(job_names::SEATS, Some(term)).await?
            && let Some(finished) = last.finished_at
            && now - finished < interval
        {
            tracing::info!(last_ok = %finished, "last good run is younger than the interval");
            return Ok(Summary::nothing_to_do(format!(
                "last good run finished at {finished}, less than {} minutes ago",
                window.interval_minutes
            )));
        }
        let mut run = Run::begin(ctx, job_names::SEATS, Some(term)).await?;
        let result = body(&mut run, f, term, deadline).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    deadline: Duration,
) -> Result<(RunOutcome, Option<String>), JobError> {
    let poll_set = run.ctx().store.poll_set(term).await?;
    run.set_targets(poll_set.len());
    let stop_at = run.started() + deadline;
    let mut readings: Vec<(Crn, Seats)> = Vec::with_capacity(BATCH);
    let mut note = None;
    for (_, crn) in poll_set {
        if OffsetDateTime::now_utc() >= stop_at {
            note = Some("deadline reached; partial coverage".to_owned());
            break;
        }
        let span = tracing::info_span!("page", crn = %crn);
        let result = one_crn(run, f, term, crn).instrument(span).await;
        match result {
            Ok(Some(seats)) => readings.push((crn, seats)),
            Ok(None) => {}
            Err(e) => {
                flush(run, term, &mut readings).await?;
                return Err(e);
            }
        }
        if readings.len() >= BATCH {
            flush(run, term, &mut readings).await?;
        }
    }
    flush(run, term, &mut readings).await?;
    Ok((RunOutcome::Ok, note))
}

async fn flush(
    run: &mut Run<'_>,
    term: TermCode,
    readings: &mut Vec<(Crn, Seats)>,
) -> Result<(), JobError> {
    if readings.is_empty() {
        return Ok(());
    }
    run.ctx().store.record_seats(term, readings).await?;
    run.add_rows(readings.len() as u64);
    readings.clear();
    Ok(())
}

async fn one_crn<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
    crn: Crn,
) -> Result<Option<Seats>, JobError> {
    let url = urls::enrollment(term, crn)?;
    let Some(page) = run.fetch_live(f, &url).await? else {
        return Ok(None);
    };
    match parse_enrollment(&page.text) {
        Ok(seats) => Ok(Some(seats)),
        Err(e) => {
            run.issue(
                url.as_str(),
                IssueSeverity::Error,
                "unreadable_enrollment",
                serde_json::json!({ "crn": crn.to_string(), "error": e.to_string() }),
            )
            .await?;
            Ok(None)
        }
    }
}
