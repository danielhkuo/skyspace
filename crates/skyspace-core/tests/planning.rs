#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Engine scenarios: the requirement engine, the plan model's checks and
//! the placement preview, each against one fixture plan.

mod common;

use common::{
    all, attribute_course, barch_bundle, bioe_bundle, bmus_bundle, bscs_bundle, bundle, code,
    course, entry_id, hours, info, manual, off_term, plan, planned, program, requirement_id,
    requires, rice_term, self_check, term_id, uuid,
};
use skyspace_core::catalog::Attribute;
use skyspace_core::evaluate::{CourseFacts, Outcome, Report, RequirementReport, evaluate};
use skyspace_core::plan::{CreditOrigin, EntryId, NonCourseClaim, PlanId, TermId, TermKind};
use skyspace_core::program::{CatalogYear, ProgramId, ProgramKind, RequirementId};
use skyspace_core::term::{Credits, Season};
use skyspace_core::warn::{
    PrereqProblem, PrereqVerdict, Warning, preview_placement, requirement_matches,
};

fn web_id(name: &str) -> uuid::Uuid {
    let ids: serde_json::Value = serde_json::from_str(include_str!("golden-ids.json")).unwrap();
    ids[name].as_str().unwrap().parse().unwrap()
}

fn find<'a>(report: &'a Report, label: &str) -> Option<&'a RequirementReport> {
    report.rules().into_iter().find(|r| r.label == label)
}

fn by_id(report: &Report, id: RequirementId) -> &RequirementReport {
    report
        .rules()
        .into_iter()
        .find(|r| r.requirement == id)
        .unwrap()
}

#[test]
fn reports_one_program_per_plan_program_in_sidebar_order() {
    let report = evaluate(&bscs_bundle());
    let names: Vec<&str> = report.programs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["University", "Computer Science, BSCS", "Statistics minor"]
    );
}

#[test]
fn flags_comp_415_in_the_same_term_as_its_prerequisite() {
    let report = evaluate(&bscs_bundle());
    let hits: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(
            |w| matches!(w, Warning::Prerequisite { course, .. } if *course == code("COMP 415")),
        )
        .collect();
    assert_eq!(hits.len(), 1);
    assert!(matches!(
        hits[0],
        Warning::Prerequisite {
            problem: PrereqProblem::SameTerm,
            ..
        }
    ));
}

#[test]
fn counts_an_analyzing_diversity_course_for_its_distribution_slot_too() {
    let report = evaluate(&bscs_bundle());
    assert_eq!(
        find(&report, "Analyzing Diversity").unwrap().outcome,
        Outcome::Met
    );
    // Group I holds a transfer card pinned by claim: shown, never met.
    let group_one = find(&report, "Distribution Group I").unwrap();
    assert_eq!(group_one.outcome, Outcome::Partial);
    let claimed: usize = group_one.children.iter().map(|c| c.claimed_by.len()).sum();
    assert_eq!(claimed, 1);
    let university = &report.programs[0];
    assert_eq!(university.progress.requirements_claimed, 1);
}

#[test]
fn never_lets_ap_or_ib_credit_fill_a_distribution_slot() {
    let report = evaluate(&bscs_bundle());
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::IncomingCreditIneligible { .. }))
    );
    let math_105 = EntryId(web_id("entry-math-105-3"));
    let ap_on_group = report
        .rules()
        .iter()
        .any(|r| r.label.starts_with("Distribution Group") && r.filled_by.contains(&math_105));
    assert!(!ap_on_group);
}

#[test]
fn progress_ignores_term_order() {
    let bundle = bscs_bundle();
    let report = evaluate(&bundle);
    let mut reversed = bundle.clone();
    reversed.plan.terms.reverse();
    let other = evaluate(&reversed);
    let met = |r: &Report| -> Vec<u16> {
        r.programs
            .iter()
            .map(|p| p.progress.requirements_met)
            .collect()
    };
    assert_eq!(met(&other), met(&report));
}

