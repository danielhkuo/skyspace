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
/// constraint; seats are absent because they are never archived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    fn as_str(self) -> &'static str {
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

/// The cross-list group's canonical code is its smallest member, so every
/// member's record derives the same alias rows.
async fn write_aliases(conn: &mut PgConnection, course: &Course) -> Result<(), StoreError> {
    if course.cross_list.is_empty() {
        return Ok(());
    }
    let mut group: Vec<&CourseCode> = course
        .cross_list
        .iter()
        .chain(core::iter::once(&course.code))
        .collect();
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

async fn upsert_listing(
    conn: &mut PgConnection,
    term: TermCode,
    listing: &SectionListing,
) -> Result<(), StoreError> {
    let course_id = ensure_course(&mut *conn, &listing.code, &listing.title, false).await?;
    let credits = credits_to_db(listing.credits)?;
    let section_id: i64 = sqlx::query_scalar(
        "insert into sections (term_code, crn, course_id, section_code, title, part_of_term, credits_kind, \
            credits_min_cents, credits_max_cents, final_exam) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         on conflict (term_code, crn) do update set \
            course_id = excluded.course_id, section_code = excluded.section_code, title = excluded.title, \
            part_of_term = coalesce(excluded.part_of_term, sections.part_of_term), \
            credits_kind = excluded.credits_kind, credits_min_cents = excluded.credits_min_cents, \
            credits_max_cents = excluded.credits_max_cents, \
            final_exam = coalesce(excluded.final_exam, sections.final_exam), \
            last_seen_at = now(), withdrawn_at = null \
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
        tx.commit().await?;
        Ok(written)
    }

    /// Write a subject listing's sections. A course row is created when
    /// missing, with the listing's title; a seen section loses its
    /// `withdrawn_at`. Returns the number of sections written.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn upsert_sections(
        &self,
        term: TermCode,
        rows: &[SectionListing],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        for listing in rows {
            upsert_listing(&mut tx, term, listing).await?;
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
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn upsert_section_xml(
        &self,
        term: TermCode,
        row: &SectionXmlRow,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let credits = row.credits.map(credits_to_db).transpose()?;
        let section_id: Option<i64> = sqlx::query_scalar(
            "update sections set \
                part_of_term = coalesce($3, part_of_term), part_of_term_label = coalesce($4, part_of_term_label), \
                final_exam = coalesce($5, final_exam), attributes = $6, school = coalesce($7, school), \
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
        .fetch_optional(&mut *tx)
        .await?;
        let Some(section_id) = section_id else {
            return Ok(false);
        };
        write_class_meetings(&mut tx, section_id, &row.meetings, false).await?;
        sqlx::query("delete from meetings where section_id = $1 and kind = 'final'")
            .bind(section_id)
            .execute(&mut *tx)
            .await?;
        if let Some(exam) = &row.final_exam_meeting {
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

    /// Open an `ingest_runs` row and return its id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn start_run(&self, job: &str, term: Option<TermCode>) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into ingest_runs (job, term_code, started_at) values ($1, $2, now()) returning id",
        )
        .bind(job)
        .bind(term.map(|t| t.to_string()))
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
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
        let row: Option<RunRow> = sqlx::query_as(
            "select id, job, term_code, started_at, finished_at, outcome, requests, targets, failures, \
                    bytes, rows_written, error \
             from ingest_runs where job = $1 and outcome = 'ok' and ($2::text is null or term_code = $2) \
             order by finished_at desc limit 1",
        )
        .bind(job)
        .bind(term.map(|t| t.to_string()))
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
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
        CanaryRow, IssueSeverity, RawResponseInput, RawSource, RunOutcome, RunSummaryRow,
        SectionXmlRow,
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
