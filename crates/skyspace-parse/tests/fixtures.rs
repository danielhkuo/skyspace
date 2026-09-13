#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Hand-written fixtures that mimic Rice's markup, checked against insta
//! snapshots. Real pages are exercised by `live_pages.rs` when available.

use skyspace_core::Timestamp;
use skyspace_core::catalog::{FinalExam, MeetingPattern};
use skyspace_core::prereq::PrereqExpr;
use skyspace_core::program::{
    CatalogYear, CourseSelector, ProgramId, ProgramKind, RequirementBody, Review,
};
use skyspace_core::{Crn, TermCode};
use skyspace_parse::{
    IssueCode, ParseError, RefKind, parse_associated_sections, parse_catalog_subject,
    parse_enrollment, parse_program_index, parse_program_page, parse_reference_list,
    parse_section_detail, parse_subject_listing, sel,
};

const LISTING: &str = include_str!("fixtures/listing.html");
const SEARCH_FORM: &str = include_str!("fixtures/search-form.html");
const DETAIL: &str = include_str!("fixtures/detail.html");
const ASSOC: &str = include_str!("fixtures/assoc.xml");
const ENROLLMENT: &str = include_str!("fixtures/enrollment.xml");
const CATALIST: &str = include_str!("fixtures/catalist.html");
const GA_INDEX: &str = include_str!("fixtures/ga-index.html");
const GA_BSCS: &str = include_str!("fixtures/ga-bscs.html");
const GA_BMUS: &str = include_str!("fixtures/ga-bmus.html");

fn fall_2026() -> TermCode {
    TermCode::parse("202710").unwrap()
}