#[test]
fn matching_prefers_the_constrained_requirement() {
    // COMP 140 fits Core › COMP 140 and Distribution Group III; COMP 182 fits
    // the group too. Both must land, so neither may be flagged.
    let report = evaluate(&bscs_bundle());
    let fills_nothing = report.warnings.iter().filter(|w| {
        matches!(w, Warning::FillsNoRequirement { course, .. }
            if *course == code("COMP 140") || *course == code("COMP 182"))
    });
    assert_eq!(fills_nothing.count(), 0);
}

#[test]
fn reports_an_empty_plan_as_unmet_not_partial() {
    let mut bundle = bscs_bundle();
    bundle.plan.terms.clear();
    bundle.plan.incoming_credit.clear();
    bundle.plan.self_checks.clear();
    let report = evaluate(&bundle);
    for program in &report.programs {
        assert_eq!(program.root.outcome, Outcome::Unmet, "{}", program.name);
    }
}

#[test]
fn says_a_prerequisite_comes_later_when_placed_after_the_course() {
    let mut bundle = bscs_bundle();
    let fall26 = TermId(web_id("term-fall-2026"));
    let spring27 = TermId(web_id("term-spring-2027"));
    let mut moved = None;
    for term in &mut bundle.plan.terms {
        if term.id == fall26
            && let TermKind::Rice { courses, .. } = &mut term.kind
        {
            let at = courses
                .iter()
                .position(|c| c.course == code("COMP 382"))
                .unwrap();
            moved = Some(courses.remove(at));
        }
    }
    for term in &mut bundle.plan.terms {
        if term.id == spring27
            && let TermKind::Rice { courses, .. } = &mut term.kind
        {
            courses.push(moved.take().unwrap());
        }
    }
    let report = evaluate(&bundle);
    let later = report.warnings.iter().filter(|w| {
        matches!(w, Warning::Prerequisite { course, problem: PrereqProblem::Later { .. }, .. }
            if *course == code("COMP 415"))
    });
    assert_eq!(later.count(), 1);
}

#[test]
fn flags_a_course_whose_catalog_designation_changed() {
    let mut bundle = bscs_bundle();
    for term in &mut bundle.plan.terms {
        let TermKind::Rice { courses, .. } = &mut term.kind else {
            continue;
        };
        for c in courses.iter_mut().filter(|c| c.course == code("HIST 117")) {
            c.observed = Some(skyspace_core::plan::ObservedFacts {
                at: skyspace_core::Timestamp(1_724_112_000),
                catalog_year: CatalogYear(2024),
                title: "The world since 1492".to_owned(),
                credits: skyspace_core::term::CreditRange::Fixed(hours(3)),
                attributes: vec![
                    Attribute::AnalyzingDiversity,
                    Attribute::DistributionOne,
                    Attribute::DistributionTwo,
                ],
            });
        }
    }
    let report = evaluate(&bundle);
    let changed: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::CourseFactsChanged { .. }))
        .collect();
    assert_eq!(changed.len(), 1);
    assert!(matches!(
        changed[0],
        Warning::CourseFactsChanged { lost, .. } if *lost == vec![Attribute::DistributionTwo]
    ));
    let unchanged = evaluate(&bscs_bundle());
    assert!(
        !unchanged
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::CourseFactsChanged { .. }))
    );
}

#[test]
fn checks_the_two_department_constraint_from_department_data() {
    let label = "From at least two departments";
    let with_transfer = evaluate(&bscs_bundle());
    assert!(matches!(
        find(&with_transfer, label).unwrap().outcome,
        Outcome::NeedsStudentCheck { .. }
    ));
    let mut bundle = bscs_bundle();
    bundle.plan.incoming_credit.retain(|c| c.code != "TRAN 100");
    let no_transfer = evaluate(&bundle);
    let checked = find(&no_transfer, label).unwrap();
    assert_eq!(checked.outcome, Outcome::Met);
    assert_eq!(checked.progress.requirements_checkable, 1);
    assert!(
        !no_transfer
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::SelfCheck { label: l, .. } if l == label))
    );
}

