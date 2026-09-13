//! The subject listing: `!SWKSCAT.cat?p_action=QUERY&p_term=&p_subj=`.

use scraper::{ElementRef, Html};
use skyspace_core::catalog::{
    FinalExam, Instructor, Meeting, MeetingPattern, NetId, SectionListing,
};
use skyspace_core::term::PartOfTermCode;
use skyspace_core::{CourseCode, Crn, SectionNumber, TermCode};

use crate::text::{clean, credit_range, element_text, meeting_time};
use crate::{IssueCode, ParseError, ParseReport, Parsed, sel};

const DOCUMENT: &str = "subject listing";

/// Every section row of one subject listing.
///
/// The `Course Schedule - <label>` header is checked against `term` first,
/// because Rice answers an unknown term code with the current term's rows.
/// A row whose meeting text cannot be read is kept with
/// `MeetingPattern::Unparsed`; a row without a CRN or code is dropped and
/// reported.
///
/// # Errors
/// [`ParseError::WrongTerm`] when the header names another term;
/// [`ParseError::SelectorMissing`] when the page has no header or no
/// `td.cls-crn` cell, which is what the search form looks like.
pub fn parse_subject_listing(
    html: &str,
    term: TermCode,
) -> Result<Parsed<Vec<SectionListing>>, ParseError> {
    let doc = Html::parse_document(html);
    check_header(&doc, term)?;
    let crn_cell = sel("td.cls-crn")?;
    if doc.select(&crn_cell).next().is_none() {
        return Err(ParseError::SelectorMissing {
            selector: "td.cls-crn",
            document: DOCUMENT,
        });
    }
    let row = sel("tr")?;
    let mut report = ParseReport::default();
    let mut rows = Vec::new();
    for tr in doc.select(&row) {
        if tr.select(&crn_cell).next().is_none() {
            continue;
        }
        report.saw_row();
        if let Some(listing) = parse_row(tr, term, &mut report)? {
            report.kept_row();
            rows.push(listing);
        }
    }
    Ok(Parsed {
        value: rows,
        report,
    })
}

/// The season word and calendar year in `Course Schedule - Fall Semester 2026`
/// must match the requested code.
fn check_header(doc: &Html, term: TermCode) -> Result<(), ParseError> {
    let heading = sel("h2")?;
    let label = doc
        .select(&heading)
        .map(element_text)
        .find_map(|text| text.strip_prefix("Course Schedule - ").map(str::to_owned))
        .ok_or(ParseError::SelectorMissing {
            selector: "h2",
            document: DOCUMENT,
        })?;
    let wrong = || ParseError::WrongTerm {
        wanted: term,
        got: label.clone(),
    };
    let words: Vec<&str> = label.split(' ').collect();
    let (Some(season), Some(year)) = (words.first(), words.last()) else {
        return Err(wrong());
    };
    let year: u16 = year.parse().map_err(|_| wrong())?;
    let quadmester = label.contains("Quadmester");
    let expected = season_word(term);
    if !season.eq_ignore_ascii_case(expected)
        || year != term.calendar_year()
        || quadmester != term.is_quadmester()
    {
        return Err(wrong());
    }
    Ok(())
}

/// Rice's label word for each of the seven season codes.
fn season_word(term: TermCode) -> &'static str {
    match term.season_code() {
        5 | 30 => "Summer",
        10 | 11 => "Fall",
        15 => "Winter",
        _ => "Spring",
    }
}

