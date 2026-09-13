//! Catalog reads: terms, the section search, the class and course pages,
//! seats and the data version behind the `ETag`.

use skyspace_core::catalog::{Attribute, Course, Seats, Section};
use skyspace_core::code::{CourseCode, Crn, Subject};
use skyspace_core::term::{PartOfTermCode, Season, TermCode};
use sqlx::{PgConnection, Postgres, QueryBuilder};
use time::OffsetDateTime;

use crate::convert::{crn_to_db, season_from_db, term_from_db};
use crate::courses::load_course;
use crate::error::StoreError;
use crate::pool::{Page, Store};
use crate::sections::{SECTION_COLUMNS, SectionRow, load_sections, seat_states};

/// One row of `terms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermRow {
    /// Rice's code.
    pub code: TermCode,
    /// Rice's own label, "Fall Semester 2026". Never synthesised.
    pub label: String,
    /// `None` for a quadmester.
    pub season: Option<Season>,
    /// Whether this is the term the site opens on.
    pub is_current: bool,
}

#[derive(sqlx::FromRow)]
struct TermDbRow {
    code: String,
    label: String,
    season: Option<String>,
    is_current: bool,
}

impl TryFrom<TermDbRow> for TermRow {
    type Error = StoreError;

    fn try_from(row: TermDbRow) -> Result<Self, Self::Error> {
        Ok(Self {
            code: term_from_db("terms", &row.code)?,
            label: row.label,
            season: row
                .season
                .as_deref()
                .map(|s| season_from_db("terms", s))
                .transpose()?,
            is_current: row.is_current,
        })
    }
}

/// The freshness of one ingest job, for `/api/v1/meta`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct JobRow {
    /// The job name as `ingest_runs.job` records it.
    pub job: String,
    /// When the last successful run finished, if one ever did.
    pub last_ok: Option<OffsetDateTime>,
    /// The outcome of the most recent run, `None` while it is still running.
    pub last_run_outcome: Option<String>,
}

/// What `/api/v1/meta` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaRows {
    /// `terms.is_current`, or `None` before an operator sets one.
    pub current_term: Option<TermCode>,
    /// Every term held, in code order.
    pub terms: Vec<TermRow>,
    /// One row per job that has ever run.
    pub jobs: Vec<JobRow>,
}

/// A subject seen in a term's sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectRow {
    /// The subject code.
    pub subject: Subject,
    /// Distinct courses with a live section this term.
    pub course_count: u32,
    /// Live sections this term.
    pub section_count: u32,
}

/// A part of term seen in a term's sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartOfTermRow {
    /// Rice's code.
    pub code: PartOfTermCode,
    /// Rice's label, once the XML pull has supplied it.
    pub label: Option<String>,
    /// Live sections in this part of term.
    pub section_count: u32,
}

/// Sort order for the section search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SectionSort {
    /// Code match first, then title rank, then course number.
    #[default]
    Relevance,
    /// Subject, then number, then section.
    CourseNumber,
    /// Fewest hours first.
    Credits,
    /// Most open seats first; unpolled sections last.
    OpenSeats,
}

/// The section search, already validated by the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionQuery {
    /// The term to search.
    pub term: TermCode,
    /// Keyword: code, title or instructor name.
    pub q: Option<String>,
    /// Any of these subjects.
    pub subject: Vec<Subject>,
    /// Any of these departments, from the catalog record for the term's year.
    pub department: Vec<String>,
    /// Any of these schools, from the section XML.
    pub school: Vec<String>,
    /// Any of these attributes, on the section or its catalog record.
    pub attr: Vec<Attribute>,
    /// Course levels: 100, 200, ...
    pub level: Vec<u16>,
    /// Hundredths, inclusive: the section's range must reach this.
    pub credits_min: Option<u16>,
    /// Hundredths, inclusive: the section's range must start under this.
    pub credits_max: Option<u16>,
    /// Bit 0 is Monday. Every timed class meeting must fall on these days.
    pub day_mask: Option<u8>,
    /// Minutes from midnight: no timed class meeting starts earlier.
    pub starts_after: Option<u16>,
    /// Minutes from midnight: no timed class meeting ends later.
    pub ends_before: Option<u16>,
    /// Any of these part-of-term codes.
    pub part_of_term: Vec<PartOfTermCode>,
    /// Only sections with a polled open seat; outside the poll set is excluded.
    pub open_seats_only: bool,
    /// Only sections with a timed class meeting.
    pub scheduled_only: bool,
    /// Sort order.
    pub sort: SectionSort,
    /// Rows to skip.
    pub offset: u32,
    /// Rows to return.
    pub limit: u16,
}