#[test]
fn never_counts_a_self_check_toward_requirements_met() {
    let report = evaluate(&bscs_bundle());
    let bscs = report
        .programs
        .iter()
        .find(|p| p.program == ProgramId(web_id("prog-bscs")))
        .unwrap();
    assert!(bscs.progress.self_checks > 0);
    assert!(bscs.progress.requirements_checkable < 30);
    assert!(bscs.progress.requirements_met <= bscs.progress.requirements_checkable);
}

#[test]
fn preview_marks_missing_same_term_and_satisfied() {
    let bundle = bscs_bundle();
    let previews = preview_placement(
        &bundle,
        &code("COMP 415"),
        Some(EntryId(web_id("entry-comp-415-1"))),
    );
    let by_term = |name: &str| {
        previews
            .iter()
            .find(|p| p.term == TermId(web_id(name)))
            .unwrap()
    };
    assert!(matches!(
        by_term("term-fall-2025").prerequisites,
        PrereqVerdict::Missing { .. }
    ));
    assert!(matches!(
        by_term("term-fall-2026").prerequisites,
        PrereqVerdict::SameTerm { .. }
    ));
    assert_eq!(
        by_term("term-spring-2027").prerequisites,
        PrereqVerdict::Satisfied
    );
    assert_eq!(
        by_term("term-spring-2028").prerequisites,
        PrereqVerdict::Satisfied
    );
    // The moving card is not its own duplicate.
    assert_eq!(by_term("term-spring-2027").duplicate_of, None);
    assert_eq!(by_term("term-spring-2028").credits_after, hours(4));
}

#[test]
fn preview_matches_post_drop_warning() {
    let bundle = bscs_bundle();
    let course = code("COMP 415");
    let entry = EntryId(web_id("entry-comp-415-1"));
    for preview in preview_placement(&bundle, &course, Some(entry)) {
        let mut dropped = bundle.clone();
        let mut card = None;
        for term in &mut dropped.plan.terms {
            if let TermKind::Rice { courses, .. } = &mut term.kind
                && let Some(at) = courses.iter().position(|c| c.id == entry)
            {
                card = Some(courses.remove(at));
            }
        }
        let Some(card) = card else { continue };
        let Some(target) = dropped.plan.terms.iter_mut().find(|t| t.id == preview.term) else {
            continue;
        };
        let TermKind::Rice { courses, .. } = &mut target.kind else {
            continue;
        };
        courses.push(card);
        let warned = evaluate(&dropped).warnings.iter().any(|w| {
            matches!(w, Warning::Prerequisite { term, course: c, .. }
                if *term == preview.term && *c == course)
        });
        match preview.prerequisites {
            PrereqVerdict::Missing { .. } | PrereqVerdict::SameTerm { .. } => assert!(warned),
            PrereqVerdict::Satisfied => assert!(!warned),
            PrereqVerdict::Unknown => {}
        }
    }
}

#[test]
fn requirement_matches_uses_aliases() {
    let bundle = bscs_bundle();
    let bscs = bundle
        .programs
        .iter()
        .find(|p| p.id == ProgramId(web_id("prog-bscs")))
        .unwrap();
    let matched = requirement_matches(
        bscs,
        RequirementId(web_id("requirement-probstat")),
        &[code("ECON 307"), code("COMP 140")],
        &bundle.facts,
    );
    assert_eq!(matched, vec![code("ECON 307")]);
}

// --------------------------------------------------------- launch cases

