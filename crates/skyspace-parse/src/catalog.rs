//! The course catalog: `!SWKSCAT.cat?p_action=CATALIST&p_acyr_code=&p_subj=`,
//! one page per subject holding every course record for one academic year.

use std::collections::BTreeSet;

use scraper::{ElementRef, Html};
use skyspace_core::CourseCode;
use skyspace_core::catalog::{Course, CourseFlags, CourseType, GradeMode};
use skyspace_core::prereq::{MutualExclusion, PrereqExpr, PrereqObservation};
use skyspace_core::program::CatalogYear;
use skyspace_core::term::CreditRange;

use crate::detail::field_name;
use crate::labels::{DetailField, Labelled, attribute_from_text, labelled_values, restrictions};
use crate::text::{clean, code_run, credit_range, element_text, re};
use crate::{IssueCode, ParseError, ParseReport, Parsed, sel};

const DOCUMENT: &str = "course catalog";

/// Every course record on one `CATALIST` page. `Prerequisite(s):` goes
/// through `PrereqExpr::parse`; a text the grammar cannot read is stored as
/// `Unparsed` and counted so someone can look. The fixed sentences at the
/// end of the description become `CourseFlags`, `MutualExclusion`,
/// `cross_list` and `equivalents`.
///
/// # Errors
/// [`ParseError::SelectorMissing`] when the page holds no `div.course`
/// block, which is what an unknown subject or year returns.
pub fn parse_catalog_subject(
    html: &str,
    year: CatalogYear,
) -> Result<Parsed<Vec<Course>>, ParseError> {
    let doc = Html::parse_document(html);
    let block = sel("div.course")?;
    let heading = sel("h3")?;
    let mut report = ParseReport::default();
    let mut courses = Vec::new();
    let mut any = false;
    for course in doc.select(&block) {
        any = true;
        report.saw_row();
        let title_line = course
            .select(&heading)
            .next()
            .map(element_text)
            .unwrap_or_default();
        let Some(code) = code_from_heading(&title_line) else {
            report.issue(IssueCode::RowMissingKey, format!("heading {title_line:?}"));
            continue;
        };
        if let Some(record) = parse_course(course, code, year, &mut report)? {
            report.kept_row();
            courses.push(record);
        }
    }
    if !any {
        return Err(ParseError::SelectorMissing {
            selector: "div.course",
            document: DOCUMENT,
        });
    }
    Ok(Parsed {
        value: courses,
        report,
    })
}

/// `COMP 140 - COMPUTATIONAL THINKING` names the code before the dash.
fn code_from_heading(text: &str) -> Option<CourseCode> {
    let head = text.split(" - ").next()?;
    CourseCode::parse(head).ok()
}

fn parse_course(
    block: ElementRef<'_>,
    code: CourseCode,
    year: CatalogYear,
    report: &mut ParseReport,
) -> Result<Option<Course>, ParseError> {
    let at = code.to_string();
    let labelled = labelled_values(block, report, &at)?;
    let mut fields = Fields::default();
    for item in &labelled {
        let Some(field) = item.field else {
            continue;
        };
        if item.value.is_empty() && item.lines.is_empty() {
            fields.note_present(field);
            continue;
        }
        report.filled(field_name(field));
        fields.take(field, item, report, &at)?;
    }
    let Some(credits) = fields.credits.or_else(|| {
        report.issue(IssueCode::RowMissingKey, format!("{at}: no credit hours"));
        None
    }) else {
        return Ok(None);
    };
    let description = fields.description.clone();
    let sentences = trailing_sentences(&description, &at, report)?;
    let prerequisites = fields.prerequisite_observation(&code, year, report, &at);
    Ok(Some(Course {
        catalog_year: year,
        code,
        title: fields.title,
        credits,
        department: fields.department,
        attributes: fields.attributes,
        grade_mode: fields.grade_mode.map(GradeMode),
        course_type: fields.course_type.map(CourseType),
        restrictions: fields.restrictions,
        prerequisites,
        description,
        flags: sentences.flags,
        mutual_exclusions: sentences.exclusions,
        cross_list: sentences.cross_list,
        equivalents: sentences.equivalents,
    }))
}

/// The labelled values of one record, before they become a `Course`.
#[derive(Default)]
struct Fields {
    title: String,
    department: String,
    credits: Option<CreditRange>,
    attributes: BTreeSet<skyspace_core::catalog::Attribute>,
    grade_mode: Option<String>,
    course_type: Option<String>,
    restrictions: Option<skyspace_core::catalog::Restrictions>,
    prerequisite_text: Option<String>,
    corequisite_text: Option<String>,
    description: String,
}