impl SectionQuery {
    /// An empty search of one term with the API's defaults: scheduled only,
    /// relevance, 25 rows.
    #[must_use]
    pub fn for_term(term: TermCode) -> Self {
        Self {
            term,
            q: None,
            subject: Vec::new(),
            department: Vec::new(),
            school: Vec::new(),
            attr: Vec::new(),
            level: Vec::new(),
            credits_min: None,
            credits_max: None,
            day_mask: None,
            starts_after: None,
            ends_before: None,
            part_of_term: Vec::new(),
            open_seats_only: false,
            scheduled_only: true,
            sort: SectionSort::Relevance,
            offset: 0,
            limit: 25,
        }
    }
}

/// One page of the section search with the counts the rail shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionPageRows {
    /// The rows, with the cursor for the next page.
    pub page: Page<Section>,
    /// Matching sections across the whole search.
    pub total: u32,
    /// Distinct courses across the whole search.
    pub course_count: u32,
    /// Rows `scheduled_only` removed, so nothing vanishes silently.
    pub unscheduled_hidden: u32,
}

#[derive(sqlx::FromRow)]
struct CountRow {
    total: i64,
    course_count: i64,
}

/// The `ingest_runs.job` name of the catalog job, whose runs are keyed by
/// academic year rather than term.
pub const CATALOG_JOB: &str = "catalog";

/// The jobs whose successful run moves `data_version`, the catalog
/// `ETag`: every job that writes what `/api/v1/sections`, the class page
/// or the course page serve. Seats are polled and served with their own
/// `as_of`, so they are not in the list. `skyspace-ingest` asserts its
/// job names against this list.
pub const DATA_VERSION_JOBS: [&str; 5] =
    ["listings", "detail", "reference", "sections", CATALOG_JOB];

/// Leading digits of a course number, as an integer.
const NUMBER_EXPR: &str = "nullif(regexp_replace(c.number, '[^0-9].*$', ''), '')::int";
const COURSE_ORDER: &str = "c.subject, nullif(regexp_replace(c.number, '[^0-9].*$', ''), '')::int, c.number, s.section_code";

/// Keywords longer than this are cut: nothing in the catalog is longer,
/// and a long keyword is only a longer scan.
pub const MAX_KEYWORD_CHARS: usize = 200;