#[test]
fn bioe_missing_prerequisite_warns_and_unknown_course_is_silent() {
    let report = evaluate(&bioe_bundle());
    let prereq: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::Prerequisite { .. }))
        .collect();
    assert_eq!(prereq.len(), 1);
    assert!(matches!(
        prereq[0],
        Warning::Prerequisite { course, prerequisite, problem: PrereqProblem::NotInPlan, .. }
            if *course == code("BIOE 322") && *prerequisite == code("MATH 102")
    ));
    assert!(!report.warnings.iter().any(|w| matches!(
        w,
        Warning::Prerequisite { course, .. } | Warning::PrerequisiteUnparsed { course, .. }
            if *course == code("BIOE 391")
    )));
    // The unverifiable footnote is reported and out of the checkable count.
    let root = &report.programs[0].root;
    assert_eq!(root.progress.self_checks, 1);
    assert_eq!(root.progress.requirements_checkable, 7);
    assert_eq!(report.programs[0].progress.credits_required, hours(131));
}

#[test]
fn repeated_lesson_fills_eight_slots_and_is_partial_at_seven() {
    let report = evaluate(&bmus_bundle());
    let lessons = by_id(&report, requirement_id("bmus-lessons"));
    assert_eq!(lessons.filled_by.len(), 7);
    assert_eq!(lessons.outcome, Outcome::Partial);
    assert_eq!(lessons.progress.credits_required, hours(24));
    assert_eq!(lessons.progress.credits_met, hours(21));
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::DuplicateCourse { .. }))
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::FillsNoRequirement { .. }))
    );
}

#[test]
fn zero_hour_recital_is_met() {
    let report = evaluate(&bmus_bundle());
    let junior = by_id(&report, requirement_id("bmus-junior"));
    assert_eq!(junior.outcome, Outcome::Met);
    assert_eq!(junior.progress.credits_met, Credits::ZERO);
    assert_eq!(junior.progress.credits_required, Credits::ZERO);
    assert_eq!(
        by_id(&report, requirement_id("bmus-recitals")).outcome,
        Outcome::Met
    );
}

#[test]
fn non_course_claim_stays_a_self_check() {
    let report = evaluate(&bmus_bundle());
    let piano = by_id(&report, requirement_id("bmus-piano"));
    assert_eq!(
        piano.outcome,
        Outcome::NeedsStudentCheck { confirmed: None }
    );
    assert_eq!(piano.claimed_in, Some(term_id("bmus-term-2")));
    assert_eq!(piano.progress.requirements_checkable, 0);
    assert_eq!(piano.progress.self_checks, 1);
    let area = by_id(&report, requirement_id("bmus-piano-area"));
    assert!(matches!(area.outcome, Outcome::NeedsStudentCheck { .. }));
    assert!(!area.is_complete());
    let root = &report.programs[0].root;
    assert_eq!(root.progress.self_checks, 2);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::SelfCheck { requirement, .. } if *requirement == requirement_id("bmus-piano")))
    );
}

#[test]
fn self_check_not_in_totals() {
    let bundle = bmus_bundle();
    let before = evaluate(&bundle);
    let mut ticked = bundle.clone();
    ticked.plan.self_checks.push(self_check("bmus-piano"));
    ticked.plan.self_checks.push(self_check("bmus-footnote"));
    let after = evaluate(&ticked);
    assert_eq!(
        before.programs[0].progress.requirements_percent(),
        after.programs[0].progress.requirements_percent()
    );
    assert_eq!(
        before.programs[0].progress.requirements_met,
        after.programs[0].progress.requirements_met
    );
    assert_eq!(after.programs[0].progress.self_checks_confirmed, 2);
    let piano = by_id(&after, requirement_id("bmus-piano"));
    assert!(matches!(
        piano.outcome,
        Outcome::NeedsStudentCheck { confirmed: Some(_) }
    ));
}

