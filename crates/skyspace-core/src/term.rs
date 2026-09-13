//! Terms, seasons, credit hours and board positions.
//!
//! `TermCode` is Rice's data key (`202710`); `TermPosition` orders the plan
//! board and exists for terms Rice has not published yet. Credit hours are
//! hundredths of a credit hour everywhere, in a `u16`, so sums are exact and
//! identical in wasm and on the server.

use core::fmt;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::code::CodeError;

/// Why a term code could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TermError {
    /// The text is not six ASCII digits.
    #[error("term code must be six digits, got {0:?}")]
    NotSixDigits(String),
    /// The academic year is outside `TermCode::MIN_YEAR..=TermCode::MAX_YEAR`.
    #[error("academic year {0} is out of range")]
    Year(u16),
    /// The season code is not one of the seven Rice publishes.
    #[error("season code {0} is out of range")]
    Season(u8),
}

/// Rice's term code: academic year, then a two-digit season code.
///
/// Fall 2026 is `202710`, academic year 2027. Field order matters: the
/// derived `Ord` is academic year, then raw season code, which is start-date
/// order within a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct TermCode {
    year: u16,
    season: u8,
}

impl TermCode {
    /// Lowest academic year accepted.
    pub const MIN_YEAR: u16 = 2000;
    /// Highest academic year accepted.
    pub const MAX_YEAR: u16 = 2099;
    /// Season code for the fall semester.
    pub const FALL: u8 = 10;
    /// Season code for the spring semester.
    pub const SPRING: u8 = 20;
    /// Season code for the summer semester.
    pub const SUMMER: u8 = 30;
    /// The four MBA quadmester codes, which never reach a board.
    const QUADMESTERS: [u8; 4] = [5, 11, 15, 25];

    /// Build a code from its parts.
    ///
    /// # Errors
    /// `TermError::Year` outside the accepted range; `TermError::Season` for a
    /// code that is not one of the seven Rice publishes.
    pub fn new(year: u16, season: u8) -> Result<Self, TermError> {
        if !(Self::MIN_YEAR..=Self::MAX_YEAR).contains(&year) {
            return Err(TermError::Year(year));
        }
        let known = season == Self::FALL
            || season == Self::SPRING
            || season == Self::SUMMER
            || Self::QUADMESTERS.contains(&season);
        if !known {
            return Err(TermError::Season(season));
        }
        Ok(Self { year, season })
    }

    /// Read a six-digit code such as `202710`.
    ///
    /// # Errors
    /// `TermError::NotSixDigits`, then whatever [`TermCode::new`] returns.
    pub fn parse(raw: &str) -> Result<Self, TermError> {
        let raw = raw.trim();
        if raw.len() != 6 || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return Err(TermError::NotSixDigits(raw.to_owned()));
        }
        let (year, season) = raw.split_at(4);
        let year: u16 = year
            .parse()
            .map_err(|_| TermError::NotSixDigits(raw.to_owned()))?;
        let season: u8 = season
            .parse()
            .map_err(|_| TermError::NotSixDigits(raw.to_owned()))?;
        Self::new(year, season)
    }

    /// The academic year: Fall 2026 is 2027.
    #[must_use]
    pub const fn academic_year(self) -> u16 {
        self.year
    }

    /// The General Announcements edition in force: Fall 2026 (academic
    /// year 2027) is governed by the 2026-27 announcements, `CatalogYear(2026)`.
    /// The catalog of course records (`CATALIST`) is keyed by the academic
    /// year instead; the two differ by one and must not be confused.
    #[must_use]
    pub const fn catalog_year(self) -> u16 {
        self.year - 1
    }

    /// The raw two-digit season code.
    #[must_use]
    pub const fn season_code(self) -> u8 {
        self.season
    }

    /// The board season, for the three semester codes; `None` for a quadmester.
    #[must_use]
    pub const fn season(self) -> Option<Season> {
        match self.season {
            Self::FALL => Some(Season::Fall),
            Self::SPRING => Some(Season::Spring),
            Self::SUMMER => Some(Season::Summer),
            _ => None,
        }
    }

    /// True for the four MBA quadmester codes.
    #[must_use]
    pub const fn is_quadmester(self) -> bool {
        matches!(self.season, 5 | 11 | 15 | 25)
    }

    /// The calendar year the term starts in: codes up to `11` are the year
    /// before the academic year, codes from `15` are the academic year.
    #[must_use]
    pub const fn calendar_year(self) -> u16 {
        if self.season <= 11 {
            self.year.saturating_sub(1)
        } else {
            self.year
        }
    }

    /// The board position, for the three semester codes.
    #[must_use]
    pub fn position(self) -> Option<TermPosition> {
        self.season().map(|season| TermPosition {
            academic_year: self.year,
            season,
        })
    }

    /// `year * 100 + season`, for archive tables. Both casts widen.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        (self.year as u32) * 100 + (self.season as u32)
    }
}

