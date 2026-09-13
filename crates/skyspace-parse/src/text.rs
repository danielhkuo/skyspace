//! Text helpers shared by the HTML parsers: whitespace, codes, dates, times.

use regex::Regex;
use scraper::ElementRef;
use skyspace_core::CourseCode;
use skyspace_core::catalog::{CalendarDate, DaySet, MeetingTime, MinuteOfDay};
use skyspace_core::code::CodeError;
use skyspace_core::term::{CreditRange, Credits};

use crate::{IssueCode, ParseError, ParseReport};

/// Build a regex from a pattern in our own source. A bad pattern is a bug of
/// the same kind as a bad selector, so it reports as one.
///
/// # Errors
/// [`ParseError::BadSelector`] when the pattern does not compile.
pub(crate) fn re(pattern: &'static str) -> Result<Regex, ParseError> {
    Regex::new(pattern).map_err(|_| ParseError::BadSelector { selector: pattern })
}

/// Collapse runs of whitespace, including NBSP, to one space and trim.
pub(crate) fn clean(raw: &str) -> String {
    raw.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every text node under the element, cleaned.
pub(crate) fn element_text(element: ElementRef<'_>) -> String {
    clean(&element.text().collect::<String>())
}

/// Read credit hours as Rice or GA print them. Text with more than two
/// decimals is rounded to hundredths and reported, because the core type
/// holds hundredths and dropping the row would lose a real section.
pub(crate) fn credit_range(raw: &str, report: &mut ParseReport, at: &str) -> Option<CreditRange> {
    let text = clean(raw);
    if text.is_empty() {
        return None;
    }
    match CreditRange::parse(&text) {
        Ok(range) => Some(range),
        Err(CodeError::Credits(_)) => {
            let rounded = round_to_cents(&text);
            match CreditRange::parse(&rounded) {
                Ok(range) if rounded != text => {
                    report.issue(IssueCode::SubCentCredits, format!("{at}: {text:?}"));
                    Some(range)
                }
                _ => {
                    report.issue(IssueCode::RowMissingKey, format!("{at}: credits {text:?}"));
                    None
                }
            }
        }
        Err(_) => {
            report.issue(IssueCode::RowMissingKey, format!("{at}: credits {text:?}"));
            None
        }
    }
}

/// `1.234` becomes `1.23`; everything else is returned as it was.
fn round_to_cents(text: &str) -> String {
    text.split(' ')
        .map(|word| match word.split_once('.') {
            Some((whole, fraction))
                if fraction.len() > 2 && fraction.bytes().all(|b| b.is_ascii_digit()) =>
            {
                format!("{whole}.{}", &fraction[..2])
            }
            _ => word.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whole credit hours from a total such as `120`; a range keeps its minimum.
pub(crate) fn credits_min(raw: &str) -> Option<Credits> {
    CreditRange::parse(&clean(raw)).ok().map(CreditRange::min)
}

/// `3:00PM - 3:50PM MWF` as a weekly pattern; `None` for anything else,
/// including an end at or before the start.
///
/// # Errors
/// [`ParseError::BadSelector`] if the pattern in this file is broken.
pub(crate) fn meeting_time(text: &str) -> Result<Option<MeetingTime>, ParseError> {
    let pattern =
        re(r"^(\d{1,2}):(\d{2})\s*([AP]M)\s*-\s*(\d{1,2}):(\d{2})\s*([AP]M)\s+([MTWRFSU]+)$")?;
    let cleaned = clean(text);
    let Some(caps) = pattern.captures(&cleaned) else {
        return Ok(None);
    };
    let field = |i: usize| caps.get(i).map(|m| m.as_str()).unwrap_or_default();
    let start = clock_minutes(field(1), field(2), field(3));
    let end = clock_minutes(field(4), field(5), field(6));
    let (Some(start), Some(end)) = (start, end) else {
        return Ok(None);
    };
    if end <= start {
        return Ok(None);
    }
    let Ok(days) = DaySet::from_letters(field(7)) else {
        return Ok(None);
    };
    let (Ok(start), Ok(end)) = (MinuteOfDay::new(start), MinuteOfDay::new(end)) else {
        return Ok(None);
    };
    Ok(Some(MeetingTime { days, start, end }))
}

/// Twelve-hour clock parts to minutes after midnight.
fn clock_minutes(hour: &str, minute: &str, meridiem: &str) -> Option<u16> {
    let hour: u16 = hour.parse().ok()?;
    let minute: u16 = minute.parse().ok()?;
    if !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }
    let hour = match (meridiem, hour) {
        ("AM", 12) => 0,
        ("PM", 12) => 12,
        ("PM", h) => h + 12,
        (_, h) => h,
    };
    Some(hour * 60 + minute)
}

/// `HHMM` from the section XML to minutes after midnight.
pub(crate) fn hhmm_minutes(text: &str) -> Option<MinuteOfDay> {
    let text = text.trim();
    if text.len() != 4 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (hours, minutes) = text.split_at(2);
    let hours: u16 = hours.parse().ok()?;
    let minutes: u16 = minutes.parse().ok()?;
    if minutes > 59 {
        return None;
    }
    MinuteOfDay::new(hours * 60 + minutes).ok()
}

/// `24-AUG-2026` or `4-DEC-2026` as a calendar date.
pub(crate) fn rice_date(text: &str) -> Option<CalendarDate> {
    let mut parts = text.trim().split('-');
    let day: u8 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let year: u16 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || day == 0 || day > 31 {
        return None;
    }
    Some(CalendarDate { year, month, day })
}

fn month_number(name: &str) -> Option<u8> {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let upper = name.to_ascii_uppercase();
    MONTHS
        .iter()
        .position(|m| *m == upper)
        .and_then(|i| u8::try_from(i + 1).ok())
}

/// Codes from a comma-, slash- or space-separated run such as
/// `COMP 404/COMP 504` or `COMP 427, COMP 541`. Stops at the first word pair
/// that is not a code, so a trailing sentence is left behind. Returns the
/// codes read and whether every word was consumed.
pub(crate) fn code_run(text: &str) -> (Vec<CourseCode>, bool) {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '/' || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect();
    let mut codes = Vec::new();
    let mut at = 0;
    while at + 1 < words.len() {
        let (subject, number) = (words[at], words[at + 1].trim_end_matches('.'));
        match CourseCode::new(subject, number) {
            Ok(code) => {
                codes.push(code);
                at += 2;
            }
            Err(_) => break,
        }
    }
    (codes, at == words.len())
}