/// Lowercase alphanumerics only: `COMP 140`, `comp-140` and `comp140` are one key.
fn normalise_code(q: &str) -> String {
    q.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The keyword a query carries once trimmed and capped, or `None` when
/// there is none.
fn keyword_of(q: &SectionQuery) -> Option<String> {
    let keyword = q.q.as_deref()?.trim();
    if keyword.is_empty() {
        return None;
    }
    Some(keyword.chars().take(MAX_KEYWORD_CHARS).collect())
}

/// A literal for `like`/`ilike`: `%`, `_` and the escape character itself
/// are escaped, so a keyword of `%` matches a percent sign, not everything.
fn like_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn count_from_db(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// `from ... where ...` shared by the page and the counts.
fn push_search(qb: &mut QueryBuilder<'_, Postgres>, q: &SectionQuery, with_scheduled: bool) {
    qb.push(
        " from sections s join courses c on c.id = s.course_id \
          left join course_catalog cc on cc.course_id = c.id and cc.catalog_year = ",
    );
    qb.push_bind(i32::from(q.term.academic_year()));
    qb.push(" left join section_seat_state st on st.section_id = s.id where s.term_code = ");
    qb.push_bind(q.term.to_string());
    qb.push(" and s.withdrawn_at is null");
    if let Some(keyword) = keyword_of(q) {
        // A keyword with no letter or digit has no code form: only the
        // title and instructor branches apply, so `%` matches nothing
        // rather than every course.
        let norm = normalise_code(&keyword);
        qb.push(" and (");
        if norm.is_empty() {
            qb.push("false");
        } else {
            qb.push("c.code_norm = ");
            qb.push_bind(norm.clone());
            qb.push(" or c.code_norm like ");
            qb.push_bind(format!("{norm}%"));
            qb.push(" or c.code_norm % ");
            qb.push_bind(norm);
        }
        qb.push(" or to_tsvector('english', c.title) @@ plainto_tsquery('english', ");
        qb.push_bind(keyword.clone());
        qb.push(
            ") or exists (select 1 from section_instructors si join instructors i on i.id = si.instructor_id \
               where si.section_id = s.id and i.name ilike ",
        );
        qb.push_bind(format!("%{}%", like_literal(&keyword)));
        qb.push(" escape '\\'))");
    }
    if !q.subject.is_empty() {
        qb.push(" and c.subject = any(");
        qb.push_bind(
            q.subject
                .iter()
                .map(|s| s.as_str().to_owned())
                .collect::<Vec<_>>(),
        );
        qb.push(")");
    }
    if !q.department.is_empty() {
        qb.push(" and cc.department = any(");
        qb.push_bind(q.department.clone());
        qb.push(")");
    }
    if !q.school.is_empty() {
        qb.push(" and s.school = any(");
        qb.push_bind(q.school.clone());
        qb.push(")");
    }
    if !q.attr.is_empty() {
        let codes: Vec<String> = q.attr.iter().map(|a| a.code().to_owned()).collect();
        qb.push(" and (s.attributes && ");
        qb.push_bind(codes.clone());
        qb.push(" or cc.attributes && ");
        qb.push_bind(codes);
        qb.push(")");
    }
    if !q.level.is_empty() {
        qb.push(format!(" and ({NUMBER_EXPR} / 100 * 100) = any("));
        qb.push_bind(q.level.iter().map(|l| i32::from(*l)).collect::<Vec<_>>());
        qb.push(")");
    }
    if let Some(min) = q.credits_min {
        qb.push(" and s.credits_max_cents >= ");
        qb.push_bind(i32::from(min));
    }
    if let Some(max) = q.credits_max {
        qb.push(" and s.credits_min_cents <= ");
        qb.push_bind(i32::from(max));
    }
    push_meeting_filters(qb, q, with_scheduled);
    if !q.part_of_term.is_empty() {
        qb.push(" and s.part_of_term = any(");
        qb.push_bind(
            q.part_of_term
                .iter()
                .map(|p| p.0.clone())
                .collect::<Vec<_>>(),
        );
        qb.push(")");
    }
    if q.open_seats_only {
        qb.push(" and st.capacity > st.enrolled");
    }
}

fn push_meeting_filters(
    qb: &mut QueryBuilder<'_, Postgres>,
    q: &SectionQuery,
    with_scheduled: bool,
) {
    const TIMED: &str = "m.section_id = s.id and m.kind = 'class' and m.start_time is not null";
    if let Some(mask) = q.day_mask {
        qb.push(format!(
            " and not exists (select 1 from meetings m where {TIMED} and (m.day_mask & ~"
        ));
        qb.push_bind(i32::from(mask));
        qb.push(") <> 0)");
    }
    if let Some(minute) = q.starts_after {
        qb.push(format!(" and not exists (select 1 from meetings m where {TIMED} and m.start_time < time '00:00' + "));
        qb.push_bind(i32::from(minute));
        qb.push(" * interval '1 minute')");
    }
    if let Some(minute) = q.ends_before {
        qb.push(format!(" and not exists (select 1 from meetings m where {TIMED} and m.end_time > time '00:00' + "));
        qb.push_bind(i32::from(minute));
        qb.push(" * interval '1 minute')");
    }
    if with_scheduled && q.scheduled_only {
        qb.push(format!(
            " and exists (select 1 from meetings m where {TIMED})"
        ));
    }
}

fn push_order(qb: &mut QueryBuilder<'_, Postgres>, q: &SectionQuery) {
    qb.push(" order by ");
    match q.sort {
        SectionSort::Relevance => {
            if let Some(keyword) = keyword_of(q) {
                let norm = normalise_code(&keyword);
                if !norm.is_empty() {
                    qb.push("case when c.code_norm = ");
                    qb.push_bind(norm.clone());
                    qb.push(" then 0 when c.code_norm like ");
                    qb.push_bind(format!("{norm}%"));
                    qb.push(" then 1 else 2 end, ");
                }
                qb.push("ts_rank(to_tsvector('english', c.title), plainto_tsquery('english', ");
                qb.push_bind(keyword);
                qb.push(")) desc, ");
            }
        }
        SectionSort::CourseNumber => {}
        SectionSort::Credits => {
            qb.push("s.credits_min_cents, s.credits_max_cents, ");
        }
        SectionSort::OpenSeats => {
            qb.push("(st.capacity - st.enrolled) desc nulls last, ");
        }
    }
    qb.push(COURSE_ORDER);
}

async fn search_counts(
    conn: &mut PgConnection,
    q: &SectionQuery,
    with_scheduled: bool,
) -> Result<CountRow, StoreError> {
    let mut qb =
        QueryBuilder::new("select count(*) as total, count(distinct c.id) as course_count");
    push_search(&mut qb, q, with_scheduled);
    Ok(qb.build_query_as::<CountRow>().fetch_one(conn).await?)
}

impl Store {
    /// Every term held, in code order.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a code the core type rejects.
    pub async fn terms(&self) -> Result<Vec<TermRow>, StoreError> {
        let rows: Vec<TermDbRow> =
            sqlx::query_as("select code, label, season, is_current from terms order by code")
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(TermRow::try_from).collect()
    }

    /// The current term, if an operator has set one.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a code the core type rejects.
    pub async fn current_term(&self) -> Result<Option<TermCode>, StoreError> {
        let code: Option<String> = sqlx::query_scalar("select code from terms where is_current")
            .fetch_optional(&self.pool)
            .await?;
        code.as_deref()
            .map(|c| term_from_db("terms", c))
            .transpose()
    }

    /// Make `code` the current term, clearing the previous one. `false` when
    /// the term is not held.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn set_current_term(&self, code: TermCode) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("update terms set is_current = false where is_current")
            .execute(&mut *tx)
            .await?;
        let done = sqlx::query("update terms set is_current = true where code = $1")
            .bind(code.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(done.rows_affected() > 0)
    }

    /// What `/api/v1/meta` needs, in three queries.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a term the core type rejects.
    pub async fn meta(&self) -> Result<MetaRows, StoreError> {
        let terms = self.terms().await?;
        let current_term = terms.iter().find(|t| t.is_current).map(|t| t.code);
        let jobs: Vec<JobRow> = sqlx::query_as(
            "select distinct on (r.job) r.job, \
                    (select max(o.finished_at) from ingest_runs o where o.job = r.job and o.outcome = 'ok') as last_ok, \
                    r.outcome as last_run_outcome \
             from ingest_runs r order by r.job, r.started_at desc",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(MetaRows {
            current_term,
            terms,
            jobs,
        })
    }

    /// Epoch seconds of the last successful run of any job in
    /// [`DATA_VERSION_JOBS`] for the term (the catalog job is keyed by the
    /// term's academic year), or 0 before any: the catalog `ETag` input.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn data_version(&self, term: TermCode) -> Result<i64, StoreError> {
        let at: Option<OffsetDateTime> = sqlx::query_scalar(
            "select max(finished_at) from ingest_runs \
             where outcome = 'ok' and job = any($3) \
               and term_code = case when job = $4 then $2 else $1 end",
        )
        .bind(term.to_string())
        .bind(term.academic_year().to_string())
        .bind(DATA_VERSION_JOBS.map(str::to_owned).to_vec())
        .bind(CATALOG_JOB)
        .fetch_one(&self.pool)
        .await?;
        Ok(at.map_or(0, OffsetDateTime::unix_timestamp))
    }

    /// One page of sections with the counts the rail shows. Keyword matching
    /// ranks an exact code first, then a code prefix, then title rank.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` for a row a core type rejects, or
    /// `Json` for a stored detail document that no longer deserialises.
    pub async fn search_sections(&self, q: &SectionQuery) -> Result<SectionPageRows, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let mut qb = QueryBuilder::new(format!("select {SECTION_COLUMNS}"));
        push_search(&mut qb, q, true);
        push_order(&mut qb, q);
        qb.push(" limit ");
        qb.push_bind(i64::from(q.limit));
        qb.push(" offset ");
        qb.push_bind(i64::from(q.offset));
        let rows: Vec<SectionRow> = qb.build_query_as().fetch_all(&mut *conn).await?;
        let counts = search_counts(&mut conn, q, true).await?;
        let unscheduled_hidden = if q.scheduled_only {
            let all = search_counts(&mut conn, q, false).await?;
            count_from_db(all.total.saturating_sub(counts.total))
        } else {
            0
        };
        let items = load_sections(&mut conn, rows).await?;
        let total = count_from_db(counts.total);
        let end = q
            .offset
            .saturating_add(u32::try_from(items.len()).unwrap_or(u32::MAX));
        let next = (end < total).then(|| end.to_string());
        Ok(SectionPageRows {
            page: Page { items, next },
            total,
            course_count: count_from_db(counts.course_count),
            unscheduled_hidden,
        })
    }

    /// One live section by term and CRN.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json` as for `search_sections`.
    pub async fn section(&self, term: TermCode, crn: Crn) -> Result<Option<Section>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let row: Option<SectionRow> = sqlx::query_as(&format!(
            "select {SECTION_COLUMNS} from sections s join courses c on c.id = s.course_id \
             where s.term_code = $1 and s.crn = $2 and s.withdrawn_at is null"
        ))
        .bind(term.to_string())
        .bind(crn_to_db(crn)?)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(load_sections(&mut conn, vec![row]).await?.pop())
    }

    /// A course's catalog record (the term's academic year, else the newest
    /// year held) with its live sections in the term. `None` when no
    /// catalog year holds the course.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json` as for `search_sections`.
    pub async fn course(
        &self,
        term: TermCode,
        code: &CourseCode,
    ) -> Result<Option<(Course, Vec<Section>)>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        course_in_term(&mut conn, term, code).await
    }

    /// Several courses at once, each with its sections and whether any live
    /// section exists this term. Codes with no catalog record are omitted.
    ///
    /// # Errors
    /// `StoreError::Database`, `Corrupt` or `Json` as for `search_sections`.
    pub async fn courses_by_code(
        &self,
        term: TermCode,
        codes: &[CourseCode],
    ) -> Result<Vec<(Course, Vec<Section>, bool)>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let mut out = Vec::with_capacity(codes.len());
        for code in codes {
            if let Some((course, sections)) = course_in_term(&mut conn, term, code).await? {
                let offered = !sections.is_empty();
                out.push((course, sections, offered));
            }
        }
        Ok(out)
    }

    /// Current seats for up to a request's worth of CRNs. A CRN outside the
    /// poll set has no row and is absent from the result.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a count that is not a `u16`.
    pub async fn seats(
        &self,
        term: TermCode,
        crns: &[Crn],
    ) -> Result<Vec<(Crn, Seats)>, StoreError> {
        #[derive(sqlx::FromRow)]
        struct IdRow {
            id: i64,
            crn: i32,
        }
        let mut conn = self.pool.acquire().await?;
        let wanted = crns
            .iter()
            .map(|c| crn_to_db(*c))
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<IdRow> =
            sqlx::query_as("select id, crn from sections where term_code = $1 and crn = any($2)")
                .bind(term.to_string())
                .bind(&wanted)
                .fetch_all(&mut *conn)
                .await?;
        let section_ids: Vec<i64> = ids.iter().map(|r| r.id).collect();
        let mut seats = seat_states(&mut conn, &section_ids).await?;
        ids.into_iter()
            .filter_map(|row| seats.remove(&row.id).map(|s| (row.crn, s)))
            .map(|(crn, s)| Ok((crate::convert::crn_from_db("sections", crn)?, s)))
            .collect()
    }

    /// Subjects with a live section in the term, with counts.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a subject the core type rejects.
    pub async fn subjects(&self, term: TermCode) -> Result<Vec<SubjectRow>, StoreError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            subject: String,
            course_count: i64,
            section_count: i64,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "select c.subject, count(distinct c.id) as course_count, count(*) as section_count \
             from sections s join courses c on c.id = s.course_id \
             where s.term_code = $1 and s.withdrawn_at is null group by c.subject order by c.subject",
        )
        .bind(term.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(SubjectRow {
                    subject: Subject::new(&row.subject)
                        .map_err(|e| crate::error::corrupt("courses", "subject", e))?,
                    course_count: count_from_db(row.course_count),
                    section_count: count_from_db(row.section_count),
                })
            })
            .collect()
    }

    /// Parts of term with a live section in the term, with counts.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn parts_of_term(&self, term: TermCode) -> Result<Vec<PartOfTermRow>, StoreError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            code: String,
            label: Option<String>,
            section_count: i64,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "select part_of_term as code, max(part_of_term_label) as label, count(*) as section_count \
             from sections where term_code = $1 and withdrawn_at is null and part_of_term is not null \
             group by part_of_term order by part_of_term",
        )
        .bind(term.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| PartOfTermRow {
                code: PartOfTermCode(row.code),
                label: row.label,
                section_count: count_from_db(row.section_count),
            })
            .collect())
    }
}

