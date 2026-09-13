//! Ingest writes: runs, archived responses, parse issues, terms, courses,
//! sections and their weekly enrichments, and the advisory job lock.

use std::collections::BTreeSet;

use skyspace_core::catalog::{
    Attribute, Course, FinalExam, Instructor, Meeting, SectionDetail, SectionListing,
};
use skyspace_core::code::{CourseCode, Crn};
use skyspace_core::program::CatalogYear;
use skyspace_core::term::{CreditRange, PartOfTermCode, TermCode};
use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, Postgres};
use time::OffsetDateTime;

use crate::convert::{
    attributes_to_db, credits_to_db, crn_to_db, final_exam_to_db, meeting_to_db, season_to_db,
    timestamp_to_db,
};
use crate::courses::year_to_db;
use crate::error::StoreError;
use crate::pool::Store;

/// A term as the `TERMS` reference list publishes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermInput {
    /// Rice's code; season and academic year derive from it.
    pub code: TermCode,
    /// Rice's own label. Never synthesised.
    pub label: String,
}

/// How a run ended. Matches the `ingest_runs.outcome` check constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// Guards passed, rows written.
    Ok,
    /// Stopped early; exit code 1.
    Failed,
    /// Bytes archived, catalog untouched, alert sent; exit code 2.
    Quarantined,
}

impl RunOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Quarantined => "quarantined",
        }
    }

    /// Read the stored text back.
    ///
    /// # Errors
    /// `StoreError::Corrupt` for a value outside the check constraint.
    pub fn parse(text: &str) -> Result<Self, StoreError> {
        match text {
            "ok" => Ok(Self::Ok),
            "failed" => Ok(Self::Failed),
            "quarantined" => Ok(Self::Quarantined),
            other => Err(crate::error::corrupt(
                "ingest_runs",
                "outcome",
                format!("{other:?}"),
            )),
        }
    }
}

/// The numbers a finished run records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummaryRow {
    /// How it ended.
    pub outcome: RunOutcome,
    /// Requests made.
    pub requests: u32,
    /// Requests planned; fewer made means it stopped early.
    pub targets: u32,
    /// Requests that failed after retries.
    pub failures: u32,
    /// Bytes fetched.
    pub bytes: u64,
    /// Rows written to layer-2 tables.
    pub rows_written: u64,
    /// The failure, for the run list.
    pub error: Option<String>,
}

/// One `ingest_runs` row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunRow {
    /// Run id.
    pub id: i64,
    /// Job name.
    pub job: String,
    /// The term, for per-term jobs.
    pub term_code: Option<String>,
    /// When it started.
    pub started_at: OffsetDateTime,
    /// When it finished; null while running.
    pub finished_at: Option<OffsetDateTime>,
    /// `ok`, `failed`, `quarantined`; null while running.
    pub outcome: Option<String>,
    /// Requests made.
    pub requests: i32,
    /// Requests planned.
    pub targets: i32,
    /// Failed requests.
    pub failures: i32,
    /// Bytes fetched.
    pub bytes: i64,
    /// Rows written.
    pub rows_written: i64,
    /// The failure text.
    pub error: Option<String>,
}

/// What kind of document a `raw_responses` row indexes. Matches the check
/// constraint; seats are absent because they are never archived. Ingest
/// re-exports this as its `Source`, so one enum serves the constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RawSource {
    /// A subject listing page.
    Listing,
    /// A `CATALIST` page.
    Catalog,
    /// `ASSOCIATED-SECTIONS` XML.
    SectionXml,
    /// A section detail page.
    Detail,
    /// A reference list.
    Reference,
    /// The GA program index.
    ProgramIndex,
    /// A GA program page.
    Program,
}

impl RawSource {
    /// Every source, in the order of the check constraint.
    pub const ALL: [Self; 7] = [
        Self::Listing,
        Self::Catalog,
        Self::SectionXml,
        Self::Detail,
        Self::Reference,
        Self::ProgramIndex,
        Self::Program,
    ];

    /// The stored text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Listing => "listing",
            Self::Catalog => "catalog",
            Self::SectionXml => "section_xml",
            Self::Detail => "detail",
            Self::Reference => "reference",
            Self::ProgramIndex => "program_index",
            Self::Program => "program",
        }
    }
}

impl core::str::FromStr for RawSource {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| {
                format!(
                    "unknown source {text:?}; one of {}",
                    Self::ALL.map(Self::as_str).join(", ")
                )
            })
    }
}

impl core::fmt::Display for RawSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a run is keyed by in `ingest_runs.term_code`: a term for the
/// per-term jobs, an academic year for the catalog job (so its guards
/// compare against the last good run of the same year), nothing for the
/// rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKey {
    /// A Rice term code.
    Term(TermCode),
    /// A catalog year, stored as its four digits.
    Year(CatalogYear),
}

impl RunKey {
    /// The `ingest_runs.term_code` text.
    #[must_use]
    pub fn as_db(self) -> String {
        match self {
            Self::Term(term) => term.to_string(),
            Self::Year(year) => year.0.to_string(),
        }
    }
}

/// One `raw_responses` row joined to its run's outcome, as the replay and
/// diff commands read it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RawResponseRow {
    /// Row id.
    pub id: i64,
    /// The run that fetched it.
    pub run_id: i64,
    /// The URL fetched.
    pub url: String,
    /// The term, for per-term documents.
    pub term_code: Option<String>,
    /// When.
    pub fetched_at: OffsetDateTime,
    /// The `Content-Type` header, for decoding.
    pub content_type: Option<String>,
    /// The archive key.
    pub sha256: Vec<u8>,
    /// The fetching run's outcome; null while it runs.
    pub run_outcome: Option<String>,
}

impl RawResponseRow {
    /// The archive key as an array.
    ///
    /// # Errors
    /// `StoreError::Corrupt` when the column is not 32 bytes.
    pub fn key(&self) -> Result<[u8; 32], StoreError> {
        <[u8; 32]>::try_from(self.sha256.as_slice())
            .map_err(|_| crate::error::corrupt("raw_responses", "sha256", "not 32 bytes"))
    }
}

const RUN_COLUMNS: &str = "id, job, term_code, started_at, finished_at, outcome, requests, targets, failures, \
                           bytes, rows_written, error";

const RAW_COLUMNS: &str = "rr.id, rr.run_id, rr.url, rr.term_code, rr.fetched_at, rr.content_type, \
                           rr.sha256, r.outcome as run_outcome";

/// Which weekly work queue a CRN is due on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueColumn {
    /// `sections.xml_fetched_at`.
    Xml,
    /// `sections.detail_fetched_at`.
    Detail,
}

/// One fetched document, after the bytes are on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponseInput {
    /// The run that fetched it.
    pub run_id: i64,
    /// What it is.
    pub source: RawSource,
    /// The URL fetched.
    pub url: String,
    /// The term, for per-term documents.
    pub term: Option<TermCode>,
    /// When.
    pub fetched_at: OffsetDateTime,
    /// HTTP status.
    pub status: u16,
    /// The `Content-Type` header.
    pub content_type: Option<String>,
    /// Bytes before decoding.
    pub byte_len: u32,
    /// The archive key.
    pub sha256: [u8; 32],
    /// Response headers worth keeping.
    pub headers: serde_json::Value,
}

/// How bad a parse issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// The row was kept.
    Warn,
    /// The row was lost.
    Error,
}

/// What the weekly `ASSOCIATED-SECTIONS` XML adds to a listed section. The
/// parse crate's `SectionXml` is converted into this by ingest; the store
/// does not depend on the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionXmlRow {
    /// Which section.
    pub crn: Crn,
    /// Part-of-term code.
    pub part_of_term: Option<PartOfTermCode>,
    /// Part-of-term label, for the reference read.
    pub part_of_term_label: Option<String>,
    /// The exam code.
    pub final_exam: FinalExam,
    /// Credits with `low`/`high`; `None` leaves the listing's value.
    pub credits: Option<CreditRange>,
    /// This term's designation.
    pub attributes: BTreeSet<Attribute>,
    /// The school, for the search filter.
    pub school: Option<String>,
    /// Class meetings, with dates.
    pub meetings: Vec<Meeting>,
    /// The final exam slot, when scheduled.
    pub final_exam_meeting: Option<Meeting>,
    /// Instructors in Rice's order.
    pub instructors: Vec<Instructor>,
}

