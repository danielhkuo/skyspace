//! The label-driven pages: section detail and the `CATALIST` course record.
//! Both print `<b>Label:</b> value` inside bare `<div>`s, so they share one
//! label table and one value reader instead of a selector per field.

use scraper::{ElementRef, Node};
use skyspace_core::catalog::{
    Attribute, RestrictionClause, RestrictionDimension, RestrictionEffect, Restrictions,
};

use crate::text::{clean, element_text, re};
use crate::{IssueCode, ParseError, ParseReport, sel};

/// One field of the detail page or course record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetailField {
    /// `Long Title:`.
    LongTitle,
    /// `Department:`.
    Department,
    /// `Instructor:`.
    Instructor,
    /// `Meeting:`.
    Meeting,
    /// `Part of Term:`.
    PartOfTerm,
    /// `Grade Mode:`.
    GradeMode,
    /// `Course Type:`.
    CourseType,
    /// `Language of Instruction:`.
    Language,
    /// `Distribution Group:`.
    Attributes,
    /// `Method of Instruction:`.
    Method,
    /// `Credit Hours:`.
    CreditHours,
    /// `Course Syllabus:`.
    Syllabus,
    /// `Course Materials:`.
    Materials,
    /// `Restrictions:`.
    Restrictions,
    /// `Prerequisites:` on the section page, `Prerequisite(s):` on the record.
    Prerequisites,
    /// `Section Max Enrollment:`.
    MaxEnrollment,
    /// `Section Enrolled:`.
    Enrolled,
    /// `Reserved Seats for <group>:`, prefix-matched.
    ReservedSeats,
    /// `Enrollment data as of:`.
    EnrollmentAsOf,
    /// `Additional Fees:`.
    Fees,
    /// `Final Exam:`.
    FinalExam,
    /// `Description:`.
    Description,
    /// `Course URL:`.
    CourseUrl,
    /// `Corequisite:`.
    Corequisite,
    /// `Analyzing Diversity:`.
    AnalyzingDiversity,
}

/// Labels are trimmed before lookup: Rice's trailing space is inconsistent.
/// `Reserved Seats for <group>:` is prefix-matched and the group kept as
/// `SeatReservation::label`.
pub const DETAIL_LABELS: &[(&str, DetailField)] = &[
    ("Long Title:", DetailField::LongTitle),
    ("Department:", DetailField::Department),
    ("Instructor:", DetailField::Instructor),
    ("Meeting:", DetailField::Meeting),
    ("Part of Term:", DetailField::PartOfTerm),
    ("Grade Mode:", DetailField::GradeMode),
    ("Course Type:", DetailField::CourseType),
    ("Language of Instruction:", DetailField::Language),
    ("Distribution Group:", DetailField::Attributes),
    ("Method of Instruction:", DetailField::Method),
    ("Credit Hours:", DetailField::CreditHours),
    ("Course Syllabus:", DetailField::Syllabus),
    ("Course Materials:", DetailField::Materials),
    ("Restrictions:", DetailField::Restrictions),
    ("Prerequisites:", DetailField::Prerequisites),
    ("Prerequisite(s):", DetailField::Prerequisites),
    ("Section Max Enrollment:", DetailField::MaxEnrollment),
    ("Section Enrolled:", DetailField::Enrolled),
    ("Reserved Seats for", DetailField::ReservedSeats),
    ("Enrollment data as of:", DetailField::EnrollmentAsOf),
    ("Additional Fees:", DetailField::Fees),
    ("Final Exam:", DetailField::FinalExam),
    ("Description:", DetailField::Description),
    ("Course URL:", DetailField::CourseUrl),
    ("Corequisite:", DetailField::Corequisite),
    ("Analyzing Diversity:", DetailField::AnalyzingDiversity),
];

/// The field a printed label names, or `None` for a label Rice added.
#[must_use]
pub fn lookup_label(label: &str) -> Option<DetailField> {
    let label = clean(label);
    DETAIL_LABELS.iter().find_map(|(text, field)| {
        let hit = if *field == DetailField::ReservedSeats {
            label.starts_with(text)
        } else {
            label == *text
        };
        hit.then_some(*field)
    })
}

/// A `<b>` label and the text that follows it.
#[derive(Debug, Clone)]
pub(crate) struct Labelled {
    /// The known field, or `None` for a label outside the table.
    pub field: Option<DetailField>,
    /// The label as printed, cleaned.
    pub label: String,
    /// Everything after the label up to the next label, cleaned.
    pub value: String,
    /// Leaf `<div>` texts in the same region, for the restriction lines.
    pub lines: Vec<String>,
}

/// Every `<b>` label under `root`, in page order, with its value. Unknown
/// labels are reported as [`IssueCode::UnknownDetailLabel`] and kept, so
/// a new Rice field is visible instead of dropped.
///
/// # Errors
/// [`ParseError::BadSelector`] only; a page with no labels is an empty list.
pub(crate) fn labelled_values(
    root: ElementRef<'_>,
    report: &mut ParseReport,
    at: &str,
) -> Result<Vec<Labelled>, ParseError> {
    let bold = sel("b")?;
    let mut out = Vec::new();
    for b in root.select(&bold) {
        let label = element_text(b);
        if label.is_empty() {
            continue;
        }
        let field = lookup_label(&label);
        if field.is_none() {
            report.issue(IssueCode::UnknownDetailLabel, format!("{at}: {label:?}"));
        }
        let region = value_region(b, &bold);
        let value = clean(&region.iter().map(node_text).collect::<Vec<_>>().join(" "));
        let lines = region.iter().flat_map(leaf_div_lines).collect();
        out.push(Labelled {
            field,
            label,
            value,
            lines,
        });
    }
    Ok(out)
}

