//! Subjects, course numbers, course codes, CRNs and section numbers.
//!
//! Validated newtypes have a private field, a constructor that can reject,
//! and `into`/`try_from` serde so the wire cannot build a value the
//! constructor would refuse. Label newtypes are `transparent`.

use core::cmp::Ordering;
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// The only error type this crate exposes to most callers: evaluation cannot
/// fail, reading a course code from text can.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    /// Nothing to read.
    #[error("course code is empty")]
    Empty,
    /// Not two to eight uppercase ASCII letters.
    #[error("bad subject code {0:?}")]
    Subject(String),
    /// A subject with nothing after it.
    #[error("course code {0:?} has no number")]
    MissingNumber(String),
    /// Not one to four digits followed by at most two uppercase letters.
    #[error("bad course number {0:?}")]
    CourseNumber(String),
    /// Not one of Rice's credit-hour spellings.
    #[error("bad credit hours {0:?}")]
    Credits(String),
}

/// A subject code such as `COMP`. Two to eight uppercase ASCII letters.
///
/// Every one of the 106 codes in the 2026-27 catalog is exactly four, but the
/// rule stays loose: the closed `SUBJECTS` list lives in ingest, which reports
/// a code outside it as a pull warning. A tighter rule here would reject a
/// real row.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct Subject(String);

impl Subject {
    /// Validate and normalise to uppercase.
    ///
    /// # Errors
    /// `CodeError::Subject` when the text is not two to eight ASCII letters.
    pub fn new(raw: &str) -> Result<Self, CodeError> {
        let text = raw.trim();
        let ok = (2..=8).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphabetic());
        if !ok {
            return Err(CodeError::Subject(raw.to_owned()));
        }
        Ok(Self(text.to_ascii_uppercase()))
    }

    /// The code as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Subject> for String {
    fn from(subject: Subject) -> Self {
        subject.0
    }
}

impl TryFrom<String> for Subject {
    type Error = CodeError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

/// A course number such as `140`: one to four leading ASCII digits, then at
/// most two uppercase letters. All 6,480 catalog numbers are three bare
/// digits; letters appear in section numbers, never here. Loose on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct CourseNumber(String);

impl CourseNumber {
    /// Validate and normalise to uppercase.
    ///
    /// # Errors
    /// `CodeError::CourseNumber` when the shape is wrong.
    pub fn new(raw: &str) -> Result<Self, CodeError> {
        let text = raw.trim();
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        let letters = text.len() - digits;
        let ok = (1..=4).contains(&digits)
            && letters <= 2
            && text.bytes().skip(digits).all(|b| b.is_ascii_alphabetic());
        if !ok {
            return Err(CodeError::CourseNumber(raw.to_owned()));
        }
        Ok(Self(text.to_ascii_uppercase()))
    }

    /// The number as text, letters included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The leading digits. Total and panic-free: `new` guarantees one to
    /// four digits, which always fit a `u16`.
    #[must_use]
    pub fn numeric(&self) -> u16 {
        self.0
            .bytes()
            .take_while(u8::is_ascii_digit)
            .fold(0u16, |acc, b| {
                acc.saturating_mul(10).saturating_add(u16::from(b - b'0'))
            })
    }

    /// `numeric() / 100 * 100`: the catalog level filter and range selectors.
    #[must_use]
    pub fn level(&self) -> u16 {
        self.numeric() / 100 * 100
    }
}

impl fmt::Display for CourseNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<CourseNumber> for String {
    fn from(number: CourseNumber) -> Self {
        number.0
    }
}

impl TryFrom<String> for CourseNumber {
    type Error = CodeError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(&raw)
    }
}

/// A course identified the way Rice writes it, for example `COMP 140`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseCode {
    /// Subject code, for example `COMP`.
    pub subject: Subject,
    /// Course number, for example `140`.
    pub number: CourseNumber,
}

impl CourseCode {
    /// Build a code from validated parts.
    ///
    /// # Errors
    /// Whatever [`Subject::new`] or [`CourseNumber::new`] returns.
    pub fn new(subject: &str, number: &str) -> Result<Self, CodeError> {
        Ok(Self {
            subject: Subject::new(subject)?,
            number: CourseNumber::new(number)?,
        })
    }

