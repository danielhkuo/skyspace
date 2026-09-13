#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

//! Builders shared by the integration tests. Ids are deterministic so the
//! bundles they build can be written as golden files.

use std::collections::BTreeSet;

use skyspace_core::Timestamp;
use skyspace_core::catalog::Attribute;
use skyspace_core::code::{CourseCode, Subject};
use skyspace_core::evaluate::{CourseFacts, CourseInfo, PlanBundle};
use skyspace_core::plan::{
    CreditOrigin, EntryId, ManualCourseCard, NonCourseClaim, Plan, PlanId, PlanTerm, PlannedCourse,
    SelfCheck, TermId, TermKind,
};
use skyspace_core::prereq::{PrereqExpr, PrereqFact, Prerequisite};
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, CreditScope, NonCourseKind, Program, ProgramId,
    ProgramKind, Requirement, RequirementBody, RequirementId, Review, SourceRef,
};
use skyspace_core::term::{CreditRange, Credits, Season, TermPosition};
use skyspace_core::warn::CreditLimits;
use uuid::Uuid;

/// A stable UUID from a name, so fixtures and golden files agree run to run.
pub fn uuid(name: &str) -> Uuid {
    let mut bytes = [0u8; 16];
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    bytes[..8].copy_from_slice(&h.to_be_bytes());
    let mut g: u64 = h.rotate_left(29) ^ 0x9e37_79b9_7f4a_7c15;
    for b in name.bytes().rev() {
        g ^= u64::from(b);
        g = g.wrapping_mul(0x0100_0000_01b3);
    }
    bytes[8..].copy_from_slice(&g.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn code(raw: &str) -> CourseCode {
    CourseCode::parse(raw).unwrap()
}

pub fn hours(h: u16) -> Credits {
    Credits::from_cents(h * 100)
}

pub fn fixed(h: u16) -> CreditRange {
    CreditRange::Fixed(hours(h))
}

pub fn requirement_id(name: &str) -> RequirementId {
    RequirementId(uuid(&format!("requirement-{name}")))
}

pub fn program_id(name: &str) -> ProgramId {
    ProgramId(uuid(&format!("program-{name}")))
}

pub fn term_id(name: &str) -> TermId {
    TermId(uuid(&format!("term-{name}")))
}

pub fn entry_id(name: &str) -> EntryId {
    EntryId(uuid(&format!("entry-{name}")))
}

pub fn source() -> SourceRef {
    SourceRef {
        url: "https://ga.rice.edu/programs-study/departments-programs/".to_owned(),
        anchor: None,
    }
}

pub fn position(academic_year: u16, season: Season) -> TermPosition {
    TermPosition {
        academic_year,
        season,
    }
}

pub fn req(
    name: &str,
    label: &str,
    hours: Option<CreditRange>,
    body: RequirementBody,
) -> Requirement {
    Requirement {
        id: requirement_id(name),
        label: label.to_owned(),
        hours,
        source: source(),
        body,
    }
}

pub fn all(name: &str, label: &str, of: Vec<Requirement>) -> Requirement {
    req(name, label, None, RequirementBody::All { of })
}

pub fn select(name: &str, label: &str, count: u8, of: Vec<Requirement>) -> Requirement {
    req(name, label, None, RequirementBody::Select { count, of })
}

pub fn course(name: &str, raw: &str, h: Option<u16>) -> Requirement {
    course_n(name, raw, h, 1)
}

pub fn course_n(name: &str, raw: &str, h: Option<u16>, semesters: u8) -> Requirement {
    req(
        name,
        raw,
        h.map(fixed),
        RequirementBody::Course {
            filter: CourseFilter {
                include: vec![CourseSelector::Code { code: code(raw) }],
                exclude: vec![],
            },
            semesters,
        },
    )
}

pub fn course_filter(name: &str, label: &str, h: Option<u16>, filter: CourseFilter) -> Requirement {
    req(
        name,
        label,
        h.map(fixed),
        RequirementBody::Course {
            filter,
            semesters: 1,
        },
    )
}

pub fn attribute_course(name: &str, label: &str, attribute: Attribute) -> Requirement {
    course_filter(
        name,
        label,
        Some(3),
        CourseFilter {
            include: vec![CourseSelector::Attribute { attribute }],
            exclude: vec![],
        },
    )
}

pub fn credits_rule(name: &str, label: &str, minimum: u16, from: CourseFilter) -> Requirement {
    req(
        name,
        label,
        None,
        RequirementBody::Credits {
            minimum: hours(minimum),
            scope: CreditScope::Additional,
            from,
        },
    )
}

pub fn non_course(name: &str, description: &str) -> Requirement {
    req(
        name,
        description,
        None,
        RequirementBody::NonCourse {
            non_course_kind: NonCourseKind::ProficiencyExam,
            description: description.to_owned(),
        },
    )
}

pub fn unverifiable(name: &str, text: &str) -> Requirement {
    req(
        name,
        text,
        None,
        RequirementBody::Unverifiable {
            text: text.to_owned(),
        },
    )
}

pub fn program(
    name: &str,
    kind: ProgramKind,
    credential: &str,
    total: Option<u16>,
    root: Requirement,
) -> Program {
    Program {
        id: program_id(name),
        catalog_year: CatalogYear(2026),
        slug: name.to_owned(),
        kind,
        name: name.to_owned(),
        credential: credential.to_owned(),
        total_credits: total.map(hours),
        source: source(),
        review: Review {
            reviewed_by: "fixture".to_owned(),
            published_at: Timestamp(1_757_548_800),
        },
        root,
        retired_requirements: vec![],
    }
}

pub fn planned(name: &str, raw: &str, h: u16) -> PlannedCourse {
    PlannedCourse {
        id: entry_id(name),
        course: code(raw),
        credits: hours(h),
        fills: vec![],
        claims: vec![],
        observed: None,
        carried: None,
        note: None,
    }
}

pub fn rice_term(
    name: &str,
    academic_year: u16,
    season: Season,
    courses: Vec<PlannedCourse>,
) -> PlanTerm {
    PlanTerm {
        id: term_id(name),
        position: position(academic_year, season),
        label: None,
        kind: TermKind::Rice {
            code: None,
            courses,
        },
        non_course: vec![],
    }
}

pub fn off_term(
    name: &str,
    academic_year: u16,
    season: Season,
    claims: Vec<NonCourseClaim>,
) -> PlanTerm {
    PlanTerm {
        id: term_id(name),
        position: position(academic_year, season),
        label: Some("Co-op".to_owned()),
        kind: TermKind::Off,
        non_course: claims,
    }
}

pub fn manual(
    name: &str,
    origin: CreditOrigin,
    raw_code: &str,
    h: u16,
    equivalent: Option<&str>,
) -> ManualCourseCard {
    ManualCourseCard {
        id: entry_id(name),
        origin,
        code: raw_code.to_owned(),
        title: raw_code.to_owned(),
        credits: hours(h),
        institution: None,
        rice_equivalent: equivalent.map(code),
        credits_source: None,
        fills: vec![],
        claims: vec![],
        note: None,
    }
}

pub fn info(raw: &str, h: u16, attributes: &[Attribute], repeatable: bool) -> CourseInfo {
    CourseInfo {
        code: code(raw),
        title: raw.to_owned(),
        credits: fixed(h),
        attributes: attributes.iter().copied().collect::<BTreeSet<_>>(),
        department: Some(code(raw).subject.as_str().to_owned()),
        repeatable,
        seasons_offered: vec![],
        terms_observed: 0,
        offered_now: true,
    }
}

pub fn requires(raw: &str, expr: &str) -> Prerequisite {
    Prerequisite {
        course: code(raw),
        fact: PrereqFact::Requires {
            published_for: CatalogYear(2026),
            expr: PrereqExpr::parse(expr),
            published: expr.to_owned(),
            corequisite: None,
        },
    }
}

pub fn limits() -> CreditLimits {
    CreditLimits {
        fall_spring: hours(18),
        music_and_architecture: hours(20),
        summer: None,
    }
}

pub fn plan(name: &str, programs: &[&Program], terms: Vec<PlanTerm>) -> Plan {
    Plan {
        id: PlanId(uuid(&format!("plan-{name}"))),
        name: name.to_owned(),
        catalog_year: CatalogYear(2026),
        matriculation: position(2027, Season::Fall),
        programs: programs.iter().map(|p| p.id).collect(),
        incoming_credit: vec![],
        terms,
        self_checks: vec![],
    }
}

pub fn bundle(
    plan: Plan,
    programs: Vec<Program>,
    facts: CourseFacts,
    prerequisites: Vec<Prerequisite>,
) -> PlanBundle {
    PlanBundle {
        plan,
        programs,
        facts,
        prerequisites,
        exclusions: vec![],
        limits: limits(),
        invalidations: vec![],
        today: position(2027, Season::Fall),
    }
}

pub fn subject(raw: &str) -> Subject {
    Subject::new(raw).unwrap()
}

pub fn self_check(name: &str) -> SelfCheck {
    SelfCheck {
        requirement: requirement_id(name),
        reason: skyspace_core::plan::SelfCheckReason::Other,
        note: Some("checked by hand".to_owned()),
    }
}

// ------------------------------------------------------------- launch cases

/// Bioengineering: a prescribed sequence, one prerequisite absent from the
/// plan, and one course never seen (no facts, no prerequisite row).
pub fn bioe_bundle() -> PlanBundle {
    let root = all(
        "bioe-root",
        "Bioengineering, BSBE",
        vec![
            all(
                "bioe-core",
                "Core Requirements",
                vec![
                    course("bioe-chem-121", "CHEM 121", Some(3)),
                    course("bioe-chem-122", "CHEM 122", Some(3)),
                    course("bioe-math-101", "MATH 101", Some(3)),
                    course("bioe-math-102", "MATH 102", Some(3)),
                    course("bioe-bioe-252", "BIOE 252", Some(3)),
                    course("bioe-bioe-322", "BIOE 322", Some(3)),
                    course("bioe-bioe-391", "BIOE 391", Some(3)),
                ],
            ),
            unverifiable(
                "bioe-freshman",
                "Students should complete these courses during their freshman year",
            ),
        ],
    );
    let bioe = program(
        "bioengineering-bsbe",
        ProgramKind::Major,
        "BSBE",
        Some(131),
        root,
    );
    let mut facts = CourseFacts::default();
    for (raw, h) in [
        ("CHEM 121", 3),
        ("CHEM 122", 3),
        ("MATH 101", 3),
        ("MATH 102", 3),
        ("BIOE 252", 3),
        ("BIOE 322", 3),
    ] {
        facts.insert(info(raw, h, &[], false));
    }
    let prerequisites = vec![
        requires("CHEM 122", "CHEM 121"),
        requires("MATH 102", "MATH 101"),
        requires("BIOE 322", "BIOE 252 AND (MATH 102 OR MATH 106)"),
        requires("BIOE 252", "CHEM 121 AND MATH 101"),
    ];
    let plan = plan(
        "bioe",
        &[&bioe],
        vec![
            rice_term(
                "bioe-fall-1",
                2027,
                Season::Fall,
                vec![
                    planned("bioe-chem-121", "CHEM 121", 3),
                    planned("bioe-math-101", "MATH 101", 3),
                ],
            ),
            rice_term(
                "bioe-spring-1",
                2027,
                Season::Spring,
                vec![
                    planned("bioe-chem-122", "CHEM 122", 3),
                    planned("bioe-bioe-252", "BIOE 252", 3),
                ],
            ),
            rice_term(
                "bioe-fall-2",
                2028,
                Season::Fall,
                vec![
                    // MATH 102 is absent from the plan: a Prerequisite warning.
                    planned("bioe-bioe-322", "BIOE 322", 3),
                    // BIOE 391 has no facts and no prerequisite row: silence.
                    planned("bioe-bioe-391", "BIOE 391", 3),
                ],
            ),
        ],
    );
    bundle(plan, vec![bioe], facts, prerequisites)
}

/// Bassoon `BMus`: eight lesson slots, zero-credit recitals, the piano
/// proficiency exam as a claimed non-course rule, one unverifiable row.
pub fn bmus_bundle() -> PlanBundle {
    let root = all(
        "bmus-root",
        "Bassoon Performance, BMus",
        vec![
            all(
                "bmus-study",
                "Individual and Ensemble Study",
                vec![
                    course_n("bmus-lessons", "MUSI 457", Some(3), 8),
                    course_filter(
                        "bmus-secondary",
                        "Secondary Lessons",
                        Some(1),
                        CourseFilter {
                            include: vec![CourseSelector::NumberRange {
                                subject: Some(subject("MUSI")),
                                low: 251,
                                high: 297,
                            }],
                            exclude: vec![CourseSelector::Code {
                                code: code("MUSI 281"),
                            }],
                        },
                    ),
                ],
            ),
            all(
                "bmus-recitals",
                "Recitals",
                vec![
                    course("bmus-junior", "MUSI 341", Some(0)),
                    course("bmus-senior", "MUSI 441", Some(0)),
                ],
            ),
            all(
                "bmus-piano-area",
                "Piano Proficiency Exam",
                vec![non_course(
                    "bmus-piano",
                    "Students must complete and pass the Piano Proficiency Exam",
                )],
            ),
            unverifiable(
                "bmus-footnote",
                "Consult the department for jury requirements",
            ),
        ],
    );
    let bmus = program(
        "bassoon-performance-bmus",
        ProgramKind::Major,
        "BMus",
        Some(120),
        root,
    );
    let mut facts = CourseFacts::default();
    facts.insert(info("MUSI 457", 3, &[], true));
    facts.insert(info("MUSI 341", 0, &[], false));
    facts.insert(info("MUSI 441", 0, &[], false));
    facts.insert(info("MUSI 260", 1, &[], true));
    let seasons = [
        (2027, Season::Fall),
        (2027, Season::Spring),
        (2028, Season::Fall),
        (2028, Season::Spring),
        (2029, Season::Fall),
        (2029, Season::Spring),
        (2030, Season::Fall),
    ];
    let mut terms: Vec<PlanTerm> = seasons
        .iter()
        .enumerate()
        .map(|(i, (year, season))| {
            let mut courses = vec![planned(&format!("bmus-lesson-{i}"), "MUSI 457", 3)];
            if i == 0 {
                courses.push(planned("bmus-secondary-card", "MUSI 260", 1));
            }
            if i == 3 {
                courses.push(planned("bmus-junior-card", "MUSI 341", 0));
            }
            rice_term(&format!("bmus-term-{i}"), *year, *season, courses)
        })
        .collect();
    // Seven lessons planned, one short: the lesson rule is Partial.
    terms.push(rice_term(
        "bmus-term-7",
        2030,
        Season::Spring,
        vec![planned("bmus-senior-card", "MUSI 441", 0)],
    ));
    let mut plan = plan("bmus", &[&bmus], terms);
    plan.terms[2].non_course.push(NonCourseClaim {
        requirement: requirement_id("bmus-piano"),
        label: "Piano Proficiency Exam".to_owned(),
    });
    bundle(plan, vec![bmus], facts, vec![])
}

/// Direct-entry `BArch`: 192 hours from the program, two ARCH 500 cards in
/// two Rice terms filling two sibling rules, no duplicate warning.
pub fn barch_bundle() -> PlanBundle {
    let root = all(
        "barch-root",
        "Architecture, BArch (direct entry)",
        vec![
            all(
                "barch-core",
                "Core Requirements",
                vec![
                    course("barch-arch-101", "ARCH 101", Some(6)),
                    course("barch-arch-102", "ARCH 102", Some(6)),
                ],
            ),
            all(
                "barch-precept",
                "Preceptorship and Advanced Requirements",
                vec![
                    course("barch-precept-1", "ARCH 500", Some(15)),
                    course("barch-precept-2", "ARCH 500", Some(15)),
                ],
            ),
            credits_rule(
                "barch-electives",
                "Additional Electives",
                6,
                CourseFilter::default(),
            ),
        ],
    );
    let barch = program(
        "architecture-barch-direct-entry",
        ProgramKind::Major,
        "BArch",
        Some(192),
        root,
    );
    let mut facts = CourseFacts::default();
    facts.insert(info("ARCH 101", 6, &[], false));
    facts.insert(info("ARCH 102", 6, &[], false));
    facts.insert(info("ARCH 500", 15, &[], false));
    facts.insert(info("HIST 210", 3, &[Attribute::DistributionOne], false));
    let plan = plan(
        "barch",
        &[&barch],
        vec![
            rice_term(
                "barch-fall-1",
                2027,
                Season::Fall,
                vec![
                    planned("barch-arch-101", "ARCH 101", 6),
                    planned("barch-hist-210", "HIST 210", 3),
                ],
            ),
            rice_term(
                "barch-spring-1",
                2027,
                Season::Spring,
                vec![planned("barch-arch-102", "ARCH 102", 6)],
            ),
            rice_term(
                "barch-fall-4",
                2030,
                Season::Fall,
                vec![planned("barch-precept-a", "ARCH 500", 15)],
            ),
            rice_term(
                "barch-spring-4",
                2030,
                Season::Spring,
                vec![planned("barch-precept-b", "ARCH 500", 15)],
            ),
        ],
    );
    bundle(plan, vec![barch], facts, vec![])
}

/// The BSCS-with-university case exported from the web fixture.
pub fn bscs_bundle() -> PlanBundle {
    serde_json::from_str(include_str!("../golden/comp-bscs-2026/bundle.json")).unwrap()
}
