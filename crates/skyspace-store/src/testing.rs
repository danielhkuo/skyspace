//! Fixture builders shared by the query tests. Each `#[sqlx::test]` gets
//! its own database with the migrations applied; these build the rows.

use std::collections::BTreeSet;

use skyspace_core::Timestamp;
use skyspace_core::catalog::{
    Attribute, Course, CourseFlags, DaySet, FinalExam, Meeting, MeetingPattern, MeetingTime,
    MinuteOfDay, Seats, SectionListing,
};
use skyspace_core::code::{CourseCode, Crn, SectionNumber};
use skyspace_core::plan::{
    AccountId, EntryId, Plan, PlanId, PlanTerm, PlannedCourse, ScheduleId, TermId, TermKind,
};
use skyspace_core::prereq::{MutualExclusion, PrereqExpr, PrereqObservation};
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, Program, ProgramId, ProgramKind, Requirement,
    RequirementBody, RequirementId, Review, SourceRef,
};
use skyspace_core::schedule::{Candidate, TermSchedule};
use skyspace_core::term::{CreditRange, Credits, Season, TermCode, TermPosition};
use uuid::Uuid;

use crate::ingest::TermInput;
use crate::pool::Store;

pub fn code(raw: &str) -> CourseCode {
    CourseCode::parse(raw).unwrap()
}

pub fn term(raw: &str) -> TermCode {
    TermCode::parse(raw).unwrap()
}

pub fn fall() -> TermCode {
    term("202710")
}

pub async fn seed_term(store: &Store, raw: &str) -> TermCode {
    let code = term(raw);
    store
        .upsert_terms(&[TermInput {
            code,
            label: format!("Term {raw}"),
        }])
        .await
        .unwrap();
    code
}

pub async fn seed_account(store: &Store, email: &str) -> AccountId {
    store.create_account(email).await.unwrap().account_id()
}

pub fn timed(days: &str, start: u16, end: u16) -> Meeting {
    Meeting {
        pattern: MeetingPattern::Timed(MeetingTime {
            days: DaySet::from_letters(days).unwrap(),
            start: MinuteOfDay::new(start).unwrap(),
            end: MinuteOfDay::new(end).unwrap(),
        }),
        dates: None,
    }
}

pub fn unparsed() -> Meeting {
    Meeting {
        pattern: MeetingPattern::Unparsed("TBA".to_owned()),
        dates: None,
    }
}

pub fn listing(crn: u32, raw_code: &str, title: &str, meetings: Vec<Meeting>) -> SectionListing {
    SectionListing {
        crn: Crn(crn),
        term: fall(),
        code: code(raw_code),
        section: SectionNumber("001".to_owned()),
        title: title.to_owned(),
        credits: CreditRange::Fixed(Credits::from_cents(300)),
        part_of_term: None,
        instructors: Vec::new(),
        meetings,
        final_exam: FinalExam::Scheduled,
    }
}

pub fn course(raw_code: &str, title: &str, year: u16) -> Course {
    Course {
        catalog_year: CatalogYear(year),
        code: code(raw_code),
        title: title.to_owned(),
        credits: CreditRange::Fixed(Credits::from_cents(300)),
        department: "Computer Science".to_owned(),
        attributes: BTreeSet::new(),
        grade_mode: None,
        course_type: None,
        restrictions: None,
        prerequisites: None,
        description: format!("About {title}."),
        flags: CourseFlags::default(),
        mutual_exclusions: Vec::new(),
        cross_list: Vec::new(),
        equivalents: Vec::new(),
    }
}

pub fn with_prereq(mut course: Course, raw: &str) -> Course {
    course.prerequisites = Some(PrereqObservation {
        course: course.code.clone(),
        raw: raw.to_owned(),
        parsed: PrereqExpr::parse(raw),
        corequisite: None,
        catalog_year: course.catalog_year,
    });
    course
}

pub fn with_exclusion(mut course: Course, with: &[&str], raw: &str) -> Course {
    course.mutual_exclusions.push(MutualExclusion {
        with: with.iter().map(|c| code(c)).collect(),
        raw: raw.to_owned(),
    });
    course
}

pub fn with_attribute(mut course: Course, attribute: Attribute) -> Course {
    course.attributes.insert(attribute);
    course
}

pub fn seats(enrolled: u16, capacity: u16, as_of: i64) -> Seats {
    Seats {
        enrolled,
        capacity,
        waitlist_count: 0,
        waitlist_capacity: 10,
        as_of: Timestamp(as_of),
    }
}

pub fn source() -> SourceRef {
    SourceRef {
        url: "https://ga.rice.edu/programs-study/example/".to_owned(),
        anchor: None,
    }
}

pub fn req(label: &str, body: RequirementBody) -> Requirement {
    Requirement {
        id: RequirementId(Uuid::nil()),
        label: label.to_owned(),
        hours: None,
        source: source(),
        body,
    }
}

pub fn all(label: &str, of: Vec<Requirement>) -> Requirement {
    req(label, RequirementBody::All { of })
}

pub fn course_rule(raw_code: &str) -> Requirement {
    let mut r = req(
        raw_code,
        RequirementBody::Course {
            filter: CourseFilter {
                include: vec![CourseSelector::Code {
                    code: code(raw_code),
                }],
                exclude: Vec::new(),
            },
            semesters: 1,
        },
    );
    r.hours = Some(CreditRange::Fixed(Credits::from_cents(300)));
    r
}

pub fn program(slug: &str, year: u16, root: Requirement) -> Program {
    Program {
        id: ProgramId(Uuid::nil()),
        catalog_year: CatalogYear(year),
        slug: slug.to_owned(),
        kind: ProgramKind::Major,
        name: "Example Major".to_owned(),
        credential: "BS".to_owned(),
        total_credits: Some(Credits::from_cents(12000)),
        source: source(),
        review: Review {
            reviewed_by: "reviewer".to_owned(),
            published_at: Timestamp(1_700_000_000),
        },
        root,
        retired_requirements: Vec::new(),
    }
}

pub fn planned(raw_code: &str) -> PlannedCourse {
    PlannedCourse {
        id: EntryId(Uuid::new_v4()),
        course: code(raw_code),
        credits: Credits::from_cents(300),
        fills: Vec::new(),
        claims: Vec::new(),
        observed: None,
        carried: None,
        note: None,
    }
}

pub fn plan(name: &str, year: u16, programs: Vec<ProgramId>, codes: &[&str]) -> Plan {
    Plan {
        id: PlanId(Uuid::nil()),
        name: name.to_owned(),
        catalog_year: CatalogYear(year),
        matriculation: TermPosition {
            academic_year: 2027,
            season: Season::Fall,
        },
        programs,
        incoming_credit: Vec::new(),
        terms: vec![PlanTerm {
            id: TermId(Uuid::new_v4()),
            position: TermPosition {
                academic_year: 2027,
                season: Season::Fall,
            },
            label: None,
            kind: TermKind::Rice {
                code: Some(fall()),
                courses: codes.iter().map(|c| planned(c)).collect(),
            },
            non_course: Vec::new(),
        }],
        self_checks: Vec::new(),
    }
}

pub fn schedule(name: &str, crns: &[u32]) -> TermSchedule {
    TermSchedule {
        id: ScheduleId(Uuid::new_v4()),
        name: name.to_owned(),
        term: fall(),
        candidates: vec![Candidate {
            course: code("COMP 140"),
            sections: crns.iter().map(|c| Crn(*c)).collect(),
            visible: true,
            colour: 1,
        }],
        busy: Vec::new(),
    }
}
