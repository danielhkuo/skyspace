//! Catalog domain types: meetings, seats, instructors, sections and courses.
//!
//! **Two-level rule for missing data.** `Option` means Rice published no
//! value. A record not fetched is a `None` **record**, not a struct of `None`
//! fields, so "not loaded" never collapses into "has none".

use core::fmt;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Timestamp;
use crate::code::{CourseCode, Crn, SectionNumber};
use crate::prereq::{MutualExclusion, PrereqObservation};
use crate::program::CatalogYear;
use crate::term::{CreditRange, PartOfTermCode, TermCode};

/// Why a meeting value could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeetingError {
    /// A day letter outside `MTWRFSU`.
    #[error("unknown day letter {0:?}")]
    DayLetter(char),
    /// Bit 7 set in a day mask.
    #[error("day bits {0:#010b} set a bit above Sunday")]
    DayBits(u8),
    /// A minute at or past 24:00.
    #[error("minute {0} is past the end of the day")]
    Minute(u16),
}

/// A seven-bit set of weekdays. Monday is bit 0, Sunday is bit 6. Rice writes
/// Thursday `R` and Sunday `U`; the wire form is Rice's letters.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(into = "String", try_from = "String")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct DaySet(u8);

impl DaySet {
    /// Monday.
    pub const MONDAY: Self = Self(1 << 0);
    /// Tuesday.
    pub const TUESDAY: Self = Self(1 << 1);
    /// Wednesday.
    pub const WEDNESDAY: Self = Self(1 << 2);
    /// Thursday, Rice's `R`.
    pub const THURSDAY: Self = Self(1 << 3);
    /// Friday.
    pub const FRIDAY: Self = Self(1 << 4);
    /// Saturday; MUSI 334 002 meets Saturday.
    pub const SATURDAY: Self = Self(1 << 5);
    /// Sunday, Rice's `U`; MUSI 342 002 meets Sunday.
    pub const SUNDAY: Self = Self(1 << 6);

    const LETTERS: [(char, u8); 7] = [
        ('M', 1 << 0),
        ('T', 1 << 1),
        ('W', 1 << 2),
        ('R', 1 << 3),
        ('F', 1 << 4),
        ('S', 1 << 5),
        ('U', 1 << 6),
    ];

    /// Read Rice's letters, in any order: `"TR"`, `"MWF"`.
    ///
    /// # Errors
    /// `MeetingError::DayLetter` for a character outside `MTWRFSU`.
    pub fn from_letters(raw: &str) -> Result<Self, MeetingError> {
        let mut bits = 0u8;
        for letter in raw.trim().chars() {
            let upper = letter.to_ascii_uppercase();
            let bit = Self::LETTERS
                .iter()
                .find(|(l, _)| *l == upper)
                .map(|(_, b)| *b)
                .ok_or(MeetingError::DayLetter(letter))?;
            bits |= bit;
        }
        Ok(Self(bits))
    }

    /// Build from a bit mask.
    ///
    /// # Errors
    /// `MeetingError::DayBits` when bit 7 is set.
    pub const fn from_bits(bits: u8) -> Result<Self, MeetingError> {
        if bits & 0x80 != 0 {
            return Err(MeetingError::DayBits(bits));
        }
        Ok(Self(bits))
    }

    /// The raw mask.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Rice's letters, Monday first.
    #[must_use]
    pub fn to_letters(self) -> String {
        Self::LETTERS
            .iter()
            .filter(|(_, b)| self.0 & b != 0)
            .map(|(l, _)| *l)
            .collect()
    }

    /// The days both sets share.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// True when the sets share a day.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// True for a section with no schedule.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl From<DaySet> for String {
    fn from(days: DaySet) -> Self {
        days.to_letters()
    }
}

impl TryFrom<String> for DaySet {
    type Error = MeetingError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::from_letters(&raw)
    }
}

/// Minutes from midnight, campus local; 870 is 14:30. Wall-clock, never an
/// instant: a weekly pattern holds across a daylight-saving change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "u16", try_from = "u16")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "number"))]
pub struct MinuteOfDay(u16);

