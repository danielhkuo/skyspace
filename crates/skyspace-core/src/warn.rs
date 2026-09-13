//! Warnings: one flat enum carrying what is wrong and where, and one check
//! function per warning, each testable against one fixture plan. The engine
//! warns; it never blocks.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::catalog::Attribute;
use crate::code::CourseCode;
use crate::evaluate::{
    Card, CourseFacts, Outcome, PlanBundle, Report, RequirementReport, collect_cards, evaluate,
};
use crate::plan::{CreditOrigin, CreditsSource, EntryId, FillBasis, Plan, TermId, TermKind};
use crate::prereq::{
    Exclusion, PrereqFact, Prerequisite, TakenIndex, Truth, evaluate_prereq, prereq_fact,
};
use crate::program::{
    CatalogYear, FilterMatch, Program, ProgramId, ProgramKind, RequirementBody, RequirementId,
};
use crate::term::{Credits, Season, TermCode};

/// Something the student should know. No severity field and no wrapper
/// struct: badge choice is display, and it changes more often than this list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Warning {
    /// A course card that fills no slot in any program.
    FillsNoRequirement {
        /// Column.
        term: TermId,
        /// Card.
        entry: EntryId,
        /// Course.
        course: CourseCode,
    },
    /// One canonical code on two cards that fill no separate slots and are not repeatable.
    DuplicateCourse {
        /// Canonical code.
        course: CourseCode,
        /// Columns, board order; incoming credit has none.
        terms: Vec<TermId>,
    },
    /// Informational: Rice's 18-hour norm is not a cap.
    OverSemesterLoad {
        /// Column.
        term: TermId,
        /// Hours planned.
        planned: Credits,
        /// Rice's norm.
        normal: Credits,
    },
    /// No published version for that program.
    ProgramUnavailable {
        /// Program.
        program: ProgramId,
        /// The year the plan asked for.
        catalog_year: CatalogYear,
    },
    /// Evaluated against a different catalog year.
    ProgramYearSubstituted {
        /// Program.
        program: ProgramId,
        /// The year the plan asked for.
        wanted: CatalogYear,
        /// The year used.
        used: CatalogYear,
    },
    /// A choice points at a rule the course does not match.
    RequirementChoiceUnmatched {
        /// Column.
        term: TermId,
        /// Card.
        entry: EntryId,
        /// Rule.
        requirement: RequirementId,
        /// The student's basis for the pin, when recorded.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        basis: Option<FillBasis>,
    },
    /// A choice points at a rule this version lacks.
    RequirementChoiceMissing {
        /// Column.
        term: TermId,
        /// Card.
        entry: EntryId,
        /// Rule.
        requirement: RequirementId,
        /// Removed in review, as opposed to never existed.
        retired: bool,
    },
    /// An attribute rule cannot be judged: no facts for this course.
    AttributeUnknown {
        /// Column.
        term: TermId,
        /// Card.
        entry: EntryId,
        /// Course.
        course: CourseCode,
    },
    /// A prerequisite is absent, in the same term, or later. Banner enforces these at registration.
    Prerequisite {
        /// Column of the course that needs it.
        term: TermId,
        /// The course that needs it.
        course: CourseCode,
        /// The prerequisite.
        prerequisite: CourseCode,
        /// What is wrong.
        problem: PrereqProblem,
        /// The edition the rule is from.
        published_for: CatalogYear,
    },
    /// A prerequisite we could not read; Rice's text verbatim, as a self-check.
    PrerequisiteUnparsed {
        /// Column.
        term: TermId,
        /// Course.
        course: CourseCode,
        /// Rice's text.
        published: String,
    },
    /// Both halves of a mutual exclusion are in the plan.
    MutuallyExclusive {
        /// The later course.
        blocked: CourseCode,
        /// Its column.
        blocked_term: TermId,
        /// The earlier course.
        blocker: CourseCode,
        /// Its column.
        blocker_term: TermId,
        /// Rice's sentence.
        published: String,
    },
    /// The course has not been seen in this season.
    SeasonUnlikely {
        /// Column.
        term: TermId,
        /// Course.
        course: CourseCode,
        /// The season planned.
        planned_in: Season,
        /// How many terms we have seen the course in.
        terms_seen: u8,
    },
    /// From the catalog-diff job.
    PlanInvalidated {
        /// What changed.
        detail: InvalidationDetail,
    },
    /// AP and IB credit never counts toward distribution or Analyzing Diversity.
    IncomingCreditIneligible {
        /// Card.
        entry: EntryId,
        /// Rule.
        requirement: RequirementId,
        /// Where the credit came from.
        origin: CreditOrigin,
    },
    /// The catalog changed under a placed course since the student added it.
    CourseFactsChanged {
        /// Column.
        term: TermId,
        /// Card.
        entry: EntryId,
        /// Course.
        course: CourseCode,
        /// The edition the card was placed under.
        observed_year: CatalogYear,
        /// Attributes carried then and gone now.
        lost: Vec<Attribute>,
        /// Attributes gained since.
        gained: Vec<Attribute>,
        /// Hours then, when they differ.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credits_before: Option<Credits>,
        /// Hours now, when they differ.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        credits_now: Option<Credits>,
    },
    /// A manual card whose hours the student typed: hours only, unverified.
    ManualCredits {
        /// Card.
        entry: EntryId,
        /// Hours.
        credits: Credits,
    },
    /// One card filling requirements in two programs where at least one is
    /// not a major; the minor or certificate may limit overlap on its page.
    DoubleCounted {
        /// Card.
        entry: EntryId,
        /// Course.
        course: CourseCode,
        /// The programs.
        programs: Vec<ProgramId>,
    },
    /// A requirement the engine cannot verify, surfaced as its own row.
    SelfCheck {
        /// Program.
        program: ProgramId,
        /// Rule.
        requirement: RequirementId,
        /// Its label.
        label: String,
        /// What the student must check.
        text: String,
    },
}

