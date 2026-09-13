#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Runs every parser over real Rice pages when `SKYSPACE_RICE_PAGES` names a
//! directory holding them. Pulled data is never committed (AGENTS.md §4), so
//! without the variable every test here is skipped.

use skyspace_core::catalog::MeetingPattern;
use skyspace_core::prereq::PrereqExpr;
use skyspace_core::program::{CatalogYear, RequirementBody};
use skyspace_core::{Crn, TermCode, Timestamp};
use skyspace_parse::{
    IssueCode, RefKind, parse_associated_sections, parse_catalog_subject, parse_enrollment,
    parse_program_index, parse_program_page, parse_reference_list, parse_section_detail,
    parse_subject_listing,
};

fn page(name: &str) -> Option<String> {
    let dir = std::env::var("SKYSPACE_RICE_PAGES").ok()?;
    Some(std::fs::read_to_string(format!("{dir}/{name}")).expect(name))
}

fn fall_2026() -> TermCode {
    TermCode::parse("202710").unwrap()
}

#[test]
fn listing_comp() {
    let Some(html) = page("listing-comp.html") else {
        return;
    };
    let parsed = parse_subject_listing(&html, fall_2026()).unwrap();
    let rows = &parsed.value;
    eprintln!(
        "listing: {} rows, {} seen, issues {:?}",
        rows.len(),
        parsed.report.rows_seen,
        parsed.report.issues
    );
    assert!(rows.len() > 100, "{}", rows.len());
    assert_eq!(parsed.report.rows_seen, parsed.report.rows_kept);
    assert!(rows.iter().all(|r| r.crn.0 > 0));
    assert!(rows.iter().any(|r| r.meetings.len() == 2));
    assert!(rows.iter().any(|r| r.meetings.is_empty()));
    assert!(rows.iter().all(|r| r.term == fall_2026()));
    assert!(
        rows.iter()
            .flat_map(|r| &r.meetings)
            .all(|m| matches!(m.pattern, MeetingPattern::Timed(_)))
    );
    assert!(
        rows.iter()
            .any(|r| r.instructors.iter().any(|i| i.net_id.is_some()))
    );
    assert!(!parsed.report.has_issue(IssueCode::UnknownFinalExam));
    assert!(parsed.report.fill_count("final_exam") > 100);
    assert!(
        parse_subject_listing(&html, TermCode::parse("202720").unwrap()).is_err(),
        "the header names Fall 2026, so Spring must be WrongTerm"
    );
}

#[test]
fn detail_12422() {
    let Some(html) = page("detail-12422.html") else {
        return;
    };
    let parsed = parse_section_detail(&html, fall_2026(), Crn(12422), Timestamp(1)).unwrap();
    eprintln!(
        "detail: {:?}\nissues {:?}",
        parsed.value, parsed.report.issues
    );
    let detail = &parsed.value;
    assert_eq!(detail.long_title, "COMPUTATIONAL THINKING");
    assert_eq!(detail.department, "Computer Science");
    assert_eq!(detail.reserved.len(), 1);
    assert_eq!(detail.reserved[0].capacity, 60);
    assert_eq!(detail.reserved[0].available, 2);
    assert!(detail.reserved[0].label.contains("Matriculants"));
    assert!(detail.fees.is_empty());
    assert!(
        detail
            .restrictions
            .as_ref()
            .is_some_and(|r| !r.clauses.is_empty())
    );
    assert_eq!(detail.attributes.len(), 1);
    assert!(!parsed.report.has_issue(IssueCode::UnknownDetailLabel));
}

#[test]
fn assoc_12312() {
    let Some(xml) = page("assoc-12312.xml") else {
        return;
    };
    let parsed = parse_associated_sections(&xml, fall_2026()).unwrap();
    eprintln!(
        "assoc: {} sections, issues {:?}",
        parsed.value.len(),
        parsed.report.issues
    );
    assert!(!parsed.value.is_empty());
    for section in &parsed.value {
        assert!(!section.meetings.is_empty());
        assert!(section.meetings.iter().all(|m| m.dates.is_some()));
        assert!(section.part_of_term.is_some());
    }
    assert!(parse_associated_sections(&xml, TermCode::parse("202720").unwrap()).is_err());
}