impl MinuteOfDay {
    /// The first minute past the end of the day.
    const END_OF_DAY: u16 = 24 * 60;

    /// Build from minutes after midnight.
    ///
    /// # Errors
    /// `MeetingError::Minute` at or past 24:00, which is 1440.
    pub fn new(minutes: u16) -> Result<Self, MeetingError> {
        if minutes >= Self::END_OF_DAY {
            return Err(MeetingError::Minute(minutes));
        }
        Ok(Self(minutes))
    }

    /// Minutes after midnight.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl From<MinuteOfDay> for u16 {
    fn from(minute: MinuteOfDay) -> Self {
        minute.0
    }
}

impl TryFrom<u16> for MinuteOfDay {
    type Error = MeetingError;

    fn try_from(minutes: u16) -> Result<Self, Self::Error> {
        Self::new(minutes)
    }
}

/// A weekly pattern: days and a half-open `[start, end)` time range, so
/// back-to-back classes do not conflict. No validating constructor: the only
/// invalid case (end at or before start) is caught in ingest as
/// `MeetingPattern::Unparsed`, and `overlaps` is false for such a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MeetingTime {
    /// Which weekdays.
    pub days: DaySet,
    /// First minute of the meeting.
    pub start: MinuteOfDay,
    /// First minute after the meeting.
    pub end: MinuteOfDay,
}

impl MeetingTime {
    /// Shares a day and overlaps in time.
    #[must_use]
    pub const fn overlaps(self, other: Self) -> bool {
        self.days.intersects(other.days) && self.start.0 < other.end.0 && other.start.0 < self.end.0
    }
}

/// A calendar date with no timezone and no clock behind it. Field order
/// makes the derived `Ord` chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CalendarDate {
    /// Four-digit year.
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

/// Rice's part of term as dates: full term, first-year writing, or a summer
/// session. Both ends inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DateSpan {
    /// First day.
    pub start: CalendarDate,
    /// Last day.
    pub end: CalendarDate,
}

impl DateSpan {
    /// True when the two closed spans share a day.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// What the listing printed for a meeting. An enum because a listing-only
/// meeting has no date span, and unparsed text is shown, ignored by conflict
/// detection, and never discarded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum MeetingPattern {
    /// Days and times were read.
    Timed(MeetingTime),
    /// Rice's text, verbatim.
    Unparsed(String),
}

/// One weekly meeting of a section.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Meeting {
    /// The weekly pattern.
    pub pattern: MeetingPattern,
    /// `None` only for a listing-sourced meeting the XML pull has not covered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dates: Option<DateSpan>,
}

impl Meeting {
    /// Both timed, patterns overlap, and date spans overlap. Unknown dates
    /// count as overlapping: a false warning beats a missed conflict, because
    /// the product warns and never blocks.
    #[must_use]
    pub fn conflicts_with(&self, other: &Self) -> bool {
        let (MeetingPattern::Timed(a), MeetingPattern::Timed(b)) = (&self.pattern, &other.pattern)
        else {
            return false;
        };
        if !a.overlaps(*b) {
            return false;
        }
        match (self.dates, other.dates) {
            (Some(x), Some(y)) => x.overlaps(y),
            _ => true,
        }
    }
}

/// Rice's final-exam status. Kept apart from weekly meetings so conflict
/// code cannot count one as the other; Rice publishes no exam time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum FinalExam {
    /// `Scheduled Final Exam-OTR Room`.
    Scheduled,
    /// `Scheduled Final Exam-Dept Room`.
    ScheduledDeptRoom,
    /// `Scheduled Online Final Exam`.
    ScheduledOnline,
    /// `Take-Home Exam`.
    TakeHome,
    /// `GR Course-Dept Schedules Exam`.
    DeptSchedules,
    /// `No Final Exam`.
    NoExam,
    /// Anything else, or nothing printed.
    Unknown,
}

