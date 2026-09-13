//! The evaluation function: a plan, the programs it claims and the facts
//! about its courses in; one `Report` out. Total: a report for any input,
//! never an error.
//!
//! Steps, per program: collect every card in board order, resolved to
//! canonical codes; flatten the tree to slots in document order; apply the
//! student's choices; match remaining cards to remaining slots by maximum
//! bipartite matching; run the credit rules; fold outcomes and progress up
//! the tree.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ENGINE_VERSION;
use crate::catalog::Attribute;
use crate::code::CourseCode;
use crate::plan::{
    CreditOrigin, EntryId, FillClaim, Plan, PlanId, SelfCheckReason, TermId, TermKind,
};
use crate::prereq::{Exclusion, Prerequisite};
use crate::program::{
    CatalogYear, CourseFilter, FilterMatch, Program, ProgramId, Requirement, RequirementBody,
    RequirementId, SourceRef,
};
use crate::term::{CreditRange, Credits, Season, TermPosition};
use crate::warn::{CreditLimits, InvalidationDetail, Warning, report_warnings, warnings};

/// What the engine knows about one course, from the catalog record plus the
/// terms we scraped. Carries no prerequisites and no exclusions: those are
/// per-course rows on `PlanBundle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseInfo {
    /// The canonical code.
    pub code: CourseCode,
    /// Rice's long title, for cards and suggestion chips.
    pub title: String,
    /// Read only when the GA prints no hours for a rule.
    pub credits: CreditRange,
    /// The only field an `Attribute` selector reads.
    pub attributes: BTreeSet<Attribute>,
    /// Rice's "Department:" line. Not the subject code: FREN and ITAL share one department.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// `CourseFlags::repeatable`; read by the duplicate check.
    pub repeatable: bool,
    /// Read by the season check.
    pub seasons_offered: Vec<Season>,
    /// How many terms we have seen the course in.
    pub terms_observed: u8,
    /// Offered in the current term; drives the suggestion chips' dot.
    pub offered_now: bool,
}

/// The facts for the courses a plan names. `BTreeMap`, not `HashMap`:
/// iteration order must match on server and browser. Maps are private
/// because a struct-keyed map cannot cross serde; the type serialises as two
/// vectors.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CourseFacts {
    by_code: BTreeMap<CourseCode, CourseInfo>,
    aliases: BTreeMap<CourseCode, CourseCode>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, rename = "CourseFacts"))]
struct CourseFactsWire {
    courses: Vec<CourseInfo>,
    aliases: Vec<(CourseCode, CourseCode)>,
}

impl Serialize for CourseFacts {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CourseFactsWire {
            courses: self.by_code.values().cloned().collect(),
            aliases: self
                .aliases
                .iter()
                .map(|(a, c)| (a.clone(), c.clone()))
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CourseFacts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CourseFactsWire::deserialize(deserializer)?;
        let mut facts = Self::default();
        for info in wire.courses {
            facts.insert(info);
        }
        for (alias, canonical) in wire.aliases {
            facts.add_alias(alias, canonical);
        }
        Ok(facts)
    }
}

impl CourseFacts {
    /// Add or replace the facts for a course, keyed by its code.
    pub fn insert(&mut self, info: CourseInfo) {
        self.by_code.insert(info.code.clone(), info);
    }

    /// Record that `alias` is the same course as `canonical`.
    pub fn add_alias(&mut self, alias: CourseCode, canonical: CourseCode) {
        self.aliases.insert(alias, canonical);
    }

    /// The first code the General Announcements print. An unknown code comes
    /// back unchanged. Every code comparison calls this first.
    #[must_use]
    pub fn canonical(&self, code: &CourseCode) -> CourseCode {
        self.aliases
            .get(code)
            .cloned()
            .unwrap_or_else(|| code.clone())
    }

    /// What we hold about a course, by canonical code.
    #[must_use]
    pub fn get(&self, code: &CourseCode) -> Option<&CourseInfo> {
        self.by_code.get(&self.canonical(code))
    }

    /// Every course we hold facts for, in code order.
    pub fn courses(&self) -> impl Iterator<Item = &CourseInfo> {
        self.by_code.values()
    }

    /// Every alias pair, in alias order.
    pub fn aliases(&self) -> impl Iterator<Item = (&CourseCode, &CourseCode)> {
        self.aliases.iter()
    }
}

