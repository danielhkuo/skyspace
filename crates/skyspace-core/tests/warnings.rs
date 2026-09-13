#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Warning scenarios that the launch fixtures do not reach: duplicates
//! absorbed by other rules, pins on incoming credit, exclusions against
//! incoming credit, and the shape of the self-check rows.

mod common;

use common::{
    all, attribute_course, away_term, bundle, code, course, credits_rule, entry_id, info, manual,
    non_course, plan, planned, program, requirement_id, rice_term, term_id,
};
use skyspace_core::catalog::Attribute;
use skyspace_core::evaluate::{CourseFacts, Outcome, evaluate};
use skyspace_core::plan::CreditOrigin;
use skyspace_core::prereq::Exclusion;
use skyspace_core::program::{CatalogYear, CourseFilter, ProgramKind};
use skyspace_core::term::Season;
use skyspace_core::warn::Warning;

fn count(warnings: &[Warning], pick: impl Fn(&Warning) -> bool) -> usize {
    warnings.iter().filter(|w| pick(w)).count()
}

#[test]
fn duplicate_absorbed_by_a_credits_rule_still_warns() {
    let root = all(
        "root",
        "Root",
        vec![
            course("c140", "COMP 140", Some(4)),
            credits_rule("electives", "Free electives", 12, CourseFilter::default()),
        ],
    );
    let major = program("major", ProgramKind::Major, "BA", Some(120), root);
    let mut facts = CourseFacts::default();
    facts.insert(info("COMP 140", 4, &[], false));
    let plan = plan(
        "dup",
        &[&major],
        vec![
            rice_term(
                "fall",
                2027,
                Season::Fall,
                vec![planned("a", "COMP 140", 4)],
            ),
            rice_term(
                "spring",
                2027,
                Season::Spring,
                vec![planned("b", "COMP 140", 4)],
            ),
        ],
    );
    let report = evaluate(&bundle(plan, vec![major], facts, vec![]));
    let duplicates: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::DuplicateCourse { .. }))
        .collect();
    assert_eq!(duplicates.len(), 1);
    assert!(matches!(
        duplicates[0],
        Warning::DuplicateCourse { course, terms }
            if *course == code("COMP 140") && *terms == vec![term_id("fall"), term_id("spring")]
    ));
}

#[test]
fn duplicate_split_across_two_programs_still_warns() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("r1", "Root", vec![course("c1", "COMP 140", Some(4))]),
    );
    let minor = program(
        "minor",
        ProgramKind::Minor,
        "",
        None,
        all("r2", "Root", vec![course("c2", "COMP 140", Some(4))]),
    );
    let mut facts = CourseFacts::default();
    facts.insert(info("COMP 140", 4, &[], false));
    let mut second = planned("b", "COMP 140", 4);
    second.fills = vec![requirement_id("c2")];
    let plan = plan(
        "dup",
        &[&major, &minor],
        vec![
            rice_term(
                "fall",
                2027,
                Season::Fall,
                vec![planned("a", "COMP 140", 4)],
            ),
            rice_term("spring", 2027, Season::Spring, vec![second]),
        ],
    );
    let report = evaluate(&bundle(plan, vec![major, minor], facts, vec![]));
    assert_eq!(
        count(&report.warnings, |w| matches!(
            w,
            Warning::DuplicateCourse { .. }
        )),
        1
    );
}

#[test]
fn attribute_unknown_warns_once_per_card() {
    let with_group = |name: &str| {
        program(
            name,
            ProgramKind::Major,
            "BA",
            None,
            all(
                &format!("{name}-root"),
                "Root",
                vec![attribute_course(
                    &format!("{name}-d1"),
                    "Distribution Group I",
                    Attribute::DistributionOne,
                )],
            ),
        )
    };
    let (p1, p2, p3) = (with_group("p1"), with_group("p2"), with_group("p3"));
    let plan = plan(
        "attr",
        &[&p1, &p2, &p3],
        vec![rice_term(
            "fall",
            2027,
            Season::Fall,
            vec![planned("hist", "HIST 117", 3)],
        )],
    );
    let report = evaluate(&bundle(
        plan,
        vec![p1, p2, p3],
        CourseFacts::default(),
        vec![],
    ));
    assert_eq!(
        count(&report.warnings, |w| matches!(
            w,
            Warning::AttributeUnknown { .. }
        )),
        1
    );
}

#[test]
fn second_pin_on_a_full_slot_warns_and_falls_to_the_pool() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("root", "Root", vec![course("c", "COMP 140", Some(4))]),
    );
    let mut first = planned("a", "COMP 140", 4);
    let mut second = planned("b", "MATH 101", 3);
    first.fills = vec![requirement_id("c")];
    second.fills = vec![requirement_id("c")];
    let plan = plan(
        "pins",
        &[&major],
        vec![rice_term("fall", 2027, Season::Fall, vec![first, second])],
    );
    let report = evaluate(&bundle(plan, vec![major], CourseFacts::default(), vec![]));
    let rule = &report.programs[0].root.children[0];
    assert_eq!(rule.outcome, Outcome::Met);
    assert_eq!(rule.filled_by, vec![entry_id("a")]);
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::RequirementChoiceUnmatched { term: Some(t), entry, requirement, .. }
            if *t == term_id("fall") && *entry == entry_id("b") && *requirement == requirement_id("c")
    )));
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::FillsNoRequirement { entry, .. } if *entry == entry_id("b")
    )));
}

