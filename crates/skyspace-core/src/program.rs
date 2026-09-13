//! The requirement tree and the reviewed `Program` document.
//!
//! Identifiers are UUID newtypes minted by the store, never derived from a
//! label: a rename would lose every student's saved choice.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Timestamp;
use crate::catalog::Attribute;
use crate::code::{CourseCode, Subject};
use crate::evaluate::CourseFacts;
use crate::term::{CreditRange, Credits};

/// One program across catalog years.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct ProgramId(pub Uuid);

/// One requirement inside one program version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
pub struct RequirementId(pub Uuid);

/// A General Announcements edition: `CatalogYear(2026)` is 2026-2027.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CatalogYear(pub u16);

/// Where a requirement came from, shown on every rule in the product.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SourceRef {
    /// The General Announcements page.
    pub url: String,
    /// A fragment on that page, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

/// One node of the requirement tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Requirement {
    /// Stable identifier.
    pub id: RequirementId,
    /// The label as printed.
    pub label: String,
    /// GA's `hourscol`: `3-4` is min 300, max 400; `1 or 3` is `Either`; blank is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<CreditRange>,
    /// Per-rule source link.
    pub source: SourceRef,
    /// What the node means.
    pub body: RequirementBody,
}

/// The body of a requirement. Struct variants only: serde's internally
/// tagged form cannot hold a newtype variant wrapping a sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum RequirementBody {
    /// Program root and area header: every child.
    All {
        /// The children, in document order.
        of: Vec<Requirement>,
    },
    /// "Select N course(s) from the following:", also "one of these sequences".
    Select {
        /// How many children must be met.
        count: u8,
        /// The options.
        of: Vec<Requirement>,
    },
    /// A course row. `semesters` is GA's "(minimum of 8 semesters)" in a
    /// title cell; 1 for an ordinary row. One slot per semester.
    Course {
        /// Which courses fill it; an `or` row is a second selector.
        filter: CourseFilter,
        /// Slots.
        semesters: u8,
    },
    /// "Select N credit hours from the following:".
    Credits {
        /// Hours needed.
        minimum: Credits,
        /// Whether cards filling another rule count.
        scope: CreditScope,
        /// Which courses count.
        from: CourseFilter,
    },
    /// Always a self-check, never in a progress total.
    NonCourse {
        /// What sort of thing.
        non_course_kind: NonCourseKind,
        /// What the student must do.
        description: String,
    },
    /// What the reviewer could not express above, in the page's own words,
    /// so it is visible instead of missing. Always a self-check.
    Unverifiable {
        /// The original text.
        text: String,
    },
    /// A constraint across the sibling course slots: their cards must come
    /// from at least `minimum` departments. Checked when every card's
    /// department is known; a self-check otherwise.
    DistinctDepartments {
        /// How many departments.
        minimum: u8,
        /// The sentence as printed.
        text: String,
    },
}

/// `Any` counts every matching card, including cards filling another rule
/// of this program; `Additional` only cards filling no other rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CreditScope {
    /// Every matching card.
    Any,
    /// Only cards that fill no other rule ("additional" on the page).
    Additional,
}

/// What sort of non-course requirement. Recitals and the preceptorship are
/// courses, so they are not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum NonCourseKind {
    /// The piano proficiency exam.
    ProficiencyExam,
    /// A portfolio review.
    Portfolio,
    /// Anything else.
    Other,
}

/// Which courses a rule accepts.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseFilter {
    /// Any match; empty matches every course.
    pub include: Vec<CourseSelector>,
    /// Any match rejects.
    pub exclude: Vec<CourseSelector>,
}

/// One way to name courses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CourseSelector {
    /// Exactly this code.
    Code {
        /// The code.
        code: CourseCode,
    },
    /// Any course in a subject; how FWIS and LPAP are expressed.
    Subject {
        /// The subject.
        subject: Subject,
    },
    /// A number range, ends included, optionally within a subject.
    NumberRange {
        /// The subject, or any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subject: Option<Subject>,
        /// Lowest number.
        low: u16,
        /// Highest number.
        high: u16,
    },
    /// Distribution I-III or Analyzing Diversity.
    Attribute {
        /// The attribute.
        attribute: Attribute,
    },
}