/// The only argument `evaluate` takes, and the body of the bundle endpoint.
/// This plan's courses only: a term's sections never reach the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanBundle {
    /// The plan.
    pub plan: Plan,
    /// One published version per id in `plan.programs` where held; missing
    /// is not an error. Several versions of one id may be sent; the engine
    /// picks by catalog year.
    pub programs: Vec<Program>,
    /// Facts for the plan's courses.
    #[cfg_attr(feature = "ts", ts(as = "CourseFactsWire"))]
    pub facts: CourseFacts,
    /// One row per course in the plan.
    pub prerequisites: Vec<Prerequisite>,
    /// Folded exclusion rows.
    pub exclusions: Vec<Exclusion>,
    /// Rice's normal semester load.
    pub limits: CreditLimits,
    /// From the diff job; may be empty.
    #[serde(default)]
    pub invalidations: Vec<InvalidationDetail>,
    /// Core has no clock: anything before this is done.
    pub today: TermPosition,
}

/// Whether a rule is met.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Outcome {
    /// The plan satisfies the rule.
    Met,
    /// Some of it.
    Partial,
    /// None of it.
    Unmet,
    /// Cannot decide. Out of every `Progress` count but the self-check counts.
    NeedsStudentCheck {
        /// The student's reason, when ticked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confirmed: Option<SelfCheckReason>,
    },
}

/// Counts for one rule and everything under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Progress {
    /// Checkable rules that are met.
    pub requirements_met: u16,
    /// Rules the engine can decide; self-checks excluded.
    pub requirements_checkable: u16,
    /// Rules held only by a pinned card the filter rejects: "on your say-so", never met.
    pub requirements_claimed: u16,
    /// Hours on the cards filling rules here.
    pub credits_met: Credits,
    /// Hours the rules ask for.
    pub credits_required: Credits,
    /// Above zero, `credits_required` is a lower bound and the interface says so.
    pub credits_unknown: u16,
    /// Reported, never counted above.
    pub self_checks: u16,
    /// Self-checks the student ticked.
    pub self_checks_confirmed: u16,
}

impl Progress {
    /// Rules met over rules checkable, rounded down; 100 when nothing is required.
    #[must_use]
    pub fn requirements_percent(&self) -> u8 {
        percent(
            u32::from(self.requirements_met),
            u32::from(self.requirements_checkable),
        )
    }

    /// Hours met over hours required, rounded down; 100 when nothing is required.
    #[must_use]
    pub fn credits_percent(&self) -> u8 {
        percent(
            u32::from(self.credits_met.cents()),
            u32::from(self.credits_required.cents()),
        )
    }

    fn add(&mut self, other: Self) {
        self.requirements_met = self.requirements_met.saturating_add(other.requirements_met);
        self.requirements_checkable = self
            .requirements_checkable
            .saturating_add(other.requirements_checkable);
        self.requirements_claimed = self
            .requirements_claimed
            .saturating_add(other.requirements_claimed);
        self.credits_met = self.credits_met.saturating_add(other.credits_met);
        self.credits_required = self.credits_required.saturating_add(other.credits_required);
        self.credits_unknown = self.credits_unknown.saturating_add(other.credits_unknown);
        self.self_checks = self.self_checks.saturating_add(other.self_checks);
        self.self_checks_confirmed = self
            .self_checks_confirmed
            .saturating_add(other.self_checks_confirmed);
    }
}

fn percent(numerator: u32, denominator: u32) -> u8 {
    if denominator == 0 {
        return 100;
    }
    let value = numerator.saturating_mul(100) / denominator;
    u8::try_from(value.min(100)).unwrap_or(100)
}

/// One rule's outcome, with everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequirementReport {
    /// Which rule.
    pub requirement: RequirementId,
    /// Its label.
    pub label: String,
    /// Its source.
    pub source: SourceRef,
    /// The verdict.
    pub outcome: Outcome,
    /// This rule and everything under it.
    pub progress: Progress,
    /// Board order. Card ids, not codes: a lesson taken four times is four cards.
    pub filled_by: Vec<EntryId>,
    /// The subset of `filled_by` that sits there by claim, not by match.
    pub claimed_by: Vec<EntryId>,
    /// From a `NonCourseClaim`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_in: Option<TermId>,
    /// Children, document order.
    pub children: Vec<RequirementReport>,
}

impl RequirementReport {
    /// `Met` and every self-check under it confirmed. The interface calls
    /// this, not a match on `outcome`, before a green tick.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.outcome == Outcome::Met
            && self.progress.self_checks == self.progress.self_checks_confirmed
    }

    fn walk<'a>(&'a self, out: &mut Vec<&'a RequirementReport>) {
        out.push(self);
        for child in &self.children {
            child.walk(out);
        }
    }
}