async fn course_in_term(
    conn: &mut PgConnection,
    term: TermCode,
    code: &CourseCode,
) -> Result<Option<(Course, Vec<Section>)>, StoreError> {
    let year = skyspace_core::program::CatalogYear(term.academic_year());
    let Some(course) = load_course(&mut *conn, code, year).await? else {
        return Ok(None);
    };
    let rows: Vec<SectionRow> = sqlx::query_as(&format!(
        "select {SECTION_COLUMNS} from sections s join courses c on c.id = s.course_id \
         where s.term_code = $1 and c.subject = $2 and c.number = $3 and s.withdrawn_at is null \
         order by s.section_code"
    ))
    .bind(term.to_string())
    .bind(code.subject.as_str())
    .bind(code.number.as_str())
    .fetch_all(&mut *conn)
    .await?;
    let sections = load_sections(conn, rows).await?;
    Ok(Some((course, sections)))
}

#[cfg(test)]
mod tests {
    use skyspace_core::catalog::Attribute;
    use skyspace_core::code::{Crn, Subject};
    use skyspace_core::program::CatalogYear;
    use skyspace_core::term::PartOfTermCode;
    use sqlx::PgPool;

    use super::{SectionQuery, SectionSort};
    use crate::ingest::{RunOutcome, RunSummaryRow};
    use crate::pool::Store;
    use crate::testing::{
        code, course, fall, listing, seats, seed_term, term, timed, unparsed, with_attribute,
    };

