//! Conversions between column shapes and core types. Every `TryFrom` into a
//! core type names its table and column in the error.

use std::collections::BTreeSet;

use skyspace_core::Timestamp;
use skyspace_core::catalog::{
    Attribute, CalendarDate, DateSpan, DaySet, FinalExam, Meeting, MeetingPattern, MeetingTime,
    MinuteOfDay,
};
use skyspace_core::code::{CourseCode, Crn};
use skyspace_core::term::{CreditRange, Credits, Season, TermCode};
use time::{Date, Month, OffsetDateTime, Time};

use crate::error::{StoreError, corrupt};

/// The three `credits_kind` values, as one small struct so a section row and
/// a catalog row share one conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CreditColumns {
    pub kind: &'static str,
    pub min: i16,
    pub max: i16,
}

/// Hundredths into `smallint`.
pub(crate) fn cents_to_db(credits: Credits) -> Result<i16, StoreError> {
    i16::try_from(credits.cents()).map_err(|_| {
        StoreError::Input(format!(
            "{} hundredths do not fit smallint",
            credits.cents()
        ))
    })
}

/// `smallint` into hundredths.
pub(crate) fn cents_from_db(
    table: &'static str,
    column: &'static str,
    value: i16,
) -> Result<Credits, StoreError> {
    u16::try_from(value)
        .map(Credits::from_cents)
        .map_err(|_| corrupt(table, column, format!("negative credits {value}")))
}

/// A `CreditRange` as its three columns.
pub(crate) fn credits_to_db(range: CreditRange) -> Result<CreditColumns, StoreError> {
    let (kind, min, max) = match range {
        CreditRange::Fixed(c) => ("fixed", c, c),
        CreditRange::Range { min, max } => ("range", min, max),
        CreditRange::Either(a, b) => ("either", a, b),
    };
    Ok(CreditColumns {
        kind,
        min: cents_to_db(min)?,
        max: cents_to_db(max)?,
    })
}

/// Three columns back into a `CreditRange`.
pub(crate) fn credits_from_db(
    table: &'static str,
    kind: &str,
    min: i16,
    max: i16,
) -> Result<CreditRange, StoreError> {
    let min = cents_from_db(table, "credits_min_cents", min)?;
    let max = cents_from_db(table, "credits_max_cents", max)?;
    match kind {
        "fixed" => Ok(CreditRange::Fixed(min)),
        "range" => Ok(CreditRange::Range { min, max }),
        "either" => Ok(CreditRange::Either(min, max)),
        other => Err(corrupt(
            table,
            "credits_kind",
            format!("unknown kind {other:?}"),
        )),
    }
}

/// Unix seconds into a `timestamptz` value.
pub(crate) fn timestamp_to_db(at: Timestamp) -> Result<OffsetDateTime, StoreError> {
    OffsetDateTime::from_unix_timestamp(at.0)
        .map_err(|e| StoreError::Input(format!("timestamp {}: {e}", at.0)))
}

/// A `timestamptz` value into Unix seconds.
pub(crate) fn timestamp_from_db(at: OffsetDateTime) -> Timestamp {
    Timestamp(at.unix_timestamp())
}

/// Attribute codes for a `text[]` column.
pub(crate) fn attributes_to_db(attributes: &BTreeSet<Attribute>) -> Vec<String> {
    attributes.iter().map(|a| a.code().to_owned()).collect()
}

/// A `text[]` column back into attributes.
pub(crate) fn attributes_from_db(
    table: &'static str,
    codes: &[String],
) -> Result<BTreeSet<Attribute>, StoreError> {
    codes
        .iter()
        .map(|code| {
            Attribute::from_code(code)
                .ok_or_else(|| corrupt(table, "attributes", format!("unknown attribute {code:?}")))
        })
        .collect()
}

/// A season's column text.
pub(crate) fn season_to_db(season: Season) -> &'static str {
    match season {
        Season::Fall => "fall",
        Season::Spring => "spring",
        Season::Summer => "summer",
    }
}

/// Column text back into a season.
pub(crate) fn season_from_db(table: &'static str, text: &str) -> Result<Season, StoreError> {
    match text {
        "fall" => Ok(Season::Fall),
        "spring" => Ok(Season::Spring),
        "summer" => Ok(Season::Summer),
        other => Err(corrupt(
            table,
            "season",
            format!("unknown season {other:?}"),
        )),
    }
}

