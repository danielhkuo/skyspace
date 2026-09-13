//! Degree programs: the draft queue, publishing a reviewed program, and
//! rebuilding the requirement tree from its rows.

use std::collections::BTreeMap;

use skyspace_core::Timestamp;
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, CreditScope, NonCourseKind, Program, ProgramId,
    ProgramKind, Requirement, RequirementBody, RequirementId, Review, SourceRef,
};
use skyspace_core::term::{CreditRange, Credits};
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::convert::{
    cents_from_db, cents_to_db, credits_from_db, credits_to_db, hex, sha256, timestamp_from_db,
    timestamp_to_db,
};
use crate::courses::{year_from_db, year_to_db};
use crate::error::{StoreError, corrupt};
use crate::pool::Store;

/// A `program_drafts` row id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DraftId(pub i64);

/// A draft's review state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftState {
    /// Waiting for a reviewer.
    Pending,
    /// Published.
    Approved,
    /// Refused.
    Rejected,
}

impl DraftState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    fn parse(text: &str) -> Result<Self, StoreError> {
        match text {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            other => Err(corrupt("program_drafts", "state", format!("{other:?}"))),
        }
    }
}

/// What the requirements job writes for review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftInput {
    /// The run that extracted it.
    pub run_id: i64,
    /// GA slug.
    pub slug: String,
    /// The edition.
    pub catalog_year: CatalogYear,
    /// The page.
    pub source_url: String,
    /// The archived page's key.
    pub source_sha256: [u8; 32],
    /// The serialised `ProgramDraft`.
    pub body: serde_json::Value,
}

/// One `program_drafts` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRow {
    /// Row id.
    pub id: DraftId,
    /// The run that extracted it.
    pub run_id: i64,
    /// GA slug.
    pub slug: String,
    /// The edition.
    pub catalog_year: CatalogYear,
    /// The page.
    pub source_url: String,
    /// The archived page's key.
    pub source_sha256: Vec<u8>,
    /// The serialised `ProgramDraft`.
    pub body: serde_json::Value,
    /// Review state.
    pub state: DraftState,
    /// Who reviewed it.
    pub reviewer: Option<String>,
    /// When.
    pub reviewed_at: Option<OffsetDateTime>,
    /// When it was extracted.
    pub created_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct DraftDbRow {
    id: i64,
    run_id: i64,
    slug: String,
    catalog_year: i16,
    source_url: String,
    source_sha256: Vec<u8>,
    body: serde_json::Value,
    state: String,
    reviewer: Option<String>,
    reviewed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

impl TryFrom<DraftDbRow> for DraftRow {
    type Error = StoreError;

    fn try_from(row: DraftDbRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: DraftId(row.id),
            run_id: row.run_id,
            slug: row.slug,
            catalog_year: year_from_db("program_drafts", row.catalog_year)?,
            source_url: row.source_url,
            source_sha256: row.source_sha256,
            body: row.body,
            state: DraftState::parse(&row.state)?,
            reviewer: row.reviewer,
            reviewed_at: row.reviewed_at,
            created_at: row.created_at,
        })
    }
}

/// Where a published version came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionSource {
    /// Extracted from General Announcements and reviewed.
    Ga,
    /// Hand-encoded, as the university-wide requirements are.
    Manual,
}

/// One program version, for the program list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSummaryRow {
    /// Stable id.
    pub id: ProgramId,
    /// GA slug.
    pub slug: String,
    /// What sort of program.
    pub kind: ProgramKind,
    /// Display name.
    pub name: String,
    /// `BSCS`, `BMus`, or empty.
    pub credential: String,
    /// The edition.
    pub catalog_year: CatalogYear,
    /// The page's total, when it prints one.
    pub total_credits: Option<Credits>,
}

const DRAFT_COLUMNS: &str = "id, run_id, slug, catalog_year, source_url, source_sha256, body, state, reviewer, reviewed_at, created_at";

fn kind_to_db(kind: ProgramKind) -> &'static str {
    match kind {
        ProgramKind::University => "university",
        ProgramKind::Major => "major",
        ProgramKind::Minor => "minor",
        ProgramKind::Certificate => "certificate",
        ProgramKind::Concentration => "concentration",
    }
}

fn kind_from_db(text: &str) -> Result<ProgramKind, StoreError> {
    match text {
        "university" => Ok(ProgramKind::University),
        "major" => Ok(ProgramKind::Major),
        "minor" => Ok(ProgramKind::Minor),
        "certificate" => Ok(ProgramKind::Certificate),
        "concentration" => Ok(ProgramKind::Concentration),
        other => Err(corrupt("programs", "kind", format!("{other:?}"))),
    }
}

fn scope_to_db(scope: CreditScope) -> &'static str {
    match scope {
        CreditScope::Any => "any",
        CreditScope::Additional => "additional",
    }
}