#[test]
fn mmus_range_with_exclusion_matches() {
    let report = evaluate(&bmus_bundle());
    let secondary = by_id(&report, requirement_id("bmus-secondary"));
    assert_eq!(secondary.outcome, Outcome::Met);
    assert_eq!(secondary.filled_by, vec![entry_id("bmus-secondary-card")]);
}

#[test]
fn barch_total_is_192_hours() {
    let report = evaluate(&barch_bundle());
    let program = &report.programs[0];
    assert_eq!(program.progress.credits_required, hours(192));
    assert_eq!(program.declared_credits, Some(hours(192)));
    assert_eq!(report.progress.credits_required, hours(192));
    let precept = by_id(&report, requirement_id("barch-precept"));
    assert_eq!(precept.outcome, Outcome::Met);
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::DuplicateCourse { .. }))
    );
    // HIST 210 lands in the free-elective allowance, so it fills something.
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::FillsNoRequirement { .. }))
    );
    let electives = by_id(&report, requirement_id("barch-electives"));
    assert_eq!(electives.outcome, Outcome::Partial);
    assert_eq!(electives.progress.credits_met, hours(3));
}

// ---------------------------------------------------------- small cases

fn one_rule_bundle() -> skyspace_core::evaluate::PlanBundle {
    let root = all(
        "one-root",
        "One",
        vec![course("one-comp-140", "COMP 140", Some(4))],
    );
    let major = program("one", ProgramKind::Major, "BA", Some(120), root);
    let mut facts = CourseFacts::default();
    facts.insert(info("COMP 140", 4, &[Attribute::DistributionThree], false));
    let plan = plan(
        "one",
        &[&major],
        vec![rice_term(
            "one-fall",
            2027,
            Season::Fall,
            vec![planned("one-comp-140", "COMP 140", 4)],
        )],
    );
    bundle(plan, vec![major], facts, vec![])
}

#[test]
fn unverifiable_is_never_met() {
    let mut bundle = one_rule_bundle();
    bundle.programs[0].root = all(
        "root",
        "Root",
        vec![common::unverifiable("u", "Consult your advisor.")],
    );
    bundle.plan.self_checks.push(self_check("u"));
    let report = evaluate(&bundle);
    let rule = by_id(&report, requirement_id("u"));
    assert!(matches!(
        rule.outcome,
        Outcome::NeedsStudentCheck { confirmed: Some(_) }
    ));
    assert_eq!(rule.progress.requirements_met, 0);
    assert_eq!(report.programs[0].progress.requirements_checkable, 0);
    assert_eq!(report.programs[0].progress.requirements_percent(), 100);
}

#[test]
fn choice_beats_the_match_and_warns_on_mismatch() {
    let mut bundle = one_rule_bundle();
    if let TermKind::Rice { courses, .. } = &mut bundle.plan.terms[0].kind {
        courses[0].course = code("MATH 101");
        courses[0].fills = vec![requirement_id("one-comp-140")];
    }
    let report = evaluate(&bundle);
    let rule = by_id(&report, requirement_id("one-comp-140"));
    assert_eq!(rule.filled_by, vec![entry_id("one-comp-140")]);
    assert_eq!(rule.claimed_by, vec![entry_id("one-comp-140")]);
    assert_eq!(rule.outcome, Outcome::Partial);
    assert_eq!(rule.progress.requirements_claimed, 1);
    assert_eq!(rule.progress.requirements_met, 0);
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::RequirementChoiceUnmatched { requirement, basis: None, .. }
            if *requirement == requirement_id("one-comp-140")
    )));
}

#[test]
fn choice_on_retired_requirement_warns() {
    let mut bundle = one_rule_bundle();
    bundle.programs[0]
        .retired_requirements
        .push(requirement_id("gone"));
    if let TermKind::Rice { courses, .. } = &mut bundle.plan.terms[0].kind {
        courses[0].fills = vec![requirement_id("gone"), requirement_id("never")];
    }
    let report = evaluate(&bundle);
    let missing: Vec<bool> = report
        .warnings
        .iter()
        .filter_map(|w| match w {
            Warning::RequirementChoiceMissing { retired, .. } => Some(*retired),
            _ => None,
        })
        .collect();
    assert_eq!(missing, vec![true, false]);
    // The card still matches its rule through the free pool.
    assert_eq!(
        by_id(&report, requirement_id("one-comp-140")).outcome,
        Outcome::Met
    );
}