/// Rice's one-letter `EXAM` code; `Unknown` has no code and stores as null.
pub(crate) fn final_exam_to_db(exam: FinalExam) -> Option<&'static str> {
    match exam {
        FinalExam::Scheduled => Some("S"),
        FinalExam::ScheduledDeptRoom => Some("D"),
        FinalExam::ScheduledOnline => Some("O"),
        FinalExam::TakeHome => Some("T"),
        FinalExam::DeptSchedules => Some("G"),
        FinalExam::NoExam => Some("N"),
        FinalExam::Unknown => None,
    }
}

/// The stored code back into the enum; null is `Unknown`.
pub(crate) fn final_exam_from_db(code: Option<&str>) -> FinalExam {
    code.map_or(FinalExam::Unknown, FinalExam::from_code)
}

/// A term code column.
pub(crate) fn term_from_db(table: &'static str, text: &str) -> Result<TermCode, StoreError> {
    TermCode::parse(text).map_err(|e| corrupt(table, "term_code", e))
}

/// A CRN column.
pub(crate) fn crn_from_db(table: &'static str, value: i32) -> Result<Crn, StoreError> {
    u32::try_from(value)
        .map(Crn)
        .map_err(|_| corrupt(table, "crn", format!("negative crn {value}")))
}

/// A CRN into its `integer` column.
pub(crate) fn crn_to_db(crn: Crn) -> Result<i32, StoreError> {
    i32::try_from(crn.0)
        .map_err(|_| StoreError::Input(format!("crn {} does not fit integer", crn.0)))
}

/// Subject and number columns into a code.
pub(crate) fn code_from_db(
    table: &'static str,
    subject: &str,
    number: &str,
) -> Result<CourseCode, StoreError> {
    CourseCode::new(subject, number).map_err(|e| corrupt(table, "subject", e))
}

/// A `u16` count column.
pub(crate) fn u16_from_db(
    table: &'static str,
    column: &'static str,
    value: i32,
) -> Result<u16, StoreError> {
    u16::try_from(value).map_err(|_| corrupt(table, column, format!("{value} is not a u16")))
}

/// A meeting's columns, in both directions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeetingColumns {
    pub day_mask: i16,
    pub start_time: Option<Time>,
    pub end_time: Option<Time>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub raw: String,
}

fn minute_to_time(minute: MinuteOfDay) -> Result<Time, StoreError> {
    let total = minute.get();
    let hour =
        u8::try_from(total / 60).map_err(|_| StoreError::Input(format!("minute {total}")))?;
    let min = u8::try_from(total % 60).map_err(|_| StoreError::Input(format!("minute {total}")))?;
    Time::from_hms(hour, min, 0).map_err(|e| StoreError::Input(format!("minute {total}: {e}")))
}

fn time_to_minute(
    table: &'static str,
    column: &'static str,
    at: Time,
) -> Result<MinuteOfDay, StoreError> {
    let total = u16::from(at.hour()) * 60 + u16::from(at.minute());
    MinuteOfDay::new(total).map_err(|e| corrupt(table, column, e))
}

fn calendar_to_date(date: CalendarDate) -> Result<Date, StoreError> {
    let year = i32::from(date.year);
    let month =
        Month::try_from(date.month).map_err(|e| StoreError::Input(format!("month: {e}")))?;
    Date::from_calendar_date(year, month, date.day)
        .map_err(|e| StoreError::Input(format!("date: {e}")))
}

fn date_to_calendar(
    table: &'static str,
    column: &'static str,
    date: Date,
) -> Result<CalendarDate, StoreError> {
    let year = u16::try_from(date.year())
        .map_err(|_| corrupt(table, column, format!("year {}", date.year())))?;
    Ok(CalendarDate {
        year,
        month: u8::from(date.month()),
        day: date.day(),
    })
}