impl FinalExam {
    /// The six listing strings from `rice-data.md`; anything else is `Unknown`.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        match label.trim() {
            "Scheduled Final Exam-OTR Room" => Self::Scheduled,
            "Scheduled Final Exam-Dept Room" => Self::ScheduledDeptRoom,
            "Scheduled Online Final Exam" => Self::ScheduledOnline,
            "Take-Home Exam" => Self::TakeHome,
            "GR Course-Dept Schedules Exam" => Self::DeptSchedules,
            "No Final Exam" => Self::NoExam,
            _ => Self::Unknown,
        }
    }

    /// Rice's one-letter `EXAM code` from the section XML.
    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "S" => Self::Scheduled,
            "D" => Self::ScheduledDeptRoom,
            "O" => Self::ScheduledOnline,
            "T" => Self::TakeHome,
            "G" => Self::DeptSchedules,
            "N" => Self::NoExam,
            _ => Self::Unknown,
        }
    }
}

/// Live seat counts from Rice's `ENROLLMENT` feed. No `enrolled <= capacity`
/// invariant: Rice may publish an over-enrolled section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Seats {
    /// `current-enrolled`.
    pub enrolled: u16,
    /// `max-enrolled`.
    pub capacity: u16,
    /// `wait-count`.
    pub waitlist_count: u16,
    /// `wait-capacity`.
    pub waitlist_capacity: u16,
    /// Rice's own `time-now`, shown to the student as freshness.
    pub as_of: Timestamp,
}

/// Below this many open seats a section is "nearly full".
pub const NEARLY_FULL_THRESHOLD: u16 = 5;

impl Seats {
    /// Open seats; saturates, because a hand-written subtraction underflows.
    #[must_use]
    pub const fn open(&self) -> u16 {
        self.capacity.saturating_sub(self.enrolled)
    }

    /// Open waitlist places; saturates.
    #[must_use]
    pub const fn waitlist_open(&self) -> u16 {
        self.waitlist_capacity.saturating_sub(self.waitlist_count)
    }

    /// In core, so the catalog row, the schedule grid and the PDF agree.
    #[must_use]
    pub const fn status(&self) -> SeatStatus {
        let open = self.open();
        if open > NEARLY_FULL_THRESHOLD {
            SeatStatus::Open
        } else if open > 0 {
            SeatStatus::NearlyFull
        } else if self.waitlist_open() > 0 {
            SeatStatus::WaitlistOpen
        } else {
            SeatStatus::Full
        }
    }
}

/// A reserved-seat group from the detail page: `60 (2 Available)`. Rice
/// prints capacity and available, never per-group enrolled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SeatReservation {
    /// Free text: "Fall Semester 2026 Matriculants".
    pub label: String,
    /// Seats held for the group.
    pub capacity: u16,
    /// Seats still open in the group.
    pub available: u16,
}

/// The seat mark. No `Unknown` variant: not knowing is `section.seats == None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SeatStatus {
    /// More than `NEARLY_FULL_THRESHOLD` seats open.
    Open,
    /// One to `NEARLY_FULL_THRESHOLD` seats open.
    NearlyFull,
    /// No seats, but the waitlist has room.
    WaitlistOpen,
    /// No seats and no waitlist room.
    Full,
}

/// Rice's four course attributes, the whole `ATTRS` vocabulary. No
/// `Other(String)` variant: the endpoint is authoritative, so an unknown code
/// means a person must look. FWIS and LPAP are subjects, not attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Attribute {
    /// Analyzing Diversity.
    #[serde(rename = "AD")]
    AnalyzingDiversity,
    /// Distribution Group I.
    #[serde(rename = "GRP1")]
    DistributionOne,
    /// Distribution Group II.
    #[serde(rename = "GRP2")]
    DistributionTwo,
    /// Distribution Group III.
    #[serde(rename = "GRP3")]
    DistributionThree,
}