impl Fields {
    /// A printed label with nothing after it still counts as printed for
    /// the prerequisite field, where "printed empty" and "absent" differ.
    fn note_present(&mut self, field: DetailField) {
        if field == DetailField::Prerequisites && self.prerequisite_text.is_none() {
            self.prerequisite_text = Some(String::new());
        }
    }

    fn take(
        &mut self,
        field: DetailField,
        item: &Labelled,
        report: &mut ParseReport,
        at: &str,
    ) -> Result<(), ParseError> {
        let value = item.value.clone();
        match field {
            DetailField::LongTitle => self.title = value,
            DetailField::Department => self.department = value,
            DetailField::CreditHours => self.credits = credit_range(&value, report, at),
            DetailField::Attributes | DetailField::AnalyzingDiversity => {
                match attribute_from_text(&value) {
                    Some(attribute) => {
                        self.attributes.insert(attribute);
                    }
                    None => report.issue(IssueCode::UnknownAttribute, format!("{at}: {value:?}")),
                }
            }
            DetailField::GradeMode => self.grade_mode = Some(value),
            DetailField::CourseType => self.course_type = Some(value),
            DetailField::Restrictions => self.restrictions = restrictions(item)?,
            DetailField::Prerequisites => self.prerequisite_text = Some(value),
            DetailField::Corequisite => self.corequisite_text = Some(value),
            DetailField::Description => self.description = value,
            _ => {}
        }
        Ok(())
    }

    /// `None` only when Rice printed neither label.
    fn prerequisite_observation(
        &self,
        code: &CourseCode,
        year: CatalogYear,
        report: &mut ParseReport,
        at: &str,
    ) -> Option<PrereqObservation> {
        if self.prerequisite_text.is_none() && self.corequisite_text.is_none() {
            return None;
        }
        let raw = self.prerequisite_text.clone().unwrap_or_default();
        let parsed = if raw.is_empty() {
            PrereqExpr::Unparsed(String::new())
        } else {
            PrereqExpr::parse(&raw)
        };
        if !raw.is_empty() && matches!(parsed, PrereqExpr::Unparsed(_)) {
            report.issue(IssueCode::UnparsedPrerequisite, format!("{at}: {raw:?}"));
        }
        let corequisite = self
            .corequisite_text
            .as_deref()
            .and_then(|text| CourseCode::parse(text.trim_end_matches('.')).ok());
        Some(PrereqObservation {
            course: code.clone(),
            raw,
            parsed,
            corequisite,
            catalog_year: year,
        })
    }
}

/// What the fixed sentences at the end of a description said.
#[derive(Default)]
struct Sentences {
    flags: CourseFlags,
    exclusions: Vec<MutualExclusion>,
    cross_list: Vec<CourseCode>,
    equivalents: Vec<CourseCode>,
}

fn trailing_sentences(
    description: &str,
    at: &str,
    report: &mut ParseReport,
) -> Result<Sentences, ParseError> {
    let mut out = Sentences {
        flags: CourseFlags {
            repeatable: description.contains("Repeatable for Credit."),
            instructor_permission: description.contains("Instructor Permission Required."),
            second_half: description.contains("Expected to be taught 2nd half of the term."),
        },
        ..Sentences::default()
    };
    let exclusion =
        re(r"Mutually Exclusive: Cannot register for [^.]*? if student has credit for ([^.]+)\.")?;
    for caps in exclusion.captures_iter(description) {
        let raw = clean(caps.get(0).map_or("", |m| m.as_str()));
        let list = caps.get(1).map_or("", |m| m.as_str());
        let (with, complete) = code_run(list);
        if with.is_empty() || !complete {
            report.issue(IssueCode::UnreadableExclusion, format!("{at}: {raw:?}"));
        }
        out.exclusions.push(MutualExclusion {
            with: if complete { with } else { Vec::new() },
            raw,
        });
    }
    let cross = re(r"Cross-list:\s*([^.]*)")?;
    for caps in cross.captures_iter(description) {
        let (codes, _) = code_run(caps.get(1).map_or("", |m| m.as_str()));
        out.cross_list.extend(codes);
    }
    let equivalency = re(r"Equivalency:\s*([^.]*)")?;
    for caps in equivalency.captures_iter(description) {
        let (codes, _) = code_run(caps.get(1).map_or("", |m| m.as_str()));
        out.equivalents.extend(codes);
    }
    Ok(out)
}