    /// Three sections: a timed COMP 140, an unscheduled COMP 182 and a timed
    /// MATH 101 whose catalog record carries GRP3.
    async fn seed_catalog(store: &Store) {
        seed_term(store, "202710").await;
        store
            .upsert_courses(
                CatalogYear(2027),
                &[
                    course("COMP 140", "Computational Thinking", 2027),
                    course("COMP 182", "Algorithmic Thinking", 2027),
                    with_attribute(
                        course("MATH 101", "Single Variable Calculus I", 2027),
                        Attribute::DistributionThree,
                    ),
                ],
            )
            .await
            .unwrap();
        let mut math = listing(
            30003,
            "MATH 101",
            "CALCULUS I",
            vec![timed("TR", 13 * 60, 14 * 60)],
        );
        math.part_of_term = Some(PartOfTermCode("1".to_owned()));
        store
            .upsert_sections(
                fall(),
                &[
                    listing(
                        10001,
                        "COMP 140",
                        "COMPUTATIONAL THINKING",
                        vec![timed("MWF", 9 * 60, 9 * 60 + 50)],
                    ),
                    listing(20002, "COMP 182", "ALGORITHMIC THINKING", vec![unparsed()]),
                    math,
                ],
            )
            .await
            .unwrap();
    }

    #[sqlx::test]
    async fn terms_current_and_meta(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        seed_term(&store, "202705").await;
        assert_eq!(store.current_term().await.unwrap(), None);
        assert!(!store.set_current_term(term("202720")).await.unwrap());
        assert!(store.set_current_term(term("202710")).await.unwrap());
        assert_eq!(store.current_term().await.unwrap(), Some(fall()));
        let run = store.start_run("listings", Some(fall())).await.unwrap();
        let meta = store.meta().await.unwrap();
        assert_eq!(meta.current_term, Some(fall()));
        assert_eq!(meta.terms.len(), 2);
        assert_eq!(meta.terms[0].season, None);
        assert_eq!(meta.terms[1].label, "Term 202710");
        assert_eq!(meta.jobs.len(), 1);
        assert_eq!(meta.jobs[0].last_run_outcome, None);
        assert_eq!(meta.jobs[0].last_ok, None);
        let summary = RunSummaryRow {
            outcome: RunOutcome::Ok,
            requests: 1,
            targets: 1,
            failures: 0,
            bytes: 10,
            rows_written: 3,
            error: None,
        };
        assert!(store.finish_run(run, &summary).await.unwrap());
        let meta = store.meta().await.unwrap();
        assert_eq!(meta.jobs[0].last_run_outcome.as_deref(), Some("ok"));
        assert!(meta.jobs[0].last_ok.is_some());
    }