/// One program's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProgramReport {
    /// Which program.
    pub program: ProgramId,
    /// Copied so the sidebar needs only the report.
    pub name: String,
    /// The year the plan asked for.
    pub catalog_year: CatalogYear,
    /// Differs only on a substitution, which warns.
    pub evaluated_with: CatalogYear,
    /// The tree.
    pub root: RequirementReport,
    /// The program's totals; `credits_required` is the declared total when there is one.
    pub progress: Progress,
    /// `Program::total_credits`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_credits: Option<Credits>,
}

/// Returned by `evaluate`, crosses the WASM boundary, returned unchanged by
/// the report endpoint, drawn into the PDF. One report shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Report {
    /// Which plan.
    pub plan: PlanId,
    /// `ENGINE_VERSION`.
    pub engine_version: String,
    /// Sidebar order. A program with nothing published is absent here, and warned.
    pub programs: Vec<ProgramReport>,
    /// Not the sum of program totals: a course in two programs is still one course.
    pub progress: Progress,
    /// One flat list, so no caller can compute a report and forget them.
    pub warnings: Vec<Warning>,
}

impl Report {
    /// Every rule report in document order, program by program.
    #[must_use]
    pub fn rules(&self) -> Vec<&RequirementReport> {
        let mut out = Vec::new();
        for program in &self.programs {
            program.root.walk(&mut out);
        }
        out
    }
}

/// A card as the matcher sees it: canonical code, hours, and choices.
#[derive(Debug, Clone)]
pub(crate) struct Card {
    pub(crate) entry: EntryId,
    pub(crate) term: Option<TermId>,
    pub(crate) code: Option<CourseCode>,
    pub(crate) credits: Credits,
    pub(crate) fills: Vec<RequirementId>,
    pub(crate) claims: Vec<FillClaim>,
    pub(crate) origin: Option<CreditOrigin>,
}

impl Card {
    fn bars_attribute_slots(&self) -> bool {
        self.origin.is_some_and(CreditOrigin::bars_attribute_slots)
    }

    fn basis_for(&self, requirement: RequirementId) -> Option<crate::plan::FillBasis> {
        self.claims
            .iter()
            .find(|c| c.requirement == requirement)
            .map(|c| c.basis.clone())
    }
}

/// Every card on the board in board order: incoming credit, then each term's
/// cards. Manual cards carry a code only through `rice_equivalent`.
pub(crate) fn collect_cards(plan: &Plan, facts: &CourseFacts) -> Vec<Card> {
    let mut cards = Vec::new();
    for card in &plan.incoming_credit {
        cards.push(Card {
            entry: card.id,
            term: None,
            code: card.rice_equivalent.as_ref().map(|c| facts.canonical(c)),
            credits: card.credits,
            fills: card.fills.clone(),
            claims: card.claims.clone(),
            origin: Some(card.origin),
        });
    }
    for term in &plan.terms {
        match &term.kind {
            TermKind::Rice { courses, .. } => {
                for course in courses {
                    cards.push(Card {
                        entry: course.id,
                        term: Some(term.id),
                        code: Some(facts.canonical(&course.course)),
                        credits: course.credits,
                        fills: course.fills.clone(),
                        claims: course.claims.clone(),
                        origin: None,
                    });
                }
            }
            TermKind::Away { cards: manual } => {
                for card in manual {
                    cards.push(Card {
                        entry: card.id,
                        term: Some(term.id),
                        code: card.rice_equivalent.as_ref().map(|c| facts.canonical(c)),
                        credits: card.credits,
                        fills: card.fills.clone(),
                        claims: card.claims.clone(),
                        origin: Some(card.origin),
                    });
                }
            }
            TermKind::Off => {}
        }
    }
    cards
}

#[derive(Debug, Clone)]
struct Slot<'a> {
    requirement: RequirementId,
    filter: &'a CourseFilter,
}

fn flatten_slots<'a>(requirement: &'a Requirement, out: &mut Vec<Slot<'a>>) {
    match &requirement.body {
        RequirementBody::Course { filter, semesters } => {
            for _ in 0..*semesters {
                out.push(Slot {
                    requirement: requirement.id,
                    filter,
                });
            }
        }
        RequirementBody::All { of } | RequirementBody::Select { of, .. } => {
            for child in of {
                flatten_slots(child, out);
            }
        }
        RequirementBody::Credits { .. }
        | RequirementBody::NonCourse { .. }
        | RequirementBody::Unverifiable { .. }
        | RequirementBody::DistinctDepartments { .. } => {}
    }
}

