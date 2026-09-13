//! The plan document. One plan is one route to a degree. The engine warns
//! about it and never refuses an edit.
//!
//! Ids are newtypes over `Uuid`. Core compares ids and never mints one: the
//! store mints saved ids, the browser mints unsaved-card ids.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Timestamp;
use crate::catalog::Attribute;
use crate::code::CourseCode;
use crate::program::{CatalogYear, ProgramId, RequirementId};
use crate::term::{CreditRange, Credits, TermCode, TermPosition};

macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "string"))]
        pub struct $name(pub Uuid);
    };
}

id_newtype!(
    /// A signed-in account.
    AccountId
);
id_newtype!(
    /// One plan.
    PlanId
);
id_newtype!(
    /// One term column on the board.
    TermId
);
id_newtype!(
    /// One card: a `PlannedCourse` or a `ManualCourseCard`.
    EntryId
);
id_newtype!(
    /// One schedule.
    ScheduleId
);
id_newtype!(
    /// One busy block on a schedule.
    BusyId
);
id_newtype!(
    /// One saved-course collection.
    CollectionId
);

/// The plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Plan {
    /// Minted by the store.
    pub id: PlanId,
    /// Display name.
    pub name: String,
    /// Defaults to matriculation; Rice lets a student pick any year up to graduation.
    pub catalog_year: CatalogYear,
    /// When the student started.
    pub matriculation: TermPosition,
    /// Includes the one `University` program.
    pub programs: Vec<ProgramId>,
    /// Transfer, AP and IB work, drawn as its own column.
    pub incoming_credit: Vec<ManualCourseCard>,
    /// Sorted by (position, id); id breaks ties.
    pub terms: Vec<PlanTerm>,
    /// Requirements ticked by hand.
    pub self_checks: Vec<SelfCheck>,
}

impl Plan {
    /// The last term on the board. Not stored: the columns are the truth.
    #[must_use]
    pub fn expected_graduation(&self) -> Option<TermPosition> {
        self.terms.iter().map(|t| t.position).max()
    }
}

/// A requirement the student ticked by hand, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SelfCheck {
    /// Which requirement.
    pub requirement: RequirementId,
    /// Why it counts.
    pub reason: SelfCheckReason,
    /// Optional note, shown on the PDF.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Why a self-check was ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SelfCheckReason {
    /// Transfer credit.
    Transfer,
    /// AP or IB credit.
    ApOrIb,
    /// Study abroad.
    StudyAbroad,
    /// An advisor approved a substitution.
    AdvisorApproved,
    /// Anything else, with a note.
    Other,
}

/// One column on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanTerm {
    /// Column id.
    pub id: TermId,
    /// Board order.
    pub position: TermPosition,
    /// "Study abroad — Madrid", "Co-op — Chevron".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What the column holds.
    pub kind: TermKind,
    /// Non-course requirements claimed in this term; allowed in all three kinds.
    pub non_course: Vec<NonCourseClaim>,
}

/// Payload inside the variant, so a catalog course in an `Off` term cannot
/// be built. Serde writes `{"rice": {...}}`, `{"away": {...}}` and `"off"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum TermKind {
    /// Fall, spring or summer at Rice.
    Rice {
        /// `None` = not yet published by Rice.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<TermCode>,
        /// Catalog courses, in board order.
        courses: Vec<PlannedCourse>,
    },
    /// Study abroad, transfer, visiting.
    Away {
        /// Manual cards only.
        cards: Vec<ManualCourseCard>,
    },
    /// Gap, leave, co-op: no courses, no credit.
    Off,
}

impl PlanTerm {
    /// Sums the variant's cards; `Off` is `Credits::ZERO`.
    #[must_use]
    pub fn planned_credits(&self) -> Credits {
        match &self.kind {
            TermKind::Rice { courses, .. } => sum_credits(courses.iter().map(|c| c.credits)),
            TermKind::Away { cards } => sum_credits(cards.iter().map(|c| c.credits)),
            TermKind::Off => Credits::ZERO,
        }
    }
}

/// Saturating fold: a student can type any number into a manual card.
pub(crate) fn sum_credits(values: impl Iterator<Item = Credits>) -> Credits {
    values.fold(Credits::ZERO, Credits::saturating_add)
}

/// A catalog course on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlannedCourse {
    /// Card id.
    pub id: EntryId,
    /// The course.
    pub course: CourseCode,
    /// Editable: Rice publishes ranges. `ZERO` when we hold no record, and
    /// `ZERO` is also a real value (recitals).
    pub credits: Credits,
    /// Per-course override, at most one per program; never edits the rule.
    pub fills: Vec<RequirementId>,
    /// Why a pin the filter rejects should still count; without one the pin is "unsure".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<FillClaim>,
    /// The catalog's word on the course when it was placed; compared on every evaluation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<ObservedFacts>,
    /// What this card was as a manual card, so Rice → Away → Rice loses nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carried: Option<CarriedCard>,
    /// A note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What the catalog said about a course when the student placed it, kept on
/// the card so a later catalog can be compared against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ObservedFacts {
    /// When the card was placed.
    pub at: Timestamp,
    /// The plan's catalog year at the time.
    pub catalog_year: CatalogYear,
    /// The title then.
    pub title: String,
    /// The hours then.
    pub credits: CreditRange,
    /// The attributes then, sorted.
    pub attributes: Vec<Attribute>,
}