/// One `canaries` row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CanaryRow {
    /// Row id.
    pub id: i64,
    /// Which parser.
    pub source: String,
    /// The term, when the document is per term.
    pub term_code: Option<String>,
    /// The document.
    pub url: String,
    /// Expected row count.
    pub expect_rows: Option<i32>,
    /// A field expected to be filled.
    pub expect_field: Option<String>,
    /// Why this document.
    pub note: String,
}

/// A session-scoped advisory lock. Holds its connection: the lock lives as
/// long as the session. `release` unlocks and returns the connection to the
/// pool; dropping without `release` detaches the connection so the session
/// closes and Postgres releases the lock. Best effort: a process killed
/// mid-run releases it when its socket closes.
pub struct JobLock {
    conn: Option<PoolConnection<Postgres>>,
    key: i64,
}

impl JobLock {
    /// Unlock and hand the connection back to the pool.
    ///
    /// # Errors
    /// `StoreError::Database` when the unlock statement fails; the connection
    /// is then detached and closed, which still releases the lock.
    pub async fn release(mut self) -> Result<(), StoreError> {
        if let Some(mut conn) = self.conn.take() {
            let result = sqlx::query("select pg_advisory_unlock($1)")
                .bind(self.key)
                .execute(&mut *conn)
                .await;
            if let Err(e) = result {
                drop(conn.detach());
                return Err(e.into());
            }
        }
        Ok(())
    }
}

impl Drop for JobLock {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn.detach());
        }
    }
}

fn db_u32(value: u32) -> Result<i32, StoreError> {
    i32::try_from(value).map_err(|_| StoreError::Input(format!("{value} does not fit integer")))
}

fn db_u64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Input(format!("{value} does not fit bigint")))
}

