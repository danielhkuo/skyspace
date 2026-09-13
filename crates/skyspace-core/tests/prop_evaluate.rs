#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Engine invariants. Helpers live here, not on `Plan`: a test helper on a
//! domain type is a public API only tests use.

mod common;

use std::collections::BTreeSet;

use proptest::prelude::*;
use skyspace_core::catalog::Attribute;
use skyspace_core::code::CourseCode;
use skyspace_core::evaluate::{CourseFacts, CourseInfo, Outcome, PlanBundle, Report, evaluate};
use skyspace_core::plan::{PlanTerm, PlannedCourse, TermKind};
use skyspace_core::program::{
    CourseFilter, CourseSelector, Program, ProgramKind, Requirement, RequirementBody,
};
use skyspace_core::term::{CreditRange, Credits, Season};

const SUBJECTS: [&str; 4] = ["COMP", "MATH", "HIST", "MUSI"];
const NUMBERS: [u16; 6] = [101, 140, 182, 210, 310, 457];

fn all_codes() -> Vec<CourseCode> {
    let mut out = Vec::new();
    for s in SUBJECTS {
        for n in NUMBERS {
            out.push(common::code(&format!("{s} {n}")));
        }
    }
    out
}

fn known_course() -> impl Strategy<Value = CourseCode> {
    prop::sample::select(all_codes())
}

fn facts() -> CourseFacts {
    let mut facts = CourseFacts::default();
    for (i, code) in all_codes().into_iter().enumerate() {
        let attributes: BTreeSet<Attribute> = match i % 4 {
            0 => [Attribute::DistributionOne].into(),
            1 => [Attribute::DistributionTwo, Attribute::AnalyzingDiversity].into(),
            2 => [Attribute::DistributionThree].into(),
            _ => BTreeSet::new(),
        };
        facts.insert(CourseInfo {
            title: code.to_string(),
            credits: CreditRange::Fixed(Credits::from_cents(300)),
            attributes,
            department: Some(code.subject.as_str().to_owned()),
            repeatable: code.subject.as_str() == "MUSI",
            seasons_offered: vec![],
            terms_observed: 0,
            offered_now: true,
            code,
        });
    }
    facts
}

fn selector() -> impl Strategy<Value = CourseSelector> {
    prop_oneof![
        known_course().prop_map(|code| CourseSelector::Code { code }),
        prop::sample::select(SUBJECTS.to_vec()).prop_map(|s| CourseSelector::Subject {
            subject: common::subject(s)
        }),
        (0u16..400, 100u16..300).prop_map(|(low, span)| CourseSelector::NumberRange {
            subject: None,
            low,
            high: low + span,
        }),
        prop::sample::select(vec![
            Attribute::DistributionOne,
            Attribute::DistributionTwo,
            Attribute::DistributionThree,
            Attribute::AnalyzingDiversity,
        ])
        .prop_map(|attribute| CourseSelector::Attribute { attribute }),
    ]
}

fn leaf(index: usize) -> impl Strategy<Value = Requirement> {
    prop_oneof![
        4 => (selector(), 1u8..=3).prop_map(move |(sel, semesters)| common::req(
            &format!("leaf-{index}"),
            "course",
            None,
            RequirementBody::Course {
                filter: CourseFilter {
                    include: vec![sel],
                    exclude: vec![],
                },
                semesters,
            },
        )),
        1 => (1u16..=12).prop_map(move |h| common::req(
            &format!("leaf-{index}"),
            "credits",
            None,
            RequirementBody::Credits {
                minimum: common::hours(h),
                scope: skyspace_core::program::CreditScope::Additional,
                from: CourseFilter::default(),
            },
        )),
        1 => Just(common::unverifiable(&format!("leaf-{index}"), "see advisor")),
        1 => Just(common::non_course(&format!("leaf-{index}"), "exam")),
    ]
}