/// Which cards each requirement received, in board order, and which of
/// those sit there by claim rather than by match.
#[derive(Debug, Default)]
struct Matching {
    assigned: BTreeMap<RequirementId, Vec<usize>>,
    claimed: BTreeSet<EntryId>,
}

impl Matching {
    fn give(&mut self, requirement: RequirementId, card: usize) {
        self.assigned.entry(requirement).or_default().push(card);
    }
}

/// Maximum bipartite matching by augmenting paths. Slots are visited in
/// document order and candidates in board order, so assignment is
/// deterministic. Greedy gets the COMP 140 case wrong: if COMP 140 fits a
/// core slot or a distribution slot and a second card fits only the
/// distribution slot, COMP 140 must take the core slot.
fn match_slots(candidates: &[Vec<usize>], card_count: usize) -> Vec<Option<usize>> {
    let mut slot_of_card: Vec<Option<usize>> = vec![None; card_count];
    for slot in 0..candidates.len() {
        let mut seen = vec![false; card_count];
        augment(slot, candidates, &mut seen, &mut slot_of_card);
    }
    slot_of_card
}

fn augment(
    slot: usize,
    candidates: &[Vec<usize>],
    seen: &mut [bool],
    slot_of_card: &mut [Option<usize>],
) -> bool {
    let Some(cards) = candidates.get(slot) else {
        return false;
    };
    for &card in cards {
        // An out-of-range card index counts as already used.
        let Some(was_seen) = seen.get(card).copied() else {
            continue;
        };
        if was_seen {
            continue;
        }
        if let Some(flag) = seen.get_mut(card) {
            *flag = true;
        }
        let holder = slot_of_card.get(card).copied().flatten();
        let free = match holder {
            None => true,
            Some(other) => augment(other, candidates, seen, slot_of_card),
        };
        if free && let Some(entry) = slot_of_card.get_mut(card) {
            *entry = Some(slot);
            return true;
        }
    }
    false
}

/// Everything one program evaluation needs to read.
struct ProgramContext<'a> {
    program: &'a Program,
    cards: &'a [Card],
    facts: &'a CourseFacts,
    matching: Matching,
    consumed: BTreeSet<EntryId>,
    confirmed: BTreeMap<RequirementId, SelfCheckReason>,
    claimed_in: BTreeMap<RequirementId, TermId>,
}

/// Choices first, then the free pool through the matcher.
fn match_cards(
    program: &Program,
    cards: &[Card],
    facts: &CourseFacts,
    warnings: &mut Vec<Warning>,
) -> Matching {
    let mut slots = Vec::new();
    flatten_slots(&program.root, &mut slots);
    let mut matching = Matching::default();
    let mut slot_taken = vec![false; slots.len()];
    let slot_requirements: BTreeSet<RequirementId> = slots.iter().map(|s| s.requirement).collect();
    let mut free: Vec<usize> = Vec::new();

    for (index, card) in cards.iter().enumerate() {
        let Some(choice) = card
            .fills
            .iter()
            .copied()
            .find(|r| slot_requirements.contains(r))
        else {
            free.push(index);
            continue;
        };
        let Some(slot_index) = slots
            .iter()
            .enumerate()
            .position(|(i, s)| s.requirement == choice && !slot_taken[i])
        else {
            free.push(index);
            continue;
        };
        slot_taken[slot_index] = true;
        matching.give(choice, index);
        let Some(slot) = slots.get(slot_index) else {
            continue;
        };
        // A pin the filter rejects is a claim: shown in the slot, never
        // counted as met, and it must say why. AP/IB on an attribute slot is
        // one too, whatever the filter says, because Rice's own policy rejects it.
        let barred = card.bars_attribute_slots() && slot.filter.needs_attribute();
        let rejected = card
            .code
            .as_ref()
            .is_none_or(|code| slot.filter.matches(code, facts) == FilterMatch::No);
        if barred {
            matching.claimed.insert(card.entry);
            if let Some(origin) = card.origin {
                warnings.push(Warning::IncomingCreditIneligible {
                    entry: card.entry,
                    requirement: choice,
                    origin,
                });
            }
        } else if rejected {
            matching.claimed.insert(card.entry);
            if let Some(term) = card.term {
                warnings.push(Warning::RequirementChoiceUnmatched {
                    term,
                    entry: card.entry,
                    requirement: choice,
                    basis: card.basis_for(choice),
                });
            }
        }
    }

    let mut candidates: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    let mut unknown_warned: BTreeSet<EntryId> = BTreeSet::new();
    for (free_index, &card_index) in free.iter().enumerate() {
        let Some(card) = cards.get(card_index) else {
            continue;
        };
        let Some(code) = &card.code else {
            continue;
        };
        for (slot_index, slot) in slots.iter().enumerate() {
            if slot_taken[slot_index]
                || (card.bars_attribute_slots() && slot.filter.needs_attribute())
            {
                continue;
            }
            match slot.filter.matches(code, facts) {
                FilterMatch::Yes => {
                    if let Some(list) = candidates.get_mut(slot_index) {
                        list.push(free_index);
                    }
                }
                FilterMatch::Unknown => {
                    if let Some(term) = card.term
                        && unknown_warned.insert(card.entry)
                    {
                        warnings.push(Warning::AttributeUnknown {
                            term,
                            entry: card.entry,
                            course: code.clone(),
                        });
                    }
                }
                FilterMatch::No => {}
            }
        }
    }
    let slot_of_card = match_slots(&candidates, free.len());
    // Hand out in board order so `filled_by` and credit sums stay board-ordered.
    for (free_index, &card_index) in free.iter().enumerate() {
        if let Some(Some(slot_index)) = slot_of_card.get(free_index)
            && let Some(slot) = slots.get(*slot_index)
        {
            matching.give(slot.requirement, card_index);
        }
    }
    matching
}