    /// Read `COMP 140`, `comp140`, `COMP-140` or `COMP\u{a0}140`.
    ///
    /// # Errors
    /// `CodeError::Empty`, `Subject`, `MissingNumber` or `CourseNumber`.
    pub fn parse(raw: &str) -> Result<Self, CodeError> {
        let text = raw.trim();
        if text.is_empty() {
            return Err(CodeError::Empty);
        }
        let letters = text.chars().take_while(char::is_ascii_alphabetic).count();
        let (subject, rest) = text.split_at(letters);
        let subject = Subject::new(subject)?;
        let rest = rest.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '\u{a0}');
        if rest.is_empty() {
            return Err(CodeError::MissingNumber(raw.to_owned()));
        }
        Ok(Self {
            subject,
            number: CourseNumber::new(rest)?,
        })
    }
}

/// Subject, then the numeric part, then the text: `COMP 140` sorts before
/// `COMP 1000`. The derived version would compare text. `Ord` is required
/// because `CourseFacts` keys a `BTreeMap` on it.
impl Ord for CourseCode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.subject
            .cmp(&other.subject)
            .then_with(|| self.number.numeric().cmp(&other.number.numeric()))
            .then_with(|| self.number.as_str().cmp(other.number.as_str()))
    }
}

impl PartialOrd for CourseCode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for CourseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.subject, self.number)
    }
}

impl FromStr for CourseCode {
    type Err = CodeError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw)
    }
}

/// Rice's course reference number. Not validated: a CRN is a join key, and
/// rejecting an unverified shape drops a real row. Only meaningful with its
/// term, because Rice reuses CRNs across terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Crn(pub u32);

impl fmt::Display for Crn {
    /// Five digits, so a leading zero survives the integer column.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:05}", self.0)
    }
}

/// A section number: always three characters, with letters in position one
/// or three (`001`, `901`, `S01`, `0F1`). Text, not a number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SectionNumber(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    fn code(raw: &str) -> CourseCode {
        CourseCode::parse(raw).unwrap()
    }

    #[test]
    fn course_codes_parse_every_spelling() {
        for raw in [
            "COMP 140",
            "comp140",
            "COMP-140",
            "COMP\u{a0}140",
            " comp 140 ",
        ] {
            assert_eq!(code(raw).to_string(), "COMP 140", "{raw:?}");
        }
        assert_eq!(code("MATH 105").number.numeric(), 105);
        assert_eq!(code("COMP 140").number.level(), 100);
        assert_eq!(code("COMP 382").number.level(), 300);
        assert_eq!(code("FWIS 100").number.level(), 100);
    }

    #[test]
    fn course_codes_reject_bad_shapes() {
        assert_eq!(CourseCode::parse(""), Err(CodeError::Empty));
        assert!(matches!(
            CourseCode::parse("COMP"),
            Err(CodeError::MissingNumber(_))
        ));
        assert!(matches!(
            CourseCode::parse("C 140"),
            Err(CodeError::Subject(_))
        ));
        assert!(matches!(
            CourseCode::parse("COMP 14000"),
            Err(CodeError::CourseNumber(_))
        ));
        assert!(matches!(
            CourseCode::parse("COMP 140ABC"),
            Err(CodeError::CourseNumber(_))
        ));
        assert!(matches!(
            CourseCode::parse("COMP ABC"),
            Err(CodeError::CourseNumber(_))
        ));
    }

    #[test]
    fn course_codes_sort_numerically() {
        assert!(code("COMP 140") < code("COMP 1000"));
        assert!(code("COMP 140") < code("COMP 182"));
        assert!(code("COMP 999") < code("ECON 100"));
        assert!(code("MUSI 251") < code("MUSI 251A"));
    }

    #[test]
    fn crn_display_keeps_leading_zero() {
        assert_eq!(Crn(123).to_string(), "00123");
        assert_eq!(Crn(12422).to_string(), "12422");
    }

    #[test]
    fn validated_newtypes_reject_from_json() {
        assert!(serde_json::from_str::<Subject>("\"comp1\"").is_err());
        assert!(serde_json::from_str::<CourseNumber>("\"abc\"").is_err());
        let ok: CourseCode = serde_json::from_str(r#"{"subject":"COMP","number":"140"}"#).unwrap();
        assert_eq!(ok, code("COMP 140"));
        assert_eq!(
            serde_json::to_string(&ok).unwrap(),
            r#"{"subject":"COMP","number":"140"}"#
        );
    }
}