/// Ensure a `courses` row exists and return its id. `title` replaces the
/// stored one only when `replace_title` is set: the listing's short title
/// must not overwrite the catalog's long title.
async fn ensure_course(
    conn: &mut PgConnection,
    code: &CourseCode,
    title: &str,
    replace_title: bool,
) -> Result<i64, StoreError> {
    let id: i64 = sqlx::query_scalar(
        "insert into courses (subject, number, title) values ($1, $2, $3) \
         on conflict (subject, number) do update set \
            title = case when $4 then excluded.title else courses.title end, \
            last_seen = now() \
         returning id",
    )
    .bind(code.subject.as_str())
    .bind(code.number.as_str())
    .bind(title)
    .bind(replace_title)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

async fn write_catalog_record(
    conn: &mut PgConnection,
    course_id: i64,
    course: &Course,
) -> Result<(), StoreError> {
    let year = year_to_db(course.catalog_year)?;
    let credits = credits_to_db(course.credits)?;
    let (prerequisite_text, prerequisite_expr, coreq) = match &course.prerequisites {
        Some(p) => (
            Some(p.raw.clone()),
            Some(serde_json::to_value(&p.parsed)?),
            p.corequisite.clone(),
        ),
        None => (None, None, None),
    };
    let restrictions = course
        .restrictions
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;
    let equivalents: Vec<String> = course.equivalents.iter().map(ToString::to_string).collect();
    sqlx::query(
        "insert into course_catalog (course_id, catalog_year, department, credits_kind, credits_min_cents, \
            credits_max_cents, attributes, grade_mode, course_type, restriction_text, restrictions, \
            prerequisite_text, prerequisite_expr, corequisite_subject, corequisite_number, description, \
            repeatable, instructor_permission, second_half, equivalents, fetched_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, now()) \
         on conflict (course_id, catalog_year) do update set \
            department = excluded.department, credits_kind = excluded.credits_kind, \
            credits_min_cents = excluded.credits_min_cents, credits_max_cents = excluded.credits_max_cents, \
            attributes = excluded.attributes, grade_mode = excluded.grade_mode, course_type = excluded.course_type, \
            restriction_text = excluded.restriction_text, restrictions = excluded.restrictions, \
            prerequisite_text = excluded.prerequisite_text, prerequisite_expr = excluded.prerequisite_expr, \
            corequisite_subject = excluded.corequisite_subject, corequisite_number = excluded.corequisite_number, \
            description = excluded.description, repeatable = excluded.repeatable, \
            instructor_permission = excluded.instructor_permission, second_half = excluded.second_half, \
            equivalents = excluded.equivalents, fetched_at = now()",
    )
    .bind(course_id)
    .bind(year)
    .bind(&course.department)
    .bind(credits.kind)
    .bind(credits.min)
    .bind(credits.max)
    .bind(attributes_to_db(&course.attributes))
    .bind(course.grade_mode.as_ref().map(|g| g.0.clone()))
    .bind(course.course_type.as_ref().map(|t| t.0.clone()))
    .bind(course.restrictions.as_ref().map(|r| r.raw.clone()))
    .bind(restrictions)
    .bind(prerequisite_text)
    .bind(prerequisite_expr)
    .bind(coreq.as_ref().map(|c| c.subject.as_str().to_owned()))
    .bind(coreq.as_ref().map(|c| c.number.as_str().to_owned()))
    .bind(&course.description)
    .bind(course.flags.repeatable)
    .bind(course.flags.instructor_permission)
    .bind(course.flags.second_half)
    .bind(equivalents)
    .execute(conn)
    .await?;
    Ok(())
}

async fn write_prerequisite_index(
    conn: &mut PgConnection,
    course_id: i64,
    course: &Course,
) -> Result<(), StoreError> {
    let year = year_to_db(course.catalog_year)?;
    sqlx::query("delete from course_prerequisites where course_id = $1 and catalog_year = $2")
        .bind(course_id)
        .bind(year)
        .execute(&mut *conn)
        .await?;
    let codes = course
        .prerequisites
        .as_ref()
        .map(|p| p.parsed.codes())
        .unwrap_or_default();
    for (ordinal, code) in codes.iter().enumerate() {
        sqlx::query(
            "insert into course_prerequisites (course_id, catalog_year, ordinal, subject, number) \
             values ($1, $2, $3, $4, $5)",
        )
        .bind(course_id)
        .bind(year)
        .bind(
            i16::try_from(ordinal)
                .map_err(|_| StoreError::Input("too many prerequisites".to_owned()))?,
        )
        .bind(code.subject.as_str())
        .bind(code.number.as_str())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn write_exclusions(
    conn: &mut PgConnection,
    course_id: i64,
    course: &Course,
) -> Result<(), StoreError> {
    let year = year_to_db(course.catalog_year)?;
    sqlx::query("delete from course_exclusions where course_id = $1 and catalog_year = $2")
        .bind(course_id)
        .bind(year)
        .execute(&mut *conn)
        .await?;
    let mut ordinal: i16 = 0;
    for sentence in &course.mutual_exclusions {
        for code in &sentence.with {
            sqlx::query(
                "insert into course_exclusions (course_id, catalog_year, ordinal, subject, number, published) \
                 values ($1, $2, $3, $4, $5, $6)",
            )
            .bind(course_id)
            .bind(year)
            .bind(ordinal)
            .bind(code.subject.as_str())
            .bind(code.number.as_str())
            .bind(&sentence.raw)
            .execute(&mut *conn)
            .await?;
            ordinal = ordinal.saturating_add(1);
        }
    }
    Ok(())
}

/// Write one alias group: its smallest member is canonical and every other
/// member points at it. The rule is the same whether the group comes from
/// a catalog `Cross-list:` sentence or a GA `STAT 310 / ECON 307` row, so
/// the two sources never disagree on direction, and because canonical is
/// always the smaller code the map has no cycles.
async fn write_alias_group(
    conn: &mut PgConnection,
    members: impl IntoIterator<Item = &CourseCode>,
) -> Result<(), StoreError> {
    let mut group: Vec<&CourseCode> = members.into_iter().collect();
    group.sort();
    group.dedup();
    let Some(canonical) = group.first().copied() else {
        return Ok(());
    };
    for member in group.iter().skip(1) {
        sqlx::query(
            "insert into course_aliases (alias_subject, alias_number, canonical_subject, canonical_number) \
             values ($1, $2, $3, $4) on conflict (alias_subject, alias_number) do update set \
             canonical_subject = excluded.canonical_subject, canonical_number = excluded.canonical_number",
        )
        .bind(member.subject.as_str())
        .bind(member.number.as_str())
        .bind(canonical.subject.as_str())
        .bind(canonical.number.as_str())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Collapse chains after alias writes: an alias whose canonical is itself
/// an alias is repointed at that alias's canonical, until no row changes.
/// Canonical is always the smaller code, so every chain descends and the
/// loop ends; afterwards no canonical is an alias.
async fn flatten_aliases(conn: &mut PgConnection) -> Result<(), StoreError> {
    loop {
        let done = sqlx::query(
            "update course_aliases a set canonical_subject = b.canonical_subject, canonical_number = b.canonical_number \
             from course_aliases b \
             where a.canonical_subject = b.alias_subject and a.canonical_number = b.alias_number \
               and (a.alias_subject, a.alias_number) <> (b.canonical_subject, b.canonical_number)",
        )
        .execute(&mut *conn)
        .await?;
        if done.rows_affected() == 0 {
            return Ok(());
        }
    }
}

/// The cross-list group's canonical code is its smallest member, so every
/// member's record derives the same alias rows.
async fn write_aliases(conn: &mut PgConnection, course: &Course) -> Result<(), StoreError> {
    if course.cross_list.is_empty() {
        return Ok(());
    }
    write_alias_group(
        conn,
        course
            .cross_list
            .iter()
            .chain(core::iter::once(&course.code)),
    )
    .await
}

#[derive(sqlx::FromRow)]
struct DatedMeetingRow {
    day_mask: i16,
    start_time: Option<time::Time>,
    end_time: Option<time::Time>,
    start_date: Option<time::Date>,
    end_date: Option<time::Date>,
}

/// Rewrite a section's class meetings. `keep_dates` carries the dates of an
/// existing row with the same days and times over to the new row, so a
/// nightly listing does not erase what the weekly XML filled in.
async fn write_class_meetings(
    conn: &mut PgConnection,
    section_id: i64,
    meetings: &[Meeting],
    keep_dates: bool,
) -> Result<(), StoreError> {
    let existing: Vec<DatedMeetingRow> = if keep_dates {
        sqlx::query_as(
            "select day_mask, start_time, end_time, start_date, end_date from meetings \
             where section_id = $1 and kind = 'class'",
        )
        .bind(section_id)
        .fetch_all(&mut *conn)
        .await?
    } else {
        Vec::new()
    };
    sqlx::query("delete from meetings where section_id = $1 and kind = 'class'")
        .bind(section_id)
        .execute(&mut *conn)
        .await?;
    for meeting in meetings {
        let mut cols = meeting_to_db(meeting)?;
        if cols.start_date.is_none() {
            let same = existing.iter().find(|e| {
                e.day_mask == cols.day_mask
                    && e.start_time == cols.start_time
                    && e.end_time == cols.end_time
            });
            if let Some(row) = same {
                cols.start_date = row.start_date;
                cols.end_date = row.end_date;
            }
        }
        insert_meeting(&mut *conn, section_id, "class", &cols).await?;
    }
    Ok(())
}

async fn insert_meeting(
    conn: &mut PgConnection,
    section_id: i64,
    kind: &str,
    cols: &crate::convert::MeetingColumns,
) -> Result<(), StoreError> {
    sqlx::query(
        "insert into meetings (section_id, kind, day_mask, start_time, end_time, start_date, end_date, raw) \
         values ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(section_id)
    .bind(kind)
    .bind(cols.day_mask)
    .bind(cols.start_time)
    .bind(cols.end_time)
    .bind(cols.start_date)
    .bind(cols.end_date)
    .bind(&cols.raw)
    .execute(conn)
    .await?;
    Ok(())
}

async fn write_instructors(
    conn: &mut PgConnection,
    section_id: i64,
    instructors: &[Instructor],
) -> Result<(), StoreError> {
    sqlx::query("delete from section_instructors where section_id = $1")
        .bind(section_id)
        .execute(&mut *conn)
        .await?;
    for (ordinal, instructor) in instructors.iter().enumerate() {
        let instructor_id: i64 = sqlx::query_scalar(
            "insert into instructors (name, netid) values ($1, $2) \
             on conflict (name) do update set netid = coalesce(excluded.netid, instructors.netid) returning id",
        )
        .bind(&instructor.name)
        .bind(instructor.net_id.as_ref().map(|n| n.0.clone()))
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query(
            "insert into section_instructors (section_id, instructor_id, ordinal) values ($1, $2, $3) \
             on conflict (section_id, instructor_id) do nothing",
        )
        .bind(section_id)
        .bind(instructor_id)
        .bind(i16::try_from(ordinal).map_err(|_| StoreError::Input("too many instructors".to_owned()))?)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// `fetched_at` is when Rice published the listing. A withdrawal is only
/// cleared by a body newer than it, so replaying an older archived page
/// cannot resurrect a section a later good run withdrew.
async fn upsert_listing(
    conn: &mut PgConnection,
    term: TermCode,
    listing: &SectionListing,
    fetched_at: OffsetDateTime,
) -> Result<(), StoreError> {
    let course_id = ensure_course(&mut *conn, &listing.code, &listing.title, false).await?;
    let credits = credits_to_db(listing.credits)?;
    let section_id: i64 = sqlx::query_scalar(
        "insert into sections (term_code, crn, course_id, section_code, title, part_of_term, credits_kind, \
            credits_min_cents, credits_max_cents, final_exam, first_seen_at, last_seen_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11) \
         on conflict (term_code, crn) do update set \
            course_id = excluded.course_id, section_code = excluded.section_code, title = excluded.title, \
            part_of_term = coalesce(excluded.part_of_term, sections.part_of_term), \
            credits_kind = excluded.credits_kind, credits_min_cents = excluded.credits_min_cents, \
            credits_max_cents = excluded.credits_max_cents, \
            final_exam = coalesce(excluded.final_exam, sections.final_exam), \
            last_seen_at = greatest(sections.last_seen_at, $11), \
            withdrawn_at = case when sections.withdrawn_at < $11 then null else sections.withdrawn_at end \
         returning id",
    )
    .bind(term.to_string())
    .bind(crn_to_db(listing.crn)?)
    .bind(course_id)
    .bind(&listing.section.0)
    .bind(&listing.title)
    .bind(listing.part_of_term.as_ref().map(|p| p.0.clone()))
    .bind(credits.kind)
    .bind(credits.min)
    .bind(credits.max)
    .bind(final_exam_to_db(listing.final_exam))
    .bind(fetched_at)
    .fetch_one(&mut *conn)
    .await?;
    write_class_meetings(&mut *conn, section_id, &listing.meetings, true).await?;
    write_instructors(&mut *conn, section_id, &listing.instructors).await?;
    Ok(())
}

impl Store {
    /// Insert or relabel terms from the `TERMS` list. `is_current` is never
    /// touched here; `set_current_term` is the operator's call.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn upsert_terms(&self, terms: &[TermInput]) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        for term in terms {
            sqlx::query(
                "insert into terms (code, academic_year, season, label) values ($1, $2, $3, $4) \
                 on conflict (code) do update set label = excluded.label",
            )
            .bind(term.code.to_string())
            .bind(
                i16::try_from(term.code.academic_year())
                    .map_err(|_| StoreError::Input("year".to_owned()))?,
            )
            .bind(term.code.season().map(season_to_db))
            .bind(&term.label)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(terms.len() as u64)
    }

    /// Write one academic year's catalog records: `courses`,
    /// `course_catalog`, the flat prerequisite index, exclusions and the
    /// aliases a `Cross-list:` sentence implies. Returns the number of
    /// courses written.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` for a value that does not fit its
    /// column, or `Json` when an expression cannot be serialised.
    pub async fn upsert_courses(
        &self,
        year: CatalogYear,
        courses: &[Course],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut written = 0u64;
        for course in courses {
            let record = Course {
                catalog_year: year,
                ..course.clone()
            };
            let course_id = ensure_course(&mut tx, &record.code, &record.title, true).await?;
            write_catalog_record(&mut tx, course_id, &record).await?;
            write_prerequisite_index(&mut tx, course_id, &record).await?;
            write_exclusions(&mut tx, course_id, &record).await?;
            write_aliases(&mut tx, &record).await?;
            written += 1;
        }
        flatten_aliases(&mut tx).await?;
        tx.commit().await?;
        Ok(written)
    }

    /// Write alias pairs a GA page printed as `STAT 310 / ECON 307`. Each
    /// pair is one group under the same rule the catalog uses: the
    /// smaller code is canonical. Returns the number of pairs written.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn upsert_aliases(
        &self,
        pairs: &[(CourseCode, CourseCode)],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        for (a, b) in pairs {
            write_alias_group(&mut tx, [a, b]).await?;
        }
        flatten_aliases(&mut tx).await?;
        tx.commit().await?;
        Ok(pairs.len() as u64)
    }

    /// Write a subject listing's sections fetched just now. A course row
    /// is created when missing, with the listing's title; a seen section
    /// loses its `withdrawn_at`. Returns the number of sections written.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn upsert_sections(
        &self,
        term: TermCode,
        rows: &[SectionListing],
    ) -> Result<u64, StoreError> {
        self.upsert_sections_fetched_at(term, rows, OffsetDateTime::now_utc())
            .await
    }

    /// `upsert_sections` for a body fetched at `fetched_at`: what replay
    /// passes. A withdrawal later than `fetched_at` is kept, so an older
    /// archived page cannot bring back a section a later good run
    /// withdrew.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn upsert_sections_fetched_at(
        &self,
        term: TermCode,
        rows: &[SectionListing],
        fetched_at: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        for listing in rows {
            upsert_listing(&mut tx, term, listing, fetched_at).await?;
        }
        tx.commit().await?;
        Ok(rows.len() as u64)
    }

    /// The one positive withdrawal rule: after a listing run for `subject`
    /// succeeded, every live section of that subject whose CRN is not in
    /// `seen` is withdrawn. Returns the number withdrawn.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn withdraw_missing(
        &self,
        term: TermCode,
        subject: &skyspace_core::code::Subject,
        seen: &[Crn],
    ) -> Result<u64, StoreError> {
        let seen = seen
            .iter()
            .map(|c| crn_to_db(*c))
            .collect::<Result<Vec<_>, _>>()?;
        let done = sqlx::query(
            "update sections s set withdrawn_at = now() from courses c \
             where c.id = s.course_id and s.term_code = $1 and c.subject = $2 \
               and s.withdrawn_at is null and not (s.crn = any($3))",
        )
        .bind(term.to_string())
        .bind(subject.as_str())
        .bind(seen)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected())
    }

    /// Merge the weekly XML into a listed section: codes, attributes,
    /// school, dated meetings, instructors. `false` when the CRN is not held.
    ///
    /// Never blank: a feed that carries no class meeting is treated as
    /// partial, so the listing's meetings and attributes are kept; the
    /// exam slot, instructors and the scalar columns are replaced only
    /// when the feed supplies them.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn upsert_section_xml(
        &self,
        term: TermCode,
        row: &SectionXmlRow,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let credits = row.credits.map(credits_to_db).transpose()?;
        // A feed with at least one class meeting is a whole record; one
        // without is partial and must not clear what the listing filled.
        let authoritative = !row.meetings.is_empty();
        let section_id: Option<i64> = sqlx::query_scalar(
            "update sections set \
                part_of_term = coalesce($3, part_of_term), part_of_term_label = coalesce($4, part_of_term_label), \
                final_exam = coalesce($5, final_exam), \
                attributes = case when $11 then $6 else attributes end, school = coalesce($7, school), \
                credits_kind = coalesce($8, credits_kind), credits_min_cents = coalesce($9, credits_min_cents), \
                credits_max_cents = coalesce($10, credits_max_cents), xml_fetched_at = now() \
             where term_code = $1 and crn = $2 returning id",
        )
        .bind(term.to_string())
        .bind(crn_to_db(row.crn)?)
        .bind(row.part_of_term.as_ref().map(|p| p.0.clone()))
        .bind(&row.part_of_term_label)
        .bind(final_exam_to_db(row.final_exam))
        .bind(attributes_to_db(&row.attributes))
        .bind(&row.school)
        .bind(credits.map(|c| c.kind))
        .bind(credits.map(|c| c.min))
        .bind(credits.map(|c| c.max))
        .bind(authoritative)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(section_id) = section_id else {
            return Ok(false);
        };
        if authoritative {
            write_class_meetings(&mut tx, section_id, &row.meetings, false).await?;
        }
        if let Some(exam) = &row.final_exam_meeting {
            sqlx::query("delete from meetings where section_id = $1 and kind = 'final'")
                .bind(section_id)
                .execute(&mut *tx)
                .await?;
            insert_meeting(&mut tx, section_id, "final", &meeting_to_db(exam)?).await?;
        }
        if !row.instructors.is_empty() {
            write_instructors(&mut tx, section_id, &row.instructors).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Store a section's detail page: the whole document plus the columns
    /// the search and the poll read, and the reserved-seat rows. `false`
    /// when the CRN is not held.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn upsert_section_detail(
        &self,
        term: TermCode,
        crn: Crn,
        detail: &SectionDetail,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let fees_text = (!detail.fees.is_empty()).then(|| {
            detail
                .fees
                .iter()
                .map(|f| f.raw.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        });
        let section_id: Option<i64> = sqlx::query_scalar(
            "update sections set detail = $3, detail_fetched_at = $4, fees_text = $5, has_syllabus = $6 \
             where term_code = $1 and crn = $2 returning id",
        )
        .bind(term.to_string())
        .bind(crn_to_db(crn)?)
        .bind(serde_json::to_value(detail)?)
        .bind(timestamp_to_db(detail.fetched_at)?)
        .bind(fees_text)
        .bind(detail.has_syllabus)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(section_id) = section_id else {
            return Ok(false);
        };
        sqlx::query("delete from section_seat_reservations where section_id = $1")
            .bind(section_id)
            .execute(&mut *tx)
            .await?;
        for (ordinal, reservation) in detail.reserved.iter().enumerate() {
            sqlx::query(
                "insert into section_seat_reservations (section_id, ordinal, label, capacity, available) \
                 values ($1, $2, $3, $4, $5)",
            )
            .bind(section_id)
            .bind(i16::try_from(ordinal).map_err(|_| StoreError::Input("too many reservations".to_owned()))?)
            .bind(&reservation.label)
            .bind(i32::from(reservation.capacity))
            .bind(i32::from(reservation.available))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Open an `ingest_runs` row for a per-term job and return its id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn start_run(&self, job: &str, term: Option<TermCode>) -> Result<i64, StoreError> {
        self.start_run_keyed(job, term.map(RunKey::Term)).await
    }

    /// Open an `ingest_runs` row keyed by a term, a catalog year, or
    /// nothing, and return its id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn start_run_keyed(&self, job: &str, key: Option<RunKey>) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into ingest_runs (job, term_code, started_at) values ($1, $2, now()) returning id",
        )
        .bind(job)
        .bind(key.map(RunKey::as_db))
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// `select 1`: what `skyspace doctor` runs to tell a reachable
    /// database from a pool that only connected lazily.
    ///
    /// # Errors
    /// `StoreError::Database` when the round trip fails.
    pub async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query_scalar::<_, i32>("select 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }

    /// Close a run with its numbers. `false` when the id is unknown.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn finish_run(&self, id: i64, summary: &RunSummaryRow) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update ingest_runs set finished_at = now(), outcome = $2, requests = $3, targets = $4, \
                failures = $5, bytes = $6, rows_written = $7, error = $8 where id = $1",
        )
        .bind(id)
        .bind(summary.outcome.as_str())
        .bind(db_u32(summary.requests)?)
        .bind(db_u32(summary.targets)?)
        .bind(db_u32(summary.failures)?)
        .bind(db_u64(summary.bytes)?)
        .bind(db_u64(summary.rows_written)?)
        .bind(&summary.error)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The most recent successful run of `job`, for the term when given.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn last_ok_run(
        &self,
        job: &str,
        term: Option<TermCode>,
    ) -> Result<Option<RunRow>, StoreError> {
        self.last_ok_run_keyed(job, term.map(RunKey::Term)).await
    }

    /// The most recent successful run of `job` under `key` (any key when
    /// `None`): the catalog job compares against the same year's last
    /// good run, not another year's.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn last_ok_run_keyed(
        &self,
        job: &str,
        key: Option<RunKey>,
    ) -> Result<Option<RunRow>, StoreError> {
        let row: Option<RunRow> = sqlx::query_as(&format!(
            "select {RUN_COLUMNS} from ingest_runs \
             where job = $1 and outcome = 'ok' and ($2::text is null or term_code = $2) \
             order by finished_at desc limit 1"
        ))
        .bind(job)
        .bind(key.map(RunKey::as_db))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// The most recently started run of `job` whatever its outcome,
    /// including one still open: `skyspace doctor` reads it to tell a
    /// crashed run (never closed) from a running one.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn last_run(
        &self,
        job: &str,
        key: Option<RunKey>,
    ) -> Result<Option<RunRow>, StoreError> {
        let row: Option<RunRow> = sqlx::query_as(&format!(
            "select {RUN_COLUMNS} from ingest_runs \
             where job = $1 and ($2::text is null or term_code = $2) \
             order by started_at desc, id desc limit 1"
        ))
        .bind(job)
        .bind(key.map(RunKey::as_db))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Fill counts per field over the last `runs` good runs of `job` under
    /// `key` for `source`, newest run first: `(field, rows_seen,
    /// rows_filled)` per stat row. The guard turns them into rates.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn fill_history(
        &self,
        job: &str,
        key: Option<RunKey>,
        source: &str,
        runs: u32,
    ) -> Result<Vec<(String, u32, u32)>, StoreError> {
        let rows: Vec<(String, i32, i32)> = sqlx::query_as(
            "select ps.field, ps.rows_seen, ps.rows_filled from parse_stats ps \
             join ingest_runs r on r.id = ps.run_id \
             where ps.source = $3 and r.id in ( \
                select id from ingest_runs where job = $1 and outcome = 'ok' \
                  and ($2::text is null or term_code = $2) \
                order by finished_at desc limit $4) \
             order by r.finished_at desc, ps.field",
        )
        .bind(job)
        .bind(key.map(RunKey::as_db))
        .bind(source)
        .bind(i64::from(runs))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(field, seen, filled)| {
                (
                    field,
                    u32::try_from(seen).unwrap_or(0),
                    u32::try_from(filled).unwrap_or(0),
                )
            })
            .collect())
    }

    /// Live section count per subject for the term: what "rows in the
    /// last good run" means for the zero-rows guard.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn live_section_counts(
        &self,
        term: TermCode,
    ) -> Result<std::collections::BTreeMap<String, u64>, StoreError> {
        Ok(self
            .subjects(term)
            .await?
            .into_iter()
            .map(|row| {
                (
                    row.subject.as_str().to_owned(),
                    u64::from(row.section_count),
                )
            })
            .collect())
    }

    /// CRNs whose weekly document is missing or older than `stale_after`,
    /// never-fetched first.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt` for a negative CRN.
    pub async fn crns_due(
        &self,
        term: TermCode,
        column: DueColumn,
        stale_after: std::time::Duration,
    ) -> Result<Vec<Crn>, StoreError> {
        let cutoff = OffsetDateTime::now_utc() - stale_after;
        let sql = match column {
            DueColumn::Xml => {
                "select crn from sections where term_code = $1 and withdrawn_at is null \
                 and (xml_fetched_at is null or xml_fetched_at < $2) \
                 order by xml_fetched_at nulls first, id"
            }
            DueColumn::Detail => {
                "select crn from sections where term_code = $1 and withdrawn_at is null \
                 and (detail_fetched_at is null or detail_fetched_at < $2) \
                 order by detail_fetched_at nulls first, id"
            }
        };
        let rows: Vec<(i32,)> = sqlx::query_as(sql)
            .bind(term.to_string())
            .bind(cutoff)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|(crn,)| crate::convert::crn_from_db("sections", crn))
            .collect()
    }

    /// Stamp `xml_fetched_at` on the CRN a feed was requested for. The
    /// `ASSOCIATED-SECTIONS` feed lists the sections associated with a
    /// CRN and may omit the CRN itself, which would otherwise stay due.
    /// `false` when the CRN is not held.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn mark_xml_fetched(&self, term: TermCode, crn: Crn) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update sections set xml_fetched_at = now() where term_code = $1 and crn = $2",
        )
        .bind(term.to_string())
        .bind(crn_to_db(crn)?)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// What a replay reads: for every URL of `source` fetched on or after
    /// `since` (UTC midnight), the newest body that a run ending `ok`
    /// fetched, oldest first. Bodies from failed or quarantined runs and
    /// older bodies of the same URL are left out, so a replay ends on
    /// what Rice last published and cannot resurrect what a later good
    /// run withdrew.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn raw_responses_since(
        &self,
        source: RawSource,
        since: time::Date,
    ) -> Result<Vec<RawResponseRow>, StoreError> {
        let rows: Vec<RawResponseRow> = sqlx::query_as(&format!(
            "select * from (\
                select distinct on (rr.url) {RAW_COLUMNS} \
                from raw_responses rr join ingest_runs r on r.id = rr.run_id \
                where rr.source = $1 and rr.fetched_at >= $2 and r.outcome = 'ok' \
                order by rr.url, rr.fetched_at desc, rr.id desc) newest \
             order by fetched_at, id"
        ))
        .bind(source.as_str())
        .bind(since.midnight().assume_utc())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// The archived responses for one URL, newest first, with the outcome
    /// of the run that fetched each: what `skyspace archive diff` reads.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn raw_history(
        &self,
        url: &str,
        limit: u32,
    ) -> Result<Vec<RawResponseRow>, StoreError> {
        let rows: Vec<RawResponseRow> = sqlx::query_as(&format!(
            "select {RAW_COLUMNS} from raw_responses rr join ingest_runs r on r.id = rr.run_id \
             where rr.url = $1 order by rr.fetched_at desc, rr.id desc limit $2"
        ))
        .bind(url)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Index one archived document. Returns the row id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn record_raw_response(&self, input: &RawResponseInput) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into raw_responses (run_id, source, url, term_code, fetched_at, status, content_type, \
                byte_len, sha256, headers) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning id",
        )
        .bind(input.run_id)
        .bind(input.source.as_str())
        .bind(&input.url)
        .bind(input.term.map(|t| t.to_string()))
        .bind(input.fetched_at)
        .bind(i16::try_from(input.status).map_err(|_| StoreError::Input("status".to_owned()))?)
        .bind(&input.content_type)
        .bind(db_u32(input.byte_len)?)
        .bind(input.sha256.to_vec())
        .bind(&input.headers)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Record one parse issue. Returns the row id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn record_parse_issue(
        &self,
        run_id: i64,
        url: &str,
        severity: IssueSeverity,
        code: &str,
        detail: serde_json::Value,
    ) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into parse_issues (run_id, url, severity, code, detail) values ($1, $2, $3, $4, $5) returning id",
        )
        .bind(run_id)
        .bind(url)
        .bind(match severity {
            IssueSeverity::Warn => "warn",
            IssueSeverity::Error => "error",
        })
        .bind(code)
        .bind(detail)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Record fill rates: `(field, rows_seen, rows_filled)` per field for
    /// one source in one run. Re-recording a field replaces it.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn record_parse_stats(
        &self,
        run_id: i64,
        source: &str,
        fields: &[(String, u32, u32)],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        for (field, seen, filled) in fields {
            sqlx::query(
                "insert into parse_stats (run_id, source, field, rows_seen, rows_filled) values ($1, $2, $3, $4, $5) \
                 on conflict (run_id, source, field) do update set rows_seen = excluded.rows_seen, rows_filled = excluded.rows_filled",
            )
            .bind(run_id)
            .bind(source)
            .bind(field)
            .bind(db_u32(*seen)?)
            .bind(db_u32(*filled)?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(fields.len() as u64)
    }

    /// Try to take the session-scoped advisory lock for `key`. `None` when
    /// another process holds it, in which case the job exits cleanly.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn try_advisory_lock(&self, key: i64) -> Result<Option<JobLock>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let locked: bool = sqlx::query_scalar("select pg_try_advisory_lock($1)")
            .bind(key)
            .fetch_one(&mut *conn)
            .await?;
        Ok(locked.then_some(JobLock {
            conn: Some(conn),
            key,
        }))
    }

    /// Every canary for the term, plus term-less ones.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn canaries(&self, term: TermCode) -> Result<Vec<CanaryRow>, StoreError> {
        let rows: Vec<CanaryRow> = sqlx::query_as(
            "select id, source, term_code, url, expect_rows, expect_field, note from canaries \
             where term_code = $1 or term_code is null order by id",
        )
        .bind(term.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Pin a canary document. Returns the row id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn put_canary(&self, canary: &CanaryRow) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into canaries (source, term_code, url, expect_rows, expect_field, note) \
             values ($1, $2, $3, $4, $5, $6) returning id",
        )
        .bind(&canary.source)
        .bind(&canary.term_code)
        .bind(&canary.url)
        .bind(canary.expect_rows)
        .bind(&canary.expect_field)
        .bind(&canary.note)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use skyspace_core::Timestamp;
    use skyspace_core::catalog::{
        Attribute, CalendarDate, DateSpan, Fee, FinalExam, Instructor, Restrictions,
        SeatReservation, SectionDetail,
    };
    use skyspace_core::code::{Crn, Subject};
    use skyspace_core::prereq::PrereqExpr;
    use skyspace_core::program::CatalogYear;
    use skyspace_core::term::PartOfTermCode;
    use sqlx::PgPool;

    use super::{
        CanaryRow, DueColumn, IssueSeverity, RawResponseInput, RawSource, RunKey, RunOutcome,
        RunSummaryRow, SectionXmlRow,
    };
    use crate::pool::Store;
    use crate::testing::{
        code, course, fall, listing, seed_term, timed, unparsed, with_exclusion, with_prereq,
    };

    #[sqlx::test]
    async fn courses_round_trip(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let mut comp182 = with_prereq(
            course("COMP 182", "Algorithmic Thinking", 2027),
            "COMP 140 AND (MATH 101 OR MATH 105)",
        );
        comp182 = with_exclusion(
            comp182,
            &["COMP 310", "COMP 318"],
            "Cannot register for COMP 182 if student has credit for COMP 310/COMP 318.",
        );
        comp182.cross_list = vec![code("ELEC 182")];
        comp182.restrictions = Some(Restrictions {
            raw: "Must be enrolled in one of the following Level(s): Undergraduate".to_owned(),
            clauses: Vec::new(),
        });
        comp182.flags.second_half = true;
        comp182.equivalents = vec![code("COMP 183")];
        let written = store
            .upsert_courses(
                CatalogYear(2027),
                &[
                    comp182.clone(),
                    course("COMP 140", "Computational Thinking", 2027),
                ],
            )
            .await
            .unwrap();
        assert_eq!(written, 2);
        // Re-running is idempotent and replaces the per-year record.
        store
            .upsert_courses(CatalogYear(2027), &[comp182.clone()])
            .await
            .unwrap();

        let (stored, _) = store
            .course(fall(), &code("COMP 182"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored, comp182);
        let prereq = stored.prerequisites.unwrap();
        assert!(matches!(prereq.parsed, PrereqExpr::All(_)));
        let flat: Vec<(String, String)> =
            sqlx::query_as("select subject, number from course_prerequisites order by ordinal")
                .fetch_all(store.pool())
                .await
                .unwrap();
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0], ("COMP".to_owned(), "140".to_owned()));
        // The alias points at the smaller code, so both records agree.
        let alias: (String, String) = sqlx::query_as(
            "select alias_subject || ' ' || alias_number, canonical_subject || ' ' || canonical_number from course_aliases",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(alias, ("ELEC 182".to_owned(), "COMP 182".to_owned()));
        // An older year is not preferred over the term's year, but stands in when it is the only one.
        store
            .upsert_courses(CatalogYear(2026), &[course("COMP 215", "Old Title", 2026)])
            .await
            .unwrap();
        let (old, _) = store
            .course(fall(), &code("COMP 215"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(old.catalog_year, CatalogYear(2026));
    }

    #[sqlx::test]
    async fn listings_create_courses_and_withdraw_by_subject(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let mut first = listing(
            10001,
            "COMP 140",
            "COMPUTATIONAL THINKING",
            vec![timed("MWF", 540, 590)],
        );
        first.instructors = vec![Instructor {
            name: "Ada Lovelace".to_owned(),
            net_id: None,
        }];
        let rows = vec![
            first,
            listing(10002, "COMP 182", "ALGORITHMIC THINKING", vec![unparsed()]),
            listing(20001, "MATH 101", "CALCULUS I", vec![timed("TR", 780, 855)]),
        ];
        assert_eq!(store.upsert_sections(fall(), &rows).await.unwrap(), 3);
        let title: String = sqlx::query_scalar(
            "select title from courses where subject = 'COMP' and number = '140'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(title, "COMPUTATIONAL THINKING");
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.listing.instructors.len(), 1);

        let comp = Subject::new("COMP").unwrap();
        assert_eq!(
            store
                .withdraw_missing(fall(), &comp, &[Crn(10001)])
                .await
                .unwrap(),
            1
        );
        assert!(store.section(fall(), Crn(10002)).await.unwrap().is_none());
        assert!(store.section(fall(), Crn(20001)).await.unwrap().is_some());
        // Seen again: the withdrawal is cleared.
        store.upsert_sections(fall(), &rows[1..2]).await.unwrap();
        assert!(store.section(fall(), Crn(10002)).await.unwrap().is_some());
        // The catalog's long title wins once the catalog is pulled, and the listing does not undo it.
        store
            .upsert_courses(
                CatalogYear(2027),
                &[course("COMP 140", "Computational Thinking", 2027)],
            )
            .await
            .unwrap();
        store.upsert_sections(fall(), &rows[..1]).await.unwrap();
        let title: String = sqlx::query_scalar(
            "select title from courses where subject = 'COMP' and number = '140'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(title, "Computational Thinking");
    }

    #[sqlx::test]
    async fn section_xml_adds_dates_and_survives_relisting(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let rows = vec![listing(
            10001,
            "COMP 140",
            "COMPUTATIONAL THINKING",
            vec![timed("MWF", 540, 590)],
        )];
        store.upsert_sections(fall(), &rows).await.unwrap();
        let mut dated = timed("MWF", 540, 590);
        dated.dates = Some(DateSpan {
            start: CalendarDate {
                year: 2026,
                month: 8,
                day: 24,
            },
            end: CalendarDate {
                year: 2026,
                month: 12,
                day: 4,
            },
        });
        let xml = SectionXmlRow {
            crn: Crn(10001),
            part_of_term: Some(PartOfTermCode("1".to_owned())),
            part_of_term_label: Some("Full Term".to_owned()),
            final_exam: FinalExam::TakeHome,
            credits: None,
            attributes: BTreeSet::from([Attribute::DistributionThree]),
            school: Some("Engineering".to_owned()),
            meetings: vec![dated.clone()],
            final_exam_meeting: Some(timed("M", 540, 720)),
            instructors: vec![Instructor {
                name: "Ada Lovelace".to_owned(),
                net_id: None,
            }],
        };
        assert!(store.upsert_section_xml(fall(), &xml).await.unwrap());
        assert!(
            !store
                .upsert_section_xml(
                    fall(),
                    &SectionXmlRow {
                        crn: Crn(99999),
                        ..xml.clone()
                    }
                )
                .await
                .unwrap()
        );
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.listing.meetings, vec![dated.clone()]);
        assert_eq!(section.listing.final_exam, FinalExam::TakeHome);
        assert_eq!(
            section.listing.part_of_term,
            Some(PartOfTermCode("1".to_owned()))
        );
        let parts = store.parts_of_term(fall()).await.unwrap();
        assert_eq!(parts[0].label.as_deref(), Some("Full Term"));
        // The nightly listing keeps the dates the XML filled in.
        store.upsert_sections(fall(), &rows).await.unwrap();
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.listing.meetings, vec![dated]);
        let finals: i64 = sqlx::query_scalar("select count(*) from meetings where kind = 'final'")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(finals, 1);
        let q = crate::catalog::SectionQuery {
            school: vec!["Engineering".to_owned()],
            ..crate::catalog::SectionQuery::for_term(fall())
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 1);
    }

    #[sqlx::test]
    async fn section_detail_round_trip(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[listing(10001, "COMP 140", "COMPUTATIONAL THINKING", vec![])],
            )
            .await
            .unwrap();
        let detail = SectionDetail {
            long_title: "Computational Thinking".to_owned(),
            description: "An introduction.".to_owned(),
            department: "Computer Science".to_owned(),
            attributes: BTreeSet::new(),
            prerequisites_text: None,
            restrictions: None,
            grade_mode: None,
            method_of_instruction: None,
            course_type: None,
            language: None,
            reserved: vec![SeatReservation {
                label: "Fall Semester 2026 Matriculants".to_owned(),
                capacity: 20,
                available: 3,
            }],
            fees: vec![Fee {
                label: "Lab".to_owned(),
                amount_cents: Some(2500),
                raw: "Lab fee $25".to_owned(),
            }],
            has_syllabus: true,
            fetched_at: Timestamp(1_700_000_000),
        };
        assert!(
            store
                .upsert_section_detail(fall(), Crn(10001), &detail)
                .await
                .unwrap()
        );
        assert!(
            !store
                .upsert_section_detail(fall(), Crn(99999), &detail)
                .await
                .unwrap()
        );
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.detail, Some(detail));
        let (fees, reservations): (Option<String>, i64) = sqlx::query_as(
            "select s.fees_text, (select count(*) from section_seat_reservations r where r.section_id = s.id) from sections s",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(fees.as_deref(), Some("Lab fee $25"));
        assert_eq!(reservations, 1);
    }

    #[sqlx::test]
    async fn runs_responses_issues_and_stats(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let run = store.start_run("listings", Some(fall())).await.unwrap();
        assert!(
            store
                .last_ok_run("listings", Some(fall()))
                .await
                .unwrap()
                .is_none()
        );
        let response = store
            .record_raw_response(&RawResponseInput {
                run_id: run,
                source: RawSource::Listing,
                url: "https://courses.rice.edu/x".to_owned(),
                term: Some(fall()),
                fetched_at: time::OffsetDateTime::now_utc(),
                status: 200,
                content_type: Some("text/html".to_owned()),
                byte_len: 12,
                sha256: [7; 32],
                headers: serde_json::json!({"etag": "x"}),
            })
            .await
            .unwrap();
        assert!(response > 0);
        let issue = store
            .record_parse_issue(
                run,
                "https://courses.rice.edu/x",
                IssueSeverity::Warn,
                "unknown_detail_label",
                serde_json::json!({"label": "Foo:"}),
            )
            .await
            .unwrap();
        assert!(issue > 0);
        let fields = vec![("title".to_owned(), 10, 10), ("meeting".to_owned(), 10, 4)];
        assert_eq!(
            store
                .record_parse_stats(run, "listing", &fields)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .record_parse_stats(run, "listing", &fields[..1])
                .await
                .unwrap(),
            1
        );
        let summary = RunSummaryRow {
            outcome: RunOutcome::Ok,
            requests: 96,
            targets: 96,
            failures: 0,
            bytes: 4_600_000,
            rows_written: 4978,
            error: None,
        };
        assert!(store.finish_run(run, &summary).await.unwrap());
        assert!(!store.finish_run(run + 1000, &summary).await.unwrap());
        let last = store
            .last_ok_run("listings", Some(fall()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(last.id, run);
        assert_eq!(last.rows_written, 4978);
        assert_eq!(last.outcome.as_deref(), Some("ok"));
        assert!(store.last_ok_run("listings", None).await.unwrap().is_some());
        assert_eq!(
            RunOutcome::parse("quarantined").unwrap(),
            RunOutcome::Quarantined
        );
        assert!(RunOutcome::parse("meh").is_err());
    }

    #[sqlx::test]
    async fn advisory_lock_is_exclusive_and_released(pool: PgPool) {
        let store = Store::from_pool(pool);
        let lock = store.try_advisory_lock(42).await.unwrap().unwrap();
        assert!(store.try_advisory_lock(42).await.unwrap().is_none());
        assert!(store.try_advisory_lock(43).await.unwrap().is_some());
        lock.release().await.unwrap();
        let again = store.try_advisory_lock(42).await.unwrap().unwrap();
        drop(again);
        // Dropping closes the session; the server releases the lock shortly after.
        let mut reacquired = false;
        for _ in 0..50 {
            if store.try_advisory_lock(42).await.unwrap().is_some() {
                reacquired = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(reacquired);
    }

    /// The map is one direction for both sources, has no chains, and
    /// writing it twice changes nothing.
    #[sqlx::test]
    async fn aliases_point_at_the_smallest_code_from_either_source(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        // GA prints "STAT 310 / ECON 307": the pair arrives GA-first.
        store
            .upsert_aliases(&[(code("STAT 310"), code("ECON 307"))])
            .await
            .unwrap();
        let rows = |store: &Store| {
            let pool = store.pool().clone();
            async move {
                sqlx::query_as::<_, (String, String)>(
                    "select alias_subject || ' ' || alias_number, canonical_subject || ' ' || canonical_number \
                     from course_aliases order by 1",
                )
                .fetch_all(&pool)
                .await
                .unwrap()
            }
        };
        assert_eq!(
            rows(&store).await,
            vec![("STAT 310".to_owned(), "ECON 307".to_owned())]
        );
        // The catalog's sentence for the same pair agrees.
        let mut stat = course("STAT 310", "Probability", 2027);
        stat.cross_list = vec![code("ECON 307")];
        store
            .upsert_courses(CatalogYear(2027), &[stat])
            .await
            .unwrap();
        assert_eq!(
            rows(&store).await,
            vec![("STAT 310".to_owned(), "ECON 307".to_owned())]
        );
        // A chain collapses: MATH 182 -> ELEC 182 from GA, then the
        // catalog makes ELEC 182 an alias of COMP 182.
        store
            .upsert_aliases(&[(code("MATH 182"), code("ELEC 182"))])
            .await
            .unwrap();
        let mut comp = course("COMP 182", "Algorithmic Thinking", 2027);
        comp.cross_list = vec![code("ELEC 182")];
        store
            .upsert_courses(CatalogYear(2027), &[comp])
            .await
            .unwrap();
        let all = rows(&store).await;
        assert!(all.contains(&("MATH 182".to_owned(), "COMP 182".to_owned())));
        assert!(all.contains(&("ELEC 182".to_owned(), "COMP 182".to_owned())));
        let chained: i64 = sqlx::query_scalar(
            "select count(*) from course_aliases a join course_aliases b \
             on a.canonical_subject = b.alias_subject and a.canonical_number = b.alias_number",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(chained, 0, "no alias's canonical is itself an alias");
        // Idempotent.
        store
            .upsert_aliases(&[(code("MATH 182"), code("ELEC 182"))])
            .await
            .unwrap();
        assert_eq!(rows(&store).await, all);
    }

    /// A replayed body older than a withdrawal does not bring the section
    /// back; a newer one does.
    #[sqlx::test]
    async fn older_body_keeps_a_later_withdrawal(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let a = listing(10001, "COMP 140", "CT", vec![timed("MWF", 540, 590)]);
        let b = listing(10002, "COMP 140", "CT", vec![timed("TR", 540, 590)]);
        let yesterday = time::OffsetDateTime::now_utc() - time::Duration::days(1);
        store
            .upsert_sections_fetched_at(fall(), &[a.clone(), b.clone()], yesterday)
            .await
            .unwrap();
        let comp = Subject::new("COMP").unwrap();
        assert_eq!(
            store
                .withdraw_missing(fall(), &comp, &[Crn(10001)])
                .await
                .unwrap(),
            1
        );
        // Replay of yesterday's body: B stays withdrawn.
        store
            .upsert_sections_fetched_at(fall(), &[a.clone(), b.clone()], yesterday)
            .await
            .unwrap();
        assert!(store.section(fall(), Crn(10002)).await.unwrap().is_none());
        // A body fetched after the withdrawal lists it again.
        store.upsert_sections(fall(), &[b]).await.unwrap();
        assert!(store.section(fall(), Crn(10002)).await.unwrap().is_some());
    }

    /// A feed with no class meeting is partial: the listing's meetings
    /// and the earlier feed's attributes stay.
    #[sqlx::test]
    async fn section_xml_without_meetings_keeps_what_the_listing_filled(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[listing(
                    10001,
                    "COMP 140",
                    "CT",
                    vec![timed("MWF", 540, 590)],
                )],
            )
            .await
            .unwrap();
        let full = SectionXmlRow {
            crn: Crn(10001),
            part_of_term: None,
            part_of_term_label: None,
            final_exam: FinalExam::Scheduled,
            credits: None,
            attributes: BTreeSet::from([Attribute::DistributionThree]),
            school: None,
            meetings: vec![timed("MWF", 540, 590)],
            final_exam_meeting: Some(timed("M", 540, 720)),
            instructors: Vec::new(),
        };
        assert!(store.upsert_section_xml(fall(), &full).await.unwrap());
        let partial = SectionXmlRow {
            attributes: BTreeSet::new(),
            meetings: Vec::new(),
            final_exam_meeting: None,
            ..full
        };
        assert!(store.upsert_section_xml(fall(), &partial).await.unwrap());
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.listing.meetings, vec![timed("MWF", 540, 590)]);
        let (attributes, finals): (Vec<String>, i64) = sqlx::query_as(
            "select s.attributes, (select count(*) from meetings m where m.section_id = s.id and m.kind = 'final') \
             from sections s",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(attributes, vec!["GRP3".to_owned()]);
        assert_eq!(finals, 1);
        let q = crate::catalog::SectionQuery::for_term(fall());
        assert_eq!(store.search_sections(&q).await.unwrap().total, 1);
    }

    /// The queries ingest and `doctor` used to run themselves.
    #[sqlx::test]
    async fn keyed_runs_history_due_and_archive_reads(pool: PgPool) {
        let store = Store::from_pool(pool);
        store.ping().await.unwrap();
        seed_term(&store, "202710").await;
        let ok_summary = |rows: u64| RunSummaryRow {
            outcome: RunOutcome::Ok,
            requests: 1,
            targets: 1,
            failures: 0,
            bytes: 1,
            rows_written: rows,
            error: None,
        };
        // Catalog runs are keyed by year and compared within it.
        let y2027 = store
            .start_run_keyed("catalog", Some(RunKey::Year(CatalogYear(2027))))
            .await
            .unwrap();
        store
            .record_parse_stats(y2027, "catalog", &[("title".to_owned(), 10, 10)])
            .await
            .unwrap();
        store.finish_run(y2027, &ok_summary(6500)).await.unwrap();
        let y2024 = store
            .start_run_keyed("catalog", Some(RunKey::Year(CatalogYear(2024))))
            .await
            .unwrap();
        store.finish_run(y2024, &ok_summary(100)).await.unwrap();
        let last = store
            .last_ok_run_keyed("catalog", Some(RunKey::Year(CatalogYear(2027))))
            .await
            .unwrap()
            .unwrap();
        assert_eq!((last.id, last.term_code.as_deref()), (y2027, Some("2027")));
        assert!(
            store
                .last_ok_run_keyed("catalog", Some(RunKey::Year(CatalogYear(2025))))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .fill_history(
                    "catalog",
                    Some(RunKey::Year(CatalogYear(2027))),
                    "catalog",
                    7
                )
                .await
                .unwrap(),
            vec![("title".to_owned(), 10, 10)]
        );
        assert!(
            store
                .fill_history(
                    "catalog",
                    Some(RunKey::Year(CatalogYear(2024))),
                    "catalog",
                    7
                )
                .await
                .unwrap()
                .is_empty()
        );
        // `last_run` sees an open run; `last_ok_run` does not.
        let open = store.start_run("listings", Some(fall())).await.unwrap();
        assert!(
            store
                .last_ok_run("listings", Some(fall()))
                .await
                .unwrap()
                .is_none()
        );
        let seen = store
            .last_run("listings", Some(RunKey::Term(fall())))
            .await
            .unwrap()
            .unwrap();
        assert_eq!((seen.id, seen.finished_at), (open, None));

        // Due queues and the XML stamp.
        store
            .upsert_sections(
                fall(),
                &[
                    listing(10001, "COMP 140", "CT", vec![timed("MWF", 540, 590)]),
                    listing(10002, "COMP 182", "AT", vec![]),
                ],
            )
            .await
            .unwrap();
        let counts = store.live_section_counts(fall()).await.unwrap();
        assert_eq!(counts.get("COMP"), Some(&2));
        let zero = std::time::Duration::from_secs(0);
        assert_eq!(
            store.crns_due(fall(), DueColumn::Xml, zero).await.unwrap(),
            vec![Crn(10001), Crn(10002)]
        );
        assert!(store.mark_xml_fetched(fall(), Crn(10001)).await.unwrap());
        assert!(!store.mark_xml_fetched(fall(), Crn(99999)).await.unwrap());
        assert_eq!(
            store
                .crns_due(fall(), DueColumn::Xml, std::time::Duration::from_hours(1))
                .await
                .unwrap(),
            vec![Crn(10002)]
        );
        assert_eq!(
            store
                .crns_due(fall(), DueColumn::Detail, zero)
                .await
                .unwrap()
                .len(),
            2
        );

        // Archive reads: replay sees the newest body per URL from ok runs.
        let url = "https://courses.rice.edu/listing?p_subj=COMP";
        let at = |offset: i64| time::OffsetDateTime::now_utc() - time::Duration::hours(offset);
        let raw = |run: i64, fetched_at: time::OffsetDateTime, sha: u8| RawResponseInput {
            run_id: run,
            source: RawSource::Listing,
            url: url.to_owned(),
            term: Some(fall()),
            fetched_at,
            status: 200,
            content_type: Some("text/html".to_owned()),
            byte_len: 1,
            sha256: [sha; 32],
            headers: serde_json::json!({}),
        };
        store
            .record_raw_response(&raw(open, at(3), 1))
            .await
            .unwrap();
        store.finish_run(open, &ok_summary(2)).await.unwrap();
        let good = store.start_run("listings", Some(fall())).await.unwrap();
        store
            .record_raw_response(&raw(good, at(2), 2))
            .await
            .unwrap();
        store.finish_run(good, &ok_summary(2)).await.unwrap();
        let bad = store.start_run("listings", Some(fall())).await.unwrap();
        store
            .record_raw_response(&raw(bad, at(1), 3))
            .await
            .unwrap();
        store
            .finish_run(
                bad,
                &RunSummaryRow {
                    outcome: RunOutcome::Quarantined,
                    ..ok_summary(0)
                },
            )
            .await
            .unwrap();
        let since = time::OffsetDateTime::now_utc().date() - time::Duration::days(2);
        let replayable = store
            .raw_responses_since(RawSource::Listing, since)
            .await
            .unwrap();
        assert_eq!(replayable.len(), 1);
        assert_eq!(replayable[0].sha256, vec![2; 32]);
        assert_eq!(replayable[0].key().unwrap(), [2; 32]);
        assert!(
            store
                .raw_responses_since(RawSource::Detail, since)
                .await
                .unwrap()
                .is_empty()
        );
        let history = store.raw_history(url, 2).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].sha256, vec![3; 32]);
        assert_eq!(history[0].run_outcome.as_deref(), Some("quarantined"));
        assert_eq!(history[1].run_outcome.as_deref(), Some("ok"));
    }

    #[sqlx::test]
    async fn canaries_by_term(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        seed_term(&store, "202720").await;
        let row = CanaryRow {
            id: 0,
            source: "listing".to_owned(),
            term_code: Some("202710".to_owned()),
            url: "https://courses.rice.edu/comp".to_owned(),
            expect_rows: Some(40),
            expect_field: None,
            note: String::new(),
        };
        store.put_canary(&row).await.unwrap();
        store
            .put_canary(&CanaryRow {
                term_code: None,
                ..row.clone()
            })
            .await
            .unwrap();
        store
            .put_canary(&CanaryRow {
                term_code: Some("202720".to_owned()),
                ..row
            })
            .await
            .unwrap();
        assert_eq!(store.canaries(fall()).await.unwrap().len(), 2);
    }
}