/// What is wrong with a prerequisite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PrereqProblem {
    /// Nowhere on the board.
    NotInPlan,
    /// In the same term as the course that needs it.
    SameTerm,
    /// In a later term.
    Later {
        /// Where it sits.
        prerequisite_term: TermId,
    },
}

/// Not computed here: needs two catalog snapshots and core holds no history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum InvalidationDetail {
    /// A planned course is no longer offered.
    CourseNotOffered {
        /// Course.
        course: CourseCode,
        /// The last term it was seen.
        last_seen: TermCode,
    },
    /// A rule changed; the plan keeps its rules and the student is told.
    RequirementChanged {
        /// Rule.
        requirement: RequirementId,
        /// When.
        changed_in: CatalogYear,
    },
    /// A program was removed.
    ProgramRemoved {
        /// Program.
        program: ProgramId,
    },
}

/// Rice's normal semester load, for the informational note only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CreditLimits {
    /// 18 hours.
    pub fall_spring: Credits,
    /// 20 hours for music and architecture students.
    pub music_and_architecture: Credits,
    /// Rice publishes no summer figure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summer: Option<Credits>,
}

/// Two academic years. The archive is empty at launch, so the season check
/// is silent for about two years.
pub const MIN_TERMS_FOR_SEASON_WARNING: u8 = 4;

fn filled_entries(rules: &[&RequirementReport]) -> BTreeSet<EntryId> {
    rules
        .iter()
        .flat_map(|r| r.filled_by.iter().copied())
        .collect()
}

