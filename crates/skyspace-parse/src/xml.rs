//! Structured section XML: `!SWKSCAT.info?action=ASSOCIATED-SECTIONS&crn=&term=`.
//! The one intermediate type this crate owns: the feed carries no title, so
//! the store merges it into the listing row by CRN.

use std::collections::BTreeSet;

use serde::Deserialize;
use skyspace_core::catalog::{
    Attribute, DateSpan, DaySet, FinalExam, Instructor, Meeting, MeetingPattern, MeetingTime, NetId,
};
use skyspace_core::term::{CreditRange, Credits, PartOfTermCode};
use skyspace_core::{CourseCode, Crn, SectionNumber, TermCode};

use crate::text::{clean, credit_range, hhmm_minutes, rice_date};
use crate::{IssueCode, ParseError, ParseReport, Parsed};

/// One `<COURSE>` of the feed: what the listing lacks, keyed by CRN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionXml {
    /// The CRN, meaningful only with the term.
    pub crn: Crn,
    /// The course.
    pub code: CourseCode,
    /// The section number.
    pub section: SectionNumber,
    /// The part-of-term **code**, `1` or `SA1`; the listing has only labels.
    pub part_of_term: Option<PartOfTermCode>,
    /// `SCHOOL` text.
    pub school: Option<String>,
    /// From the one-letter `EXAM` code.
    pub final_exam: FinalExam,
    /// `SCHED` text: `Lecture/Laboratory`.
    pub schedule_type: Option<String>,
    /// From `CREDITS`'s `low`/`high` attributes, or its text.
    pub credits: CreditRange,
    /// Every `NAME`, with its `NETID`.
    pub instructors: Vec<Instructor>,
    /// Every `DIST` code the closed vocabulary knows.
    pub attributes: BTreeSet<Attribute>,
    /// Every class `MEETING`, with dates.
    pub meetings: Vec<Meeting>,
}

#[derive(Deserialize)]
#[serde(rename = "ASSOCIATED-SECTIONS")]
struct AssocDoc {
    #[serde(rename = "@term")]
    term: Option<String>,
    #[serde(rename = "ADDITIONAL-SECTIONS")]
    additional: Option<GroupEl>,
    #[serde(rename = "CROSSLIST-SECTIONS")]
    crosslist: Option<GroupEl>,
    #[serde(rename = "COREQ-SECTIONS")]
    coreq: Option<GroupEl>,
}

#[derive(Deserialize)]
struct GroupEl {
    #[serde(rename = "COURSES")]
    courses: Option<CoursesEl>,
}

#[derive(Deserialize)]
struct CoursesEl {
    #[serde(rename = "COURSE", default)]
    course: Vec<CourseEl>,
}

#[derive(Deserialize)]
struct CourseEl {
    #[serde(rename = "@crn")]
    crn: Option<String>,
    #[serde(rename = "@subj-code")]
    subject: Option<String>,
    #[serde(rename = "@crse-numb")]
    number: Option<String>,
    #[serde(rename = "@seq-numb")]
    section: Option<String>,
    #[serde(rename = "SESSION")]
    session: Option<Coded>,
    #[serde(rename = "SCHOOL")]
    school: Option<Coded>,
    #[serde(rename = "EXAM")]
    exam: Option<Coded>,
    #[serde(rename = "SCHED")]
    sched: Option<Coded>,
    #[serde(rename = "CREDITS")]
    credits: Option<CreditsEl>,
    #[serde(rename = "INSTRUCTORS")]
    instructors: Option<InstructorsEl>,
    #[serde(rename = "DISTS")]
    dists: Option<DistsEl>,
    #[serde(rename = "TIMES")]
    times: Option<TimesEl>,
}

#[derive(Deserialize)]
struct Coded {
    #[serde(rename = "@code")]
    code: Option<String>,
    #[serde(rename = "$text")]
    text: Option<String>,
}

#[derive(Deserialize)]
struct CreditsEl {
    #[serde(rename = "@low")]
    low: Option<String>,
    #[serde(rename = "@high")]
    high: Option<String>,
    #[serde(rename = "$text")]
    text: Option<String>,
}

#[derive(Deserialize)]
struct InstructorsEl {
    #[serde(rename = "NAME", default)]
    name: Vec<NameEl>,
}

