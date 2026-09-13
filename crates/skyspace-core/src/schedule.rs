//! One term's section choices during registration, and conflict detection.
//! Separate from a plan, works without an account.

use serde::{Deserialize, Serialize};

use crate::catalog::{DaySet, Meeting, MeetingPattern};
use crate::code::{CourseCode, Crn};
use crate::plan::{BusyId, ScheduleId};
use crate::term::{PartOfTermCode, TermCode};

/// A schedule: candidate courses, picked sections, busy blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TermSchedule {
    /// Minted in the browser: guests build schedules before any sign-in.
    pub id: ScheduleId,
    /// Display name.
    pub name: String,
    /// Which term.
    pub term: TermCode,
    /// No credit cap: thirty hours of options is the point.
    pub candidates: Vec<Candidate>,
    /// Personal commitments.
    pub busy: Vec<BusyBlock>,
}

/// A course under consideration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Candidate {
    /// The course.
    pub course: CourseCode,
    /// Empty = none picked; a lecture plus required lab is two entries.
    pub sections: Vec<Crn>,
    /// Hide to test a combination without deleting.
    pub visible: bool,
    /// Stored by the server, never interpreted.
    pub colour: u8,
}

/// A personal commitment on the grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BusyBlock {
    /// Minted in the browser.
    pub id: BusyId,
    /// What it is.
    pub label: String,
    /// Always `MeetingPattern::Timed`, but the type is shared with sections.
    pub meeting: Meeting,
}

/// Who a meeting belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum MeetingOwner {
    /// A section, by CRN.
    Section(Crn),
    /// A busy block.
    Busy(BusyId),
}

/// One meeting with its owner. The caller resolves CRNs to meetings from the
/// catalog, so this crate does no lookups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScheduledMeeting {
    /// Whose.
    pub owner: MeetingOwner,
    /// When.
    pub meeting: Meeting,
    /// Same part of term = same dates, so time alone decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_of_term: Option<PartOfTermCode>,
}

/// Two meetings that overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Conflict {
    /// The earlier meeting in input order.
    pub a: MeetingOwner,
    /// The later one.
    pub b: MeetingOwner,
    /// The days they share.
    pub days: DaySet,
}

/// Two meetings conflict when `Meeting::conflicts_with` says so, except that
/// two with different known parts of term and unknown dates do not: summer
/// sessions do not overlap.
fn conflict_days(a: &ScheduledMeeting, b: &ScheduledMeeting) -> Option<DaySet> {
    let (MeetingPattern::Timed(ta), MeetingPattern::Timed(tb)) =
        (&a.meeting.pattern, &b.meeting.pattern)
    else {
        return None;
    };
    let dates_unknown = a.meeting.dates.is_none() || b.meeting.dates.is_none();
    if dates_unknown
        && let (Some(pa), Some(pb)) = (&a.part_of_term, &b.part_of_term)
        && pa != pb
    {
        return None;
    }
    if !a.meeting.conflicts_with(&b.meeting) {
        return None;
    }
    Some(ta.days.intersection(tb.days))
}

/// Every pair of meetings that overlaps, each pair once, in input order, so
/// the result is deterministic. A section's own lecture and lab never
/// conflict with each other. Allocates only the returned vector.
#[must_use]
pub fn find_conflicts(meetings: &[ScheduledMeeting]) -> Vec<Conflict> {
    let mut out = Vec::new();
    for (i, a) in meetings.iter().enumerate() {
        for b in meetings.iter().skip(i + 1) {
            if a.owner == b.owner {
                continue;
            }
            if let Some(days) = conflict_days(a, b) {
                out.push(Conflict {
                    a: a.owner,
                    b: b.owner,
                    days,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CalendarDate, DateSpan, MeetingTime, MinuteOfDay};

    fn meeting(crn: u32, days: &str, start: u16, end: u16) -> ScheduledMeeting {
        ScheduledMeeting {
            owner: MeetingOwner::Section(Crn(crn)),
            meeting: Meeting {
                pattern: MeetingPattern::Timed(MeetingTime {
                    days: DaySet::from_letters(days).unwrap(),
                    start: MinuteOfDay::new(start).unwrap(),
                    end: MinuteOfDay::new(end).unwrap(),
                }),
                dates: None,
            },
            part_of_term: None,
        }
    }

    fn span(start_month: u8, end_month: u8) -> DateSpan {
        DateSpan {
            start: CalendarDate {
                year: 2026,
                month: start_month,
                day: 1,
            },
            end: CalendarDate {
                year: 2026,
                month: end_month,
                day: 28,
            },
        }
    }

    #[test]
    fn touching_times_do_not_conflict() {
        let a = meeting(1, "TR", 565, 640);
        let b = meeting(2, "TR", 640, 715);
        assert!(find_conflicts(&[a, b]).is_empty());
    }

    #[test]
    fn overlapping_times_conflict_once_per_pair() {
        let a = meeting(1, "MWF", 600, 650);
        let b = meeting(2, "WF", 630, 700);
        let c = meeting(3, "TR", 600, 650);
        let conflicts = find_conflicts(&[a, b, c]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].a, MeetingOwner::Section(Crn(1)));
        assert_eq!(conflicts[0].b, MeetingOwner::Section(Crn(2)));
        assert_eq!(conflicts[0].days.to_letters(), "WF");
    }

    #[test]
    fn a_sections_own_meetings_never_conflict() {
        let lecture = meeting(1, "MWF", 900, 950);
        let lab = meeting(1, "MWF", 930, 1000);
        assert!(find_conflicts(&[lecture, lab]).is_empty());
    }

    #[test]
    fn summer_sessions_do_not_conflict() {
        let mut a = meeting(1, "TR", 870, 945);
        let mut b = meeting(2, "TR", 870, 945);
        a.meeting.dates = Some(span(6, 6));
        b.meeting.dates = Some(span(7, 7));
        assert!(find_conflicts(&[a.clone(), b.clone()]).is_empty());
        // Unknown dates but different known parts of term: still no conflict.
        a.meeting.dates = None;
        b.meeting.dates = None;
        a.part_of_term = Some(PartOfTermCode("SA1".to_owned()));
        b.part_of_term = Some(PartOfTermCode("SB1".to_owned()));
        assert!(find_conflicts(&[a.clone(), b.clone()]).is_empty());
        // Same part of term, unknown dates: time alone decides.
        b.part_of_term = Some(PartOfTermCode("SA1".to_owned()));
        assert_eq!(find_conflicts(&[a, b]).len(), 1);
    }

    #[test]
    fn busy_blocks_conflict_with_sections() {
        let section = meeting(1, "M", 600, 650);
        let busy = ScheduledMeeting {
            owner: MeetingOwner::Busy(BusyId(uuid::Uuid::nil())),
            ..meeting(0, "M", 640, 700)
        };
        let conflicts = find_conflicts(&[section, busy]);
        assert_eq!(conflicts.len(), 1);
        assert!(matches!(conflicts[0].b, MeetingOwner::Busy(_)));
    }
}