/// A course card in no requirement's `filled_by` across every program.
#[must_use]
pub fn fills_no_requirement(plan: &Plan, rules: &[&RequirementReport]) -> Vec<Warning> {
    let filled = filled_entries(rules);
    let mut out = Vec::new();
    for term in &plan.terms {
        let TermKind::Rice { courses, .. } = &term.kind else {
            continue;
        };
        for course in courses {
            if !filled.contains(&course.id) {
                out.push(Warning::FillsNoRequirement {
                    term: term.id,
                    entry: course.id,
                    course: course.course.clone(),
                });
            }
        }
    }
    out
}

/// One canonical code on two or more cards, unless the course is repeatable
/// or every copy fills its own slot.
#[must_use]
pub fn duplicate_courses(
    plan: &Plan,
    facts: &CourseFacts,
    rules: &[&RequirementReport],
) -> Vec<Warning> {
    let filled = filled_entries(rules);
    let mut seen: BTreeMap<CourseCode, (Vec<TermId>, Vec<EntryId>)> = BTreeMap::new();
    for card in collect_cards(plan, facts) {
        let Some(code) = card.code else {
            continue;
        };
        let entry = seen.entry(code).or_default();
        if let Some(term) = card.term {
            entry.0.push(term);
        }
        entry.1.push(card.entry);
    }
    seen.into_iter()
        .filter(|(code, (_, entries))| {
            entries.len() >= 2
                && !facts.get(code).is_some_and(|info| info.repeatable)
                && !entries.iter().all(|e| filled.contains(e))
        })
        .map(|(course, (terms, _))| Warning::DuplicateCourse { course, terms })
        .collect()
}