impl fmt::Display for TermCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}{:02}", self.year, self.season)
    }
}

impl From<TermCode> for String {
    fn from(code: TermCode) -> Self {
        code.to_string()
    }
}

impl TryFrom<String> for TermCode {
    type Error = TermError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

/// A board season. **Variant order is load-bearing:** `TermPosition` derives
/// `Ord` on (academic year, season), so reordering silently reorders every
/// plan board. `season_order_is_academic_year_order` is the only guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Season {
    /// Fall semester, season code `10`.
    Fall,
    /// Spring semester, season code `20`.
    Spring,
    /// Summer semester, season code `30`.
    Summer,
}

/// One term as Rice publishes it in the `TERMS` and `SESSIONS` lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Term {
    /// Rice's code.
    pub code: TermCode,
    /// Rice's own `<OPT>` text, "Fall Semester 2026". Never synthesised.
    pub label: String,
    /// From `SESSIONS`: part-of-term code to label. No dates.
    pub parts: BTreeMap<PartOfTermCode, String>,
    /// When the list was pulled; Rice supplies no timestamp here.
    pub pulled_at: Timestamp,
}

/// Rice's part-of-term code. A newtype over text, not an enum: the `SESSIONS`
/// vocabulary is per term and summer sessions differ between terms.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PartOfTermCode(pub String);

/// Hundredths of a credit hour. `from_cents(300)` is three hours. The field is
/// private so a raw number is not read as whole hours.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Credits(u16);

impl Credits {
    /// Zero credit hours, a real value (recitals) and the "unknown" default.
    pub const ZERO: Self = Self(0);

    /// Build from hundredths.
    #[must_use]
    pub const fn from_cents(cents: u16) -> Self {
        Self(cents)
    }

    /// The value in hundredths.
    #[must_use]
    pub const fn cents(self) -> u16 {
        self.0
    }

    /// The only sum. Saturates: a manual card can hold any number.
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    /// `"(minimum of 8 semesters)"`: one rule, eight slots, eight times the hours.
    #[must_use]
    pub const fn saturating_mul(self, times: u8) -> Self {
        Self(self.0.saturating_mul(times as u16))
    }

    /// Read Rice's spellings: `3`, `1.5`, `.75` (no leading zero), `0`. At
    /// most two decimals.
    ///
    /// # Errors
    /// `CodeError::Credits` for anything else.
    pub fn parse(raw: &str) -> Result<Self, CodeError> {
        let text = raw.trim();
        let bad = || CodeError::Credits(raw.to_owned());
        if text.is_empty() {
            return Err(bad());
        }
        let (whole, fraction) = match text.split_once('.') {
            Some((w, f)) => (w, f),
            None => (text, ""),
        };
        if whole.is_empty() && fraction.is_empty() {
            return Err(bad());
        }
        if !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 2
        {
            return Err(bad());
        }
        let whole: u16 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| bad())?
        };
        let cents: u16 = match fraction.len() {
            0 => 0,
            1 => fraction.parse::<u16>().map_err(|_| bad())? * 10,
            _ => fraction.parse().map_err(|_| bad())?,
        };
        let total = whole
            .checked_mul(100)
            .and_then(|w| w.checked_add(cents))
            .ok_or_else(bad)?;
        Ok(Self(total))
    }
}

impl fmt::Display for Credits {
    /// Rice's own spellings: `300` prints `3`, `75` prints `.75`, `150` prints `1.5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 100;
        let cents = self.0 % 100;
        if cents == 0 {
            return write!(f, "{whole}");
        }
        let fraction = if cents.is_multiple_of(10) {
            format!(".{}", cents / 10)
        } else {
            format!(".{cents:02}")
        };
        if whole == 0 {
            write!(f, "{fraction}")
        } else {
            write!(f, "{whole}{fraction}")
        }
    }
}

/// What a section or rule prints for credit hours. `Either` is two discrete
/// values, not a range: `1 OR 3` never means 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CreditRange {
    /// One value: `3`.
    Fixed(Credits),
    /// `1 TO 4`, or GA's `3-4`.
    Range {
        /// Lower bound, inclusive.
        min: Credits,
        /// Upper bound, inclusive.
        max: Credits,
    },
    /// `1 OR 3`: exactly one of the two.
    Either(Credits, Credits),
}