/// The manual-card fields a `PlannedCourse` carries after a move into a Rice term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CarriedCard {
    /// Where the credit came from.
    pub origin: CreditOrigin,
    /// The other institution's code.
    pub code: String,
    /// The other institution's title.
    pub title: String,
    /// The institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
}

/// The student's stated reason for pinning a card to a requirement the
/// filter rejects. The engine never turns a claim into `Met`: it shows the
/// card in the slot, keeps the requirement out of the met count, and repeats
/// the basis on the PDF.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum FillBasis {
    /// "It counted when I took it."
    EarlierCatalog {
        /// Which edition.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        catalog_year: Option<CatalogYear>,
    },
    /// An advisor approved it.
    AdvisorApproved {
        /// Who.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<String>,
        /// When, free text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<String>,
    },
    /// A petition was granted.
    PetitionGranted {
        /// When, free text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<String>,
    },
    /// Seen on the Esther degree audit.
    RegistrarPosted {
        /// When, free text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<String>,
    },
    /// No basis given.
    Unsure,
}

/// A pin with its basis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FillClaim {
    /// Which requirement.
    pub requirement: RequirementId,
    /// Why.
    pub basis: FillBasis,
    /// A note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// A non-course requirement placed in a term. Display only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct NonCourseClaim {
    /// A `RequirementBody::NonCourse` rule.
    pub requirement: RequirementId,
    /// What to show on the card.
    pub label: String,
}

/// Where a manual card's credit came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CreditOrigin {
    /// Transfer credit.
    Transfer,
    /// Advanced Placement.
    AdvancedPlacement,
    /// International Baccalaureate.
    InternationalBaccalaureate,
    /// Study abroad.
    StudyAbroad,
    /// Anything else.
    Other,
}

impl CreditOrigin {
    /// AP and IB credit counts toward the total and the major, never toward
    /// distribution or Analyzing Diversity.
    #[must_use]
    pub const fn bars_attribute_slots(self) -> bool {
        matches!(
            self,
            Self::AdvancedPlacement | Self::InternationalBaccalaureate
        )
    }
}

/// Where a manual card's hours come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CreditsSource {
    /// The Rice equivalent's published hours, the normal case.
    Equivalent,
    /// The student's own figure, which the engine flags.
    Manual,
}

/// Transfer, AP, IB or study-abroad work: in `Plan::incoming_credit` when
/// earned before matriculation, in an `Away` term otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ManualCourseCard {
    /// Card id.
    pub id: EntryId,
    /// Where the credit came from.
    pub origin: CreditOrigin,
    /// The other institution's code. Free text; never parsed as a Rice code.
    pub code: String,
    /// The other institution's title.
    pub title: String,
    /// Hours.
    pub credits: Credits,
    /// The institution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
    /// The Rice course this counts as; the entry ticket to prerequisite and duplicate checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rice_equivalent: Option<CourseCode>,
    /// Absent means the Rice equivalent's published hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits_source: Option<CreditsSource>,
    /// Per-program overrides.
    pub fills: Vec<RequirementId>,
    /// Bases for pins the filter rejects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<FillClaim>,
    /// A note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Season;

    fn position() -> TermPosition {
        TermPosition {
            academic_year: 2027,
            season: Season::Fall,
        }
    }

    #[test]
    fn off_term_holds_no_credits() {
        let term = PlanTerm {
            id: TermId(Uuid::nil()),
            position: position(),
            label: Some("Co-op".to_owned()),
            kind: TermKind::Off,
            non_course: vec![],
        };
        assert_eq!(term.planned_credits(), Credits::ZERO);
        assert_eq!(serde_json::to_string(&term.kind).unwrap(), "\"off\"");
    }

    #[test]
    fn term_kinds_are_externally_tagged() {
        let rice = TermKind::Rice {
            code: None,
            courses: vec![],
        };
        assert_eq!(
            serde_json::to_string(&rice).unwrap(),
            r#"{"rice":{"courses":[]}}"#
        );
        let away = TermKind::Away { cards: vec![] };
        assert_eq!(
            serde_json::to_string(&away).unwrap(),
            r#"{"away":{"cards":[]}}"#
        );
        assert_eq!(
            serde_json::from_str::<TermKind>("\"off\"").unwrap(),
            TermKind::Off
        );
    }

    #[test]
    fn planned_credits_saturate() {
        let card = |cents: u16| ManualCourseCard {
            id: EntryId(Uuid::nil()),
            origin: CreditOrigin::Transfer,
            code: "X 1".to_owned(),
            title: String::new(),
            credits: Credits::from_cents(cents),
            institution: None,
            rice_equivalent: None,
            credits_source: None,
            fills: vec![],
            claims: vec![],
            note: None,
        };
        let term = PlanTerm {
            id: TermId(Uuid::nil()),
            position: position(),
            label: None,
            kind: TermKind::Away {
                cards: vec![card(u16::MAX), card(1)],
            },
            non_course: vec![],
        };
        assert_eq!(term.planned_credits(), Credits::from_cents(u16::MAX));
    }

    #[test]
    fn fill_basis_round_trips() {
        let basis = FillBasis::AdvisorApproved {
            who: Some("J. Smith".to_owned()),
            on: None,
        };
        let json = serde_json::to_string(&basis).unwrap();
        assert_eq!(json, r#"{"kind":"advisor_approved","who":"J. Smith"}"#);
        assert_eq!(serde_json::from_str::<FillBasis>(&json).unwrap(), basis);
        assert_eq!(
            serde_json::to_string(&FillBasis::Unsure).unwrap(),
            r#"{"kind":"unsure"}"#
        );
    }
}
