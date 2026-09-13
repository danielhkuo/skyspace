//! `pull requirements`: the GA program index, then one page per program.
//! Every page becomes a draft for review; nothing here writes `programs`.
//! `only` limits the pull to named slugs so the launch set can be
//! reviewed first.

use skyspace_core::program::CatalogYear;
use skyspace_parse::{ProgramLink, parse_program_index, parse_program_page};
use skyspace_store::{DraftInput, IssueSeverity};
use tracing::Instrument;
use url::Url;

use crate::ctx::JobCtx;
use crate::error::JobError;
use crate::fetch::{Fetch, Source};
use crate::jobs::{Run, RunOutcome, Summary, job_names};
use crate::urls;

/// Extract drafts for every program of `year`, or only the slugs in `only`.
///
/// # Errors
/// `JobError::Locked` when another run holds the job; `Store` when the
/// run row cannot be opened or closed.
pub async fn pull_requirements<F: Fetch>(
    f: &F,
    ctx: &JobCtx,
    year: CatalogYear,
    only: &[String],
) -> Result<Summary, JobError> {
    let span = tracing::info_span!("job", job = job_names::REQUIREMENTS, catalog_year = year.0);
    async move {
        let mut run = Run::begin(ctx, job_names::REQUIREMENTS, None).await?;
        let result = body(&mut run, f, year, only).await;
        run.conclude(result).await
    }
    .instrument(span)
    .await
}

async fn body<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    year: CatalogYear,
    only: &[String],
) -> Result<(RunOutcome, Option<String>), JobError> {
    run.set_targets(1);
    let index_url = urls::program_index(year)?;
    let Some(page) = run.fetch(f, &index_url, Source::ProgramIndex).await? else {
        return Err(JobError::Corrupt(
            "program index could not be fetched".to_owned(),
        ));
    };
    let index = parse_program_index(&page.text)?;
    run.report(index_url.as_str(), &index.report).await?;
    let links = select_links(index.value, only);
    for slug in only {
        if !links.iter().any(|l| &l.slug == slug) {
            run.issue(
                index_url.as_str(),
                IssueSeverity::Warn,
                "slug_not_on_index",
                serde_json::json!({ "slug": slug }),
            )
            .await?;
        }
    }
    run.add_targets(links.len());
    for link in links {
        let span = tracing::info_span!("page", program_id = %link.slug);
        one_program(run, f, year, &link).instrument(span).await?;
    }
    Ok((RunOutcome::Ok, None))
}

/// The links to pull: all of them, or those whose slug is in `only`.
fn select_links(links: Vec<ProgramLink>, only: &[String]) -> Vec<ProgramLink> {
    if only.is_empty() {
        links
    } else {
        links
            .into_iter()
            .filter(|l| only.iter().any(|s| s == &l.slug))
            .collect()
    }
}

async fn one_program<F: Fetch>(
    run: &mut Run<'_>,
    f: &F,
    year: CatalogYear,
    link: &ProgramLink,
) -> Result<(), JobError> {
    let url = Url::parse(&link.url)?;
    let Some(page) = run.fetch(f, &url, Source::Program).await? else {
        return Ok(());
    };
    let parsed = parse_program_page(&page.text, url.as_str())?;
    run.report(url.as_str(), &parsed.report).await?;
    run.ctx()
        .store
        .put_draft(&DraftInput {
            run_id: run.id(),
            slug: link.slug.clone(),
            catalog_year: year,
            source_url: url.to_string(),
            source_sha256: page.sha256,
            body: serde_json::to_value(&parsed.value)?,
        })
        .await?;
    run.add_rows(1);
    Ok(())
}
