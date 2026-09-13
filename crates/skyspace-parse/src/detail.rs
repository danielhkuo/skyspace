//! Section detail page: `!SWKSCAT.cat?p_action=COURSE&p_term=&p_crn=`.

use std::collections::BTreeSet;

use scraper::Html;
use skyspace_core::catalog::{
    CourseType, Fee, GradeMode, MethodOfInstruction, SeatReservation, SectionDetail,
};
use skyspace_core::{Crn, TermCode, Timestamp};

use crate::labels::{DetailField, Labelled, attribute_from_text, labelled_values, restrictions};
use crate::text::{clean, re};
use crate::{IssueCode, ParseError, ParseReport, Parsed};

/// The detail page, label by label. `crn` names the page in issues;
/// `fetched_at` fills `SectionDetail::fetched_at`, because the page carries
/// no time and this crate has no clock. `has_syllabus` is always false: the
/// syllabus div is filled by JavaScript, so ingest asks the `.info` feed.
///
/// # Errors
/// [`ParseError::SelectorMissing`] when the page has no `<b>` labels at all.
pub fn parse_section_detail(
    html: &str,
    term: TermCode,
    crn: Crn,
    fetched_at: Timestamp,
) -> Result<Parsed<SectionDetail>, ParseError> {
    let doc = Html::parse_document(html);
    let mut report = ParseReport::default();
    let at = format!("{term} crn {crn}");
    let labelled = labelled_values(doc.root_element(), &mut report, &at)?;
    if labelled.iter().all(|l| l.field.is_none()) {
        return Err(ParseError::SelectorMissing {
            selector: "b",
            document: "section detail",
        });
    }
    report.saw_row();
    let mut detail = SectionDetail {
        long_title: String::new(),
        description: String::new(),
        department: String::new(),
        attributes: BTreeSet::new(),
        prerequisites_text: None,
        restrictions: None,
        grade_mode: None,
        method_of_instruction: None,
        course_type: None,
        language: None,
        reserved: Vec::new(),
        fees: Vec::new(),
        has_syllabus: false,
        fetched_at,
    };
    let seats = re(r"^(\d+)\s*\((\d+)\s*Available\)")?;
    for item in &labelled {
        let Some(field) = item.field else {
            continue;
        };
        if item.value.is_empty() && item.lines.is_empty() {
            continue;
        }
        report.filled(field_name(field));
        fill(&mut detail, field, item, &seats, &mut report, &at)?;
    }
    report.kept_row();
    Ok(Parsed {
        value: detail,
        report,
    })
}

fn fill(
    detail: &mut SectionDetail,
    field: DetailField,
    item: &Labelled,
    seats: &regex::Regex,
    report: &mut ParseReport,
    at: &str,
) -> Result<(), ParseError> {
    let value = item.value.clone();
    match field {
        DetailField::LongTitle => detail.long_title = value,
        DetailField::Description => detail.description = value,
        DetailField::Department => detail.department = value,
        DetailField::Attributes | DetailField::AnalyzingDiversity => {
            match attribute_from_text(&value) {
                Some(attribute) => {
                    detail.attributes.insert(attribute);
                }
                None => report.issue(IssueCode::UnknownAttribute, format!("{at}: {value:?}")),
            }
        }
        DetailField::Prerequisites => detail.prerequisites_text = Some(value),
        DetailField::Restrictions => detail.restrictions = restrictions(item)?,
        DetailField::GradeMode => detail.grade_mode = Some(GradeMode(value)),
        DetailField::Method => detail.method_of_instruction = Some(MethodOfInstruction(value)),
        DetailField::CourseType => detail.course_type = Some(CourseType(value)),
        DetailField::Language => detail.language = Some(value),
        DetailField::ReservedSeats => match reservation(&item.label, &value, seats) {
            Some(reservation) => detail.reserved.push(reservation),
            None => report.issue(
                IssueCode::RowMissingKey,
                format!("{at}: reserved seats {:?} {value:?}", item.label),
            ),
        },
        DetailField::Fees => detail.fees.extend(fees(&value)?),
        _ => {}
    }
    Ok(())
}

/// `Reserved Seats for Fall Semester 2026 Matriculants:` and `60 (2 Available)`.
fn reservation(label: &str, value: &str, seats: &regex::Regex) -> Option<SeatReservation> {
    let group = label
        .strip_prefix("Reserved Seats for")?
        .trim()
        .trim_end_matches(':')
        .trim();
    let caps = seats.captures(value)?;
    let capacity: u16 = caps.get(1)?.as_str().parse().ok()?;
    let available: u16 = caps.get(2)?.as_str().parse().ok()?;
    Some(SeatReservation {
        label: group.to_owned(),
        capacity,
        available,
    })
}

/// `None` is no fee. Anything else is kept verbatim as one fee per
/// semicolon-separated part, with a dollar amount read when one is printed.
fn fees(value: &str) -> Result<Vec<Fee>, ParseError> {
    if value.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let amount = re(r"\$\s*(\d{1,6})(?:\.(\d{2}))?")?;
    Ok(value
        .split(';')
        .map(clean)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let amount_cents = amount.captures(&part).and_then(|caps| {
                let dollars: u32 = caps.get(1)?.as_str().parse().ok()?;
                let cents: u32 = caps.get(2).map_or(Some(0), |m| m.as_str().parse().ok())?;
                dollars.checked_mul(100)?.checked_add(cents)
            });
            let label = clean(&amount.replace_all(&part, ""))
                .trim_matches(|c: char| c == '-' || c == ':' || c == ',' || c.is_whitespace())
                .to_owned();
            Fee {
                label: if label.is_empty() {
                    part.clone()
                } else {
                    label
                },
                amount_cents,
                raw: part,
            }
        })
        .collect())
}

/// The `field_filled` name for each label.
pub(crate) const fn field_name(field: DetailField) -> &'static str {
    match field {
        DetailField::LongTitle => "long_title",
        DetailField::Department => "department",
        DetailField::Instructor => "instructor",
        DetailField::Meeting => "meeting",
        DetailField::PartOfTerm => "part_of_term",
        DetailField::GradeMode => "grade_mode",
        DetailField::CourseType => "course_type",
        DetailField::Language => "language",
        DetailField::Attributes => "distribution_group",
        DetailField::Method => "method_of_instruction",
        DetailField::CreditHours => "credit_hours",
        DetailField::Syllabus => "syllabus",
        DetailField::Materials => "materials",
        DetailField::Restrictions => "restrictions",
        DetailField::Prerequisites => "prerequisites",
        DetailField::MaxEnrollment => "max_enrollment",
        DetailField::Enrolled => "enrolled",
        DetailField::ReservedSeats => "reserved_seats",
        DetailField::EnrollmentAsOf => "enrollment_as_of",
        DetailField::Fees => "fees",
        DetailField::FinalExam => "final_exam",
        DetailField::Description => "description",
        DetailField::CourseUrl => "course_url",
        DetailField::Corequisite => "corequisite",
        DetailField::AnalyzingDiversity => "analyzing_diversity",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_without_labels_is_a_shape_change() {
        let err = parse_section_detail(
            "<html><body><h2>Course Schedule</h2></body></html>",
            TermCode::parse("202710").unwrap(),
            Crn(1),
            Timestamp(0),
        )
        .err()
        .unwrap();
        assert!(matches!(
            err,
            ParseError::SelectorMissing { selector: "b", .. }
        ));
    }
}
