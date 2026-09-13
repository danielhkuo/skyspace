//! `skyspace import university --file data/university/<year>.toml`: the
//! hand-encoded university-wide requirements, loaded as a draft into the
//! same review queue as an extracted program. One code path, not two.
//!
//! The TOML shape is documented in the file's header: one `[program]`,
//! then `[[area]]` blocks holding `[[area.rule]]` rows.

use std::path::Path;

use serde::Deserialize;
use skyspace_core::catalog::Attribute;
use skyspace_core::code::{CourseCode, Subject};
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, CreditScope, NonCourseKind, Requirement,
    RequirementBody, RequirementId, SourceRef,
};
use skyspace_core::term::{CreditRange, Credits};
use skyspace_ingest::{RunOutcome, job_names};
use skyspace_parse::{AreaDraft, ProgramDraft};
use skyspace_store::{DraftInput, RunSummaryRow};
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Deserialize)]
struct File {
    program: ProgramHeader,
    #[serde(default)]
    area: Vec<Area>,
}

#[derive(Debug, Deserialize)]
struct ProgramHeader {
    slug: String,
    name: String,
    #[serde(default)]
    credential: String,
    catalog_year: u16,
    #[serde(default)]
    total_credits: Option<String>,
    source_url: String,
}

#[derive(Debug, Deserialize)]
struct Area {
    title: String,
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct Rule {
    label: String,
    kind: String,
    #[serde(default)]
    hours: Option<String>,
    #[serde(default)]
    include: Vec<Selector>,
    #[serde(default)]
    exclude: Vec<Selector>,
    /// Credits for `credits`; a department count for `distinct_departments`.
    #[serde(default)]
    minimum: Option<toml::Value>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    semesters: Option<u8>,
    #[serde(default)]
    count: Option<u8>,
    #[serde(default)]
    of: Vec<Rule>,
    #[serde(default)]
    non_course_kind: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Selector {
    Code {
        code: String,
    },
    Subject {
        subject: String,
    },
    NumberRange {
        #[serde(default)]
        subject: Option<String>,
        low: u16,
        high: u16,
    },
    Attribute {
        attribute: String,
    },
}

/// Why the file could not be read into a draft.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The file is not the documented shape.
    #[error("{0}")]
    Shape(String),
}

fn shape(text: impl Into<String>) -> ImportError {
    ImportError::Shape(text.into())
}

fn selector(s: Selector, at: &str) -> Result<CourseSelector, ImportError> {
    Ok(match s {
        Selector::Code { code } => CourseSelector::Code {
            code: CourseCode::parse(&code)
                .map_err(|e| shape(format!("{at}: code {code:?}: {e}")))?,
        },
        Selector::Subject { subject } => CourseSelector::Subject {
            subject: Subject::new(&subject)
                .map_err(|e| shape(format!("{at}: subject {subject:?}: {e}")))?,
        },
        Selector::NumberRange { subject, low, high } => CourseSelector::NumberRange {
            subject: subject
                .map(|s| Subject::new(&s).map_err(|e| shape(format!("{at}: subject {s:?}: {e}"))))
                .transpose()?,
            low,
            high,
        },
        Selector::Attribute { attribute } => CourseSelector::Attribute {
            attribute: Attribute::from_code(&attribute).ok_or_else(|| {
                shape(format!("{at}: attribute {attribute:?} is not AD or GRP1-3"))
            })?,
        },
    })
}

fn filter(
    include: Vec<Selector>,
    exclude: Vec<Selector>,
    at: &str,
) -> Result<CourseFilter, ImportError> {
    Ok(CourseFilter {
        include: include
            .into_iter()
            .map(|s| selector(s, at))
            .collect::<Result<_, _>>()?,
        exclude: exclude
            .into_iter()
            .map(|s| selector(s, at))
            .collect::<Result<_, _>>()?,
    })
}

fn credits(raw: &str, at: &str) -> Result<Credits, ImportError> {
    Credits::parse(raw).map_err(|e| shape(format!("{at}: credits {raw:?}: {e}")))
}

/// A deterministic placeholder, the way the parser mints them: UUID v5 of
/// a fingerprint, so re-importing the same file yields the same ids. The
/// store replaces them at approval.
fn placeholder_id(slug: &str, area: usize, row: usize, text: &str) -> RequirementId {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let fingerprint = format!(
        "skyspace-import|{slug}|{area}|{row}|{}",
        clean.to_ascii_lowercase()
    );
    RequirementId(Uuid::new_v5(&Uuid::NAMESPACE_URL, fingerprint.as_bytes()))
}

fn body(
    rule: Rule,
    slug: &str,
    area: usize,
    row: usize,
    source: &SourceRef,
    at: &str,
) -> Result<(RequirementBody, Option<CreditRange>), ImportError> {
    let hours = rule
        .hours
        .as_deref()
        .map(|h| CreditRange::parse(h).map_err(|e| shape(format!("{at}: hours {h:?}: {e}"))))
        .transpose()?;
    let body = match rule.kind.as_str() {
        "course" => RequirementBody::Course {
            filter: filter(rule.include, rule.exclude, at)?,
            semesters: rule.semesters.unwrap_or(1),
        },
        "credits" => {
            let minimum = match rule.minimum {
                Some(toml::Value::String(s)) => credits(&s, at)?,
                Some(toml::Value::Integer(n)) => credits(&n.to_string(), at)?,
                _ => return Err(shape(format!("{at}: credits rule needs `minimum`"))),
            };
            let scope = match rule.scope.as_deref().unwrap_or("any") {
                "any" => CreditScope::Any,
                "additional" => CreditScope::Additional,
                other => return Err(shape(format!("{at}: scope {other:?}"))),
            };
            RequirementBody::Credits {
                minimum,
                scope,
                from: filter(rule.include, rule.exclude, at)?,
            }
        }
        "select" => {
            let count = rule
                .count
                .ok_or_else(|| shape(format!("{at}: select rule needs `count`")))?;
            let of = rule
                .of
                .into_iter()
                .enumerate()
                .map(|(i, child)| requirement(child, slug, area, row * 100 + i + 1, source))
                .collect::<Result<_, _>>()?;
            RequirementBody::Select { count, of }
        }
        "non_course" => RequirementBody::NonCourse {
            non_course_kind: match rule.non_course_kind.as_deref().unwrap_or("other") {
                "proficiency_exam" => NonCourseKind::ProficiencyExam,
                "portfolio" => NonCourseKind::Portfolio,
                "other" => NonCourseKind::Other,
                other => return Err(shape(format!("{at}: non_course_kind {other:?}"))),
            },
            description: rule.description.unwrap_or_else(|| rule.label.clone()),
        },
        "unverifiable" => RequirementBody::Unverifiable {
            text: rule.text.unwrap_or_else(|| rule.label.clone()),
        },
        "distinct_departments" => {
            let minimum = match rule.minimum {
                Some(toml::Value::Integer(n)) => {
                    u8::try_from(n).map_err(|_| shape(format!("{at}: minimum {n} does not fit")))?
                }
                _ => {
                    return Err(shape(format!(
                        "{at}: distinct_departments needs an integer `minimum`"
                    )));
                }
            };
            RequirementBody::DistinctDepartments {
                minimum,
                text: rule.text.unwrap_or_else(|| rule.label.clone()),
            }
        }
        other => return Err(shape(format!("{at}: unknown rule kind {other:?}"))),
    };
    Ok((body, hours))
}

fn requirement(
    rule: Rule,
    slug: &str,
    area: usize,
    row: usize,
    source: &SourceRef,
) -> Result<Requirement, ImportError> {
    let at = format!("area {} rule {} ({:?})", area + 1, row + 1, rule.label);
    let label = rule.label.clone();
    let id = placeholder_id(slug, area, row, &label);
    let (body, hours) = body(rule, slug, area, row, source, &at)?;
    Ok(Requirement {
        id,
        label,
        hours,
        source: source.clone(),
        body,
    })
}

/// Read the file into the draft shape the review queue holds.
///
/// # Errors
/// `ImportError::Shape` for anything outside the documented shape.
pub fn draft_from_toml(text: &str) -> Result<(ProgramDraft, String, CatalogYear), ImportError> {
    let file: File = toml::from_str(text).map_err(|e| shape(format!("toml: {e}")))?;
    let header = file.program;
    let source = SourceRef {
        url: header.source_url.clone(),
        anchor: None,
    };
    let total_credits = header
        .total_credits
        .as_deref()
        .map(|t| credits(t, "program.total_credits"))
        .transpose()?;
    let mut areas = Vec::with_capacity(file.area.len());
    for (a, area) in file.area.into_iter().enumerate() {
        let rules = area
            .rule
            .into_iter()
            .enumerate()
            .map(|(r, rule)| requirement(rule, &header.slug, a, r, &source))
            .collect::<Result<Vec<_>, _>>()?;
        areas.push(AreaDraft {
            title: area.title,
            rules,
            unparsed: Vec::new(),
        });
    }
    let draft = ProgramDraft {
        title: header.name,
        credential: header.credential,
        source_url: header.source_url,
        total_credits,
        areas,
        aliases: Vec::new(),
        footnotes: Vec::new(),
    };
    Ok((draft, header.slug, CatalogYear(header.catalog_year)))
}

/// Load the file and queue it as a pending draft under an `import` run.
///
/// # Errors
/// The file, its shape, or the store.
pub async fn university(file: &Path, config: &Config) -> anyhow::Result<i32> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", file.display()))?;
    let (draft, slug, year) = draft_from_toml(&text)?;
    let rules: usize = draft.areas.iter().map(|a| a.rules.len()).sum();
    let store = config.store().await?;
    let run = store.start_run(job_names::IMPORT, None).await?;
    let result = store
        .put_draft(&DraftInput {
            run_id: run,
            slug: slug.clone(),
            catalog_year: year,
            source_url: draft.source_url.clone(),
            source_sha256: skyspace_ingest::archive::sha256(text.as_bytes()),
            body: serde_json::to_value(&draft)?,
        })
        .await;
    let summary = RunSummaryRow {
        outcome: if result.is_ok() {
            RunOutcome::Ok
        } else {
            RunOutcome::Failed
        },
        requests: 0,
        targets: 1,
        failures: 0,
        bytes: u64::try_from(text.len()).unwrap_or(0),
        rows_written: u64::from(result.is_ok()),
        error: result.as_ref().err().map(ToString::to_string),
    };
    store.finish_run(run, &summary).await?;
    let id = result?;
    println!(
        "queued draft {} for {slug} {}: {} areas, {rules} rules; review with `skyspace review approve {} --reviewer <name>`",
        id.0,
        year.0,
        draft.areas.len(),
        id.0
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use skyspace_core::program::RequirementBody;

    use super::draft_from_toml;

    #[test]
    fn the_checked_in_file_loads() {
        let text = include_str!("../../../../data/university/2026.toml");
        let (draft, slug, year) = draft_from_toml(text).unwrap();
        assert_eq!(slug, "university-requirements");
        assert_eq!(year.0, 2026);
        assert_eq!(draft.credential, "");
        assert_eq!(
            draft.total_credits.map(skyspace_core::Credits::cents),
            Some(12000)
        );
        let group_one = &draft.areas[1];
        assert_eq!(group_one.rules.len(), 4);
        assert!(matches!(
            group_one.rules[3].body,
            RequirementBody::DistinctDepartments { minimum: 2, .. }
        ));
        // Same file, same placeholder ids.
        let (again, _, _) = draft_from_toml(text).unwrap();
        assert_eq!(again.areas[0].rules[0].id, draft.areas[0].rules[0].id);
    }

    #[test]
    fn a_bad_kind_is_named() {
        let text = "[program]\nslug='x'\nname='X'\ncatalog_year=2026\nsource_url='u'\n[[area]]\ntitle='A'\n[[area.rule]]\nlabel='r'\nkind='magic'\n";
        let err = draft_from_toml(text).unwrap_err().to_string();
        assert!(err.contains("magic"), "{err}");
    }
}