impl Attribute {
    /// `None` for an unknown code; ingest raises a pull warning.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim() {
            "AD" => Some(Self::AnalyzingDiversity),
            "GRP1" => Some(Self::DistributionOne),
            "GRP2" => Some(Self::DistributionTwo),
            "GRP3" => Some(Self::DistributionThree),
            _ => None,
        }
    }

    /// Rice's code, the wire form.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::AnalyzingDiversity => "AD",
            Self::DistributionOne => "GRP1",
            Self::DistributionTwo => "GRP2",
            Self::DistributionThree => "GRP3",
        }
    }

    /// The distribution group, for the three that are one.
    #[must_use]
    pub const fn distribution_group(self) -> Option<DistributionGroup> {
        match self {
            Self::AnalyzingDiversity => None,
            Self::DistributionOne => Some(DistributionGroup::One),
            Self::DistributionTwo => Some(DistributionGroup::Two),
            Self::DistributionThree => Some(DistributionGroup::Three),
        }
    }
}

/// The three distribution groups, without Analyzing Diversity mixed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum DistributionGroup {
    /// Group I.
    One,
    /// Group II.
    Two,
    /// Group III.
    Three,
}

/// A NetID: a display label and link, never a lookup key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NetId(pub String);

/// An instructor as Rice publishes them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Instructor {
    /// "Last, First M.", the match key.
    pub name: String,
    /// Every observed instructor has one; a section with nobody has an empty
    /// `Vec` of instructors, not a "Staff" row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_id: Option<NetId>,
}

impl Instructor {
    /// Ours, not Rice's. Best effort: two people with one name merge, one
    /// person spelled two ways splits. Label the feature approximate.
    #[must_use]
    pub fn match_key(&self) -> InstructorKey {
        let key: String = self
            .name
            .to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        InstructorKey(key)
    }
}

/// A normalised instructor name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InstructorKey(pub String);

/// Registration restrictions. Shown as text; the engine never evaluates them,
/// because a plan holds no verified level, classification or major.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Restrictions {
    /// Rice's text, always kept.
    pub raw: String,
    /// Best effort; may be empty when `raw` is not.
    pub clauses: Vec<RestrictionClause>,
}

/// One clause of a restriction: "Must be enrolled in one of the following Levels: Graduate".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RestrictionClause {
    /// Must or must not.
    pub effect: RestrictionEffect,
    /// What the clause is about.
    pub dimension: RestrictionDimension,
    /// The listed values.
    pub values: Vec<String>,
}

/// Whether a clause admits or excludes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum RestrictionEffect {
    /// "Must be ...".
    MustBe,
    /// "May not be ...".
    MustNotBe,
}

/// What a restriction clause is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum RestrictionDimension {
    /// Undergraduate or graduate.
    Level,
    /// Freshman, sophomore and so on.
    Classification,
    /// A declared major.
    Major,
    /// A degree program.
    Program,
    /// A residential college.
    College,
    /// Anything else, with Rice's word for it.
    Other(String),
}

/// A course record from `CATALIST`, keyed by academic year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Course {
    /// The General Announcements edition this record is from.
    pub catalog_year: CatalogYear,
    /// The code.
    pub code: CourseCode,
    /// Rice's long title; per-section short titles stay on `SectionListing`.
    pub title: String,
    /// Credit hours as printed.
    pub credits: CreditRange,
    /// Rice's `Department:` line, not the subject code.
    pub department: String,
    /// Distribution Group and Analyzing Diversity as printed for this year.
    pub attributes: BTreeSet<Attribute>,
    /// `Grade Mode:`, when printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade_mode: Option<GradeMode>,
    /// `Course Type:`, when printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_type: Option<CourseType>,
    /// `Restrictions:`, when printed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restrictions: Option<Restrictions>,
    /// `Prerequisite(s):`; `None` means Rice printed no field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerequisites: Option<PrereqObservation>,
    /// The description, sentences and all.
    pub description: String,
    /// The fixed sentences at the end of the description.
    pub flags: CourseFlags,
    /// "Mutually Exclusive:" sentences, one per sentence.
    pub mutual_exclusions: Vec<MutualExclusion>,
    /// "Cross-list: ECON 307." feeds the alias table.
    pub cross_list: Vec<CourseCode>,
    /// "Graduate/Undergraduate Equivalency: CHEM 584."
    pub equivalents: Vec<CourseCode>,
}