/// Three values, not two: a missing fact is not a non-match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterMatch {
    /// The course matches.
    Yes,
    /// The course does not match.
    No,
    /// An attribute selector with no facts for the course.
    Unknown,
}

impl CourseSelector {
    fn matches(&self, code: &CourseCode, facts: &CourseFacts) -> FilterMatch {
        match self {
            Self::Code { code: wanted } => {
                if wanted == code {
                    FilterMatch::Yes
                } else {
                    FilterMatch::No
                }
            }
            Self::Subject { subject } => {
                if *subject == code.subject {
                    FilterMatch::Yes
                } else {
                    FilterMatch::No
                }
            }
            Self::NumberRange { subject, low, high } => {
                let subject_ok = subject.as_ref().is_none_or(|s| *s == code.subject);
                let number = code.number.numeric();
                if subject_ok && (*low..=*high).contains(&number) {
                    FilterMatch::Yes
                } else {
                    FilterMatch::No
                }
            }
            Self::Attribute { attribute } => match facts.get(code) {
                None => FilterMatch::Unknown,
                Some(info) if info.attributes.contains(attribute) => FilterMatch::Yes,
                Some(_) => FilterMatch::No,
            },
        }
    }
}

impl CourseFilter {
    /// Whether `code` passes. `code` must already be canonical
    /// (`CourseFacts::canonical`). An exclude match rejects; an empty
    /// include matches every course; otherwise any include match accepts,
    /// and an attribute selector with no facts makes the answer `Unknown`.
    #[must_use]
    pub fn matches(&self, code: &CourseCode, facts: &CourseFacts) -> FilterMatch {
        if self
            .exclude
            .iter()
            .any(|s| s.matches(code, facts) == FilterMatch::Yes)
        {
            return FilterMatch::No;
        }
        if self.include.is_empty() {
            return FilterMatch::Yes;
        }
        let mut unknown = false;
        for selector in &self.include {
            match selector.matches(code, facts) {
                FilterMatch::Yes => return FilterMatch::Yes,
                FilterMatch::Unknown => unknown = true,
                FilterMatch::No => {}
            }
        }
        if unknown {
            FilterMatch::Unknown
        } else {
            FilterMatch::No
        }
    }

    /// True when any include selector reads facts.
    #[must_use]
    pub fn needs_attribute(&self) -> bool {
        self.include
            .iter()
            .any(|s| matches!(s, CourseSelector::Attribute { .. }))
    }

    /// The one code this filter names, when it names exactly one and nothing else.
    #[must_use]
    pub fn single_code(&self) -> Option<&CourseCode> {
        match self.include.as_slice() {
            [CourseSelector::Code { code }] => Some(code),
            _ => None,
        }
    }
}

/// What the General Announcements publish, not how a plan uses a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ProgramKind {
    /// The university-wide requirements, one program every plan carries.
    University,
    /// A major.
    Major,
    /// A minor.
    Minor,
    /// A certificate.
    Certificate,
    /// A concentration.
    Concentration,
}

/// Who approved the draft and when. Not a state machine: the parser produces
/// a draft; only a review makes a `Program`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Review {
    /// From `program_versions.reviewed_by`.
    pub reviewed_by: String,
    /// Supplied by the caller; core has no clock.
    pub published_at: Timestamp,
}

/// One reviewed set of requirements: one program, one catalog year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Program {
    /// Stable across catalog years.
    pub id: ProgramId,
    /// The edition these rules are from.
    pub catalog_year: CatalogYear,
    /// GA slug; display and re-extraction match. Never an id.
    pub slug: String,
    /// What sort of program.
    pub kind: ProgramKind,
    /// Display name.
    pub name: String,
    /// `BSCS`, `BMus`; empty for university requirements.
    pub credential: String,
    /// `None` when the page prints no total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_credits: Option<Credits>,
    /// The page.
    pub source: SourceRef,
    /// Who approved it.
    pub review: Review,
    /// Always an `All` node.
    pub root: Requirement,
    /// Identifiers of rules removed in review, never reused.
    pub retired_requirements: Vec<RequirementId>,
}

