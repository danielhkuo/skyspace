//! `pull reference`: the seven `!SWKSCAT.info` lists. Only `TERMS` has a
//! table; the others are archived and counted so a new subject code or
//! attribute is visible in the archive before a parser needs it.

use skyspace_core::term::TermCode;
use skyspace_parse::RefKind;
use skyspace_store::{IssueSeverity, TermInput};
use tracing::Instrument;

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::Fetch;
use crate::jobs::{Run, RunOutcome, Summary, fetch_reference, job_names};
use crate::urls;

const KINDS: [RefKind; 7] = [
    RefKind::Terms,
    RefKind::Subjects,
    RefKind::Departments,
    RefKind::Schools,
    RefKind::Sessions,
    RefKind::Years,
    RefKind::Attrs,
];

/// Pull every reference list for `term` and upsert the terms.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_reference_lists<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    term: TermCode,
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::REFERENCE, term_code = %term);
    async move {
        let mut run = Run::begin(ctx, job_names::REFERENCE, Some(term)).await?;
        run.set_targets(KINDS.len());
        let result = body(&mut run, f, term).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    term: TermCode,
) -> Result<(RunOutcome, Option<String>), JobError> {
    let mut seen_term = false;
    for kind in KINDS {
        let url = urls::reference(kind, term)?;
        let Some(entries) = fetch_reference(run, f, kind, term).await? else {
            continue;
        };
        tracing::info!(kind = ?kind, rows_kept = entries.len(), "parsed reference list");
        if kind == RefKind::Terms {
            let mut terms = Vec::with_capacity(entries.len());
            for entry in &entries {
                match TermCode::parse(&entry.code) {
                    Ok(code) => {
                        seen_term |= code == term;
                        terms.push(TermInput {
                            code,
                            label: entry.label.clone(),
                        });
                    }
                    Err(e) => {
                        run.issue(
                            url.as_str(),
                            IssueSeverity::Warn,
                            "unknown_term_code",
                            serde_json::json!({ "code": entry.code, "error": e.to_string() }),
                        )
                        .await?;
                    }
                }
            }
            let written = run.ctx().store.upsert_terms(&terms).await?;
            run.add_rows(written);
        }
    }
    let note = (!seen_term).then(|| format!("term {term} is not in Rice's TERMS list"));
    Ok((RunOutcome::Ok, note))
}