/// Fixed-form trailing sentences in a description. Prefix-matched; absence
/// is false, never unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseFlags {
    /// "Repeatable for Credit."
    pub repeatable: bool,
    /// "Instructor Permission Required."
    pub instructor_permission: bool,
    /// "Expected to be taught 2nd half of the term."
    pub second_half: bool,
}

/// Rice's grade mode label. A newtype until the first catalog pull reports
/// the distinct values across all 106 subjects.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GradeMode(pub String);

/// Rice's method-of-instruction label. Same story as `GradeMode`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MethodOfInstruction(pub String);

/// Rice's course-type label. Same story as `GradeMode`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseType(pub String);

/// An additional fee from the detail page.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Fee {
    /// What the fee is for.
    pub label: String,
    /// The amount in cents, when it could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_cents: Option<u32>,
    /// Rice's text.
    pub raw: String,
}

/// One row of the subject listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SectionListing {
    /// The CRN, meaningful only with `term`.
    pub crn: Crn,
    /// The term.
    pub term: TermCode,
    /// The course.
    pub code: CourseCode,
    /// The section number.
    pub section: SectionNumber,
    /// Short title; differs between sections (MUSI 531 has nine).
    pub title: String,
    /// Differs between sections (MUSI 649: `1 TO 3`, `3`, `2`).
    pub credits: CreditRange,
    /// Part of term; differs between sections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of_term: Option<PartOfTermCode>,
    /// Rice's order; empty for 7% of MUSI rows; up to seven.
    pub instructors: Vec<Instructor>,
    /// Empty means no schedule; two or more is a lecture plus lab.
    pub meetings: Vec<Meeting>,
    /// Final exam status.
    pub final_exam: FinalExam,
}

impl SectionListing {
    /// Any meeting with parsed days and times. Only 42% of Fall 2026
    /// sections qualify, and the catalog hides the rest by default.
    #[must_use]
    pub fn is_scheduled(&self) -> bool {
        self.meetings
            .iter()
            .any(|m| matches!(m.pattern, MeetingPattern::Timed(_)))
    }
}

/// The detail page, pulled weekly. Absent as a whole until the page is
/// pulled; optional fields inside are absent when Rice printed nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SectionDetail {
    /// `Long Title:`.
    pub long_title: String,
    /// `Description:`.
    pub description: String,
    /// `Department:`.
    pub department: String,
    /// `Distribution Group:` and `Analyzing Diversity:`; empty is normal.
    pub attributes: BTreeSet<Attribute>,
    /// The section page's simplified form; the expression lives on `Course`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerequisites_text: Option<String>,
    /// `Restrictions:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restrictions: Option<Restrictions>,
    /// `Grade Mode:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade_mode: Option<GradeMode>,
    /// `Method of Instruction:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method_of_instruction: Option<MethodOfInstruction>,
    /// `Course Type:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_type: Option<CourseType>,
    /// `Language of Instruction:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Reserved-seat groups.
    pub reserved: Vec<SeatReservation>,
    /// Additional fees.
    pub fees: Vec<Fee>,
    /// The page's syllabus div is JS-filled; ingest asks `.info?action=SYLLABUS`.
    pub has_syllabus: bool,
    /// The page carries no time, so the caller passes it in.
    pub fetched_at: Timestamp,
}

impl SectionDetail {
    /// The distribution group, if the section carries one.
    #[must_use]
    pub fn distribution_group(&self) -> Option<DistributionGroup> {
        self.attributes.iter().find_map(|a| a.distribution_group())
    }
}

