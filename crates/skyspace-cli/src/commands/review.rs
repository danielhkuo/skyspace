//! `skyspace review`: the requirement review queue. Extraction writes a
//! draft; only a person's approval here writes `programs`.

use clap::{Subcommand, ValueEnum};
use skyspace_core::program::{ProgramId, ProgramKind};
use skyspace_parse::ProgramDraft;
use skyspace_store::{DraftId, DraftRow, DraftState, VersionSource, review_now};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::config::Config;

/// A review action.
#[derive(Debug, Subcommand)]
pub enum ReviewCommand {
    /// Pending drafts, newest first.
    List,
    /// Print one draft's body as JSON.
    Export {
        /// The draft id from `review list`.
        id: i64,
    },
    /// Publish a draft as a program version and mark it approved.
    Approve {
        /// The draft id from `review list`.
        id: i64,
        /// Who reviewed it; recorded on the version.
        #[arg(long)]
        reviewer: String,
        /// What sort of program. Inferred from the slug when omitted:
        /// `university-requirements` is university, `-minor` a minor,
        /// `certificate` a certificate, `concentration` a concentration,
        /// anything else a major.
        #[arg(long, value_enum)]
        kind: Option<Kind>,
    },
    /// Refuse a draft.
    Reject {
        /// The draft id from `review list`.
        id: i64,
        /// Who refused it.
        #[arg(long)]
        reviewer: String,
    },
}

/// `ProgramKind` for `clap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// The university-wide requirements.
    University,
    /// A major.
    Major,
    /// A minor.
    Minor,
    /// A certificate.
    Certificate,
    /// A concentration.
    Concentration,
}

impl From<Kind> for ProgramKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::University => Self::University,
            Kind::Major => Self::Major,
            Kind::Minor => Self::Minor,
            Kind::Certificate => Self::Certificate,
            Kind::Concentration => Self::Concentration,
        }
    }
}

/// The kind a slug suggests. A reviewer can override it.
#[must_use]
pub fn infer_kind(slug: &str) -> ProgramKind {
    if slug == "university-requirements" {
        ProgramKind::University
    } else if slug.ends_with("-minor") || slug.contains("-minor-") {
        ProgramKind::Minor
    } else if slug.contains("certificate") {
        ProgramKind::Certificate
    } else if slug.contains("concentration") {
        ProgramKind::Concentration
    } else {
        ProgramKind::Major
    }
}

fn draft_line(d: &DraftRow) -> String {
    let unparsed: usize = serde_json::from_value::<ProgramDraft>(d.body.clone())
        .map_or(0, |p| p.areas.iter().map(|a| a.unparsed.len()).sum());
    format!(
        "{:>6}  {}  {:<40}  extracted {}  unparsed rows {}  {}",
        d.id.0,
        d.catalog_year.0,
        d.slug,
        d.created_at.date(),
        unparsed,
        d.source_url
    )
}

/// Run one review action.
///
/// # Errors
/// Connection or store errors, or a draft that is not a `ProgramDraft`.
pub async fn run(command: ReviewCommand, config: &Config) -> anyhow::Result<i32> {
    let store = config.store().await?;
    match command {
        ReviewCommand::List => {
            let drafts = store.drafts(DraftState::Pending).await?;
            if drafts.is_empty() {
                println!("no pending drafts");
            }
            for draft in &drafts {
                println!("{}", draft_line(draft));
            }
            println!("{} pending", drafts.len());
            Ok(0)
        }
        ReviewCommand::Export { id } => {
            let Some(draft) = store.draft(DraftId(id)).await? else {
                println!("no draft {id}");
                return Ok(1);
            };
            println!("{}", serde_json::to_string_pretty(&draft.body)?);
            Ok(0)
        }
        ReviewCommand::Approve { id, reviewer, kind } => {
            let Some(draft) = store.draft(DraftId(id)).await? else {
                println!("no draft {id}");
                return Ok(1);
            };
            let body: ProgramDraft = serde_json::from_value(draft.body.clone())?;
            let kind = kind.map_or_else(|| infer_kind(&draft.slug), ProgramKind::from);
            let program = body.into_program(
                ProgramId(Uuid::new_v4()),
                draft.catalog_year,
                kind,
                draft.slug.clone(),
                review_now(&reviewer, OffsetDateTime::now_utc()),
            );
            let Some(program_id) = store
                .approve_draft(DraftId(id), &reviewer, &program)
                .await?
            else {
                println!("no draft {id}");
                return Ok(1);
            };
            if kind == ProgramKind::University {
                // Hand-encoded rules are `source = 'manual'`; the approval
                // path only knows GA, so the version is published again
                // with the right source. Fingerprints keep every rule id.
                store
                    .publish_program(&program, &reviewer, VersionSource::Manual)
                    .await?;
            }
            println!(
                "approved draft {id}: {} {} for {} as program {} with {} rules",
                draft.slug,
                program.credential,
                draft.catalog_year.0,
                program_id.0,
                program.requirement_count()
            );
            Ok(0)
        }
        ReviewCommand::Reject { id, reviewer } => {
            if store.reject_draft(DraftId(id), &reviewer).await? {
                println!("rejected draft {id}");
                Ok(0)
            } else {
                println!("no draft {id}");
                Ok(1)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use skyspace_core::program::ProgramKind;

    use super::infer_kind;

    #[test]
    fn kinds_from_slugs() {
        assert_eq!(
            infer_kind("university-requirements"),
            ProgramKind::University
        );
        assert_eq!(infer_kind("computer-science-bscs"), ProgramKind::Major);
        assert_eq!(infer_kind("data-science-minor"), ProgramKind::Minor);
        assert_eq!(infer_kind("teaching-certificate"), ProgramKind::Certificate);
    }
}