impl ProgramContext<'_> {
    fn card(&self, index: usize) -> Option<&Card> {
        self.cards.get(index)
    }

    fn assigned(&self, requirement: RequirementId) -> &[usize] {
        self.matching
            .assigned
            .get(&requirement)
            .map_or(&[], Vec::as_slice)
    }

    fn credits_of(&self, indices: &[usize]) -> Credits {
        crate::plan::sum_credits(
            indices
                .iter()
                .filter_map(|i| self.card(*i))
                .map(|c| c.credits),
        )
    }

    fn self_check(&self, requirement: &Requirement) -> RequirementReport {
        let reason = self.confirmed.get(&requirement.id).copied();
        let progress = Progress {
            self_checks: 1,
            self_checks_confirmed: u16::from(reason.is_some()),
            ..Progress::default()
        };
        self.leaf(
            requirement,
            Outcome::NeedsStudentCheck { confirmed: reason },
            progress,
            vec![],
            vec![],
        )
    }

    fn leaf(
        &self,
        requirement: &Requirement,
        outcome: Outcome,
        progress: Progress,
        filled_by: Vec<EntryId>,
        claimed_by: Vec<EntryId>,
    ) -> RequirementReport {
        RequirementReport {
            requirement: requirement.id,
            label: requirement.label.clone(),
            source: requirement.source.clone(),
            outcome,
            progress,
            filled_by,
            claimed_by,
            claimed_in: self.claimed_in.get(&requirement.id).copied(),
            children: vec![],
        }
    }

    fn course_rule(
        &self,
        requirement: &Requirement,
        filter: &CourseFilter,
        semesters: u8,
    ) -> RequirementReport {
        let filled = self.assigned(requirement.id);
        let matched = filled
            .iter()
            .filter(|i| {
                self.card(**i)
                    .is_some_and(|c| !self.matching.claimed.contains(&c.entry))
            })
            .count();
        let met = matched >= usize::from(semesters);
        let mut progress = Progress {
            requirements_checkable: 1,
            requirements_met: u16::from(met),
            requirements_claimed: u16::from(!met && filled.len() > matched),
            credits_met: self.credits_of(filled),
            ..Progress::default()
        };
        match requirement.hours {
            Some(hours) => progress.credits_required = hours.min().saturating_mul(semesters),
            None => match filter.single_code().and_then(|code| self.facts.get(code)) {
                Some(info) => {
                    progress.credits_required = info.credits.min().saturating_mul(semesters);
                }
                None => progress.credits_unknown = 1,
            },
        }
        let outcome = if met {
            Outcome::Met
        } else if filled.is_empty() {
            Outcome::Unmet
        } else {
            Outcome::Partial
        };
        let entries: Vec<EntryId> = filled
            .iter()
            .filter_map(|i| self.card(*i))
            .map(|c| c.entry)
            .collect();
        let claimed_by = entries
            .iter()
            .copied()
            .filter(|e| self.matching.claimed.contains(e))
            .collect();
        self.leaf(requirement, outcome, progress, entries, claimed_by)
    }

    fn credits_rule(
        &self,
        requirement: &Requirement,
        minimum: Credits,
        scope: crate::program::CreditScope,
        from: &CourseFilter,
    ) -> RequirementReport {
        // `Additional` counts only cards filling no other requirement; a
        // free-elective allowance consumes cards in board order until it is
        // full, so fills-no-requirement fires only past the allowance.
        let mut filled_by = Vec::new();
        let mut total = Credits::ZERO;
        for card in self.cards {
            if total >= minimum {
                break;
            }
            let eligible = match &card.code {
                None => from.include.is_empty(),
                Some(code) => from.matches(code, self.facts) == FilterMatch::Yes,
            };
            let free =
                scope == crate::program::CreditScope::Any || !self.consumed.contains(&card.entry);
            if eligible && free {
                filled_by.push(card.entry);
                total = total.saturating_add(card.credits);
            }
        }
        let met = total >= minimum;
        let progress = Progress {
            requirements_checkable: 1,
            requirements_met: u16::from(met),
            credits_met: total,
            credits_required: minimum,
            ..Progress::default()
        };
        let outcome = if met {
            Outcome::Met
        } else if total > Credits::ZERO {
            Outcome::Partial
        } else {
            Outcome::Unmet
        };
        self.leaf(requirement, outcome, progress, filled_by, vec![])
    }

    /// The departments of the cards in the sibling course slots, and how
    /// many slots those siblings hold.
    fn distinct_departments(
        &self,
        requirement: &Requirement,
        siblings: &[Requirement],
        minimum: u8,
    ) -> RequirementReport {
        let mut entries = Vec::new();
        let mut departments: Vec<Option<&str>> = Vec::new();
        let mut slot_count = 0usize;
        for sibling in siblings {
            let RequirementBody::Course { semesters, .. } = &sibling.body else {
                continue;
            };
            slot_count += usize::from(*semesters);
            for card in self
                .assigned(sibling.id)
                .iter()
                .filter_map(|i| self.card(*i))
            {
                entries.push(card.entry);
                departments.push(
                    card.code
                        .as_ref()
                        .and_then(|code| self.facts.get(code))
                        .and_then(|info| info.department.as_deref()),
                );
            }
        }
        if departments.iter().any(Option::is_none) {
            // A card we hold no department for: the student's word.
            let mut report = self.self_check(requirement);
            report.filled_by = entries;
            return report;
        }
        let distinct: BTreeSet<&str> = departments.iter().flatten().copied().collect();
        let met = entries.len() >= slot_count && distinct.len() >= usize::from(minimum);
        let progress = Progress {
            requirements_checkable: 1,
            requirements_met: u16::from(met),
            ..Progress::default()
        };
        let outcome = if met {
            Outcome::Met
        } else if entries.is_empty() {
            Outcome::Unmet
        } else {
            Outcome::Partial
        };
        self.leaf(requirement, outcome, progress, entries, vec![])
    }

    fn group(
        &self,
        requirement: &Requirement,
        of: &[Requirement],
        select: Option<u8>,
    ) -> RequirementReport {
        let children: Vec<RequirementReport> = of
            .iter()
            .map(|child| match &child.body {
                RequirementBody::DistinctDepartments { minimum, .. } => {
                    self.distinct_departments(child, of, *minimum)
                }
                _ => self.requirement(child),
            })
            .collect();
        let mut progress = Progress::default();
        let mut met_children = 0u16;
        let mut checkable_children = 0u16;
        let mut any_progress = false;
        let mut any_self_check = false;
        for child in &children {
            progress.add(child.progress);
            if child.progress.requirements_checkable > 0 {
                checkable_children = checkable_children.saturating_add(1);
                if child.outcome == Outcome::Met {
                    met_children = met_children.saturating_add(1);
                }
            }
            match &child.outcome {
                Outcome::Met | Outcome::Partial => any_progress = true,
                Outcome::NeedsStudentCheck { confirmed } => {
                    any_self_check = true;
                    any_progress |= confirmed.is_some();
                }
                Outcome::Unmet => {}
            }
        }
        let outcome = match select {
            Some(count) => {
                progress.requirements_met = met_children.min(u16::from(count));
                progress.requirements_checkable = u16::from(count);
                if let Some(hours) = requirement.hours {
                    progress.credits_required = hours.min();
                } else {
                    progress.credits_required = Credits::ZERO;
                    progress.credits_unknown = progress.credits_unknown.saturating_add(1);
                }
                if met_children >= u16::from(count) {
                    Outcome::Met
                } else if any_progress {
                    Outcome::Partial
                } else {
                    Outcome::Unmet
                }
            }
            None => {
                if checkable_children == 0 && any_self_check {
                    Outcome::NeedsStudentCheck { confirmed: None }
                } else if met_children == checkable_children {
                    Outcome::Met
                } else if any_progress {
                    Outcome::Partial
                } else {
                    Outcome::Unmet
                }
            }
        };
        RequirementReport {
            requirement: requirement.id,
            label: requirement.label.clone(),
            source: requirement.source.clone(),
            outcome,
            progress,
            filled_by: vec![],
            claimed_by: vec![],
            claimed_in: self.claimed_in.get(&requirement.id).copied(),
            children,
        }
    }

    fn requirement(&self, requirement: &Requirement) -> RequirementReport {
        match &requirement.body {
            RequirementBody::All { of } => self.group(requirement, of, None),
            RequirementBody::Select { count, of } => self.group(requirement, of, Some(*count)),
            RequirementBody::Course { filter, semesters } => {
                self.course_rule(requirement, filter, *semesters)
            }
            RequirementBody::Credits {
                minimum,
                scope,
                from,
            } => self.credits_rule(requirement, *minimum, *scope, from),
            RequirementBody::NonCourse { .. }
            | RequirementBody::Unverifiable { .. }
            | RequirementBody::DistinctDepartments { .. } => self.self_check(requirement),
        }
    }
}