impl Requirement {
    /// Depth-first, document order.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Requirement)) {
        visit(self);
        if let RequirementBody::All { of } | RequirementBody::Select { of, .. } = &self.body {
            for child in of {
                child.walk(visit);
            }
        }
    }

    /// Every node under and including this one, document order.
    #[must_use]
    pub fn flatten(&self) -> Vec<&Requirement> {
        let mut out = Vec::new();
        self.walk(&mut |r| out.push(r));
        out
    }
}

impl Program {
    /// Every node in the tree, for `every_requirement_is_reported_once`.
    #[must_use]
    pub fn requirement_count(&self) -> usize {
        self.root.flatten().len()
    }

    /// Find a requirement by id.
    #[must_use]
    pub fn requirement(&self, id: RequirementId) -> Option<&Requirement> {
        self.root.flatten().into_iter().find(|r| r.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::CourseInfo;
    use std::collections::BTreeSet;

    fn code(raw: &str) -> CourseCode {
        CourseCode::parse(raw).unwrap()
    }

    fn facts_with(code: &CourseCode, attributes: &[Attribute]) -> CourseFacts {
        let mut facts = CourseFacts::default();
        facts.insert(CourseInfo {
            code: code.clone(),
            title: String::new(),
            credits: CreditRange::Fixed(Credits::from_cents(300)),
            attributes: attributes.iter().copied().collect::<BTreeSet<_>>(),
            department: None,
            repeatable: false,
            seasons_offered: vec![],
            terms_observed: 0,
            offered_now: false,
        });
        facts
    }

    #[test]
    fn filters_match_by_code_subject_range_and_attribute() {
        let facts = CourseFacts::default();
        let fwis = CourseFilter {
            include: vec![CourseSelector::NumberRange {
                subject: Some(Subject::new("FWIS").unwrap()),
                low: 101,
                high: 299,
            }],
            exclude: vec![],
        };
        assert_eq!(fwis.matches(&code("FWIS 100"), &facts), FilterMatch::No);
        assert_eq!(fwis.matches(&code("FWIS 150"), &facts), FilterMatch::Yes);
        assert_eq!(fwis.matches(&code("COMP 150"), &facts), FilterMatch::No);

        let mmus = CourseFilter {
            include: vec![CourseSelector::NumberRange {
                subject: Some(Subject::new("MUSI").unwrap()),
                low: 251,
                high: 297,
            }],
            exclude: vec![CourseSelector::Code {
                code: code("MUSI 281"),
            }],
        };
        assert_eq!(mmus.matches(&code("MUSI 281"), &facts), FilterMatch::No);
        assert_eq!(mmus.matches(&code("MUSI 260"), &facts), FilterMatch::Yes);

        let any = CourseFilter::default();
        assert_eq!(any.matches(&code("XXXX 999"), &facts), FilterMatch::Yes);

        let grp3 = CourseFilter {
            include: vec![CourseSelector::Attribute {
                attribute: Attribute::DistributionThree,
            }],
            exclude: vec![],
        };
        assert_eq!(
            grp3.matches(&code("COMP 140"), &facts),
            FilterMatch::Unknown
        );
        let known = facts_with(&code("COMP 140"), &[Attribute::DistributionThree]);
        assert_eq!(grp3.matches(&code("COMP 140"), &known), FilterMatch::Yes);
        let none = facts_with(&code("COMP 140"), &[]);
        assert_eq!(grp3.matches(&code("COMP 140"), &none), FilterMatch::No);
        assert!(grp3.needs_attribute());
        assert!(!mmus.needs_attribute());
    }

    #[test]
    fn requirement_body_is_internally_tagged() {
        let body = RequirementBody::NonCourse {
            non_course_kind: NonCourseKind::ProficiencyExam,
            description: "Piano".to_owned(),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"non_course","nonCourseKind":"proficiency_exam","description":"Piano"}"#
        );
        assert_eq!(
            serde_json::from_str::<RequirementBody>(&json).unwrap(),
            body
        );
        let selector = CourseSelector::NumberRange {
            subject: None,
            low: 300,
            high: 499,
        };
        let json = serde_json::to_string(&selector).unwrap();
        assert_eq!(json, r#"{"kind":"number_range","low":300,"high":499}"#);
        assert_eq!(
            serde_json::from_str::<CourseSelector>(&json).unwrap(),
            selector
        );
    }
}