fn program(index: usize) -> impl Strategy<Value = Program> {
    let leaves = (2usize..=10)
        .prop_flat_map(move |n| (0..n).map(|i| leaf(index * 100 + i)).collect::<Vec<_>>());
    (leaves, 1u8..=3, any::<bool>()).prop_map(move |(leaves, count, select)| {
        let half = leaves.len() / 2;
        let (first, second) = leaves.split_at(half);
        let group_a = common::all(&format!("p{index}-a"), "Area A", first.to_vec());
        let group_b = if select {
            common::select(&format!("p{index}-b"), "Area B", count, second.to_vec())
        } else {
            common::all(&format!("p{index}-b"), "Area B", second.to_vec())
        };
        common::program(
            &format!("program-{index}"),
            if index == 0 {
                ProgramKind::University
            } else {
                ProgramKind::Major
            },
            "BA",
            Some(120),
            common::all(&format!("p{index}-root"), "Root", vec![group_a, group_b]),
        )
    })
}

fn term(index: usize) -> impl Strategy<Value = PlanTerm> {
    prop::collection::vec(known_course(), 0..=6).prop_map(move |codes| {
        let courses: Vec<PlannedCourse> = codes
            .into_iter()
            .enumerate()
            .map(|(i, code)| common::planned(&format!("t{index}-c{i}"), &code.to_string(), 3))
            .collect();
        let season = match index % 3 {
            0 => Season::Fall,
            1 => Season::Spring,
            _ => Season::Summer,
        };
        let year = 2027 + u16::try_from(index / 3).unwrap_or(0);
        common::rice_term(&format!("t{index}"), year, season, courses)
    })
}

fn plan_bundle() -> impl Strategy<Value = PlanBundle> {
    let programs = (1usize..=4).prop_flat_map(|n| (0..n).map(program).collect::<Vec<_>>());
    let terms = (1usize..=8).prop_flat_map(|n| (0..n).map(term).collect::<Vec<_>>());
    (programs, terms).prop_map(|(programs, terms)| {
        let refs: Vec<&Program> = programs.iter().collect();
        let plan = common::plan("prop", &refs, terms);
        common::bundle(plan, programs, facts(), vec![])
    })
}

fn add_to_first_rice_term(bundle: &PlanBundle, course: &CourseCode) -> PlanBundle {
    let mut out = bundle.clone();
    for term in &mut out.plan.terms {
        if let TermKind::Rice { courses, .. } = &mut term.kind {
            courses.push(common::planned("added", &course.to_string(), 3));
            break;
        }
    }
    out
}

fn reverse_within_terms(bundle: &PlanBundle) -> PlanBundle {
    let mut out = bundle.clone();
    for term in &mut out.plan.terms {
        if let TermKind::Rice { courses, .. } = &mut term.kind {
            courses.reverse();
        }
    }
    out
}

fn as_json(report: &Report) -> String {
    serde_json::to_string_pretty(report).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn evaluation_is_deterministic(bundle in plan_bundle()) {
        prop_assert_eq!(as_json(&evaluate(&bundle)), as_json(&evaluate(&bundle)));
    }

    #[test]
    fn self_checks_stay_out_of_progress(bundle in plan_bundle()) {
        let report = evaluate(&bundle);
        let leaf_self_checks = report
            .rules()
            .iter()
            .filter(|r| r.children.is_empty() && matches!(r.outcome, Outcome::NeedsStudentCheck { .. }))
            .count();
        prop_assert_eq!(usize::from(report.progress.self_checks), leaf_self_checks);
        prop_assert!(report.progress.requirements_met <= report.progress.requirements_checkable);
        for program in &report.programs {
            prop_assert!(program.progress.requirements_met <= program.progress.requirements_checkable);
        }
    }

    #[test]
    fn every_requirement_is_reported_once(bundle in plan_bundle()) {
        let report = evaluate(&bundle);
        let expected: usize = bundle.programs.iter().map(Program::requirement_count).sum();
        prop_assert_eq!(report.rules().len(), expected);
    }

    #[test]
    fn progress_ignores_course_order(bundle in plan_bundle()) {
        let a = evaluate(&bundle).progress;
        let b = evaluate(&reverse_within_terms(&bundle)).progress;
        prop_assert_eq!(a, b);
    }

    #[test]
    fn adding_a_course_never_lowers_progress(bundle in plan_bundle(), course in known_course()) {
        let before = evaluate(&bundle).progress.requirements_met;
        let after = evaluate(&add_to_first_rice_term(&bundle, &course)).progress.requirements_met;
        prop_assert!(after >= before);
    }

    #[test]
    fn bundle_round_trips_through_json(bundle in plan_bundle()) {
        let text = serde_json::to_string(&bundle).unwrap();
        let back: PlanBundle = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(back, bundle);
    }
}