#[test]
fn incoming_pin_the_filter_rejects_warns_without_a_term() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("root", "Root", vec![course("c", "COMP 140", Some(4))]),
    );
    let mut card = manual(
        "transfer",
        CreditOrigin::Transfer,
        "CS 101",
        4,
        Some("MATH 101"),
    );
    card.fills = vec![requirement_id("c")];
    let mut plan = plan(
        "incoming",
        &[&major],
        vec![rice_term("fall", 2027, Season::Fall, vec![])],
    );
    plan.incoming_credit = vec![card];
    let report = evaluate(&bundle(plan, vec![major], CourseFacts::default(), vec![]));
    let rule = &report.programs[0].root.children[0];
    assert_eq!(rule.claimed_by, vec![entry_id("transfer")]);
    let unmatched: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::RequirementChoiceUnmatched { .. }))
        .collect();
    assert_eq!(unmatched.len(), 1);
    assert!(matches!(
        unmatched[0],
        Warning::RequirementChoiceUnmatched { term: None, entry, .. } if *entry == entry_id("transfer")
    ));
    let json = serde_json::to_string(unmatched[0]).unwrap();
    assert!(!json.contains("\"term\""), "{json}");
}

#[test]
fn stale_pin_on_an_incoming_card_warns_without_a_term() {
    let mut major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("root", "Root", vec![course("c", "COMP 140", Some(4))]),
    );
    major.retired_requirements.push(requirement_id("gone"));
    let mut card = manual(
        "ap",
        CreditOrigin::AdvancedPlacement,
        "AP CS",
        4,
        Some("COMP 140"),
    );
    card.fills = vec![requirement_id("gone")];
    let mut plan = plan(
        "stale",
        &[&major],
        vec![rice_term("fall", 2027, Season::Fall, vec![])],
    );
    plan.incoming_credit = vec![card];
    let report = evaluate(&bundle(plan, vec![major], CourseFacts::default(), vec![]));
    assert!(report.warnings.iter().any(|w| matches!(
        w,
        Warning::RequirementChoiceMissing { term: None, entry, retired: true, .. }
            if *entry == entry_id("ap")
    )));
}

#[test]
fn exclusion_against_incoming_credit_warns() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("root", "Root", vec![]),
    );
    let mut plan = plan(
        "excl",
        &[&major],
        vec![rice_term(
            "fall",
            2027,
            Season::Fall,
            vec![planned("comp-318", "COMP 318", 3)],
        )],
    );
    plan.incoming_credit = vec![manual(
        "transfer",
        CreditOrigin::Transfer,
        "CS 310",
        3,
        Some("COMP 310"),
    )];
    let mut bundle = bundle(plan, vec![major], CourseFacts::default(), vec![]);
    bundle.exclusions = vec![Exclusion {
        blocked: code("COMP 318"),
        blocker: code("COMP 310"),
        published_for: CatalogYear(2026),
        published: "Cannot register for COMP 318 if student has credit for COMP 310.".to_owned(),
    }];
    let report = evaluate(&bundle);
    let exclusions: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::MutuallyExclusive { .. }))
        .collect();
    assert_eq!(exclusions.len(), 1);
    assert!(matches!(
        exclusions[0],
        Warning::MutuallyExclusive { blocked, blocked_term: Some(t), blocker, blocker_term: None, .. }
            if *blocked == code("COMP 318") && *t == term_id("fall") && *blocker == code("COMP 310")
    ));
}

#[test]
fn self_check_rows_are_emitted_for_leaves_only() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all(
            "root",
            "Root",
            vec![all(
                "area",
                "Piano area",
                vec![non_course("exam", "Pass the exam")],
            )],
        ),
    );
    let plan = plan(
        "self",
        &[&major],
        vec![rice_term("fall", 2027, Season::Fall, vec![])],
    );
    let report = evaluate(&bundle(plan, vec![major], CourseFacts::default(), vec![]));
    let rows: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::SelfCheck { .. }))
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0],
        Warning::SelfCheck { requirement, .. } if *requirement == requirement_id("exam")
    ));
}

#[test]
fn away_card_with_an_equivalent_that_fills_nothing_warns() {
    let major = program(
        "major",
        ProgramKind::Major,
        "BA",
        None,
        all("root", "Root", vec![]),
    );
    let mut plan = plan(
        "away",
        &[&major],
        vec![away_term(
            "madrid",
            2028,
            Season::Fall,
            vec![
                manual(
                    "hist",
                    CreditOrigin::StudyAbroad,
                    "HIS 1",
                    3,
                    Some("HIST 117"),
                ),
                manual("untyped", CreditOrigin::StudyAbroad, "ART 1", 3, None),
            ],
        )],
    );
    plan.incoming_credit = vec![manual(
        "ap",
        CreditOrigin::AdvancedPlacement,
        "AP CS",
        4,
        Some("COMP 140"),
    )];
    let report = evaluate(&bundle(plan, vec![major], CourseFacts::default(), vec![]));
    let fills_nothing: Vec<&Warning> = report
        .warnings
        .iter()
        .filter(|w| matches!(w, Warning::FillsNoRequirement { .. }))
        .collect();
    assert_eq!(fills_nothing.len(), 1);
    assert!(matches!(
        fills_nothing[0],
        Warning::FillsNoRequirement { term, entry, course }
            if *term == term_id("madrid") && *entry == entry_id("hist") && *course == code("HIST 117")
    ));
}