fn non_course_to_db(kind: NonCourseKind) -> &'static str {
    match kind {
        NonCourseKind::ProficiencyExam => "proficiency_exam",
        NonCourseKind::Portfolio => "portfolio",
        NonCourseKind::Other => "other",
    }
}

/// The text a rule's fingerprint hashes: the page's own words for a rule
/// that carries them, else the label.
fn source_text(requirement: &Requirement) -> &str {
    match &requirement.body {
        RequirementBody::NonCourse { description, .. } => description,
        RequirementBody::Unverifiable { text }
        | RequirementBody::DistinctDepartments { text, .. } => text,
        RequirementBody::All { .. }
        | RequirementBody::Select { .. }
        | RequirementBody::Course { .. }
        | RequirementBody::Credits { .. } => &requirement.label,
    }
}

/// Lowercase, NBSP to space, runs of whitespace to one space.
fn normalise(text: &str) -> String {
    text.to_lowercase()
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `sha256(path + "\n" + normalised text)` as hex, where `path` is the
/// ordinal path from the root (`0/2/1`).
fn fingerprint(path: &str, text: &str) -> String {
    hex(&sha256(format!("{path}\n{}", normalise(text)).as_bytes()))
}

/// One requirement flattened for insertion, parent before child.
struct FlatRequirement<'a> {
    requirement: &'a Requirement,
    id: Uuid,
    parent: Option<Uuid>,
    ordinal: i16,
    fingerprint: String,
}

fn flatten<'a>(
    node: &'a Requirement,
    parent: Option<Uuid>,
    ordinal: i16,
    path: &str,
    existing: &mut BTreeMap<String, Uuid>,
    out: &mut Vec<FlatRequirement<'a>>,
) {
    let fingerprint = fingerprint(path, source_text(node));
    let id = existing.remove(&fingerprint).unwrap_or_else(Uuid::new_v4);
    out.push(FlatRequirement {
        requirement: node,
        id,
        parent,
        ordinal,
        fingerprint,
    });
    if let RequirementBody::All { of } | RequirementBody::Select { of, .. } = &node.body {
        for (index, child) in of.iter().enumerate() {
            let ordinal = i16::try_from(index).unwrap_or(i16::MAX);
            flatten(
                child,
                Some(id),
                ordinal,
                &format!("{path}/{index}"),
                existing,
                out,
            );
        }
    }
}

fn code_selectors(filter: &CourseFilter) -> Vec<&skyspace_core::code::CourseCode> {
    filter
        .include
        .iter()
        .filter_map(|s| match s {
            CourseSelector::Code { code } => Some(code),
            _ => None,
        })
        .collect()
}

/// The kind-specific columns of one `requirements` row.
struct BodyColumns<'a> {
    kind: &'static str,
    select_count: Option<i16>,
    semesters: i16,
    min_credits: Option<i16>,
    scope: Option<&'static str>,
    non_course_kind: Option<&'static str>,
    min_departments: Option<i16>,
    description: Option<&'a str>,
    filter: Option<&'a CourseFilter>,
}

impl<'a> BodyColumns<'a> {
    fn of(body: &'a RequirementBody) -> Result<Self, StoreError> {
        let mut cols = Self {
            kind: "all",
            select_count: None,
            semesters: 1,
            min_credits: None,
            scope: None,
            non_course_kind: None,
            min_departments: None,
            description: None,
            filter: None,
        };
        match body {
            RequirementBody::All { .. } => {}
            RequirementBody::Select { count, .. } => {
                cols.kind = "select";
                cols.select_count = Some(i16::from(*count));
            }
            RequirementBody::Course { filter, semesters } => {
                cols.kind = "course";
                cols.semesters = i16::from(*semesters);
                cols.filter = Some(filter);
            }
            RequirementBody::Credits {
                minimum,
                scope,
                from,
            } => {
                cols.kind = "credits";
                cols.min_credits = Some(cents_to_db(*minimum)?);
                cols.scope = Some(scope_to_db(*scope));
                cols.filter = Some(from);
            }
            RequirementBody::NonCourse {
                non_course_kind,
                description,
            } => {
                cols.kind = "non_course";
                cols.non_course_kind = Some(non_course_to_db(*non_course_kind));
                cols.description = Some(description);
            }
            RequirementBody::Unverifiable { .. } => cols.kind = "unverifiable",
            RequirementBody::DistinctDepartments { minimum, .. } => {
                cols.kind = "distinct_departments";
                cols.min_departments = Some(i16::from(*minimum));
            }
        }
        Ok(cols)
    }
}