/// Every `PlannedCourse` in every `Rice` term against its own prerequisite row.
#[must_use]
pub fn prerequisite_problems(
    plan: &Plan,
    rows: &[Prerequisite],
    facts: &CourseFacts,
) -> Vec<Warning> {
    let index = TakenIndex::build(plan, facts);
    let mut out = Vec::new();
    for term in &plan.terms {
        let TermKind::Rice { courses, .. } = &term.kind else {
            continue;
        };
        for course in courses {
            let canonical = facts.canonical(&course.course);
            let PrereqFact::Requires {
                published_for,
                expr,
                published,
                ..
            } = prereq_fact(rows, &canonical)
            else {
                continue;
            };
            let result = evaluate_prereq(expr, term.position, &index, facts);
            match result.truth {
                Truth::Satisfied => {}
                Truth::Unknown => out.push(Warning::PrerequisiteUnparsed {
                    term: term.id,
                    course: course.course.clone(),
                    published: published.clone(),
                }),
                Truth::Missing => {
                    let problems = result
                        .missing
                        .into_iter()
                        .map(|c| (c, PrereqProblem::NotInPlan))
                        .chain(
                            result
                                .same_term
                                .into_iter()
                                .map(|c| (c, PrereqProblem::SameTerm)),
                        )
                        .chain(result.later.into_iter().map(|(c, t)| {
                            (
                                c,
                                PrereqProblem::Later {
                                    prerequisite_term: t,
                                },
                            )
                        }));
                    for (prerequisite, problem) in problems {
                        out.push(Warning::Prerequisite {
                            term: term.id,
                            course: course.course.clone(),
                            prerequisite,
                            problem,
                            published_for: *published_for,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Both codes of a row in the plan: one warning per unordered pair, naming
/// the earlier course as blocker. The check reads the plan, not the term.
#[must_use]
pub fn exclusion_problems(plan: &Plan, rows: &[Exclusion], facts: &CourseFacts) -> Vec<Warning> {
    let index = TakenIndex::build(plan, facts);
    let mut warned: BTreeSet<(CourseCode, CourseCode)> = BTreeSet::new();
    let mut out = Vec::new();
    for row in rows {
        let a = facts.canonical(&row.blocked);
        let b = facts.canonical(&row.blocker);
        let (Some(first), Some(second)) = (index.earliest(&a), index.earliest(&b)) else {
            continue;
        };
        let (Some(first_term), Some(second_term)) = (first.term, second.term) else {
            continue;
        };
        let pair = if a <= b { (a, b) } else { (b, a) };
        if !warned.insert(pair) {
            continue;
        }
        // Name the earlier course as blocker, whichever way Rice wrote it.
        let (later_code, later_term, earlier_code, earlier_term) = if second.when <= first.when {
            (
                row.blocked.clone(),
                first_term,
                row.blocker.clone(),
                second_term,
            )
        } else {
            (
                row.blocker.clone(),
                second_term,
                row.blocked.clone(),
                first_term,
            )
        };
        out.push(Warning::MutuallyExclusive {
            blocked: later_code,
            blocked_term: later_term,
            blocker: earlier_code,
            blocker_term: earlier_term,
            published: row.published.clone(),
        });
    }
    out
}

/// A course planned in a season we have never seen it offered in, once we
/// have seen enough terms to say so.
#[must_use]
pub fn season_unlikely(plan: &Plan, facts: &CourseFacts) -> Vec<Warning> {
    let mut out = Vec::new();
    for term in &plan.terms {
        let TermKind::Rice { courses, .. } = &term.kind else {
            continue;
        };
        for course in courses {
            let Some(info) = facts.get(&course.course) else {
                continue;
            };
            if info.terms_observed >= MIN_TERMS_FOR_SEASON_WARNING
                && !info.seasons_offered.contains(&term.position.season)
            {
                out.push(Warning::SeasonUnlikely {
                    term: term.id,
                    course: course.course.clone(),
                    planned_in: term.position.season,
                    terms_seen: info.terms_observed,
                });
            }
        }
    }
    out
}

/// The diff job's findings, as warnings.
#[must_use]
pub fn invalidations(details: &[InvalidationDetail]) -> Vec<Warning> {
    details
        .iter()
        .cloned()
        .map(|detail| Warning::PlanInvalidated { detail })
        .collect()
}

/// Rice terms planned above the normal load. Informational only.
#[must_use]
pub fn over_semester_load(
    plan: &Plan,
    limits: &CreditLimits,
    music_or_architecture: bool,
) -> Vec<Warning> {
    let mut out = Vec::new();
    for term in &plan.terms {
        if !matches!(term.kind, TermKind::Rice { .. }) {
            continue;
        }
        let normal = match term.position.season {
            Season::Summer => limits.summer,
            Season::Fall | Season::Spring => Some(if music_or_architecture {
                limits.music_and_architecture
            } else {
                limits.fall_spring
            }),
        };
        let planned = term.planned_credits();
        if let Some(normal) = normal
            && planned > normal
        {
            out.push(Warning::OverSemesterLoad {
                term: term.id,
                planned,
                normal,
            });
        }
    }
    out
}

/// The catalog moved under a placed course: compare what it said when the
/// student added it with what it says now.
#[must_use]
pub fn course_facts_changed(plan: &Plan, facts: &CourseFacts) -> Vec<Warning> {
    let mut out = Vec::new();
    for term in &plan.terms {
        let TermKind::Rice { courses, .. } = &term.kind else {
            continue;
        };
        for course in courses {
            let Some(was) = &course.observed else {
                continue;
            };
            let Some(now) = facts.get(&course.course) else {
                continue;
            };
            let was_attrs: BTreeSet<Attribute> = was.attributes.iter().copied().collect();
            let lost: Vec<Attribute> = was_attrs.difference(&now.attributes).copied().collect();
            let gained: Vec<Attribute> = now.attributes.difference(&was_attrs).copied().collect();
            let before = was.credits.min();
            let after = now.credits.min();
            if lost.is_empty() && gained.is_empty() && before == after {
                continue;
            }
            let (credits_before, credits_now) = if before == after {
                (None, None)
            } else {
                (Some(before), Some(after))
            };
            out.push(Warning::CourseFactsChanged {
                term: term.id,
                entry: course.id,
                course: course.course.clone(),
                observed_year: was.catalog_year,
                lost,
                gained,
                credits_before,
                credits_now,
            });
        }
    }
    out
}

/// Hours typed by hand: a manual card with no Rice equivalent, or one the
/// student overrode.
#[must_use]
pub fn manual_credits(plan: &Plan) -> Vec<Warning> {
    let away = plan.terms.iter().flat_map(|t| match &t.kind {
        TermKind::Away { cards } => cards.as_slice(),
        _ => &[],
    });
    plan.incoming_credit
        .iter()
        .chain(away)
        .filter(|card| {
            card.rice_equivalent.is_none() || card.credits_source == Some(CreditsSource::Manual)
        })
        .map(|card| Warning::ManualCredits {
            entry: card.id,
            credits: card.credits,
        })
        .collect()
}

/// One card in the slots of two programs, where at least one is not a
/// major. A note, not a fault: Rice sets no university-wide cap on courses
/// shared by two departmental majors.
#[must_use]
pub(crate) fn double_counted(
    report: &Report,
    programs: &[&Program],
    cards: &[Card],
) -> Vec<Warning> {
    let kind_of = |id: ProgramId| programs.iter().find(|p| p.id == id).map(|p| p.kind);
    let mut programs_of: BTreeMap<EntryId, Vec<ProgramId>> = BTreeMap::new();
    let mut code_of: BTreeMap<EntryId, CourseCode> = cards
        .iter()
        .filter_map(|c| c.code.clone().map(|code| (c.entry, code)))
        .collect();
    for program in &report.programs {
        if kind_of(program.program) == Some(ProgramKind::University) {
            continue;
        }
        let mut rules = Vec::new();
        walk(&program.root, &mut rules);
        for rule in rules {
            for entry in &rule.filled_by {
                programs_of.entry(*entry).or_default().push(program.program);
            }
        }
    }
    let mut out = Vec::new();
    for (entry, ids) in programs_of {
        let mut distinct: Vec<ProgramId> = Vec::new();
        for id in ids {
            if !distinct.contains(&id) {
                distinct.push(id);
            }
        }
        if distinct.len() > 1
            && distinct
                .iter()
                .any(|id| kind_of(*id) != Some(ProgramKind::Major))
            && let Some(course) = code_of.remove(&entry)
        {
            out.push(Warning::DoubleCounted {
                entry,
                course,
                programs: distinct,
            });
        }
    }
    out
}

fn walk<'a>(rule: &'a RequirementReport, out: &mut Vec<&'a RequirementReport>) {
    out.push(rule);
    for child in &rule.children {
        walk(child, out);
    }
}

/// Every unconfirmed self-check, as its own row, so it is never silent.
#[must_use]
pub fn self_checks(report: &Report, programs: &[&Program]) -> Vec<Warning> {
    let mut out = Vec::new();
    for program_report in &report.programs {
        let program = programs.iter().find(|p| p.id == program_report.program);
        let mut rules = Vec::new();
        walk(&program_report.root, &mut rules);
        for rule in rules {
            if rule.outcome != (Outcome::NeedsStudentCheck { confirmed: None }) {
                continue;
            }
            let text = program
                .and_then(|p| p.requirement(rule.requirement))
                .map_or_else(
                    || rule.label.clone(),
                    |r| match &r.body {
                        RequirementBody::NonCourse { description, .. } => description.clone(),
                        RequirementBody::Unverifiable { text }
                        | RequirementBody::DistinctDepartments { text, .. } => text.clone(),
                        _ => r.label.clone(),
                    },
                );
            out.push(Warning::SelfCheck {
                program: program_report.program,
                requirement: rule.requirement,
                label: rule.label.clone(),
                text,
            });
        }
    }
    out
}

/// Every plan-level warning for one plan. Total. Called by `evaluate` after
/// the program reports are folded; `Report::rules()` supplies the slice.
/// Checks run in a fixed order and concatenate, so output order is
/// deterministic; board order decides inside each.
#[must_use]
pub fn warnings(bundle: &PlanBundle, rules: &[&RequirementReport]) -> Vec<Warning> {
    let plan = &bundle.plan;
    let facts = &bundle.facts;
    let mut out = fills_no_requirement(plan, rules);
    out.extend(duplicate_courses(plan, facts, rules));
    out.extend(prerequisite_problems(plan, &bundle.prerequisites, facts));
    out.extend(exclusion_problems(plan, &bundle.exclusions, facts));
    out.extend(season_unlikely(plan, facts));
    out.extend(invalidations(&bundle.invalidations));
    out.extend(over_semester_load(
        plan,
        &bundle.limits,
        music_or_architecture(bundle),
    ));
    out.extend(course_facts_changed(plan, facts));
    out.extend(manual_credits(plan));
    out
}

/// Whether any claimed program is a music or architecture degree, which
/// raises Rice's normal load to 20 hours.
fn music_or_architecture(bundle: &PlanBundle) -> bool {
    bundle.programs.iter().any(|p| {
        bundle.plan.programs.contains(&p.id)
            && (p.credential.starts_with("BMus")
                || p.credential.starts_with("BArch")
                || p.credential.starts_with("MMus"))
    })
}

/// The warnings that need the whole report and the programs it was built
/// from: double counting and the self-check rows.
#[must_use]
pub(crate) fn report_warnings(
    report: &Report,
    programs: &[&Program],
    cards: &[Card],
) -> Vec<Warning> {
    let mut out = double_counted(report, programs, cards);
    out.extend(self_checks(report, programs));
    out
}

/// What dropping `course` into one term would mean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlacementPreview {
    /// The column.
    pub term: TermId,
    /// Prerequisite verdict for a drop here.
    pub prerequisites: PrereqVerdict,
    /// `Some` when the course is already in the plan elsewhere and is not repeatable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<TermId>,
    /// The term's planned credits plus this card's.
    pub credits_after: Credits,
    /// Rules whose progress would rise; a filter pass, not a full matching.
    pub fills: Vec<(ProgramId, RequirementId)>,
}

/// The prerequisite verdict for one term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PrereqVerdict {
    /// Every named course is in place.
    Satisfied,
    /// Absent from the plan, or placed in this term or later.
    Missing {
        /// The named courses.
        courses: Vec<CourseCode>,
    },
    /// A named prerequisite sits in the target term itself.
    SameTerm {
        /// The named courses.
        courses: Vec<CourseCode>,
    },
    /// No catalog record, or an `Unparsed` clause on the deciding path.
    Unknown,
}

/// Which requirements' progress would rise if `code` joined the plan: a
/// filter pass over the unmet course rules, not a matching.
fn requirements_raised_by(
    bundle: &PlanBundle,
    report: &Report,
    code: &CourseCode,
) -> Vec<(ProgramId, RequirementId)> {
    let mut open: BTreeSet<RequirementId> = BTreeSet::new();
    for program in &report.programs {
        let mut rules = Vec::new();
        walk(&program.root, &mut rules);
        open.extend(
            rules
                .iter()
                .filter(|r| r.outcome != Outcome::Met)
                .map(|r| r.requirement),
        );
    }
    let mut out = Vec::new();
    for program in &bundle.programs {
        if !report
            .programs
            .iter()
            .any(|p| p.program == program.id && p.evaluated_with == program.catalog_year)
        {
            continue;
        }
        for requirement in program.root.flatten() {
            let RequirementBody::Course { filter, .. } = &requirement.body else {
                continue;
            };
            if open.contains(&requirement.id)
                && filter.matches(code, &bundle.facts) == FilterMatch::Yes
            {
                out.push((program.id, requirement.id));
            }
        }
    }
    out
}

/// What dropping `course` into each term would mean. Reads the same rows and
/// index the warning functions read, so the preview and the post-drop
/// warning cannot disagree. `moving` is the entry being dragged, when it is
/// already on the board, so the index is built without it. Only `Rice`
/// terms get a prerequisite verdict; `Away` terms return `Unknown` and `Off`
/// terms are omitted.
#[must_use]
pub fn preview_placement(
    bundle: &PlanBundle,
    course: &CourseCode,
    moving: Option<EntryId>,
) -> Vec<PlacementPreview> {
    let plan = &bundle.plan;
    let facts = &bundle.facts;
    let canonical = facts.canonical(course);
    let index = TakenIndex::build_without(plan, facts, moving);
    let fact = prereq_fact(&bundle.prerequisites, &canonical);
    let info = facts.get(&canonical);
    let already = index.earliest(&canonical);
    let duplicate_of = match already {
        Some(placed) if !info.is_some_and(|i| i.repeatable) => placed.term,
        _ => None,
    };
    let credits = moving
        .and_then(|entry| moving_credits(plan, entry))
        .or_else(|| info.map(|i| i.credits.min()))
        .unwrap_or(Credits::ZERO);
    let report = evaluate(bundle);
    let fills = requirements_raised_by(bundle, &report, &canonical);

    let mut out = Vec::new();
    for term in &plan.terms {
        let (is_rice, moving_here) = match &term.kind {
            TermKind::Rice { courses, .. } => (
                true,
                moving.is_some_and(|m| courses.iter().any(|c| c.id == m)),
            ),
            TermKind::Away { cards } => (
                false,
                moving.is_some_and(|m| cards.iter().any(|c| c.id == m)),
            ),
            TermKind::Off => continue,
        };
        let credits_after = if moving_here {
            term.planned_credits()
        } else {
            term.planned_credits().saturating_add(credits)
        };
        let prerequisites = if is_rice {
            match fact {
                PrereqFact::Unknown => PrereqVerdict::Unknown,
                PrereqFact::NoneRequired { .. } => PrereqVerdict::Satisfied,
                PrereqFact::Requires { expr, .. } => {
                    let r = evaluate_prereq(expr, term.position, &index, facts);
                    match r.truth {
                        Truth::Satisfied => PrereqVerdict::Satisfied,
                        Truth::Unknown => PrereqVerdict::Unknown,
                        Truth::Missing if !r.same_term.is_empty() && r.missing.is_empty() => {
                            PrereqVerdict::SameTerm {
                                courses: r.same_term,
                            }
                        }
                        Truth::Missing => {
                            let mut courses = r.missing;
                            courses.extend(r.same_term);
                            courses.extend(r.later.into_iter().map(|(c, _)| c));
                            PrereqVerdict::Missing { courses }
                        }
                    }
                }
            }
        } else {
            PrereqVerdict::Unknown
        };
        out.push(PlacementPreview {
            term: term.id,
            prerequisites,
            duplicate_of: duplicate_of.filter(|t| *t != term.id),
            credits_after,
            fills: fills.clone(),
        });
    }
    out
}

fn moving_credits(plan: &Plan, entry: EntryId) -> Option<Credits> {
    let cards = collect_cards(plan, &CourseFacts::default());
    cards.iter().find(|c| c.entry == entry).map(|c| c.credits)
}

/// Which of `codes` match `requirement`'s filter, canonicalised through
/// `facts`. Empty for a rule with no filter.
#[must_use]
pub fn requirement_matches(
    program: &Program,
    requirement: RequirementId,
    codes: &[CourseCode],
    facts: &CourseFacts,
) -> Vec<CourseCode> {
    let Some(RequirementBody::Course { filter, .. }) =
        program.requirement(requirement).map(|r| &r.body)
    else {
        return vec![];
    };
    codes
        .iter()
        .filter(|code| filter.matches(&facts.canonical(code), facts) == FilterMatch::Yes)
        .cloned()
        .collect()
}