#[test]
fn off_term_holds_no_courses_but_may_hold_a_claim() {
    let mut bundle = one_rule_bundle();
    bundle.plan.terms.push(off_term(
        "gap",
        2028,
        Season::Fall,
        vec![NonCourseClaim {
            requirement: requirement_id("one-comp-140"),
            label: "x".to_owned(),
        }],
    ));
    let report = evaluate(&bundle);
    assert_eq!(report.progress.credits_met, hours(4));
    assert_eq!(
        by_id(&report, requirement_id("one-comp-140")).claimed_in,
        Some(term_id("gap"))
    );
}

#[test]
fn fwis_100_does_not_meet_fwis() {
    let fwis = common::course_filter(
        "fwis",
        "FWIS",
        Some(3),
        skyspace_core::program::CourseFilter {
            include: vec![skyspace_core::program::CourseSelector::NumberRange {
                subject: Some(common::subject("FWIS")),
                low: 101,
                high: 299,
            }],
            exclude: vec![],
        },
    );
    let university = program(
        "university",
        ProgramKind::University,
        "",
        None,
        all("u-root", "University", vec![fwis]),
    );
    let plan = plan(
        "fwis",
        &[&university],
        vec![rice_term(
            "fwis-fall",
            2027,
            Season::Fall,
            vec![planned("fwis-100", "FWIS 100", 3)],
        )],
    );
    let report = evaluate(&bundle(
        plan,
        vec![university],
        CourseFacts::default(),
        vec![],
    ));
    assert_eq!(
        by_id(&report, requirement_id("fwis")).outcome,
        Outcome::Unmet
    );
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::FillsNoRequirement { course, .. } if *course == code("FWIS 100")
    )));
}

#[test]
fn unknown_attribute_does_not_meet_and_warns() {
    let mut bundle = one_rule_bundle();
    bundle.programs[0].root = all(
        "root",
        "Root",
        vec![attribute_course(
            "grp3",
            "Distribution Group III",
            Attribute::DistributionThree,
        )],
    );
    bundle.facts = CourseFacts::default();
    let report = evaluate(&bundle);
    assert_eq!(
        by_id(&report, requirement_id("grp3")).outcome,
        Outcome::Unmet
    );
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::AttributeUnknown { course, .. } if *course == code("COMP 140")
    )));
}

#[test]
fn credits_required_is_stable() {
    let bundle = bscs_bundle();
    let before = evaluate(&bundle);
    let mut more = bundle.clone();
    if let TermKind::Rice { courses, .. } = &mut more.plan.terms[0].kind {
        courses.push(planned("extra", "PHIL 101", 3));
    }
    let after = evaluate(&more);
    for (a, b) in before.programs.iter().zip(after.programs.iter()) {
        assert_eq!(a.progress.credits_required, b.progress.credits_required);
    }
}

#[test]
fn missing_year_substitutes_and_warns() {
    let mut bundle = one_rule_bundle();
    bundle.programs[0].catalog_year = CatalogYear(2027);
    let report = evaluate(&bundle);
    assert_eq!(report.programs.len(), 1);
    assert_eq!(report.programs[0].evaluated_with, CatalogYear(2027));
    assert_eq!(report.programs[0].catalog_year, CatalogYear(2026));
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::ProgramYearSubstituted {
            wanted: CatalogYear(2026),
            used: CatalogYear(2027),
            ..
        }
    )));
    bundle.programs.clear();
    let report = evaluate(&bundle);
    assert!(report.programs.is_empty());
    assert!(matches!(
        report.warnings[0],
        Warning::ProgramUnavailable { .. }
    ));
}