impl CreditRange {
    /// Read Rice's `3`, `1 TO 4`, `1 OR 3`, `.75 TO 3` and GA's `3`, `3-4`,
    /// `1 or 3`. Operators are case-insensitive.
    ///
    /// # Errors
    /// `CodeError::Credits` when neither side reads as credits.
    pub fn parse(raw: &str) -> Result<Self, CodeError> {
        let text = raw.trim();
        let upper = text.to_ascii_uppercase();
        if let Some((a, b)) = upper.split_once(" TO ") {
            return Ok(Self::Range {
                min: Credits::parse(a)?,
                max: Credits::parse(b)?,
            });
        }
        if let Some((a, b)) = upper.split_once(" OR ") {
            return Ok(Self::Either(Credits::parse(a)?, Credits::parse(b)?));
        }
        if let Some((a, b)) = upper.split_once('-') {
            return Ok(Self::Range {
                min: Credits::parse(a)?,
                max: Credits::parse(b)?,
            });
        }
        Ok(Self::Fixed(Credits::parse(text)?))
    }

    /// The smallest value the range allows.
    #[must_use]
    pub const fn min(self) -> Credits {
        match self {
            Self::Fixed(c) => c,
            Self::Range { min, .. } => min,
            Self::Either(a, b) => {
                if a.0 <= b.0 {
                    a
                } else {
                    b
                }
            }
        }
    }

    /// The largest value the range allows.
    #[must_use]
    pub const fn max(self) -> Credits {
        match self {
            Self::Fixed(c) => c,
            Self::Range { max, .. } => max,
            Self::Either(a, b) => {
                if a.0 >= b.0 {
                    a
                } else {
                    b
                }
            }
        }
    }
}

impl fmt::Display for CreditRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(c) => write!(f, "{c}"),
            Self::Range { min, max } => write!(f, "{min} TO {max}"),
            Self::Either(a, b) => write!(f, "{a} OR {b}"),
        }
    }
}

/// A column on the plan board. `TermCode` addresses Rice's data; this orders
/// the board, because an `Away` or `Off` term holds a slot and has no code.
/// Derived `Ord` compares `academic_year` then `season`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TermPosition {
    /// Fall 2026 is academic year 2027.
    pub academic_year: u16,
    /// Which semester of that year.
    pub season: Season,
}

impl TermPosition {
    /// Fall subtracts one from `academic_year` (saturating); spring and summer do not.
    #[must_use]
    pub fn calendar_year(self) -> u16 {
        match self.season {
            Season::Fall => self.academic_year.saturating_sub(1),
            Season::Spring | Season::Summer => self.academic_year,
        }
    }
}