fn parse_row(
    tr: ElementRef<'_>,
    term: TermCode,
    report: &mut ParseReport,
) -> Result<Option<SectionListing>, ParseError> {
    let cell = |css: &'static str| -> Result<String, ParseError> {
        Ok(tr
            .select(&sel(css)?)
            .next()
            .map(element_text)
            .unwrap_or_default())
    };
    let crn_text = cell("td.cls-crn")?;
    let Ok(crn) = crn_text.parse::<u32>() else {
        report.issue(IssueCode::RowMissingKey, format!("crn {crn_text:?}"));
        return Ok(None);
    };
    let at = format!("{term} crn {crn}");
    let course_text = cell("td.cls-crs")?;
    let Some((code, section)) = course_and_section(&course_text) else {
        report.issue(
            IssueCode::RowMissingKey,
            format!("{at}: course {course_text:?}"),
        );
        return Ok(None);
    };
    let credits_text = cell("td.cls-crd")?;
    let Some(credits) = credit_range(&credits_text, report, &at) else {
        return Ok(None);
    };
    report.filled("credits");
    let title = cell("td.cls-ttl")?;
    if !title.is_empty() {
        report.filled("title");
    }
    let part_of_term = Some(cell("td.cls-ses")?)
        .filter(|label| !label.is_empty())
        .map(PartOfTermCode);
    if part_of_term.is_some() {
        report.filled("part_of_term");
    }
    let instructors = instructors(tr)?;
    if !instructors.is_empty() {
        report.filled("instructors");
    }
    let meetings = meetings(tr, report, &at)?;
    if !meetings.is_empty() {
        report.filled("meetings");
    }
    let exam_text = cell("div.mtg-finl")?;
    let final_exam = FinalExam::from_label(&exam_text);
    if !exam_text.is_empty() {
        report.filled("final_exam");
        if final_exam == FinalExam::Unknown {
            report.issue(IssueCode::UnknownFinalExam, format!("{at}: {exam_text:?}"));
        }
    }
    Ok(Some(SectionListing {
        crn: Crn(crn),
        term,
        code,
        section,
        title,
        credits,
        part_of_term,
        instructors,
        meetings,
        final_exam,
    }))
}

/// `COMP 140 001` as a code and a section number.
fn course_and_section(text: &str) -> Option<(CourseCode, SectionNumber)> {
    let words: Vec<&str> = text.split(' ').collect();
    let [subject, number, section] = words.as_slice() else {
        return None;
    };
    let code = CourseCode::new(subject, number).ok()?;
    Some((code, SectionNumber((*section).to_owned())))
}

/// One `<div>` per instructor; the NetID rides in the link's `p_netid`
/// query parameter (or, if Rice ever moves it, a `p_netid` attribute).
fn instructors(tr: ElementRef<'_>) -> Result<Vec<Instructor>, ParseError> {
    let entry = sel("td.cls-ins div")?;
    let link = sel("a")?;
    let mut out = Vec::new();
    for div in tr.select(&entry) {
        let name = element_text(div);
        if name.is_empty() {
            continue;
        }
        let net_id = div.select(&link).next().and_then(|a| {
            a.value()
                .attr("p_netid")
                .map(str::to_owned)
                .or_else(|| a.value().attr("href").and_then(net_id_from_href))
        });
        out.push(Instructor {
            name,
            net_id: net_id.filter(|id| !id.is_empty()).map(NetId),
        });
    }
    Ok(out)
}

fn net_id_from_href(href: &str) -> Option<String> {
    let (_, rest) = href.split_once("p_netid=")?;
    let id = rest.split(['&', '#']).next().unwrap_or_default();
    Some(id.trim().to_owned())
}

/// One inner `<div>` per weekly pattern under `div.mtg-clas`; a blank
/// inner div is a section with no schedule.
fn meetings(
    tr: ElementRef<'_>,
    report: &mut ParseReport,
    at: &str,
) -> Result<Vec<Meeting>, ParseError> {
    let pattern = sel("div.mtg-clas > div")?;
    let mut out = Vec::new();
    for div in tr.select(&pattern) {
        let text = clean(&div.text().collect::<String>());
        if text.is_empty() {
            continue;
        }
        let pattern = if let Some(time) = meeting_time(&text)? {
            MeetingPattern::Timed(time)
        } else {
            report.issue(IssueCode::UnreadableMeeting, format!("{at}: {text:?}"));
            MeetingPattern::Unparsed(text)
        };
        out.push(Meeting {
            pattern,
            dates: None,
        });
    }
    Ok(out)
}
