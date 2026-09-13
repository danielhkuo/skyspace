//! `skyspace pull <job>`: the timer jobs.

use std::time::Duration;

use clap::Subcommand;
use skyspace_core::program::CatalogYear;
use skyspace_core::term::TermCode;
use skyspace_ingest::{JobError, Summary, exit_code};

use crate::config::Config;
use crate::{catalog_year, term_code};

/// One timer job.
#[derive(Debug, Subcommand)]
pub enum PullJob {
    /// Terms, subjects, departments, schools, sessions, years, attributes.
    Reference {
        /// Rice's six-digit term code.
        #[arg(long, value_parser = term_code)]
        term: TermCode,
    },
    /// One `CATALIST` page per subject for one academic year.
    Catalog {
        /// The academic year Rice keys the catalog by.
        #[arg(long, value_parser = catalog_year)]
        catalog_year: CatalogYear,
    },
    /// Section listings, one page per subject.
    Listings {
        /// Rice's six-digit term code.
        #[arg(long, value_parser = term_code)]
        term: TermCode,
    },
    /// `ASSOCIATED-SECTIONS` XML; `stale_days` skips CRNs fetched that recently.
    Sections {
        /// Rice's six-digit term code.
        #[arg(long, value_parser = term_code)]
        term: TermCode,
        /// Skip CRNs whose XML is younger than this.
        #[arg(long, default_value_t = 7)]
        stale_days: u16,
    },
    /// The 1.4 s HTML page, for reserved seats and fees; same `stale_days` rule.
    Detail {
        /// Rice's six-digit term code.
        #[arg(long, value_parser = term_code)]
        term: TermCode,
        /// Skip CRNs whose detail page is younger than this.
        #[arg(long, default_value_t = 7)]
        stale_days: u16,
    },
    /// Live seats for the poll set. Exits at once outside a poll window.
    Seats {
        /// Rice's six-digit term code.
        #[arg(long, value_parser = term_code)]
        term: TermCode,
        /// Stop polling after this long, keeping what was polled.
        #[arg(long, default_value_t = 12)]
        deadline_minutes: u16,
    },
    /// GA program index and pages, as drafts for review. `--only` is
    /// repeatable and holds program slugs.
    Requirements {
        /// The catalog year (the first year of the GA edition).
        #[arg(long, value_parser = catalog_year)]
        catalog_year: CatalogYear,
        /// Limit the pull to these slugs.
        #[arg(long)]
        only: Vec<String>,
    },
}

impl PullJob {
    fn name(&self) -> &'static str {
        use skyspace_ingest::job_names as j;
        match self {
            Self::Reference { .. } => j::REFERENCE,
            Self::Catalog { .. } => j::CATALOG,
            Self::Listings { .. } => j::LISTINGS,
            Self::Sections { .. } => j::SECTIONS,
            Self::Detail { .. } => j::DETAIL,
            Self::Seats { .. } => j::SEATS,
            Self::Requirements { .. } => j::REQUIREMENTS,
        }
    }
}

fn days(n: u16) -> Duration {
    Duration::from_secs(u64::from(n) * 24 * 3600)
}

/// Run one job with the real fetcher and print its summary.
///
/// # Errors
/// Configuration, connection, or a job error that is not a recorded run.
pub async fn run(job: PullJob, config: &Config) -> anyhow::Result<i32> {
    let name = job.name();
    let f = config.fetcher()?;
    let ctx = config.job_ctx().await?;
    let summary = match job {
        PullJob::Reference { term } => skyspace_ingest::pull_reference_lists(&f, &ctx, term).await,
        PullJob::Catalog { catalog_year } => {
            skyspace_ingest::pull_catalog(&f, &ctx, catalog_year).await
        }
        PullJob::Listings { term } => skyspace_ingest::pull_section_listings(&f, &ctx, term).await,
        PullJob::Sections { term, stale_days } => {
            skyspace_ingest::pull_section_xml(&f, &ctx, term, days(stale_days)).await
        }
        PullJob::Detail { term, stale_days } => {
            skyspace_ingest::pull_course_detail(&f, &ctx, term, days(stale_days)).await
        }
        PullJob::Seats {
            term,
            deadline_minutes,
        } => {
            let deadline = Duration::from_secs(60 * u64::from(deadline_minutes));
            skyspace_ingest::poll_seats(&f, &ctx, term, deadline).await
        }
        PullJob::Requirements { catalog_year, only } => {
            skyspace_ingest::pull_requirements(&f, &ctx, catalog_year, &only).await
        }
    };
    report(name, summary)
}

/// Print one line for the run and map its outcome to an exit code. A
/// skipped run (another process holds the lock) is not a failure.
///
/// # Errors
/// Any job error other than the lock being held.
pub fn report(name: &str, summary: Result<Summary, JobError>) -> anyhow::Result<i32> {
    match summary {
        Ok(summary) => {
            println!("{}", summary.line(name));
            Ok(exit_code(summary.outcome))
        }
        Err(JobError::Locked) => {
            println!("{name}: another run holds the lock; skipped");
            Ok(0)
        }
        Err(e) => Err(e.into()),
    }
}