async fn insert_requirement(
    conn: &mut PgConnection,
    program_id: Uuid,
    year: i16,
    row: &FlatRequirement<'_>,
) -> Result<(), StoreError> {
    let r = row.requirement;
    let hours = r.hours.map(credits_to_db).transpose()?;
    let cols = BodyColumns::of(&r.body)?;
    sqlx::query(
        "insert into requirements (id, program_id, catalog_year, parent_id, ordinal, kind, label, hours_kind, \
            hours_min_cents, hours_max_cents, select_count, semesters, min_credits_cents, credit_scope, \
            non_course_kind, min_departments, description, filter, source_text, source_url, source_anchor, fingerprint) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)",
    )
    .bind(row.id)
    .bind(program_id)
    .bind(year)
    .bind(row.parent)
    .bind(row.ordinal)
    .bind(cols.kind)
    .bind(&r.label)
    .bind(hours.map(|h| h.kind))
    .bind(hours.map(|h| h.min))
    .bind(hours.map(|h| h.max))
    .bind(cols.select_count)
    .bind(cols.semesters)
    .bind(cols.min_credits)
    .bind(cols.scope)
    .bind(cols.non_course_kind)
    .bind(cols.min_departments)
    .bind(cols.description)
    .bind(cols.filter.map(serde_json::to_value).transpose()?)
    .bind(source_text(r))
    .bind(&r.source.url)
    .bind(&r.source.anchor)
    .bind(&row.fingerprint)
    .execute(&mut *conn)
    .await?;
    if let Some(filter) = cols.filter {
        for code in code_selectors(filter) {
            sqlx::query(
                "insert into requirement_courses (requirement_id, subject, number) values ($1, $2, $3) \
                 on conflict do nothing",
            )
            .bind(row.id)
            .bind(code.subject.as_str())
            .bind(code.number.as_str())
            .execute(&mut *conn)
            .await?;
        }
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct ExistingRow {
    id: Uuid,
    fingerprint: String,
}

/// Publish one version: the `programs` row by slug, the version row, and
/// the tree with fresh ids except where a fingerprint matches a rule of the
/// previous publish of the same year. Ids that were not carried over are
/// retired, never reused.
async fn publish(
    conn: &mut PgConnection,
    program: &Program,
    reviewer: &str,
    source: VersionSource,
    from_draft: Option<DraftId>,
) -> Result<ProgramId, StoreError> {
    let program_id: Uuid = sqlx::query_scalar(
        "insert into programs (slug, kind, name, credential) values ($1, $2, $3, $4) \
         on conflict (slug) do update set kind = excluded.kind, name = excluded.name, credential = excluded.credential \
         returning id",
    )
    .bind(&program.slug)
    .bind(kind_to_db(program.kind))
    .bind(&program.name)
    .bind(&program.credential)
    .fetch_one(&mut *conn)
    .await?;
    let year = year_to_db(program.catalog_year)?;
    let previous: Vec<ExistingRow> = sqlx::query_as(
        "select id, fingerprint from requirements where program_id = $1 and catalog_year = $2",
    )
    .bind(program_id)
    .bind(year)
    .fetch_all(&mut *conn)
    .await?;
    let previously_retired: Option<Vec<Uuid>> = sqlx::query_scalar(
        "select retired_requirements from program_versions where program_id = $1 and catalog_year = $2",
    )
    .bind(program_id)
    .bind(year)
    .fetch_optional(&mut *conn)
    .await?;
    sqlx::query("delete from program_versions where program_id = $1 and catalog_year = $2")
        .bind(program_id)
        .bind(year)
        .execute(&mut *conn)
        .await?;

    let mut existing: BTreeMap<String, Uuid> = previous
        .iter()
        .map(|r| (r.fingerprint.clone(), r.id))
        .collect();
    let mut rows = Vec::new();
    flatten(&program.root, None, 0, "0", &mut existing, &mut rows);
    let mut retired: Vec<Uuid> = previously_retired.unwrap_or_default();
    retired.extend(program.retired_requirements.iter().map(|r| r.0));
    retired.extend(existing.into_values());
    retired.sort();
    retired.dedup();

    sqlx::query(
        "insert into program_versions (program_id, catalog_year, source, ga_url, total_credits_cents, from_draft, \
            published_at, reviewed_by, retired_requirements) values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(program_id)
    .bind(year)
    .bind(match source {
        VersionSource::Ga => "ga",
        VersionSource::Manual => "manual",
    })
    .bind(&program.source.url)
    .bind(program.total_credits.map(cents_to_db).transpose()?)
    .bind(from_draft.map(|d| d.0))
    .bind(timestamp_to_db(program.review.published_at)?)
    .bind(reviewer)
    .bind(&retired)
    .execute(&mut *conn)
    .await?;
    for row in &rows {
        insert_requirement(&mut *conn, program_id, year, row).await?;
    }
    Ok(ProgramId(program_id))
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    slug: String,
    kind: String,
    name: String,
    credential: String,
    ga_url: String,
    total_credits_cents: Option<i16>,
    published_at: OffsetDateTime,
    reviewed_by: String,
    retired_requirements: Vec<Uuid>,
}

#[derive(sqlx::FromRow)]
struct RequirementRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    kind: String,
    label: String,
    hours_kind: Option<String>,
    hours_min_cents: Option<i16>,
    hours_max_cents: Option<i16>,
    select_count: Option<i16>,
    semesters: i16,
    min_credits_cents: Option<i16>,
    credit_scope: Option<String>,
    non_course_kind: Option<String>,
    min_departments: Option<i16>,
    description: Option<String>,
    filter: Option<serde_json::Value>,
    source_text: String,
    source_url: String,
    source_anchor: Option<String>,
}

const REQ_TABLE: &str = "requirements";

fn small(column: &'static str, value: Option<i16>) -> Result<u8, StoreError> {
    let value = value.ok_or_else(|| corrupt(REQ_TABLE, column, "null"))?;
    u8::try_from(value).map_err(|_| corrupt(REQ_TABLE, column, value))
}

fn filter_of(row: &RequirementRow) -> Result<CourseFilter, StoreError> {
    let value = row
        .filter
        .clone()
        .ok_or_else(|| corrupt(REQ_TABLE, "filter", "null"))?;
    Ok(serde_json::from_value(value)?)
}

fn body_of(
    row: &RequirementRow,
    children: Vec<Requirement>,
) -> Result<RequirementBody, StoreError> {
    Ok(match row.kind.as_str() {
        "all" => RequirementBody::All { of: children },
        "select" => RequirementBody::Select {
            count: small("select_count", row.select_count)?,
            of: children,
        },
        "course" => RequirementBody::Course {
            filter: filter_of(row)?,
            semesters: small("semesters", Some(row.semesters))?,
        },
        "credits" => RequirementBody::Credits {
            minimum: cents_from_db(
                REQ_TABLE,
                "min_credits_cents",
                row.min_credits_cents
                    .ok_or_else(|| corrupt(REQ_TABLE, "min_credits_cents", "null"))?,
            )?,
            scope: match row.credit_scope.as_deref() {
                Some("any") => CreditScope::Any,
                Some("additional") => CreditScope::Additional,
                other => return Err(corrupt(REQ_TABLE, "credit_scope", format!("{other:?}"))),
            },
            from: filter_of(row)?,
        },
        "non_course" => RequirementBody::NonCourse {
            non_course_kind: match row.non_course_kind.as_deref() {
                Some("proficiency_exam") => NonCourseKind::ProficiencyExam,
                Some("portfolio") => NonCourseKind::Portfolio,
                Some("other") => NonCourseKind::Other,
                other => return Err(corrupt(REQ_TABLE, "non_course_kind", format!("{other:?}"))),
            },
            description: row.description.clone().unwrap_or_default(),
        },
        "unverifiable" => RequirementBody::Unverifiable {
            text: row.source_text.clone(),
        },
        "distinct_departments" => RequirementBody::DistinctDepartments {
            minimum: small("min_departments", row.min_departments)?,
            text: row.source_text.clone(),
        },
        other => return Err(corrupt(REQ_TABLE, "kind", format!("{other:?}"))),
    })
}

fn hours_of(row: &RequirementRow) -> Result<Option<CreditRange>, StoreError> {
    match (&row.hours_kind, row.hours_min_cents, row.hours_max_cents) {
        (Some(kind), Some(min), Some(max)) => Ok(Some(credits_from_db(REQ_TABLE, kind, min, max)?)),
        _ => Ok(None),
    }
}

/// Build the subtree under `parent` from rows grouped by parent id.
fn build(
    parent: Option<Uuid>,
    by_parent: &mut BTreeMap<Option<Uuid>, Vec<RequirementRow>>,
) -> Result<Vec<Requirement>, StoreError> {
    let rows = by_parent.remove(&parent).unwrap_or_default();
    rows.into_iter()
        .map(|row| {
            let children = build(Some(row.id), by_parent)?;
            Ok(Requirement {
                id: RequirementId(row.id),
                label: row.label.clone(),
                hours: hours_of(&row)?,
                source: SourceRef {
                    url: row.source_url.clone(),
                    anchor: row.source_anchor.clone(),
                },
                body: body_of(&row, children)?,
            })
        })
        .collect()
}

async fn load_program(
    conn: &mut PgConnection,
    id: ProgramId,
    year: CatalogYear,
) -> Result<Option<Program>, StoreError> {
    let db_year = year_to_db(year)?;
    let version: Option<VersionRow> = sqlx::query_as(
        "select p.slug, p.kind, p.name, p.credential, v.ga_url, v.total_credits_cents, v.published_at, \
                v.reviewed_by, v.retired_requirements \
         from programs p join program_versions v on v.program_id = p.id \
         where p.id = $1 and v.catalog_year = $2",
    )
    .bind(id.0)
    .bind(db_year)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(version) = version else {
        return Ok(None);
    };
    let rows: Vec<RequirementRow> = sqlx::query_as(
        "select id, parent_id, kind, label, hours_kind, hours_min_cents, hours_max_cents, select_count, \
                semesters, min_credits_cents, credit_scope, non_course_kind, min_departments, description, \
                filter, source_text, source_url, source_anchor \
         from requirements where program_id = $1 and catalog_year = $2 order by parent_id nulls first, ordinal",
    )
    .bind(id.0)
    .bind(db_year)
    .fetch_all(&mut *conn)
    .await?;
    let mut by_parent: BTreeMap<Option<Uuid>, Vec<RequirementRow>> = BTreeMap::new();
    for row in rows {
        by_parent.entry(row.parent_id).or_default().push(row);
    }
    let mut roots = build(None, &mut by_parent)?;
    let (Some(root), true) = (roots.pop(), roots.is_empty()) else {
        return Err(corrupt(
            REQ_TABLE,
            "parent_id",
            "a version must have exactly one root",
        ));
    };
    Ok(Some(Program {
        id,
        catalog_year: year,
        slug: version.slug,
        kind: kind_from_db(&version.kind)?,
        name: version.name,
        credential: version.credential,
        total_credits: version
            .total_credits_cents
            .map(|c| cents_from_db("program_versions", "total_credits_cents", c))
            .transpose()?,
        source: SourceRef {
            url: version.ga_url,
            anchor: None,
        },
        review: Review {
            reviewed_by: version.reviewed_by,
            published_at: timestamp_from_db(version.published_at),
        },
        root,
        retired_requirements: version
            .retired_requirements
            .into_iter()
            .map(RequirementId)
            .collect(),
    }))
}

/// The plan's year when held, else the earliest later year, else the
/// latest earlier year. Mirrors the engine's own choice.
fn pick_year(held: &[CatalogYear], wanted: CatalogYear) -> Option<CatalogYear> {
    if held.contains(&wanted) {
        return Some(wanted);
    }
    held.iter()
        .filter(|y| **y > wanted)
        .min()
        .or_else(|| held.iter().filter(|y| **y < wanted).max())
        .copied()
}

impl Store {
    /// Queue a draft for review. Returns its id.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn put_draft(&self, draft: &DraftInput) -> Result<DraftId, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into program_drafts (run_id, slug, catalog_year, source_url, source_sha256, body) \
             values ($1, $2, $3, $4, $5, $6) returning id",
        )
        .bind(draft.run_id)
        .bind(&draft.slug)
        .bind(year_to_db(draft.catalog_year)?)
        .bind(&draft.source_url)
        .bind(draft.source_sha256.to_vec())
        .bind(&draft.body)
        .fetch_one(&self.pool)
        .await?;
        Ok(DraftId(id))
    }

    /// Drafts in one state, newest first.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn drafts(&self, state: DraftState) -> Result<Vec<DraftRow>, StoreError> {
        let rows: Vec<DraftDbRow> = sqlx::query_as(&format!(
            "select {DRAFT_COLUMNS} from program_drafts where state = $1 order by created_at desc, id desc"
        ))
        .bind(state.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(DraftRow::try_from).collect()
    }

    /// One draft.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn draft(&self, id: DraftId) -> Result<Option<DraftRow>, StoreError> {
        let row: Option<DraftDbRow> = sqlx::query_as(&format!(
            "select {DRAFT_COLUMNS} from program_drafts where id = $1"
        ))
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(DraftRow::try_from).transpose()
    }

    /// Publish the program a reviewer built from a draft, and mark the draft
    /// approved. `None` when the draft id is unknown. The draft's own
    /// state is not checked: a reviewer may re-approve after a fix, and a
    /// rule whose fingerprint is unchanged keeps its id.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn approve_draft(
        &self,
        id: DraftId,
        reviewer: &str,
        program: &Program,
    ) -> Result<Option<ProgramId>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let found = sqlx::query(
            "update program_drafts set state = 'approved', reviewer = $2, reviewed_at = now() where id = $1",
        )
        .bind(id.0)
        .bind(reviewer)
        .execute(&mut *tx)
        .await?;
        if found.rows_affected() == 0 {
            return Ok(None);
        }
        let program_id = publish(&mut tx, program, reviewer, VersionSource::Ga, Some(id)).await?;
        tx.commit().await?;
        Ok(Some(program_id))
    }

    /// Publish a program that has no draft: the hand-encoded university
    /// requirements.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn publish_program(
        &self,
        program: &Program,
        reviewer: &str,
        source: VersionSource,
    ) -> Result<ProgramId, StoreError> {
        let mut tx = self.pool.begin().await?;
        let id = publish(&mut tx, program, reviewer, source, None).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Refuse a draft. `false` when the id is unknown.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn reject_draft(&self, id: DraftId, reviewer: &str) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update program_drafts set state = 'rejected', reviewer = $2, reviewed_at = now() where id = $1",
        )
        .bind(id.0)
        .bind(reviewer)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Every published program for a catalog year.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn programs(&self, year: CatalogYear) -> Result<Vec<ProgramSummaryRow>, StoreError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid,
            slug: String,
            kind: String,
            name: String,
            credential: String,
            catalog_year: i16,
            total_credits_cents: Option<i16>,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "select p.id, p.slug, p.kind, p.name, p.credential, v.catalog_year, v.total_credits_cents \
             from programs p join program_versions v on v.program_id = p.id \
             where v.catalog_year = $1 order by p.kind, p.name, p.slug",
        )
        .bind(year_to_db(year)?)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ProgramSummaryRow {
                    id: ProgramId(row.id),
                    slug: row.slug,
                    kind: kind_from_db(&row.kind)?,
                    name: row.name,
                    credential: row.credential,
                    catalog_year: year_from_db("program_versions", row.catalog_year)?,
                    total_credits: row
                        .total_credits_cents
                        .map(|c| cents_from_db("program_versions", "total_credits_cents", c))
                        .transpose()?,
                })
            })
            .collect()
    }

    /// Exactly the version for `year`, rebuilt from its rows, or `None`.
    /// Substitution between years is the engine's job.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json`.
    pub async fn program(
        &self,
        id: ProgramId,
        year: CatalogYear,
    ) -> Result<Option<Program>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        load_program(&mut conn, id, year).await
    }

    /// The catalog years a program is published for, ascending.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn program_versions(&self, id: ProgramId) -> Result<Vec<CatalogYear>, StoreError> {
        let years: Vec<i16> = sqlx::query_scalar(
            "select catalog_year from program_versions where program_id = $1 order by catalog_year",
        )
        .bind(id.0)
        .fetch_all(&self.pool)
        .await?;
        years
            .into_iter()
            .map(|y| year_from_db("program_versions", y))
            .collect()
    }

    /// One version per program id: the plan's year when held, else the
    /// earliest later, else the latest earlier. Ids with no version are
    /// absent, which the engine warns about.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json`.
    pub async fn programs_for_plan(
        &self,
        ids: &[ProgramId],
        year: CatalogYear,
    ) -> Result<Vec<Program>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let held = self.program_versions(*id).await?;
            if let Some(chosen) = pick_year(&held, year)
                && let Some(program) = load_program(&mut conn, *id, chosen).await?
            {
                out.push(program);
            }
        }
        Ok(out)
    }

    /// Requirement ids named in any plan document that no `requirements` row
    /// carries. Non-zero means re-extraction has a bug; `skyspace doctor`
    /// reports it.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn orphaned_requirement_choices(&self) -> Result<u64, StoreError> {
        let count: i64 = sqlx::query_scalar(
            "with named as (\
                select distinct (v #>> '{}')::uuid as id from plans p, lateral (\
                    select jsonb_path_query(p.body, '$.**.fills[*]') as v \
                    union all select jsonb_path_query(p.body, '$.**.requirement')) q) \
             select count(*) from named where not exists (select 1 from requirements r where r.id = named.id)",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

/// Where `publish` keeps the review timestamp: `Program::review` carries
/// it, so the caller supplies the clock.
#[must_use]
pub fn review_now(reviewed_by: &str, now: OffsetDateTime) -> Review {
    Review {
        reviewed_by: reviewed_by.to_owned(),
        published_at: Timestamp(now.unix_timestamp()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_case_and_spacing() {
        assert_eq!(
            fingerprint("0/1", "COMP 140\u{a0}Intro"),
            fingerprint("0/1", "comp  140 intro ")
        );
        assert_ne!(
            fingerprint("0/1", "COMP 140"),
            fingerprint("0/2", "COMP 140")
        );
    }

    #[test]
    fn pick_year_prefers_exact_then_later_then_earlier() {
        let held = [CatalogYear(2024), CatalogYear(2026), CatalogYear(2027)];
        assert_eq!(pick_year(&held, CatalogYear(2026)), Some(CatalogYear(2026)));
        assert_eq!(pick_year(&held, CatalogYear(2025)), Some(CatalogYear(2026)));
        assert_eq!(pick_year(&held, CatalogYear(2030)), Some(CatalogYear(2027)));
        assert_eq!(pick_year(&[], CatalogYear(2026)), None);
    }
}

#[cfg(test)]
mod sql_tests {
    use skyspace_core::program::{
        CatalogYear, CourseFilter, CourseSelector, CreditScope, NonCourseKind, RequirementBody,
        RequirementId,
    };
    use skyspace_core::term::Credits;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{DraftInput, DraftState, VersionSource};
    use crate::pool::Store;
    use crate::testing::{all, code, course_rule, plan, program, req, seed_account, seed_term};

    async fn draft(store: &Store, slug: &str, year: u16) -> super::DraftId {
        let run = store.start_run("requirements", None).await.unwrap();
        store
            .put_draft(&DraftInput {
                run_id: run,
                slug: slug.to_owned(),
                catalog_year: CatalogYear(year),
                source_url: "https://ga.rice.edu/x/".to_owned(),
                source_sha256: [1; 32],
                body: serde_json::json!({"title": slug}),
            })
            .await
            .unwrap()
    }

    fn full_program(year: u16) -> skyspace_core::program::Program {
        let mut select = req(
            "Select one",
            RequirementBody::Select {
                count: 1,
                of: vec![course_rule("MATH 101"), course_rule("MATH 105")],
            },
        );
        select.source.anchor = Some("core".to_owned());
        let credits = req(
            "Electives",
            RequirementBody::Credits {
                minimum: Credits::from_cents(900),
                scope: CreditScope::Additional,
                from: CourseFilter {
                    include: vec![CourseSelector::NumberRange {
                        subject: Some(code("COMP 300").subject),
                        low: 300,
                        high: 699,
                    }],
                    exclude: vec![CourseSelector::Code {
                        code: code("COMP 390"),
                    }],
                },
            },
        );
        let non_course = req(
            "Piano proficiency",
            RequirementBody::NonCourse {
                non_course_kind: NonCourseKind::ProficiencyExam,
                description: "Pass the piano proficiency exam.".to_owned(),
            },
        );
        let unverifiable = req(
            "Footnote",
            RequirementBody::Unverifiable {
                text: "Consult your advisor.".to_owned(),
            },
        );
        let departments = req(
            "Two departments",
            RequirementBody::DistinctDepartments {
                minimum: 2,
                text: "from at least two departments".to_owned(),
            },
        );
        let mut lesson = course_rule("MUSI 251");
        lesson.body = RequirementBody::Course {
            filter: CourseFilter {
                include: vec![CourseSelector::Code {
                    code: code("MUSI 251"),
                }],
                exclude: Vec::new(),
            },
            semesters: 8,
        };
        program(
            "example-bs",
            year,
            all(
                "Example",
                vec![
                    all("Core", vec![course_rule("COMP 140"), select]),
                    credits,
                    non_course,
                    unverifiable,
                    departments,
                    lesson,
                ],
            ),
        )
    }

    /// Strip ids so two trees compare on content.
    fn without_ids(mut p: skyspace_core::program::Program) -> skyspace_core::program::Program {
        fn clear(r: &mut skyspace_core::program::Requirement) {
            r.id = RequirementId(Uuid::nil());
            if let RequirementBody::All { of } | RequirementBody::Select { of, .. } = &mut r.body {
                of.iter_mut().for_each(clear);
            }
        }
        p.id = skyspace_core::program::ProgramId(Uuid::nil());
        clear(&mut p.root);
        p
    }

    #[sqlx::test]
    async fn drafts_queue(pool: PgPool) {
        let store = Store::from_pool(pool);
        let id = draft(&store, "computer-science-bscs", 2026).await;
        let pending = store.drafts(DraftState::Pending).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
        assert_eq!(pending[0].slug, "computer-science-bscs");
        assert!(store.reject_draft(id, "alice").await.unwrap());
        assert!(
            !store
                .reject_draft(super::DraftId(id.0 + 100), "alice")
                .await
                .unwrap()
        );
        assert!(store.drafts(DraftState::Pending).await.unwrap().is_empty());
        let row = store.draft(id).await.unwrap().unwrap();
        assert_eq!(row.state, DraftState::Rejected);
        assert_eq!(row.reviewer.as_deref(), Some("alice"));
        assert!(
            store
                .draft(super::DraftId(id.0 + 100))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn approve_round_trips_and_keeps_ids_by_fingerprint(pool: PgPool) {
        let store = Store::from_pool(pool);
        let id = draft(&store, "example-bs", 2026).await;
        let mut built = full_program(2026);
        // The reviewer argument is what gets stored, not the fixture's name.
        built.review.reviewed_by = "alice".to_owned();
        let program_id = store
            .approve_draft(id, "alice", &built)
            .await
            .unwrap()
            .unwrap();
        assert!(
            store
                .approve_draft(super::DraftId(id.0 + 100), "alice", &built)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.draft(id).await.unwrap().unwrap().state,
            DraftState::Approved
        );

        let stored = store
            .program(program_id, CatalogYear(2026))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.id, program_id);
        assert_eq!(without_ids(stored.clone()), without_ids(built.clone()));
        assert!(
            store
                .program(program_id, CatalogYear(2025))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(stored.requirement_count(), 11);
        let ids: Vec<RequirementId> = stored.root.flatten().iter().map(|r| r.id).collect();
        assert!(ids.iter().all(|i| *i != RequirementId(Uuid::nil())));
        let comp140 = stored
            .root
            .flatten()
            .into_iter()
            .find(|r| r.label == "COMP 140")
            .unwrap()
            .id;
        let indexed: i64 = sqlx::query_scalar(
            "select count(*) from requirement_courses where subject = 'COMP' and number = '140'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(indexed, 1);

        // Re-approve with one label changed: that rule gets a new id and the
        // old one is retired; every other id survives.
        let mut changed = built.clone();
        if let RequirementBody::All { of } = &mut changed.root.body {
            of[3].label = "Footnote (renamed)".to_owned();
            of[3].body = RequirementBody::Unverifiable {
                text: "Consult your advisor, please.".to_owned(),
            };
        }
        let same_id = store
            .approve_draft(id, "bob", &changed)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(same_id, program_id);
        let again = store
            .program(program_id, CatalogYear(2026))
            .await
            .unwrap()
            .unwrap();
        let comp140_again = again
            .root
            .flatten()
            .into_iter()
            .find(|r| r.label == "COMP 140")
            .unwrap()
            .id;
        assert_eq!(comp140_again, comp140);
        let old_footnote = stored
            .root
            .flatten()
            .into_iter()
            .find(|r| r.label == "Footnote")
            .unwrap()
            .id;
        let new_footnote = again
            .root
            .flatten()
            .into_iter()
            .find(|r| r.label == "Footnote (renamed)")
            .unwrap()
            .id;
        assert_ne!(old_footnote, new_footnote);
        assert_eq!(again.retired_requirements, vec![old_footnote]);
        assert_eq!(again.review.reviewed_by, "bob");

        let summaries = store.programs(CatalogYear(2026)).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].slug, "example-bs");
        assert_eq!(summaries[0].total_credits, Some(Credits::from_cents(12000)));
        assert!(store.programs(CatalogYear(2027)).await.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn versions_and_the_plan_pick(pool: PgPool) {
        let store = Store::from_pool(pool);
        let id = store
            .publish_program(&full_program(2024), "alice", VersionSource::Manual)
            .await
            .unwrap();
        let same = store
            .publish_program(&full_program(2027), "alice", VersionSource::Ga)
            .await
            .unwrap();
        assert_eq!(id, same);
        assert_eq!(
            store.program_versions(id).await.unwrap(),
            vec![CatalogYear(2024), CatalogYear(2027)]
        );
        let picked = store
            .programs_for_plan(
                &[id, skyspace_core::program::ProgramId(Uuid::new_v4())],
                CatalogYear(2026),
            )
            .await
            .unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].catalog_year, CatalogYear(2027));
        let picked = store
            .programs_for_plan(&[id], CatalogYear(2030))
            .await
            .unwrap();
        assert_eq!(picked[0].catalog_year, CatalogYear(2027));
        let picked = store
            .programs_for_plan(&[id], CatalogYear(2023))
            .await
            .unwrap();
        assert_eq!(picked[0].catalog_year, CatalogYear(2024));
        let source: String =
            sqlx::query_scalar("select source from program_versions where catalog_year = 2024")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!(source, "manual");
    }

    #[sqlx::test]
    async fn orphaned_choices_are_counted(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let program_id = store
            .publish_program(&full_program(2026), "alice", VersionSource::Ga)
            .await
            .unwrap();
        let stored = store
            .program(program_id, CatalogYear(2026))
            .await
            .unwrap()
            .unwrap();
        let real = stored.root.flatten()[1].id;
        let account = seed_account(&store, "a@rice.edu").await;
        let mut p = plan("Plan", 2026, vec![program_id], &["COMP 140"]);
        if let skyspace_core::plan::TermKind::Rice { courses, .. } = &mut p.terms[0].kind {
            courses[0].fills = vec![real];
        }
        p.self_checks.push(skyspace_core::plan::SelfCheck {
            requirement: real,
            reason: skyspace_core::plan::SelfCheckReason::Other,
            note: None,
        });
        store.create_plan(account, &p).await.unwrap();
        assert_eq!(store.orphaned_requirement_choices().await.unwrap(), 0);
        if let skyspace_core::plan::TermKind::Rice { courses, .. } = &mut p.terms[0].kind {
            courses[0].fills.push(RequirementId(Uuid::new_v4()));
        }
        store.create_plan(account, &p).await.unwrap();
        assert_eq!(store.orphaned_requirement_choices().await.unwrap(), 1);
    }
}