fn program_report(
    plan: &Plan,
    program: &Program,
    cards: &[Card],
    facts: &CourseFacts,
    warnings: &mut Vec<Warning>,
) -> ProgramReport {
    let matching = match_cards(program, cards, facts, warnings);
    let consumed: BTreeSet<EntryId> = matching
        .assigned
        .values()
        .flatten()
        .filter_map(|i| cards.get(*i))
        .map(|c| c.entry)
        .collect();
    let context = ProgramContext {
        program,
        cards,
        facts,
        matching,
        consumed,
        confirmed: plan
            .self_checks
            .iter()
            .map(|c| (c.requirement, c.reason))
            .collect(),
        claimed_in: plan
            .terms
            .iter()
            .flat_map(|t| t.non_course.iter().map(move |c| (c.requirement, t.id)))
            .collect(),
    };
    let root = context.requirement(&context.program.root);
    let mut progress = root.progress;
    if let Some(total) = program.total_credits {
        progress.credits_required = total;
    }
    ProgramReport {
        program: program.id,
        name: program.name.clone(),
        catalog_year: plan.catalog_year,
        evaluated_with: program.catalog_year,
        root,
        progress,
        declared_credits: program.total_credits,
    }
}

/// One program at a time, as the requirements sidebar refreshes it. The
/// program-scoped warnings are dropped; `evaluate` keeps them.
#[must_use]
pub fn evaluate_program(plan: &Plan, program: &Program, facts: &CourseFacts) -> ProgramReport {
    let cards = collect_cards(plan, facts);
    let mut warnings = Vec::new();
    program_report(plan, program, &cards, facts, &mut warnings)
}