/// When a course was taken, for prerequisite checks. `Incoming` sorts first,
/// so AP and transfer credit clears a prerequisite in any term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Taken {
    /// Before matriculation: transfer, AP or IB credit.
    Incoming,
    /// In a term on the board.
    Term(TermPosition),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_code_parses_fall() {
        let code = TermCode::parse("202710").unwrap();
        assert_eq!(code.academic_year(), 2027);
        assert_eq!(code.season(), Some(Season::Fall));
        assert_eq!(code.calendar_year(), 2026);
        assert_eq!(code.to_string(), "202710");
    }

    #[test]
    fn term_code_parses_spring_and_summer() {
        let spring = TermCode::parse("202720").unwrap();
        assert_eq!(spring.season(), Some(Season::Spring));
        assert_eq!(spring.calendar_year(), 2027);
        let summer = TermCode::parse("202730").unwrap();
        assert_eq!(summer.season(), Some(Season::Summer));
        assert_eq!(summer.calendar_year(), 2027);
    }

    #[test]
    fn quadmester_has_no_position() {
        let code = TermCode::parse("202705").unwrap();
        assert_eq!(code.season(), None);
        assert_eq!(code.position(), None);
        assert!(code.is_quadmester());
        assert_eq!(code.calendar_year(), 2026);
    }

    #[test]
    fn term_code_round_trips() {
        for season in [5, 10, 11, 15, 20, 25, 30] {
            let code = TermCode::new(2027, season).unwrap();
            assert_eq!(TermCode::parse(&code.to_string()), Ok(code));
        }
        assert!(TermCode::new(2027, 20).is_ok());
        assert_eq!(TermCode::new(2027, 12), Err(TermError::Season(12)));
        assert_eq!(
            TermCode::parse("20271"),
            Err(TermError::NotSixDigits("20271".to_owned()))
        );
        assert_eq!(TermCode::new(1999, 10), Err(TermError::Year(1999)));
    }

    #[test]
    fn term_code_orders_by_start_date() {
        let fall = TermCode::parse("202710").unwrap();
        let spring = TermCode::parse("202720").unwrap();
        let prior_summer = TermCode::parse("202630").unwrap();
        assert!(prior_summer < fall);
        assert!(fall < spring);
    }

    #[test]
    fn term_code_rejects_bad_json() {
        assert!(serde_json::from_str::<TermCode>("\"2027\"").is_err());
        let ok: TermCode = serde_json::from_str("\"202710\"").unwrap();
        assert_eq!(serde_json::to_string(&ok).unwrap(), "\"202710\"");
    }

    #[test]
    fn credits_parse_rice_spellings() {
        assert_eq!(Credits::parse("3"), Ok(Credits::from_cents(300)));
        assert_eq!(Credits::parse("1.5"), Ok(Credits::from_cents(150)));
        assert_eq!(Credits::parse(".75"), Ok(Credits::from_cents(75)));
        assert_eq!(Credits::parse("0"), Ok(Credits::ZERO));
        assert_eq!(Credits::parse("2.5"), Ok(Credits::from_cents(250)));
        assert!(Credits::parse("1.234").is_err());
        assert!(Credits::parse("").is_err());
        assert!(Credits::parse("three").is_err());
        assert_eq!(
            CreditRange::parse("1 TO 4"),
            Ok(CreditRange::Range {
                min: Credits::from_cents(100),
                max: Credits::from_cents(400)
            })
        );
        assert_eq!(
            CreditRange::parse("1 OR 3"),
            Ok(CreditRange::Either(
                Credits::from_cents(100),
                Credits::from_cents(300)
            ))
        );
        assert_eq!(
            CreditRange::parse(".75 TO 3"),
            Ok(CreditRange::Range {
                min: Credits::from_cents(75),
                max: Credits::from_cents(300)
            })
        );
        assert_eq!(
            CreditRange::parse("3-4"),
            Ok(CreditRange::Range {
                min: Credits::from_cents(300),
                max: Credits::from_cents(400)
            })
        );
        assert_eq!(
            CreditRange::parse("1 or 3"),
            Ok(CreditRange::Either(
                Credits::from_cents(100),
                Credits::from_cents(300)
            ))
        );
        let either = CreditRange::Either(Credits::from_cents(300), Credits::from_cents(100));
        assert_eq!(either.min(), Credits::from_cents(100));
        assert_eq!(either.max(), Credits::from_cents(300));
    }

    #[test]
    fn credits_display_uses_rice_spellings() {
        assert_eq!(Credits::from_cents(300).to_string(), "3");
        assert_eq!(Credits::from_cents(75).to_string(), ".75");
        assert_eq!(Credits::from_cents(150).to_string(), "1.5");
        assert_eq!(Credits::from_cents(0).to_string(), "0");
        assert_eq!(Credits::from_cents(1234).to_string(), "12.34");
    }

    #[test]
    fn credits_saturate() {
        let big = Credits::from_cents(u16::MAX);
        assert_eq!(big.saturating_add(Credits::from_cents(1)), big);
        assert_eq!(big.saturating_mul(8), big);
        assert_eq!(
            Credits::from_cents(300).saturating_mul(8),
            Credits::from_cents(2400)
        );
    }

    #[test]
    fn season_order_is_academic_year_order() {
        assert!(Season::Fall < Season::Spring);
        assert!(Season::Spring < Season::Summer);
        let fall_2026 = TermPosition {
            academic_year: 2027,
            season: Season::Fall,
        };
        let spring_2027 = TermPosition {
            academic_year: 2027,
            season: Season::Spring,
        };
        assert!(fall_2026 < spring_2027);
        assert_eq!(fall_2026.calendar_year(), 2026);
        assert_eq!(spring_2027.calendar_year(), 2027);
        assert!(Taken::Incoming < Taken::Term(fall_2026));
    }

    #[test]
    fn credit_range_round_trips_through_json() {
        let range = CreditRange::Either(Credits::from_cents(100), Credits::from_cents(300));
        let json = serde_json::to_string(&range).unwrap();
        assert_eq!(json, r#"{"kind":"either","value":[100,300]}"#);
        assert_eq!(serde_json::from_str::<CreditRange>(&json).unwrap(), range);
    }
}