#[test]
fn enrollment_12422() {
    let Some(xml) = page("enroll-12422.xml") else {
        return;
    };
    let seats = parse_enrollment(&xml).unwrap();
    eprintln!("seats: {seats:?}");
    assert!(seats.as_of.0 > 0);
    assert!(seats.capacity > 0);
}

#[test]
fn reference_lists() {
    let Some(terms) = page("terms.xml") else {
        return;
    };
    let entries = parse_reference_list(&terms, RefKind::Terms).unwrap();
    eprintln!("terms: {} entries", entries.len());
    assert!(
        entries
            .iter()
            .any(|e| e.label == "Fall Semester 2026" && e.code == "202710")
    );
    assert_eq!(entries.iter().filter(|e| e.current).count(), 1);
    for (name, kind, min) in [
        ("subjects.xml", RefKind::Subjects, 90),
        ("departments.xml", RefKind::Departments, 50),
        ("schools.xml", RefKind::Schools, 5),
        ("sessions.xml", RefKind::Sessions, 20),
        ("years.xml", RefKind::Years, 20),
        ("attrs.xml", RefKind::Attrs, 4),
    ] {
        let xml = page(name).unwrap();
        let entries = parse_reference_list(&xml, kind).unwrap();
        eprintln!("{name}: {} entries", entries.len());
        assert!(entries.len() >= min, "{name}: {}", entries.len());
        assert!(
            entries
                .iter()
                .all(|e| !e.code.is_empty() && !e.label.is_empty())
        );
        assert!(parse_reference_list(&xml, RefKind::Terms).is_err() || kind == RefKind::Terms);
    }
    let years = parse_reference_list(&page("years.xml").unwrap(), RefKind::Years).unwrap();
    assert_eq!(years.iter().filter(|e| e.current).count(), 1);
    let subjects = parse_reference_list(&page("subjects.xml").unwrap(), RefKind::Subjects).unwrap();
    assert!(
        subjects.iter().any(|e| e.label.contains('&')),
        "entities unescaped"
    );
}

#[test]
fn catalist_comp() {
    let Some(html) = page("catalist-comp.html") else {
        return;
    };
    let parsed = parse_catalog_subject(&html, CatalogYear(2026)).unwrap();
    let courses = &parsed.value;
    let unparsed = courses
        .iter()
        .filter(|c| {
            c.prerequisites
                .as_ref()
                .is_some_and(|p| !p.raw.is_empty() && matches!(p.parsed, PrereqExpr::Unparsed(_)))
        })
        .count();
    eprintln!(
        "catalist: {} courses, {} with prerequisites, {} unparsed, {} exclusions, {} cross-lists, issues {:?}",
        courses.len(),
        courses.iter().filter(|c| c.prerequisites.is_some()).count(),
        unparsed,
        courses
            .iter()
            .map(|c| c.mutual_exclusions.len())
            .sum::<usize>(),
        courses.iter().filter(|c| !c.cross_list.is_empty()).count(),
        parsed.report.issues
    );
    assert!(courses.len() > 100);
    assert_eq!(parsed.report.rows_seen, parsed.report.rows_kept);
    assert!(courses.iter().filter(|c| c.prerequisites.is_some()).count() > 50);
    assert!(courses.iter().any(|c| {
        c.prerequisites
            .as_ref()
            .is_some_and(|p| p.corequisite.is_some())
    }));
    assert!(courses.iter().any(|c| !c.mutual_exclusions.is_empty()));
    assert!(
        courses
            .iter()
            .all(|c| c.mutual_exclusions.iter().all(|m| !m.with.is_empty()))
    );
    assert!(courses.iter().any(|c| !c.cross_list.is_empty()));
    assert!(courses.iter().any(|c| !c.equivalents.is_empty()));
    assert!(courses.iter().any(|c| c.flags.repeatable));
    assert!(courses.iter().any(|c| !c.attributes.is_empty()));
    assert!(
        courses
            .iter()
            .all(|c| !c.description.is_empty() && !c.department.is_empty())
    );
    assert!(!parsed.report.has_issue(IssueCode::UnknownDetailLabel));
    assert!(!parsed.report.has_issue(IssueCode::UnknownAttribute));
}

