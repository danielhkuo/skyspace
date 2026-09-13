#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]
//! Fixtures and request helpers shared by the API tests. Data is seeded
//! through the store's public ingest methods, never with SQL.

use std::collections::BTreeSet;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::header::{CONTENT_TYPE, COOKIE, ORIGIN, SET_COOKIE};
use axum::http::{Method, Request, Response, StatusCode};
use serde_json::Value;
use skyspace_api::config::Config;
use skyspace_api::{AppState, Mailer, router};
use skyspace_core::Timestamp;
use skyspace_core::catalog::{
    Course, CourseFlags, DaySet, FinalExam, Meeting, MeetingPattern, MeetingTime, MinuteOfDay,
    Seats, SectionListing,
};
use skyspace_core::code::{CourseCode, Crn, SectionNumber};
use skyspace_core::plan::{
    EntryId, Plan, PlanId, PlanTerm, PlannedCourse, ScheduleId, TermId, TermKind,
};
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, Program, ProgramId, ProgramKind, Requirement,
    RequirementBody, RequirementId, Review, SourceRef,
};
use skyspace_core::schedule::{Candidate, TermSchedule};
use skyspace_core::term::{CreditRange, Credits, Season, TermCode, TermPosition};
use skyspace_store::{RunOutcome, RunSummaryRow, Store, TermInput, VersionSource};
use tower::ServiceExt;
use uuid::Uuid;

pub const ORIGIN_VALUE: &str = "http://127.0.0.1:5173";
pub const FALL: &str = "202710";

pub fn code(raw: &str) -> CourseCode {
    CourseCode::parse(raw).unwrap()
}

pub fn term(raw: &str) -> TermCode {
    TermCode::parse(raw).unwrap()
}

pub fn fall() -> TermCode {
    term(FALL)
}

/// The router and the state it was built from; the state keeps the
/// capturing mailer the sign-in helper reads.
pub fn app(pool: sqlx::PgPool) -> (Router, AppState) {
    let state = AppState::for_test(Store::from_pool(pool));
    (router(state.clone()), state)
}