#[test]
fn incoming_credit_satisfies_prerequisite() {
    let mut bundle = one_rule_bundle();
    bundle.prerequisites.push(requires("COMP 140", "MATH 101"));
    let before = evaluate(&bundle);
    assert!(
        before
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Prerequisite { .. }))
    );
    bundle.plan.incoming_credit.push(manual(
        "ap-math",
        CreditOrigin::AdvancedPlacement,
        "MATH 101",
        3,
        Some("MATH 101"),
    ));
    let after = evaluate(&bundle);
    assert!(
        !after
            .warnings
            .iter()
            .any(|w| matches!(w, Warning::Prerequisite { .. }))
    );
}

#[test]
fn unparsed_prereq_is_quoted_verbatim_and_never_satisfies() {
    let mut bundle = one_rule_bundle();
    let text = "MATH 101 AND COMP 182 OR (PHYS 101 AND PHYS 102)";
    bundle.prerequisites.push(requires("COMP 140", text));
    let report = evaluate(&bundle);
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::PrerequisiteUnparsed { published, .. } if published == text
    )));
}

#[test]
fn cross_listed_pair_is_one_duplicate_and_exclusion_warns_once() {
    let mut bundle = one_rule_bundle();
    bundle.facts.add_alias(code("ECON 307"), code("STAT 310"));
    bundle.facts.insert(info("STAT 310", 3, &[], false));
    if let TermKind::Rice { courses, .. } = &mut bundle.plan.terms[0].kind {
        courses.push(planned("stat", "STAT 310", 3));
    }
    bundle.plan.terms.push(rice_term(
        "spring",
        2027,
        Season::Spring,
        vec![planned("econ", "ECON 307", 3)],
    ));
    bundle.exclusions.push(skyspace_core::prereq::Exclusion {
        blocked: code("ECON 307"),
        blocker: code("STAT 310"),
        published_for: CatalogYear(2026),
        published: "Cannot register for ECON 307 if student has credit for STAT 310.".to_owned(),
    });
    bundle.exclusions.push(skyspace_core::prereq::Exclusion {
        blocked: code("STAT 310"),
        blocker: code("ECON 307"),
        published_for: CatalogYear(2026),
        published: "Cannot register for STAT 310 if student has credit for ECON 307.".to_owned(),
    });
    let report = evaluate(&bundle);
    let duplicates: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::DuplicateCourse { .. }))
        .collect();
    assert_eq!(duplicates.len(), 1);
    assert!(
        matches!(duplicates[0], Warning::DuplicateCourse { course, terms }
        if *course == code("STAT 310") && terms.len() == 2)
    );
    let exclusions = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::MutuallyExclusive { .. }))
        .count();
    assert_eq!(exclusions, 1);
}

#[test]
fn over_semester_load_is_informational() {
    let mut bundle = one_rule_bundle();
    if let TermKind::Rice { courses, .. } = &mut bundle.plan.terms[0].kind {
        for i in 0..5 {
            courses.push(planned(&format!("load-{i}"), &format!("MATH 10{i}"), 3));
        }
    }
    let report = evaluate(&bundle);
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::OverSemesterLoad { planned, normal, .. }
            if *planned == hours(19) && *normal == hours(18)
    )));
}

#[test]
fn warnings_are_deterministic() {
    let bundle = bscs_bundle();
    let a = evaluate(&bundle);
    let b = evaluate(&bundle);
    assert_eq!(a.warnings, b.warnings);
    assert_eq!(a, b);
}

#[test]
fn plan_id_and_engine_version_ride_on_the_report() {
    let report = evaluate(&one_rule_bundle());
    assert_eq!(report.plan, PlanId(uuid("plan-one")));
    assert_eq!(report.engine_version, skyspace_core::ENGINE_VERSION);
}