/// A section: the listing, plus what the two slower sources add when pulled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Section {
    /// Always present.
    pub listing: SectionListing,
    /// `None` = detail page not pulled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<SectionDetail>,
    /// `None` = CRN not polled; rendered as "not polled", never zero seats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<Seats>,
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minute(m: u16) -> MinuteOfDay {
        MinuteOfDay::new(m).unwrap()
    }

    fn timed(days: &str, start: u16, end: u16) -> Meeting {
        Meeting {
            pattern: MeetingPattern::Timed(MeetingTime {
                days: DaySet::from_letters(days).unwrap(),
                start: minute(start),
                end: minute(end),
            }),
            dates: None,
        }
    }

    fn span(start: (u16, u8, u8), end: (u16, u8, u8)) -> DateSpan {
        DateSpan {
            start: CalendarDate {
                year: start.0,
                month: start.1,
                day: start.2,
            },
            end: CalendarDate {
                year: end.0,
                month: end.1,
                day: end.2,
            },
        }
    }

    #[test]
    fn day_sets_read_and_print_rice_letters() {
        let tr = DaySet::from_letters("TR").unwrap();
        assert_eq!(
            tr,
            DaySet::TUESDAY.intersection(tr) | DaySet::THURSDAY.intersection(tr)
        );
        assert_eq!(DaySet::from_letters("rt").unwrap().to_letters(), "TR");
        assert_eq!(DaySet::from_letters("MWF").unwrap().to_letters(), "MWF");
        assert_eq!(DaySet::from_letters("U").unwrap(), DaySet::SUNDAY);
        assert_eq!(DaySet::from_letters("X"), Err(MeetingError::DayLetter('X')));
        assert_eq!(DaySet::from_bits(0x80), Err(MeetingError::DayBits(0x80)));
        assert!(DaySet::default().is_empty());
        assert_eq!(
            serde_json::to_string(&DaySet::from_letters("MWF").unwrap()).unwrap(),
            "\"MWF\""
        );
    }

    impl core::ops::BitOr for DaySet {
        type Output = Self;
        fn bitor(self, rhs: Self) -> Self {
            Self(self.0 | rhs.0)
        }
    }

    #[test]
    fn back_to_back_classes_do_not_conflict() {
        let first = timed("MWF", 9 * 60, 10 * 60);
        let second = timed("MWF", 10 * 60, 11 * 60);
        assert!(!first.conflicts_with(&second));
        let overlapping = timed("WF", 9 * 60 + 30, 10 * 60 + 30);
        assert!(first.conflicts_with(&overlapping));
        let other_days = timed("TR", 9 * 60, 10 * 60);
        assert!(!first.conflicts_with(&other_days));
    }

    #[test]
    fn separate_summer_sessions_do_not_conflict() {
        let mut a = timed("TR", 870, 945);
        let mut b = timed("TR", 870, 945);
        a.dates = Some(span((2026, 6, 1), (2026, 6, 26)));
        b.dates = Some(span((2026, 7, 6), (2026, 7, 31)));
        assert!(!a.conflicts_with(&b));
        b.dates = Some(span((2026, 6, 26), (2026, 7, 31)));
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn unknown_dates_conflict() {
        let mut a = timed("TR", 870, 945);
        a.dates = Some(span((2026, 6, 1), (2026, 6, 26)));
        let b = timed("TR", 870, 945);
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn unparsed_meetings_never_conflict() {
        let a = Meeting {
            pattern: MeetingPattern::Unparsed("TBA".to_owned()),
            dates: None,
        };
        let b = timed("MTWRF", 0, 1439);
        assert!(!a.conflicts_with(&b));
        assert!(!a.conflicts_with(&a));
    }

    #[test]
    fn minute_of_day_rejects_past_midnight() {
        assert!(MinuteOfDay::new(1439).is_ok());
        assert_eq!(MinuteOfDay::new(1440), Err(MeetingError::Minute(1440)));
        assert!(serde_json::from_str::<MinuteOfDay>("1441").is_err());
    }

    #[test]
    fn seat_status_grades_from_the_same_numbers() {
        let base = Seats {
            enrolled: 60,
            capacity: 72,
            waitlist_count: 0,
            waitlist_capacity: 0,
            as_of: Timestamp(0),
        };
        assert_eq!(base.status(), SeatStatus::Open);
        assert_eq!(
            Seats {
                enrolled: 67,
                ..base
            }
            .status(),
            SeatStatus::NearlyFull
        );
        assert_eq!(
            Seats {
                enrolled: 72,
                ..base
            }
            .status(),
            SeatStatus::Full
        );
        assert_eq!(
            Seats {
                enrolled: 72,
                waitlist_capacity: 20,
                ..base
            }
            .status(),
            SeatStatus::WaitlistOpen
        );
        assert_eq!(
            Seats {
                enrolled: 75,
                ..base
            }
            .open(),
            0
        );
    }

    #[test]
    fn unknown_attribute_code_is_none() {
        assert_eq!(Attribute::from_code("GRP4"), None);
        assert_eq!(
            Attribute::from_code("GRP3"),
            Some(Attribute::DistributionThree)
        );
        assert_eq!(
            serde_json::to_string(&Attribute::DistributionOne).unwrap(),
            "\"GRP1\""
        );
        assert_eq!(Attribute::AnalyzingDiversity.distribution_group(), None);
    }

    #[test]
    fn final_exam_labels_map() {
        assert_eq!(FinalExam::from_label("No Final Exam"), FinalExam::NoExam);
        assert_eq!(
            FinalExam::from_label(" Take-Home Exam "),
            FinalExam::TakeHome
        );
        assert_eq!(FinalExam::from_label("Something new"), FinalExam::Unknown);
        assert_eq!(FinalExam::from_code("S"), FinalExam::Scheduled);
    }

    #[test]
    fn instructor_match_key_normalises() {
        let a = Instructor {
            name: "Tran, Lesa".to_owned(),
            net_id: None,
        };
        let b = Instructor {
            name: "TRAN,  Lesa ".to_owned(),
            net_id: Some(NetId("lesa".to_owned())),
        };
        assert_eq!(a.match_key(), b.match_key());
    }

    #[test]
    fn missing_detail_is_not_missing_group() {
        let listing = SectionListing {
            crn: Crn(12422),
            term: TermCode::parse("202710").unwrap(),
            code: CourseCode::parse("COMP 318").unwrap(),
            section: SectionNumber("001".to_owned()),
            title: "CONCURRENT PROGRAM DESIGN".to_owned(),
            credits: CreditRange::Fixed(crate::term::Credits::from_cents(400)),
            part_of_term: None,
            instructors: vec![],
            meetings: vec![],
            final_exam: FinalExam::Unknown,
        };
        let not_loaded = Section {
            listing: listing.clone(),
            detail: None,
            seats: None,
        };
        let no_group = Section {
            listing,
            detail: Some(SectionDetail {
                long_title: String::new(),
                description: String::new(),
                department: "Computer Science".to_owned(),
                attributes: BTreeSet::new(),
                prerequisites_text: None,
                restrictions: None,
                grade_mode: None,
                method_of_instruction: None,
                course_type: None,
                language: None,
                reserved: vec![],
                fees: vec![],
                has_syllabus: false,
                fetched_at: Timestamp(0),
            }),
            seats: None,
        };
        assert!(not_loaded.detail.is_none());
        assert_eq!(
            no_group
                .detail
                .as_ref()
                .and_then(SectionDetail::distribution_group),
            None
        );
        assert_ne!(not_loaded, no_group);
        let json = serde_json::to_string(&not_loaded).unwrap();
        assert!(!json.contains("\"detail\""));
    }

    #[test]
    fn payload_enums_round_trip() {
        let pattern = MeetingPattern::Unparsed("TBA".to_owned());
        let json = serde_json::to_string(&pattern).unwrap();
        assert_eq!(json, r#"{"kind":"unparsed","value":"TBA"}"#);
        assert_eq!(
            serde_json::from_str::<MeetingPattern>(&json).unwrap(),
            pattern
        );
        let dimension = RestrictionDimension::Other("Campus".to_owned());
        let json = serde_json::to_string(&dimension).unwrap();
        assert_eq!(
            serde_json::from_str::<RestrictionDimension>(&json).unwrap(),
            dimension
        );
        let timed = timed("TR", 870, 945);
        let json = serde_json::to_string(&timed).unwrap();
        assert_eq!(serde_json::from_str::<Meeting>(&json).unwrap(), timed);
    }
}