#[derive(Deserialize)]
struct NameEl {
    #[serde(rename = "@NETID")]
    net_id: Option<String>,
    #[serde(rename = "$text")]
    text: Option<String>,
}

#[derive(Deserialize)]
struct DistsEl {
    #[serde(rename = "DIST", default)]
    dist: Vec<Coded>,
}

#[derive(Deserialize)]
struct TimesEl {
    #[serde(rename = "MEETING", default)]
    meeting: Vec<MeetingEl>,
}

#[derive(Deserialize)]
struct MeetingEl {
    #[serde(rename = "@begin-time")]
    begin_time: Option<String>,
    #[serde(rename = "@end-time")]
    end_time: Option<String>,
    #[serde(rename = "@begin-date")]
    begin_date: Option<String>,
    #[serde(rename = "@end-date")]
    end_date: Option<String>,
    #[serde(rename = "TYPE")]
    kind: Option<Coded>,
    #[serde(rename = "MON_DAY")]
    mon: Option<String>,
    #[serde(rename = "TUE_DAY")]
    tue: Option<String>,
    #[serde(rename = "WED_DAY")]
    wed: Option<String>,
    #[serde(rename = "THU_DAY")]
    thu: Option<String>,
    #[serde(rename = "FRI_DAY")]
    fri: Option<String>,
    #[serde(rename = "SAT_DAY")]
    sat: Option<String>,
    #[serde(rename = "SUN_DAY")]
    sun: Option<String>,
}

/// Every `<COURSE>` in the feed, across the additional, cross-listed and
/// corequisite groups. The feed lists the sections *associated with* the
/// requested CRN, so the requested CRN itself may be absent.
///
/// # Errors
/// [`ParseError::Xml`] when the document does not deserialise;
/// [`ParseError::WrongTerm`] when the root's `term` differs from `term`.
pub fn parse_associated_sections(
    xml: &str,
    term: TermCode,
) -> Result<Parsed<Vec<SectionXml>>, ParseError> {
    let doc: AssocDoc = quick_xml::de::from_str(xml)?;
    if let Some(got) = &doc.term
        && got.trim() != term.to_string()
    {
        return Err(ParseError::WrongTerm {
            wanted: term,
            got: got.clone(),
        });
    }
    let mut report = ParseReport::default();
    let mut sections = Vec::new();
    let groups = [doc.additional, doc.crosslist, doc.coreq];
    for course in groups
        .into_iter()
        .flatten()
        .filter_map(|group| group.courses)
        .flat_map(|courses| courses.course)
    {
        report.saw_row();
        if let Some(section) = convert(course, term, &mut report) {
            report.kept_row();
            sections.push(section);
        }
    }
    Ok(Parsed {
        value: sections,
        report,
    })
}