#[test]
fn listing_rows_become_section_listings() {
    let parsed = parse_subject_listing(LISTING, fall_2026()).unwrap();
    assert_eq!(parsed.report.rows_seen, 9);
    assert_eq!(parsed.report.rows_kept, 8);
    let rows = &parsed.value;
    assert_eq!(
        rows[1].meetings.len(),
        2,
        "COMP 222 has a lecture and a lab"
    );
    assert!(rows[2].meetings.is_empty(), "COMP 105 has no schedule");
    assert!(rows[3].instructors.is_empty(), "MUSI 457 has no instructor");
    assert_eq!(rows[5].credits.min().cents(), 75);
    assert!(matches!(
        rows[6].meetings[1].pattern,
        MeetingPattern::Unparsed(_)
    ));
    assert_eq!(rows[6].final_exam, FinalExam::Unknown);
    assert!(
        rows[6].instructors[0].net_id.is_none(),
        "an empty p_netid is no NetID"
    );
    assert_eq!(
        rows[7].credits.min().cents(),
        123,
        "1.234 rounds to hundredths"
    );
    assert!(parsed.report.has_issue(IssueCode::UnreadableMeeting));
    assert!(parsed.report.has_issue(IssueCode::UnknownFinalExam));
    assert!(parsed.report.has_issue(IssueCode::SubCentCredits));
    assert!(parsed.report.has_issue(IssueCode::RowMissingKey));
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn listing_guards_term_and_shape() {
    let spring = TermCode::parse("202720").unwrap();
    assert!(matches!(
        parse_subject_listing(LISTING, spring),
        Err(ParseError::WrongTerm { .. })
    ));
    assert!(matches!(
        parse_subject_listing(SEARCH_FORM, fall_2026()),
        Err(ParseError::SelectorMissing {
            selector: "td.cls-crn",
            ..
        })
    ));
    assert!(matches!(
        parse_subject_listing("<html><body></body></html>", fall_2026()),
        Err(ParseError::SelectorMissing { selector: "h2", .. })
    ));
    assert!(sel("td.cls-crn").is_ok());
    assert!(matches!(sel("td[["), Err(ParseError::BadSelector { .. })));
}

#[test]
fn detail_page_reads_labels() {
    let parsed =
        parse_section_detail(DETAIL, fall_2026(), Crn(12422), Timestamp(1_700_000_000)).unwrap();
    let detail = &parsed.value;
    assert_eq!(
        detail.reserved.len(),
        2,
        "the unreadable reservation is reported, not kept"
    );
    assert_eq!(detail.fees.len(), 2);
    assert_eq!(detail.fees[0].amount_cents, Some(7500));
    assert_eq!(detail.fees[1].amount_cents, Some(1200));
    assert_eq!(detail.restrictions.as_ref().unwrap().clauses.len(), 3);
    assert_eq!(detail.attributes.len(), 2);
    assert!(!detail.has_syllabus);
    assert!(parsed.report.has_issue(IssueCode::UnknownDetailLabel));
    assert!(
        parsed
            .report
            .issues
            .iter()
            .any(|i| i.detail.contains("Zoom Link"))
    );
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn associated_sections_carry_dates_and_codes() {
    let parsed = parse_associated_sections(ASSOC, fall_2026()).unwrap();
    assert_eq!(parsed.report.rows_seen, 3);
    assert_eq!(parsed.report.rows_kept, 2);
    let first = &parsed.value[0];
    assert_eq!(
        first.meetings.len(),
        2,
        "the final-exam meeting is not a weekly meeting"
    );
    assert!(first.meetings.iter().all(|m| m.dates.is_some()));
    assert_eq!(first.instructors.len(), 2);
    assert!(parsed.report.has_issue(IssueCode::UnknownAttribute));
    assert!(parsed.report.has_issue(IssueCode::UnknownFinalExam));
    assert!(parsed.report.has_issue(IssueCode::UnreadableMeeting));
    assert!(parsed.report.has_issue(IssueCode::RowMissingKey));
    assert!(matches!(
        parse_associated_sections(ASSOC, TermCode::parse("202720").unwrap()),
        Err(ParseError::WrongTerm { .. })
    ));
    assert!(matches!(
        parse_associated_sections("<nope", fall_2026()),
        Err(ParseError::Xml(_))
    ));
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn enrollment_is_four_numbers_and_rice_time() {
    let seats = parse_enrollment(ENROLLMENT).unwrap();
    insta::assert_debug_snapshot!(seats);
    assert!(matches!(
        parse_enrollment(&ENROLLMENT.replace("2026-09-12T20:47:51-05:00", "soon")),
        Err(ParseError::Timestamp(_))
    ));
}

#[test]
fn reference_lists_read_every_kind() {
    let fixtures = [
        (RefKind::Terms, include_str!("fixtures/ref-terms.xml")),
        (RefKind::Subjects, include_str!("fixtures/ref-subjects.xml")),
        (
            RefKind::Departments,
            include_str!("fixtures/ref-departments.xml"),
        ),
        (RefKind::Schools, include_str!("fixtures/ref-schools.xml")),
        (RefKind::Sessions, include_str!("fixtures/ref-sessions.xml")),
        (RefKind::Years, include_str!("fixtures/ref-years.xml")),
        (RefKind::Attrs, include_str!("fixtures/ref-attrs.xml")),
    ];
    let mut all = Vec::new();
    for (kind, xml) in fixtures {
        let entries = parse_reference_list(xml, kind).unwrap();
        assert!(!entries.is_empty());
        let other = if kind == RefKind::Terms {
            RefKind::Attrs
        } else {
            RefKind::Terms
        };
        assert!(matches!(
            parse_reference_list(xml, other),
            Err(ParseError::SelectorMissing { .. })
        ));
        all.push((kind, entries));
    }
    assert!(all[1].1[0].label.contains("African & African"));
    assert!(all[0].1.iter().any(|e| e.current && e.code == "202710"));
    assert!(all[5].1.iter().any(|e| e.current && e.code == "2027"));
    assert!(matches!(
        parse_reference_list("<TERMS></TERMS>", RefKind::Terms),
        Err(ParseError::SelectorMissing { .. })
    ));
    insta::assert_debug_snapshot!(all);
}

#[test]
fn catalog_records_carry_prerequisites_and_sentences() {
    let parsed = parse_catalog_subject(CATALIST, CatalogYear(2026)).unwrap();
    assert_eq!(parsed.report.rows_seen, 4);
    assert_eq!(parsed.report.rows_kept, 3);
    let courses = &parsed.value;
    let comp_182 = &courses[0];
    assert!(matches!(
        comp_182.prerequisites.as_ref().unwrap().parsed,
        PrereqExpr::All(_)
    ));
    assert_eq!(comp_182.mutual_exclusions[0].with.len(), 2);
    assert_eq!(comp_182.cross_list.len(), 1);
    assert_eq!(comp_182.equivalents.len(), 1);
    assert!(comp_182.flags.repeatable);
    assert_eq!(comp_182.attributes.len(), 2);
    let chem_420 = &courses[1];
    assert!(matches!(
        chem_420.prerequisites.as_ref().unwrap().parsed,
        PrereqExpr::Unparsed(_)
    ));
    assert!(chem_420.flags.instructor_permission && chem_420.flags.second_half);
    assert!(chem_420.mutual_exclusions[0].with.is_empty());
    let comp_533 = &courses[2];
    assert_eq!(
        comp_533
            .prerequisites
            .as_ref()
            .unwrap()
            .corequisite
            .as_ref()
            .unwrap()
            .to_string(),
        "COMP 504"
    );
    assert!(comp_533.prerequisites.as_ref().unwrap().raw.is_empty());
    assert!(parsed.report.has_issue(IssueCode::UnparsedPrerequisite));
    assert!(parsed.report.has_issue(IssueCode::UnknownAttribute));
    assert!(parsed.report.has_issue(IssueCode::UnreadableExclusion));
    assert!(parsed.report.has_issue(IssueCode::RowMissingKey));
    assert!(matches!(
        parse_catalog_subject(
            "<html><body><p>No courses found.</p></body></html>",
            CatalogYear(2026)
        ),
        Err(ParseError::SelectorMissing { .. })
    ));
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn program_index_merges_the_four_tables() {
    let parsed = parse_program_index(GA_INDEX).unwrap();
    let links = &parsed.value;
    assert_eq!(links.len(), 5, "{links:?}");
    let bscs = links
        .iter()
        .find(|l| l.slug == "computer-science-bscs")
        .unwrap();
    assert_eq!(
        bscs.title,
        "Bachelor of Science in Computer Science (BSCS) Degree"
    );
    assert_eq!(bscs.school.as_deref(), Some("EN"));
    let ba = links
        .iter()
        .find(|l| l.slug == "computer-science-ba")
        .unwrap();
    assert_eq!(ba.title, "BA");
    assert!(
        links
            .iter()
            .all(|l| l.url.starts_with("https://ga.rice.edu/programs-study/"))
    );
    assert!(matches!(
        parse_program_index("<html><body></body></html>"),
        Err(ParseError::SelectorMissing { .. })
    ));
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn bscs_page_becomes_areas_and_rules() {
    let url = "https://ga.rice.edu/programs-study/departments-programs/engineering/computer-science/computer-science-bscs/";
    let parsed = parse_program_page(GA_BSCS, url).unwrap();
    let draft = &parsed.value;
    assert_eq!(draft.credential, "BSCS");
    assert_eq!(
        draft.total_credits.map(skyspace_core::Credits::cents),
        Some(12_000)
    );
    assert_eq!(draft.areas.len(), 3);
    let core = &draft.areas[0];
    assert_eq!(core.unparsed, vec!["Math Courses".to_owned()]);
    assert_eq!(core.rules.len(), 5);
    assert_eq!(core.rules[0].label, "SINGLE VARIABLE CALCULUS I");
    assert!(
        matches!(&core.rules[0].body, RequirementBody::Course { filter, .. } if filter.include.len() == 2)
    );
    assert_eq!(core.rules[2].label, "(AI-ASSISTED SOFTWARE DEVELOPMENT)");
    assert!(
        matches!(&core.rules[3].body, RequirementBody::Select { count: 1, of } if of.len() == 3)
    );
    assert!(
        matches!(&core.rules[4].body, RequirementBody::Credits { minimum, from, .. } if minimum.cents() == 600 && from.include.len() == 2)
    );
    assert_eq!(draft.aliases.len(), 1);
    assert_eq!(draft.aliases[0].1.to_string(), "ECON 307");
    assert_eq!(draft.areas[2].title, "Elective Requirements");
    assert_eq!(draft.areas[2].unparsed.len(), 1);
    assert!(draft.areas[2].rules.is_empty());
    assert_eq!(draft.footnotes.len(), 3);
    let again = parse_program_page(GA_BSCS, url).unwrap();
    assert_eq!(
        again.value, parsed.value,
        "placeholder ids are deterministic"
    );
    assert!(matches!(
        parse_program_page("<html><body><h1>Nothing</h1></body></html>", url),
        Err(ParseError::SelectorMissing { .. })
    ));
    insta::assert_debug_snapshot!(parsed);
}

#[test]
fn bmus_page_reads_semesters_ranges_and_recitals() {
    let url = "https://ga.rice.edu/programs-study/departments-programs/music/music/bassoon-performance-bmus/";
    let parsed = parse_program_page(GA_BMUS, url).unwrap();
    let draft = &parsed.value;
    assert_eq!(draft.credential, "BMus");
    assert_eq!(draft.areas.len(), 4);
    let study = &draft.areas[1];
    assert!(matches!(
        study.rules[0].body,
        RequirementBody::Course { semesters: 8, .. }
    ));
    assert!(matches!(
        study.rules[1].body,
        RequirementBody::Course { semesters: 4, .. }
    ));
    let RequirementBody::Course {
        filter,
        semesters: 1,
    } = &study.rules[2].body
    else {
        panic!(
            "secondary lessons should be a range rule: {:?}",
            study.rules[2]
        );
    };
    assert_eq!(
        filter.include,
        vec![CourseSelector::NumberRange {
            subject: Some(skyspace_core::Subject::new("MUSI").unwrap()),
            low: 251,
            high: 297
        }]
    );
    assert_eq!(filter.exclude.len(), 1);
    assert_eq!(
        draft.areas[2].rules[0].hours.map(|h| h.min().cents()),
        Some(0)
    );
    assert_eq!(draft.areas[3].unparsed.len(), 1);
    assert_eq!(draft.aliases.len(), 1, "MUSI 222 / MDEM 222");
    insta::assert_debug_snapshot!(parsed);

    let program = draft.clone().into_program(
        ProgramId(uuid::Uuid::nil()),
        CatalogYear(2026),
        ProgramKind::Major,
        "bassoon-performance-bmus".to_owned(),
        Review {
            reviewed_by: "reviewer".to_owned(),
            published_at: Timestamp(0),
        },
    );
    let RequirementBody::All { of } = &program.root.body else {
        panic!("root must be All");
    };
    assert_eq!(of.len(), 4 + 1, "four areas and one footnote");
    let unverifiable = program
        .root
        .flatten()
        .into_iter()
        .filter(|r| matches!(r.body, RequirementBody::Unverifiable { .. }))
        .count();
    assert_eq!(unverifiable, 2, "the piano exam row and the footnote");
    assert_eq!(program.credential, "BMus");
    assert_eq!(program.requirement_count(), 1 + 5 + 7);
}