/// Like `app`, with `edit` applied to the test configuration first.
pub fn app_with(pool: sqlx::PgPool, edit: impl FnOnce(&mut Config)) -> (Router, AppState) {
    let mut config = Config::for_test("postgres://test");
    edit(&mut config);
    let state = AppState::new(
        Store::from_pool(pool),
        config,
        Mailer::Capture(std::sync::Arc::default()),
    );
    (router(state.clone()), state)
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

pub fn seats(enrolled: u16, capacity: u16, as_of: i64) -> Seats {
    Seats {
        enrolled,
        capacity,
        waitlist_count: 0,
        waitlist_capacity: 10,
        as_of: Timestamp(as_of),
    }
}

pub fn run_summary(outcome: RunOutcome) -> RunSummaryRow {
    RunSummaryRow {
        outcome,
        requests: 1,
        targets: 1,
        failures: 0,
        bytes: 10,
        rows_written: 3,
        error: None,
    }
}

/// Finish one run of `job` for the fall term with `outcome`.
pub async fn finish_run(store: &Store, job: &str, outcome: RunOutcome) {
    let id = store.start_run(job, Some(fall())).await.unwrap();
    assert!(store.finish_run(id, &run_summary(outcome)).await.unwrap());
}

/// Fall 2026 as the current term, three courses in catalog year 2027,
/// three sections (one without a meeting time), one polled seat row, and a
/// finished listing run.
pub async fn seed_catalog(store: &Store) {
    store
        .upsert_terms(&[TermInput {
            code: fall(),
            label: "Fall Semester 2026".to_owned(),
        }])
        .await
        .unwrap();
    assert!(store.set_current_term(fall()).await.unwrap());
    store
        .upsert_courses(
            CatalogYear(2027),
            &[
                course("COMP 140", "Computational Thinking", 2027),
                course("COMP 182", "Algorithmic Thinking", 2027),
                course("MATH 101", "Single Variable Calculus I", 2027),
                course("COMP 999", "Never Offered", 2027),
            ],
        )
        .await
        .unwrap();
    store
        .upsert_sections(
            fall(),
            &[
                listing(
                    10001,
                    "COMP 140",
                    "COMPUTATIONAL THINKING",
                    vec![timed("MWF", 9 * 60, 9 * 60 + 50)],
                ),
                listing(20002, "COMP 182", "ALGORITHMIC THINKING", vec![unparsed()]),
                listing(
                    30003,
                    "MATH 101",
                    "SINGLE VARIABLE CALCULUS I",
                    vec![timed("TR", 13 * 60, 14 * 60 + 15)],
                ),
            ],
        )
        .await
        .unwrap();
    store
        .record_seats(fall(), &[(Crn(30003), seats(10, 20, 1_700_000_000))])
        .await
        .unwrap();
    finish_run(store, "listings", RunOutcome::Ok).await;
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

/// Publish a one-rule major requiring COMP 140 for catalog year 2027.
pub async fn seed_program(store: &Store) -> ProgramId {
    let root = req(
        "Root",
        RequirementBody::All {
            of: vec![course_rule("COMP 140")],
        },
    );
    store
        .publish_program(
            &program("example-bs", 2026, root),
            "alice",
            VersionSource::Manual,
        )
        .await
        .unwrap()
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

pub fn plan(name: &str, programs: Vec<ProgramId>, codes: &[&str]) -> Plan {
    Plan {
        id: PlanId(Uuid::nil()),
        name: name.to_owned(),
        catalog_year: CatalogYear(2027),
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

/// Build a request. Mutating methods get the `Origin` and `Content-Type`
/// the guard demands; `cookie` is the `Set-Cookie` value from sign-in.
pub fn request(
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<&Value>,
) -> Request<Body> {
    let mutating = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let mut builder = Request::builder().method(method).uri(uri);
    if mutating {
        builder = builder
            .header(ORIGIN, ORIGIN_VALUE)
            .header(CONTENT_TYPE, "application/json");
    }
    if let Some(cookie) = cookie {
        builder = builder.header(COOKIE, cookie);
    }
    let body = match body {
        Some(value) => Body::from(serde_json::to_vec(value).unwrap()),
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

pub async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

pub async fn get(app: &Router, uri: &str, cookie: Option<&str>) -> Response<Body> {
    send(app, request(Method::GET, uri, cookie, None)).await
}

pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<&Value>,
) -> Response<Body> {
    send(app, request(method, uri, cookie, body)).await
}

pub async fn body_bytes(response: Response<Body>) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

pub async fn json(response: Response<Body>) -> Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&bytes)))
}

/// The `skyspace_session=...` pair from a `Set-Cookie` header, for `Cookie`.
pub fn cookie_pair(response: &Response<Body>) -> String {
    let raw = response
        .headers()
        .get(SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().to_owned()
}

/// Sign in through the real flow: request a code, read it from the
/// capturing mailer, verify it, and return the cookie pair.
pub async fn sign_in(app: &Router, state: &AppState, email: &str) -> String {
    let requested = call(
        app,
        Method::POST,
        "/api/v1/auth/email/request",
        None,
        Some(&serde_json::json!({ "email": email })),
    )
    .await;
    assert_eq!(requested.status(), StatusCode::ACCEPTED);
    let code = state
        .mailer
        .captured()
        .into_iter()
        .rev()
        .find(|c| c.to == email.to_ascii_lowercase())
        .expect("a captured code")
        .code;
    let verified = call(
        app,
        Method::POST,
        "/api/v1/auth/email/verify",
        None,
        Some(&serde_json::json!({ "email": email, "code": code })),
    )
    .await;
    assert_eq!(verified.status(), StatusCode::OK);
    cookie_pair(&verified)
}
