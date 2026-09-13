#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The engine is total: a pathological nesting depth returns a bounded
//! answer instead of exhausting the stack. Each input is 100 000 levels
//! deep and is built iteratively; each test runs on a thread with a small
//! stack, so an unbounded recursion aborts the test instead of passing on a
//! larger default stack.
//!
//! The inputs are leaked on purpose: the derived drop glue of a nested
//! `Vec` is itself recursive, and what is under test is the engine's bound,
//! not the standard library's.

mod common;

use common::{all, bundle, code, course, plan, planned, program, req, rice_term};
use skyspace_core::evaluate::{CourseFacts, Outcome, evaluate};
use skyspace_core::prereq::{PrereqExpr, PrereqFact, Prerequisite};
use skyspace_core::program::{CatalogYear, ProgramKind, RequirementBody};
use skyspace_core::term::Season;
use skyspace_core::warn::Warning;

const DEPTH: usize = 100_000;
const SMALL_STACK: usize = 256 * 1024;

fn on_small_stack(body: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(body)
        .unwrap()
        .join()
        .unwrap();
}

fn nested_all(depth: usize) -> PrereqExpr {
    let mut expr = PrereqExpr::Course(code("COMP 140"));
    for _ in 0..depth {
        expr = PrereqExpr::All(vec![expr]);
    }
    expr
}

#[test]
fn deep_parentheses_parse_to_unparsed() {
    on_small_stack(|| {
        let text = format!("{}COMP 140{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
        assert_eq!(PrereqExpr::parse(&text), PrereqExpr::Unparsed(text.clone()));
        let within = format!("{}COMP 140{}", "(".repeat(32), ")".repeat(32));
        assert_eq!(
            PrereqExpr::parse(&within),
            PrereqExpr::Course(code("COMP 140"))
        );
        let beyond = format!("{}COMP 140{}", "(".repeat(33), ")".repeat(33));
        assert_eq!(
            PrereqExpr::parse(&beyond),
            PrereqExpr::Unparsed(beyond.clone())
        );
    });
}

#[test]
fn deep_prerequisite_expression_is_unknown() {
    on_small_stack(|| {
        let major = program(
            "deep",
            ProgramKind::Major,
            "BA",
            None,
            all("root", "Root", vec![]),
        );
        let row = |expr: PrereqExpr| Prerequisite {
            course: code("COMP 182"),
            fact: PrereqFact::Requires {
                published_for: CatalogYear(2026),
                expr,
                published: "COMP 140".to_owned(),
                corequisite: None,
            },
        };
        let terms = || {
            vec![
                rice_term(
                    "fall",
                    2027,
                    Season::Fall,
                    vec![planned("comp-140", "COMP 140", 4)],
                ),
                rice_term(
                    "spring",
                    2027,
                    Season::Spring,
                    vec![planned("comp-182", "COMP 182", 4)],
                ),
            ]
        };
        let shallow = bundle(
            plan("shallow", &[&major], terms()),
            vec![major.clone()],
            CourseFacts::default(),
            vec![row(nested_all(32))],
        );
        let report = evaluate(&shallow);
        assert!(!report.warnings.iter().any(|w| matches!(
            w,
            Warning::Prerequisite { .. } | Warning::PrerequisiteUnparsed { .. }
        )));
        let deep = bundle(
            plan("deep", &[&major], terms()),
            vec![major],
            CourseFacts::default(),
            vec![row(nested_all(DEPTH))],
        );
        let report = evaluate(&deep);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::PrerequisiteUnparsed { .. }))
        );
        std::mem::forget(deep);
    });
}

#[test]
fn deep_requirement_tree_returns_a_report() {
    on_small_stack(|| {
        let mut root = course("leaf", "COMP 140", Some(4));
        for i in 0..DEPTH {
            root = req(
                &format!("nest-{i}"),
                "nest",
                None,
                RequirementBody::All { of: vec![root] },
            );
        }
        let major = program("deep", ProgramKind::Major, "BA", None, root);
        assert_eq!(major.requirement_count(), 65);
        let plan = plan(
            "deep",
            &[&major],
            vec![rice_term(
                "fall",
                2027,
                Season::Fall,
                vec![planned("comp-140", "COMP 140", 4)],
            )],
        );
        let deep = bundle(plan, vec![major], CourseFacts::default(), vec![]);
        let report = evaluate(&deep);
        let rules = report.rules();
        assert_eq!(rules.len(), 65);
        let deepest = rules.last().unwrap();
        assert!(deepest.children.is_empty());
        assert_eq!(deepest.label, "requirement tree too deep");
        assert_eq!(
            deepest.outcome,
            Outcome::NeedsStudentCheck { confirmed: None }
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::SelfCheck { label, .. } if label == "requirement tree too deep"))
        );
        std::mem::forget(deep);
    });
}