type NodeRef<'a> = <ElementRef<'a> as std::ops::Deref>::Target;

/// The nodes that hold a label's value: the label's following siblings up
/// to the next label, or, when those are blank, the parent's next sibling
/// when it carries no label of its own (Rice prints restrictions that way).
fn value_region<'a>(b: ElementRef<'a>, bold: &scraper::Selector) -> Vec<NodeRef<'a>> {
    let mut region: Vec<NodeRef<'a>> = Vec::new();
    for node in b.next_siblings() {
        if holds_label(node, bold) {
            break;
        }
        region.push(node);
    }
    let blank = region.iter().all(|node| clean(&node_text(node)).is_empty());
    if !blank {
        return region;
    }
    let Some(parent) = b.parent() else {
        return region;
    };
    for node in parent.next_siblings() {
        if holds_label(node, bold) {
            break;
        }
        if !clean(&node_text(&node)).is_empty() {
            region.push(node);
            break;
        }
    }
    region
}

fn holds_label(node: NodeRef<'_>, bold: &scraper::Selector) -> bool {
    ElementRef::wrap(node).is_some_and(|el| el.select(bold).next().is_some())
}

fn node_text(node: &NodeRef<'_>) -> String {
    match node.value() {
        Node::Text(text) => text.to_string(),
        Node::Element(_) => ElementRef::wrap(*node)
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Texts of `<div>`s with no element children, for line-structured values.
fn leaf_div_lines(node: &NodeRef<'_>) -> Vec<String> {
    let Some(element) = ElementRef::wrap(*node) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let mut stack = vec![element];
    while let Some(el) = stack.pop() {
        let children: Vec<ElementRef<'_>> = el.child_elements().collect();
        if el.value().name() == "div" && children.is_empty() {
            let text = element_text(el);
            if !text.is_empty() {
                lines.push(text);
            }
        }
        stack.extend(children.into_iter().rev());
    }
    lines
}

/// Rice's attribute as printed: `Distribution Group III`, `Analyzing
/// Diversity`, or the bare code. `None` for anything else.
#[must_use]
pub fn attribute_from_text(text: &str) -> Option<Attribute> {
    let text = clean(text);
    if let Some(attribute) = Attribute::from_code(&text) {
        return Some(attribute);
    }
    let upper = text.to_ascii_uppercase();
    if upper.contains("ANALYZING DIVERSITY") {
        return Some(Attribute::AnalyzingDiversity);
    }
    let group = upper.strip_prefix("DISTRIBUTION GROUP ")?;
    match group.trim() {
        "I" | "1" => Some(Attribute::DistributionOne),
        "II" | "2" => Some(Attribute::DistributionTwo),
        "III" | "3" => Some(Attribute::DistributionThree),
        _ => None,
    }
}

/// Registration restrictions from the printed lines: a line ending in a
/// colon opens a clause, the lines under it are its values.
///
/// # Errors
/// [`ParseError::BadSelector`] if the pattern in this file is broken.
pub(crate) fn restrictions(labelled: &Labelled) -> Result<Option<Restrictions>, ParseError> {
    let lines: Vec<&str> = if labelled.lines.is_empty() {
        labelled
            .value
            .split(':')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    } else {
        labelled.lines.iter().map(String::as_str).collect()
    };
    if lines.is_empty() {
        return Ok(None);
    }
    let dimension = re(r"(?i)following\s+([A-Za-z]+)\s*\(")?;
    let mut clauses: Vec<RestrictionClause> = Vec::new();
    for line in &lines {
        if let Some(header) = line.strip_suffix(':') {
            let effect = if header.to_ascii_lowercase().starts_with("must be") {
                RestrictionEffect::MustBe
            } else {
                RestrictionEffect::MustNotBe
            };
            let word = dimension
                .captures(header)
                .and_then(|c| c.get(1))
                .map_or_else(|| header.to_owned(), |m| m.as_str().to_owned());
            clauses.push(RestrictionClause {
                effect,
                dimension: dimension_from_word(&word),
                values: Vec::new(),
            });
        } else if let Some(clause) = clauses.last_mut() {
            clause.values.push((*line).to_owned());
        }
    }
    Ok(Some(Restrictions {
        raw: if labelled.lines.is_empty() {
            labelled.value.clone()
        } else {
            labelled.lines.join("\n")
        },
        clauses,
    }))
}

fn dimension_from_word(word: &str) -> RestrictionDimension {
    match word.to_ascii_lowercase().as_str() {
        "level" => RestrictionDimension::Level,
        "class" | "classification" => RestrictionDimension::Classification,
        "major" => RestrictionDimension::Major,
        "program" => RestrictionDimension::Program,
        "college" => RestrictionDimension::College,
        _ => RestrictionDimension::Other(word.to_owned()),
    }
}
