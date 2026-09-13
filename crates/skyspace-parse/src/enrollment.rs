//! Live seat counts: `!SWKSCAT.live?action=ENROLLMENT&crn=&term=`.

use serde::Deserialize;
use skyspace_core::Timestamp;
use skyspace_core::catalog::Seats;

use crate::ParseError;

#[derive(Deserialize)]
#[serde(rename = "ENROLLMENT")]
struct EnrollmentDoc {
    #[serde(rename = "@time-now")]
    time_now: String,
    #[serde(rename = "@wait-count")]
    wait_count: u16,
    #[serde(rename = "@wait-capacity")]
    wait_capacity: u16,
    #[serde(rename = "SECTION")]
    section: SectionEl,
}

#[derive(Deserialize)]
struct SectionEl {
    #[serde(rename = "@current-enrolled")]
    current_enrolled: u16,
    #[serde(rename = "@max-enrolled")]
    max_enrolled: u16,
}

/// Four numbers and Rice's own `time-now`, which becomes `Seats::as_of` so
/// the class page quotes Rice's clock, not ours.
///
/// # Errors
/// [`ParseError::Xml`] when the document does not deserialise;
/// [`ParseError::Timestamp`] when `time-now` is not RFC 3339.
pub fn parse_enrollment(xml: &str) -> Result<Seats, ParseError> {
    let doc: EnrollmentDoc = quick_xml::de::from_str(xml)?;
    Ok(Seats {
        enrolled: doc.section.current_enrolled,
        capacity: doc.section.max_enrolled,
        waitlist_count: doc.wait_count,
        waitlist_capacity: doc.wait_capacity,
        as_of: rfc3339_to_unix(&doc.time_now)?,
    })
}

/// `YYYY-MM-DDTHH:MM:SS±HH:MM` (or `Z`, or with fractional seconds) to Unix
/// seconds, by hand: this crate has no `time` dependency. The offset is
/// parsed, never assumed: Rice's moves between `-05:00` and `-06:00`.
///
/// # Errors
/// [`ParseError::Timestamp`] for anything else.
pub fn rfc3339_to_unix(text: &str) -> Result<Timestamp, ParseError> {
    parse_rfc3339(text.trim()).ok_or_else(|| ParseError::Timestamp(text.to_owned()))
}

fn parse_rfc3339(text: &str) -> Option<Timestamp> {
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let offset_at = rest.find(['+', '-', 'Z', 'z'])?;
    let (clock, offset) = rest.split_at(offset_at);
    let clock = clock.split_once('.').map_or(clock, |(whole, _)| whole);
    let mut clock_parts = clock.split(':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.parse().ok()?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let offset_seconds = parse_offset(offset)?;
    let days = days_from_civil(year, month, day);
    let local = days * 86_400 + hour * 3600 + minute * 60 + second;
    Some(Timestamp(local - offset_seconds))
}

/// `±HH:MM` or `Z` to seconds east of UTC.
fn parse_offset(offset: &str) -> Option<i64> {
    if offset.eq_ignore_ascii_case("Z") {
        return Some(0);
    }
    let sign = match offset.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let (hours, minutes) = offset.get(1..)?.split_once(':')?;
    let hours: i64 = hours.parse().ok()?;
    let minutes: i64 = minutes.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

/// Days since 1970-01-01 for a proleptic Gregorian date; Howard Hinnant's
/// `days_from_civil`, which is exact for every year in an `i64`.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rice_time_now_becomes_unix_seconds() {
        assert_eq!(
            rfc3339_to_unix("2026-09-11T19:11:12-05:00").unwrap(),
            Timestamp(1_789_171_872)
        );
        assert_eq!(
            rfc3339_to_unix("1970-01-01T00:00:00Z").unwrap(),
            Timestamp(0)
        );
        assert_eq!(
            rfc3339_to_unix("1970-01-01T00:00:00-06:00").unwrap(),
            Timestamp(6 * 3600)
        );
        assert_eq!(
            rfc3339_to_unix("2000-03-01T00:00:00+00:00").unwrap(),
            Timestamp(951_868_800)
        );
        assert!(rfc3339_to_unix("2026-09-11 19:11").is_err());
        assert!(rfc3339_to_unix("yesterday").is_err());
    }

    #[test]
    fn civil_days_match_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10_957);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }
}