#[test]
fn ga_index() {
    let Some(html) = page("ga-index.html") else {
        return;
    };
    let parsed = parse_program_index(&html).unwrap();
    eprintln!("index: {} programs", parsed.value.len());
    assert!(parsed.value.len() >= 300, "{}", parsed.value.len());
    let bscs = parsed
        .value
        .iter()
        .find(|l| l.slug == "computer-science-bscs")
        .unwrap();
    eprintln!("{bscs:?}");
    assert_eq!(
        bscs.url,
        "https://ga.rice.edu/programs-study/departments-programs/engineering/computer-science/computer-science-bscs/"
    );
    assert!(bscs.title.contains("Computer Science"));
    assert_eq!(bscs.school.as_deref(), Some("EN"));
    assert_eq!(bscs.department.as_deref(), Some("Computer Science"));
}

#[test]
fn ga_bscs() {
    let Some(html) = page("ga-bscs.html") else {
        return;
    };
    let parsed = parse_program_page(&html, "https://ga.rice.edu/programs-study/departments-programs/engineering/computer-science/computer-science-bscs/").unwrap();
    let draft = &parsed.value;
    eprintln!(
        "bscs: {} areas {:?}, total {:?}, {} aliases, footnotes {}, issues {:?}",
        draft.areas.len(),
        draft
            .areas
            .iter()
            .map(|a| (&a.title, a.rules.len(), a.unparsed.len()))
            .collect::<Vec<_>>(),
        draft.total_credits,
        draft.aliases.len(),
        draft.footnotes.len(),
        parsed.report.issues
    );
    assert_eq!(draft.credential, "BSCS");
    assert!(draft.areas.len() >= 4, "{}", draft.areas.len());
    assert!(
        draft
            .areas
            .iter()
            .all(|a| !a.title.starts_with("Total Credit Hours"))
    );
    let selects = draft
        .areas
        .iter()
        .flat_map(|a| &a.rules)
        .filter(|r| matches!(r.body, RequirementBody::Select { .. }))
        .count();
    assert!(selects >= 5, "{selects}");
    assert!(
        draft
            .aliases
            .iter()
            .any(|(a, b)| a.to_string() == "STAT 310" && b.to_string() == "ECON 307")
    );
    assert_eq!(
        draft.total_credits.map(skyspace_core::Credits::cents),
        Some(12_000)
    );
    assert!(!draft.footnotes.is_empty());
    let or_rule = draft.areas[0]
        .rules
        .iter()
        .find(|r| matches!(&r.body, RequirementBody::Course { filter, .. } if filter.include.len() == 2))
        .expect("MATH 101 or MATH 105");
    assert_eq!(or_rule.label, "SINGLE VARIABLE CALCULUS I");
}

#[test]
fn ga_bassoon() {
    let Some(html) = page("ga-bassoon.html") else {
        return;
    };
    let parsed = parse_program_page(&html, "https://ga.rice.edu/programs-study/departments-programs/music/music/bassoon-performance-bmus/").unwrap();
    let draft = &parsed.value;
    eprintln!(
        "bassoon: areas {:?}, total {:?}, issues {:?}",
        draft
            .areas
            .iter()
            .map(|a| (&a.title, a.rules.len(), a.unparsed.len()))
            .collect::<Vec<_>>(),
        draft.total_credits,
        parsed.report.issues
    );
    assert_eq!(draft.credential, "BMus");
    assert_eq!(draft.areas.len(), 6);
    let lessons = draft
        .areas
        .iter()
        .flat_map(|a| &a.rules)
        .find(|r| r.label.starts_with("BASSOON FOR MAJORS"))
        .unwrap();
    assert!(matches!(
        lessons.body,
        RequirementBody::Course { semesters: 8, .. }
    ));
    let recital = draft
        .areas
        .iter()
        .flat_map(|a| &a.rules)
        .find(|r| r.label == "JUNIOR RECITAL")
        .unwrap();
    assert_eq!(recital.hours.map(|h| h.min().cents()), Some(0));
    let piano = draft
        .areas
        .iter()
        .find(|a| a.title == "Piano Proficiency Exam")
        .unwrap();
    assert_eq!(piano.unparsed.len(), 1);
    assert!(piano.rules.is_empty());
}