    #[sqlx::test]
    async fn data_version_moves_with_a_finished_run(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        assert_eq!(store.data_version(fall()).await.unwrap(), 0);
        let run = store.start_run("listings", Some(fall())).await.unwrap();
        let summary = RunSummaryRow {
            outcome: RunOutcome::Ok,
            requests: 0,
            targets: 0,
            failures: 0,
            bytes: 0,
            rows_written: 0,
            error: None,
        };
        store.finish_run(run, &summary).await.unwrap();
        assert!(store.data_version(fall()).await.unwrap() > 1_700_000_000);
        let after_listings = store.data_version(fall()).await.unwrap();
        // The weekly XML and the catalog (keyed by the term's year) move it too.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let xml_run = store.start_run("sections", Some(fall())).await.unwrap();
        store.finish_run(xml_run, &summary).await.unwrap();
        let after_xml = store.data_version(fall()).await.unwrap();
        assert!(after_xml > after_listings);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let catalog_run = store
            .start_run_keyed(
                "catalog",
                Some(crate::ingest::RunKey::Year(CatalogYear(2027))),
            )
            .await
            .unwrap();
        store.finish_run(catalog_run, &summary).await.unwrap();
        assert!(store.data_version(fall()).await.unwrap() > after_xml);
        // Another year's catalog and the seats poll do not.
        let frozen = store.data_version(fall()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let other_year = store
            .start_run_keyed(
                "catalog",
                Some(crate::ingest::RunKey::Year(CatalogYear(2024))),
            )
            .await
            .unwrap();
        store.finish_run(other_year, &summary).await.unwrap();
        let seats_run = store.start_run("seats", Some(fall())).await.unwrap();
        store.finish_run(seats_run, &summary).await.unwrap();
        assert_eq!(store.data_version(fall()).await.unwrap(), frozen);
        assert_eq!(
            store
                .last_ok_run("seats", Some(fall()))
                .await
                .unwrap()
                .map(|r| r.id),
            Some(seats_run)
        );
    }

    #[sqlx::test]
    async fn search_respects_scheduled_only_and_counts(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        let mut q = SectionQuery::for_term(fall());
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.course_count, 2);
        assert_eq!(page.unscheduled_hidden, 1);
        assert_eq!(page.page.items.len(), 2);
        assert_eq!(page.page.next, None);
        assert_eq!(page.page.items[0].listing.code, code("COMP 140"));
        assert_eq!(page.page.items[0].listing.meetings.len(), 1);
        q.scheduled_only = false;
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.unscheduled_hidden, 0);
        q.limit = 2;
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.page.next.as_deref(), Some("2"));
        q.offset = 2;
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.page.items.len(), 1);
        assert_eq!(page.page.next, None);
    }

    #[sqlx::test]
    async fn search_filters(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        store
            .record_seats(fall(), &[(Crn(30003), seats(10, 20, 1_700_000_000))])
            .await
            .unwrap();
        let base = SectionQuery::for_term(fall());

        let q = SectionQuery {
            open_seats_only: true,
            ..base.clone()
        };
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.page.items[0].listing.crn, Crn(30003));
        assert!(page.page.items[0].seats.is_some());

        let q = SectionQuery {
            attr: vec![Attribute::DistributionThree],
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 1);
        let q = SectionQuery {
            attr: vec![Attribute::AnalyzingDiversity],
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 0);

        let q = SectionQuery {
            subject: vec![Subject::new("COMP").unwrap()],
            scheduled_only: false,
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);

        let q = SectionQuery {
            department: vec!["Computer Science".to_owned()],
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);

        let q = SectionQuery {
            level: vec![100],
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);

        let q = SectionQuery {
            day_mask: Some(0b1_0101),
            ..base.clone()
        };
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.page.items[0].listing.crn, Crn(10001));

        let q = SectionQuery {
            starts_after: Some(10 * 60),
            ..base.clone()
        };
        assert_eq!(
            store.search_sections(&q).await.unwrap().page.items[0]
                .listing
                .crn,
            Crn(30003)
        );
        let q = SectionQuery {
            ends_before: Some(10 * 60),
            ..base.clone()
        };
        assert_eq!(
            store.search_sections(&q).await.unwrap().page.items[0]
                .listing
                .crn,
            Crn(10001)
        );

        let q = SectionQuery {
            credits_min: Some(400),
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 0);
        let q = SectionQuery {
            credits_max: Some(300),
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);

        let q = SectionQuery {
            part_of_term: vec![PartOfTermCode("1".to_owned())],
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 1);

        let q = SectionQuery {
            sort: SectionSort::OpenSeats,
            ..base.clone()
        };
        assert_eq!(
            store.search_sections(&q).await.unwrap().page.items[0]
                .listing
                .crn,
            Crn(30003)
        );
        let q = SectionQuery {
            sort: SectionSort::Credits,
            ..base.clone()
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);
        let q = SectionQuery {
            sort: SectionSort::CourseNumber,
            ..base
        };
        assert_eq!(
            store.search_sections(&q).await.unwrap().page.items[0]
                .listing
                .crn,
            Crn(10001)
        );
    }

    #[sqlx::test]
    async fn search_keyword_ranks_code_first(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        for keyword in ["comp140", "COMP 140", "comp-140"] {
            let q = SectionQuery {
                q: Some(keyword.to_owned()),
                scheduled_only: false,
                ..SectionQuery::for_term(fall())
            };
            let page = store.search_sections(&q).await.unwrap();
            assert!(page.total >= 1, "{keyword}");
            assert_eq!(
                page.page.items[0].listing.code,
                code("COMP 140"),
                "{keyword}"
            );
        }
        let q = SectionQuery {
            q: Some("comp".to_owned()),
            scheduled_only: false,
            ..SectionQuery::for_term(fall())
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);
        let q = SectionQuery {
            q: Some("calculus".to_owned()),
            ..SectionQuery::for_term(fall())
        };
        let page = store.search_sections(&q).await.unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.page.items[0].listing.code, code("MATH 101"));
        let q = SectionQuery {
            q: Some("   ".to_owned()),
            ..SectionQuery::for_term(fall())
        };
        assert_eq!(store.search_sections(&q).await.unwrap().total, 2);
    }

    /// `%`, `_` and `\\` are characters, not wildcards; a keyword with no
    /// letter or digit matches nothing rather than everything.
    #[sqlx::test]
    async fn keyword_wildcards_are_literal(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        let mut taught = listing(40004, "COMP 140", "CT", vec![timed("MWF", 600, 650)]);
        taught.section = skyspace_core::code::SectionNumber("002".to_owned());
        taught.instructors = vec![skyspace_core::catalog::Instructor {
            name: "Ada Lovelace".to_owned(),
            net_id: None,
        }];
        store.upsert_sections(fall(), &[taught]).await.unwrap();
        let search = |q: &str| {
            let query = SectionQuery {
                q: Some(q.to_owned()),
                scheduled_only: false,
                ..SectionQuery::for_term(fall())
            };
            let store = store.clone();
            async move { store.search_sections(&query).await.unwrap().total }
        };
        assert_eq!(search("Ada").await, 1);
        assert_eq!(search("%").await, 0);
        assert_eq!(search("???").await, 0);
        assert_eq!(search("_").await, 0);
        assert_eq!(search("A%a").await, 0, "% is not a wildcard");
        assert_eq!(search("Ada_Lovelace").await, 0, "_ is not a wildcard");
        assert_eq!(search("Ada Lovelace").await, 1);
        assert_eq!(search("a\\").await, 0);
        assert_eq!(search("COMP_1").await, 3, "the code branch reads comp1");
        assert_eq!(search(&"x".repeat(5000)).await, 0);
        let mut q = SectionQuery::for_term(fall());
        q.q = Some("%".to_owned());
        q.sort = SectionSort::Relevance;
        assert_eq!(store.search_sections(&q).await.unwrap().total, 0);
    }

    #[sqlx::test]
    async fn section_course_and_seats(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        store
            .record_seats(fall(), &[(Crn(10001), seats(5, 30, 1_700_000_000))])
            .await
            .unwrap();
        let section = store.section(fall(), Crn(10001)).await.unwrap().unwrap();
        assert_eq!(section.listing.title, "COMPUTATIONAL THINKING");
        assert_eq!(section.seats.map(|s| s.capacity), Some(30));
        assert!(section.detail.is_none());
        assert!(store.section(fall(), Crn(99999)).await.unwrap().is_none());

        let (course, sections) = store
            .course(fall(), &code("COMP 140"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(course.title, "Computational Thinking");
        assert_eq!(course.catalog_year, CatalogYear(2027));
        assert_eq!(sections.len(), 1);
        assert!(
            store
                .course(fall(), &code("COMP 999"))
                .await
                .unwrap()
                .is_none()
        );

        let many = store
            .courses_by_code(
                fall(),
                &[code("COMP 140"), code("COMP 999"), code("MATH 101")],
            )
            .await
            .unwrap();
        assert_eq!(many.len(), 2);
        assert!(many.iter().all(|(_, _, offered)| *offered));

        let rows = store
            .seats(fall(), &[Crn(10001), Crn(20002)])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, Crn(10001));
        assert_eq!(rows[0].1.enrolled, 5);
    }

    #[sqlx::test]
    async fn reference_reads(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_catalog(&store).await;
        let subjects = store.subjects(fall()).await.unwrap();
        assert_eq!(subjects.len(), 2);
        assert_eq!(subjects[0].subject.as_str(), "COMP");
        assert_eq!(subjects[0].course_count, 2);
        assert_eq!(subjects[0].section_count, 2);
        let parts = store.parts_of_term(fall()).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].code.0, "1");
        assert_eq!(parts[0].label, None);
        assert_eq!(parts[0].section_count, 1);
    }
}
