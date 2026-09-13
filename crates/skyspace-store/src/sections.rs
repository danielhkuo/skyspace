//! Loading `Section` values from their rows. Shared by search, the class
//! page, the course page and the plan bundle.

use std::collections::BTreeMap;

use skyspace_core::catalog::{Instructor, NetId, Seats, Section, SectionDetail, SectionListing};
use skyspace_core::code::SectionNumber;
use skyspace_core::term::PartOfTermCode;
use sqlx::PgConnection;
use time::OffsetDateTime;

use crate::convert::{
    MeetingColumns, code_from_db, credits_from_db, crn_from_db, final_exam_from_db,
    meeting_from_db, term_from_db, timestamp_from_db, u16_from_db,
};
use crate::error::StoreError;

/// The `select` list every section query shares. Aliases: `s` sections,
/// `c` courses.
pub(crate) const SECTION_COLUMNS: &str = "s.id, s.term_code, s.crn, c.subject, c.number, \
     s.section_code, s.title, s.part_of_term, s.credits_kind, s.credits_min_cents, \
     s.credits_max_cents, s.final_exam, s.detail, s.xml_fetched_at";

/// One `sections` row joined to its course: the column shape, not the
/// domain shape.
#[derive(Debug, sqlx::FromRow)]
pub struct SectionRow {
    /// Internal id, the join key for meetings, instructors and seats.
    pub id: i64,
    /// Rice's six-digit term code.
    pub term_code: String,
    /// The CRN.
    pub crn: i32,
    /// From `courses`.
    pub subject: String,
    /// From `courses`.
    pub number: String,
    /// Three characters.
    pub section_code: String,
    /// The listing's short title.
    pub title: String,
    /// Part-of-term code, when known.
    pub part_of_term: Option<String>,
    /// `fixed`, `range` or `either`.
    pub credits_kind: String,
    /// Hundredths.
    pub credits_min_cents: i16,
    /// Hundredths.
    pub credits_max_cents: i16,
    /// Rice's one-letter exam code; null is unknown.
    pub final_exam: Option<String>,
    /// The whole `SectionDetail`, null until the detail page is pulled.
    pub detail: Option<serde_json::Value>,
    /// Null when the per-CRN XML has never been fetched.
    pub xml_fetched_at: Option<OffsetDateTime>,
}

impl TryFrom<SectionRow> for SectionListing {
    type Error = StoreError;

    /// The listing without its meetings and instructors, which live in
    /// their own tables; `load_sections` fills them.
    fn try_from(row: SectionRow) -> Result<Self, Self::Error> {
        const TABLE: &str = "sections";
        Ok(Self {
            crn: crn_from_db(TABLE, row.crn)?,
            term: term_from_db(TABLE, &row.term_code)?,
            code: code_from_db(TABLE, &row.subject, &row.number)?,
            section: SectionNumber(row.section_code),
            title: row.title,
            credits: credits_from_db(
                TABLE,
                &row.credits_kind,
                row.credits_min_cents,
                row.credits_max_cents,
            )?,
            part_of_term: row.part_of_term.map(PartOfTermCode),
            instructors: Vec::new(),
            meetings: Vec::new(),
            final_exam: final_exam_from_db(row.final_exam.as_deref()),
        })
    }
}

#[derive(sqlx::FromRow)]
struct MeetingRow {
    section_id: i64,
    day_mask: i16,
    start_time: Option<time::Time>,
    end_time: Option<time::Time>,
    start_date: Option<time::Date>,
    end_date: Option<time::Date>,
    raw: String,
}

#[derive(sqlx::FromRow)]
struct InstructorRow {
    section_id: i64,
    name: String,
    netid: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SeatRow {
    pub section_id: i64,
    pub enrolled: i32,
    pub capacity: i32,
    pub wait_count: i32,
    pub wait_capacity: i32,
    pub source_time: OffsetDateTime,
}

impl TryFrom<SeatRow> for Seats {
    type Error = StoreError;

    fn try_from(row: SeatRow) -> Result<Self, Self::Error> {
        const TABLE: &str = "section_seat_state";
        Ok(Self {
            enrolled: u16_from_db(TABLE, "enrolled", row.enrolled)?,
            capacity: u16_from_db(TABLE, "capacity", row.capacity)?,
            waitlist_count: u16_from_db(TABLE, "wait_count", row.wait_count)?,
            waitlist_capacity: u16_from_db(TABLE, "wait_capacity", row.wait_capacity)?,
            as_of: timestamp_from_db(row.source_time),
        })
    }
}

async fn class_meetings(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<BTreeMap<i64, Vec<skyspace_core::catalog::Meeting>>, StoreError> {
    let rows: Vec<MeetingRow> = sqlx::query_as(
        "select section_id, day_mask, start_time, end_time, start_date, end_date, raw \
         from meetings where section_id = any($1) and kind = 'class' order by section_id, id",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    let mut out: BTreeMap<i64, Vec<_>> = BTreeMap::new();
    for row in rows {
        let meeting = meeting_from_db(MeetingColumns {
            day_mask: row.day_mask,
            start_time: row.start_time,
            end_time: row.end_time,
            start_date: row.start_date,
            end_date: row.end_date,
            raw: row.raw,
        })?;
        out.entry(row.section_id).or_default().push(meeting);
    }
    Ok(out)
}

async fn instructors(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<BTreeMap<i64, Vec<Instructor>>, StoreError> {
    let rows: Vec<InstructorRow> = sqlx::query_as(
        "select si.section_id, i.name, i.netid from section_instructors si \
         join instructors i on i.id = si.instructor_id \
         where si.section_id = any($1) order by si.section_id, si.ordinal",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    let mut out: BTreeMap<i64, Vec<Instructor>> = BTreeMap::new();
    for row in rows {
        out.entry(row.section_id).or_default().push(Instructor {
            name: row.name,
            net_id: row.netid.map(NetId),
        });
    }
    Ok(out)
}

pub(crate) async fn seat_states(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<BTreeMap<i64, Seats>, StoreError> {
    let rows: Vec<SeatRow> = sqlx::query_as(
        "select section_id, enrolled, capacity, wait_count, wait_capacity, source_time \
         from section_seat_state where section_id = any($1)",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    rows.into_iter()
        .map(|row| Ok((row.section_id, Seats::try_from(row)?)))
        .collect()
}

/// Rows into sections, in the rows' order, with meetings, instructors,
/// detail and seats attached. Three extra queries, whatever the page size.
pub(crate) async fn load_sections(
    conn: &mut PgConnection,
    rows: Vec<SectionRow>,
) -> Result<Vec<Section>, StoreError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut meetings = class_meetings(&mut *conn, &ids).await?;
    let mut instructors = instructors(&mut *conn, &ids).await?;
    let mut seats = seat_states(&mut *conn, &ids).await?;
    rows.into_iter()
        .map(|row| {
            let id = row.id;
            let detail = row
                .detail
                .clone()
                .map(serde_json::from_value::<SectionDetail>)
                .transpose()?;
            let mut listing = SectionListing::try_from(row)?;
            listing.meetings = meetings.remove(&id).unwrap_or_default();
            listing.instructors = instructors.remove(&id).unwrap_or_default();
            Ok(Section {
                listing,
                detail,
                seats: seats.remove(&id),
            })
        })
        .collect()
}
