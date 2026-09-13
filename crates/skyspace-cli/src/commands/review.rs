//! `skyspace review`: the requirement review queue. Extraction writes a
//! draft; only a person's approval here writes `programs`.

use clap::{Subcommand, ValueEnum};
use skyspace_core::program::{ProgramId, ProgramKind};
use skyspace_parse::ProgramDraft;
use skyspace_store::{DraftId, DraftRow, DraftState, StoreError, VersionSource, review_now};
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

/// The kind to publish under: the `--kind` flag, else the `kind` an
/// imported file declared in the draft body, else what the slug suggests.
///
/// # Errors
/// When the body's `kind` is not one of the five spellings.
pub fn kind_for(
    flag: Option<Kind>,
    body: &serde_json::Value,
    slug: &str,
) -> anyhow::Result<ProgramKind> {
    if let Some(kind) = flag {
        return Ok(kind.into());
    }
    match body.get("kind").and_then(serde_json::Value::as_str) {
        Some(text) => parse_kind(text)
            .ok_or_else(|| anyhow::anyhow!("draft body names an unknown kind {text:?}")),
        None => Ok(infer_kind(slug)),
    }
}

/// The five spellings `programs.kind` accepts.
#[must_use]
pub fn parse_kind(text: &str) -> Option<ProgramKind> {
    match text {
        "university" => Some(ProgramKind::University),
        "major" => Some(ProgramKind::Major),
        "minor" => Some(ProgramKind::Minor),
        "certificate" => Some(ProgramKind::Certificate),
        "concentration" => Some(ProgramKind::Concentration),
        _ => None,
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
            let kind = kind_for(kind, &draft.body, &draft.slug)?;
            // GA prints `STAT 310 / ECON 307`; the pairs land in
            // `course_aliases` under the catalog's own rule.
            let aliases = body.aliases.clone();
            let program = body.into_program(
                ProgramId(Uuid::new_v4()),
                draft.catalog_year,
                kind,
                draft.slug.clone(),
                review_now(&reviewer, OffsetDateTime::now_utc()),
            );
            // Hand-encoded rules are `source = 'manual'`; extracted ones `ga`.
            let source = if kind == ProgramKind::University {
                VersionSource::Manual
            } else {
                VersionSource::Ga
            };
            let approved = match store
                .approve_draft_as(DraftId(id), &reviewer, &program, source)
                .await
            {
                Ok(approved) => approved,
                Err(StoreError::Input(why)) => {
                    println!("{why}");
                    return Ok(1);
                }
                Err(e) => return Err(e.into()),
            };
            let Some(program_id) = approved else {
                println!("no draft {id}");
                return Ok(1);
            };
            let alias_pairs = store.upsert_aliases(&aliases).await?;
            println!(
                "approved draft {id}: {} {} for {} as program {} with {} rules and {alias_pairs} alias pairs",
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

    use super::{Kind, infer_kind, kind_for};

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

    /// The flag wins, then the body's declared kind, then the slug.
    #[test]
    fn kind_prefers_flag_then_body_then_slug() {
        let body = serde_json::json!({ "kind": "university" });
        assert_eq!(
            kind_for(Some(Kind::Minor), &body, "anything").unwrap(),
            ProgramKind::Minor
        );
        assert_eq!(
            kind_for(None, &body, "not-the-university-slug").unwrap(),
            ProgramKind::University
        );
        assert_eq!(
            kind_for(None, &serde_json::json!({}), "data-science-minor").unwrap(),
            ProgramKind::Minor
        );
        assert!(kind_for(None, &serde_json::json!({ "kind": "magic" }), "x").is_err());
    }
}