/// Pick the version to evaluate: the plan's year when held, else the
/// earliest later year, else the latest earlier year.
fn pick_version<'a>(versions: &[&'a Program], wanted: CatalogYear) -> Option<&'a Program> {
    if let Some(exact) = versions.iter().find(|p| p.catalog_year == wanted) {
        return Some(exact);
    }
    let later = versions
        .iter()
        .filter(|p| p.catalog_year > wanted)
        .min_by_key(|p| p.catalog_year);
    if let Some(later) = later {
        return Some(later);
    }
    versions
        .iter()
        .filter(|p| p.catalog_year < wanted)
        .max_by_key(|p| p.catalog_year)
        .copied()
}

/// Choices that point at a rule no held program version has.
fn choice_missing(bundle: &PlanBundle, cards: &[Card], chosen: &[&Program]) -> Vec<Warning> {
    let live: BTreeSet<RequirementId> = chosen
        .iter()
        .flat_map(|p| p.root.flatten().into_iter().map(|r| r.id))
        .collect();
    let retired: BTreeSet<RequirementId> = bundle
        .programs
        .iter()
        .flat_map(|p| p.retired_requirements.iter().copied())
        .collect();
    let mut out = Vec::new();
    for card in cards {
        let Some(term) = card.term else {
            continue;
        };
        for requirement in &card.fills {
            if !live.contains(requirement) {
                out.push(Warning::RequirementChoiceMissing {
                    term,
                    entry: card.entry,
                    requirement: *requirement,
                    retired: retired.contains(requirement),
                });
            }
        }
    }
    out
}