fn convert(course: CourseEl, term: TermCode, report: &mut ParseReport) -> Option<SectionXml> {
    let crn_text = course.crn.clone().unwrap_or_default();
    let Ok(crn) = crn_text.trim().parse::<u32>() else {
        report.issue(
            IssueCode::RowMissingKey,
            format!("{term}: crn {crn_text:?}"),
        );
        return None;
    };
    let at = format!("{term} crn {crn}");
    let code = CourseCode::new(
        course.subject.as_deref().unwrap_or_default(),
        course.number.as_deref().unwrap_or_default(),
    );
    let Ok(code) = code else {
        report.issue(
            IssueCode::RowMissingKey,
            format!("{at}: code {:?} {:?}", course.subject, course.number),
        );
        return None;
    };
    let credits = credits(course.credits.as_ref(), report, &at)?;
    report.filled("credits");
    let part_of_term = course
        .session
        .as_ref()
        .and_then(|s| s.code.clone())
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
        .map(PartOfTermCode);
    if part_of_term.is_some() {
        report.filled("part_of_term");
    }
    let exam_code = course
        .exam
        .as_ref()
        .and_then(|e| e.code.clone())
        .unwrap_or_default();
    let final_exam = FinalExam::from_code(&exam_code);
    if !exam_code.trim().is_empty() {
        report.filled("final_exam");
        if final_exam == FinalExam::Unknown {
            report.issue(IssueCode::UnknownFinalExam, format!("{at}: {exam_code:?}"));
        }
    }
    let mut attributes = BTreeSet::new();
    for dist in course.dists.iter().flat_map(|d| &d.dist) {
        let code = dist.code.clone().unwrap_or_default();
        match Attribute::from_code(&code) {
            Some(attribute) => {
                attributes.insert(attribute);
            }
            None => report.issue(IssueCode::UnknownAttribute, format!("{at}: {code:?}")),
        }
    }
    if !attributes.is_empty() {
        report.filled("attributes");
    }
    let instructors: Vec<Instructor> = course
        .instructors
        .iter()
        .flat_map(|i| &i.name)
        .filter_map(|name| {
            let text = clean(name.text.as_deref().unwrap_or_default());
            (!text.is_empty()).then(|| Instructor {
                name: text,
                net_id: name
                    .net_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(|id| NetId(id.to_owned())),
            })
        })
        .collect();
    if !instructors.is_empty() {
        report.filled("instructors");
    }
    let meetings: Vec<Meeting> = course
        .times
        .iter()
        .flat_map(|t| &t.meeting)
        .filter(|m| !is_exam(m))
        .map(|m| meeting(m, report, &at))
        .collect();
    if !meetings.is_empty() {
        report.filled("meetings");
    }
    Some(SectionXml {
        crn: Crn(crn),
        code,
        section: SectionNumber(course.section.unwrap_or_default().trim().to_owned()),
        part_of_term,
        school: course.school.and_then(|s| s.text).map(|t| clean(&t)),
        final_exam,
        schedule_type: course.sched.and_then(|s| s.text).map(|t| clean(&t)),
        credits,
        instructors,
        attributes,
        meetings,
    })
}

/// `low`/`high` attributes first; the text (`3`, `1 TO 4`) when they are missing.
fn credits(el: Option<&CreditsEl>, report: &mut ParseReport, at: &str) -> Option<CreditRange> {
    let el = el?;
    let low = el.low.as_deref().and_then(|l| Credits::parse(l).ok());
    let high = el.high.as_deref().and_then(|h| Credits::parse(h).ok());
    match (low, high) {
        (Some(min), Some(max)) if min != max => Some(CreditRange::Range { min, max }),
        _ => match el.text.as_deref().map(clean) {
            Some(text) if !text.is_empty() => credit_range(&text, report, at),
            _ => low.map(CreditRange::Fixed),
        },
    }
}

fn is_exam(m: &MeetingEl) -> bool {
    m.kind
        .as_ref()
        .and_then(|k| k.code.as_deref())
        .is_some_and(|code| code.trim().eq_ignore_ascii_case("FINL"))
}

fn meeting(m: &MeetingEl, report: &mut ParseReport, at: &str) -> Meeting {
    let mut letters = String::new();
    for (present, letter) in [
        (&m.mon, 'M'),
        (&m.tue, 'T'),
        (&m.wed, 'W'),
        (&m.thu, 'R'),
        (&m.fri, 'F'),
        (&m.sat, 'S'),
        (&m.sun, 'U'),
    ] {
        if present.is_some() {
            letters.push(letter);
        }
    }
    let begin = m.begin_time.as_deref().unwrap_or_default();
    let end = m.end_time.as_deref().unwrap_or_default();
    let dates = match (
        m.begin_date.as_deref().and_then(rice_date),
        m.end_date.as_deref().and_then(rice_date),
    ) {
        (Some(start), Some(end)) => Some(DateSpan { start, end }),
        _ => None,
    };
    let timed = match (
        hhmm_minutes(begin),
        hhmm_minutes(end),
        DaySet::from_letters(&letters),
    ) {
        (Some(start), Some(end), Ok(days)) if start < end && !days.is_empty() => {
            Some(MeetingTime { days, start, end })
        }
        _ => None,
    };
    let pattern = if let Some(time) = timed {
        MeetingPattern::Timed(time)
    } else {
        let text = clean(&format!("{begin} - {end} {letters}"));
        report.issue(IssueCode::UnreadableMeeting, format!("{at}: {text:?}"));
        MeetingPattern::Unparsed(text)
    };
    Meeting { pattern, dates }
}