/// A core meeting into its columns. A timed pattern's `raw` is rendered
/// from the pattern, because core keeps no text for a parsed meeting.
pub(crate) fn meeting_to_db(meeting: &Meeting) -> Result<MeetingColumns, StoreError> {
    let (day_mask, start_time, end_time, raw) = match &meeting.pattern {
        MeetingPattern::Timed(time) => (
            i16::from(time.days.bits()),
            Some(minute_to_time(time.start)?),
            Some(minute_to_time(time.end)?),
            format!(
                "{} {}-{}",
                time.days.to_letters(),
                time.start.get(),
                time.end.get()
            ),
        ),
        MeetingPattern::Unparsed(text) => (0, None, None, text.clone()),
    };
    let (start_date, end_date) = match meeting.dates {
        Some(span) => (
            Some(calendar_to_date(span.start)?),
            Some(calendar_to_date(span.end)?),
        ),
        None => (None, None),
    };
    Ok(MeetingColumns {
        day_mask,
        start_time,
        end_time,
        start_date,
        end_date,
        raw,
    })
}

/// Columns back into a meeting: a start time makes it `Timed`, otherwise
/// `raw` is the unparsed text.
pub(crate) fn meeting_from_db(columns: MeetingColumns) -> Result<Meeting, StoreError> {
    const TABLE: &str = "meetings";
    let pattern = match (columns.start_time, columns.end_time) {
        (Some(start), Some(end)) => {
            let bits = u8::try_from(columns.day_mask)
                .map_err(|_| corrupt(TABLE, "day_mask", columns.day_mask))?;
            MeetingPattern::Timed(MeetingTime {
                days: DaySet::from_bits(bits).map_err(|e| corrupt(TABLE, "day_mask", e))?,
                start: time_to_minute(TABLE, "start_time", start)?,
                end: time_to_minute(TABLE, "end_time", end)?,
            })
        }
        _ => MeetingPattern::Unparsed(columns.raw),
    };
    let dates = match (columns.start_date, columns.end_date) {
        (Some(start), Some(end)) => Some(DateSpan {
            start: date_to_calendar(TABLE, "start_date", start)?,
            end: date_to_calendar(TABLE, "end_date", end)?,
        }),
        _ => None,
    };
    Ok(Meeting { pattern, dates })
}

/// SHA-256 of `bytes`, for session tokens, sign-in codes and fingerprints.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

/// Lowercase hex of a digest.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use core::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            // Writing to a String cannot fail.
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_round_trip() {
        let either = CreditRange::Either(Credits::from_cents(100), Credits::from_cents(300));
        let cols = credits_to_db(either).unwrap();
        assert_eq!(cols.kind, "either");
        assert_eq!(
            credits_from_db("t", cols.kind, cols.min, cols.max).unwrap(),
            either
        );
        assert!(credits_from_db("t", "fixed", -1, 0).is_err());
        assert!(cents_to_db(Credits::from_cents(u16::MAX)).is_err());
    }

    #[test]
    fn meetings_round_trip() {
        let timed = Meeting {
            pattern: MeetingPattern::Timed(MeetingTime {
                days: DaySet::from_letters("TR").unwrap(),
                start: MinuteOfDay::new(870).unwrap(),
                end: MinuteOfDay::new(945).unwrap(),
            }),
            dates: Some(DateSpan {
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
            }),
        };
        let cols = meeting_to_db(&timed).unwrap();
        assert_eq!(cols.day_mask, 0b1010);
        assert_eq!(cols.raw, "TR 870-945");
        assert_eq!(meeting_from_db(cols).unwrap(), timed);
        let unparsed = Meeting {
            pattern: MeetingPattern::Unparsed("TBA".to_owned()),
            dates: None,
        };
        assert_eq!(
            meeting_from_db(meeting_to_db(&unparsed).unwrap()).unwrap(),
            unparsed
        );
    }

    #[test]
    fn final_exam_codes_round_trip() {
        for exam in [
            FinalExam::Scheduled,
            FinalExam::ScheduledDeptRoom,
            FinalExam::ScheduledOnline,
            FinalExam::TakeHome,
            FinalExam::DeptSchedules,
            FinalExam::NoExam,
            FinalExam::Unknown,
        ] {
            assert_eq!(final_exam_from_db(final_exam_to_db(exam)), exam);
        }
    }

    #[test]
    fn hex_is_lowercase() {
        assert_eq!(hex(&[0, 255, 16]), "00ff10");
        assert_eq!(sha256(b"").len(), 32);
    }
}