/// Total: returns a report for any input and never fails, because a rule
/// the engine cannot verify is already `RequirementBody::Unverifiable`
/// before it arrives. The missing `Result` is the product rule written as a
/// signature.
#[must_use]
pub fn evaluate(bundle: &PlanBundle) -> Report {
    let plan = &bundle.plan;
    let facts = &bundle.facts;
    let cards = collect_cards(plan, facts);
    let mut program_warnings = Vec::new();
    let mut reports = Vec::new();
    let mut chosen: Vec<&Program> = Vec::new();
    for id in &plan.programs {
        let versions: Vec<&Program> = bundle.programs.iter().filter(|p| p.id == *id).collect();
        let Some(program) = pick_version(&versions, plan.catalog_year) else {
            program_warnings.push(Warning::ProgramUnavailable {
                program: *id,
                catalog_year: plan.catalog_year,
            });
            continue;
        };
        if program.catalog_year != plan.catalog_year {
            program_warnings.push(Warning::ProgramYearSubstituted {
                program: *id,
                wanted: plan.catalog_year,
                used: program.catalog_year,
            });
        }
        chosen.push(program);
        reports.push(program_report(
            plan,
            program,
            &cards,
            facts,
            &mut program_warnings,
        ));
    }
    program_warnings.extend(choice_missing(bundle, &cards, &chosen));

    // Plan-level progress: a course in two programs is still one course.
    let mut progress = Progress::default();
    for report in &reports {
        let p = report.progress;
        progress.add(Progress {
            credits_met: Credits::ZERO,
            credits_required: Credits::ZERO,
            ..p
        });
    }
    progress.credits_met = crate::plan::sum_credits(cards.iter().map(|c| c.credits));
    progress.credits_required = reports
        .iter()
        .filter_map(|r| r.declared_credits)
        .max()
        .unwrap_or(Credits::ZERO);

    let mut report = Report {
        plan: plan.id,
        engine_version: ENGINE_VERSION.to_owned(),
        programs: reports,
        progress,
        warnings: program_warnings,
    };
    let rest = warnings(bundle, &report.rules());
    report.warnings.extend(rest);
    let rest = report_warnings(&report, &chosen, &cards);
    report.warnings.extend(rest);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_prefers_the_constrained_requirement() {
        // Slot 0 (core) takes card 0 only; slot 1 (distribution) takes card
        // 0 or card 1. Greedy in slot order already works here, so test the
        // reverse: slot 0 takes either, slot 1 takes only card 0.
        let candidates = vec![vec![0, 1], vec![0]];
        let assignment = match_slots(&candidates, 2);
        assert_eq!(assignment, vec![Some(1), Some(0)]);
    }

    #[test]
    fn matching_tolerates_bad_indices() {
        let candidates = vec![vec![7], vec![0]];
        let assignment = match_slots(&candidates, 1);
        assert_eq!(assignment, vec![Some(1)]);
        assert_eq!(match_slots(&[], 0), Vec::<Option<usize>>::new());
    }

    #[test]
    fn percent_rounds_down_and_caps() {
        assert_eq!(percent(0, 0), 100);
        assert_eq!(percent(1, 3), 33);
        assert_eq!(percent(5, 4), 100);
    }

    #[test]
    fn facts_serialise_as_two_vectors() {
        let mut facts = CourseFacts::default();
        let stat = CourseCode::parse("STAT 310").unwrap();
        let econ = CourseCode::parse("ECON 307").unwrap();
        facts.insert(CourseInfo {
            code: stat.clone(),
            title: "PROBABILITY AND STATISTICS".to_owned(),
            credits: CreditRange::Fixed(Credits::from_cents(300)),
            attributes: BTreeSet::new(),
            department: Some("Statistics".to_owned()),
            repeatable: false,
            seasons_offered: vec![Season::Fall],
            terms_observed: 1,
            offered_now: true,
        });
        facts.add_alias(econ.clone(), stat.clone());
        assert_eq!(facts.canonical(&econ), stat);
        assert!(facts.get(&econ).is_some());
        let json = serde_json::to_string(&facts).unwrap();
        assert!(json.starts_with(r#"{"courses":[{"code":{"subject":"STAT""#));
        let back: CourseFacts = serde_json::from_str(&json).unwrap();
        assert_eq!(back, facts);
    }
}
