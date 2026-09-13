#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Job tests over the parse crate's fixtures and a throwaway database.
//! `FixtureFetch` serves bytes from a map keyed by URL, so no test opens
//! a socket to Rice.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use bytes::Bytes;
use skyspace_core::code::Subject;
use skyspace_core::program::CatalogYear;
use skyspace_core::term::TermCode;
use skyspace_ingest::archive::sha256;
use skyspace_ingest::{
    Archive, Fetch, FetchError, FetchOutcome, JobCtx, RunOutcome, Source, poll_seats, pull_catalog,
    pull_course_detail, pull_reference_lists, pull_requirements, pull_section_listings,
    pull_section_xml, replay, urls,
};
use skyspace_parse::RefKind;
use skyspace_store::{DraftState, Store};
use sqlx::PgPool;
use time::OffsetDateTime;

const LISTING: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/listing.html");
const SEARCH_FORM: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/search-form.html");
const ASSOC: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/assoc.xml");
const DETAIL: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/detail.html");
const CATALIST: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/catalist.html");
const ENROLLMENT: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/enrollment.xml");
const REF_TERMS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ref-terms.xml");
const REF_DEPARTMENTS: &[u8] =
    include_bytes!("../../skyspace-parse/tests/fixtures/ref-departments.xml");
const REF_SCHOOLS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ref-schools.xml");
const REF_SESSIONS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ref-sessions.xml");
const REF_YEARS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ref-years.xml");
const REF_ATTRS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ref-attrs.xml");
const GA_INDEX: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ga-index.html");
const GA_BSCS: &[u8] = include_bytes!("../../skyspace-parse/tests/fixtures/ga-bscs.html");

/// Two subjects, so the listing job makes more than one page request.
const SUBJECTS: &[u8] = b"<SUBJECTS term=\"202710\" year=\"2027\">\
  <SUBJECT code=\"COMP\"><VAL>COMP</VAL><OPT>Computer Science (COMP)</OPT>Computer Science</SUBJECT>\
  <SUBJECT code=\"MUSI\"><VAL>MUSI</VAL><OPT>Music (MUSI)</OPT>Music</SUBJECT>\
</SUBJECTS>";

/// Serves bytes by URL and archives them like the real fetcher would.
struct FixtureFetch {
    pages: Mutex<BTreeMap<String, Vec<u8>>>,
    archive: Archive,
    calls: Mutex<Vec<String>>,
}

impl FixtureFetch {
    fn new(archive: Archive) -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            archive,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn put(&self, url: &url::Url, bytes: &[u8]) {
        self.pages
            .lock()
            .unwrap()
            .insert(url.to_string(), bytes.to_vec());
    }

    fn remove(&self, url: &url::Url) {
        self.pages.lock().unwrap().remove(url.as_str());
    }

    fn calls(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn lookup(&self, url: &url::Url) -> Result<Vec<u8>, FetchError> {
        self.calls.lock().unwrap().push(url.to_string());
        self.pages
            .lock()
            .unwrap()
            .get(url.as_str())
            .cloned()
            .ok_or(FetchError::Status(404))
    }

    fn outcome(bytes: Vec<u8>) -> FetchOutcome {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        FetchOutcome {
            sha256: sha256(&bytes),
            text,
            body: Bytes::from(bytes),
            status: 200,
            content_type: Some("text/html; charset=UTF-8".to_owned()),
            unchanged: false,
            fetched_at: OffsetDateTime::now_utc(),
        }
    }
}

impl Fetch for FixtureFetch {
    async fn get(&self, url: &url::Url, _source: Source) -> Result<FetchOutcome, FetchError> {
        let bytes = self.lookup(url)?;
        self.archive.put(&bytes)?;
        Ok(Self::outcome(bytes))
    }

    async fn get_live(&self, url: &url::Url) -> Result<FetchOutcome, FetchError> {
        Ok(Self::outcome(self.lookup(url)?))
    }
}

fn fall() -> TermCode {
    TermCode::parse("202710").unwrap()
}

fn subject(code: &str) -> Subject {
    Subject::new(code).unwrap()
}

fn temp_archive() -> Archive {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    Archive::new(std::env::temp_dir().join(format!("skyspace-ingest-test-{nanos}")))
}

/// A context and a fetcher that knows every reference list plus the two
/// subject listings for Fall 2026.
fn setup(pool: PgPool) -> (JobCtx, FixtureFetch) {
    let archive = temp_archive();
    let ctx = JobCtx::new(Store::from_pool(pool), archive.clone());
    let f = FixtureFetch::new(archive);
    let term = fall();
    let refs = [
        (RefKind::Terms, REF_TERMS),
        (RefKind::Subjects, SUBJECTS),
        (RefKind::Departments, REF_DEPARTMENTS),
        (RefKind::Schools, REF_SCHOOLS),
        (RefKind::Sessions, REF_SESSIONS),
        (RefKind::Years, REF_YEARS),
        (RefKind::Attrs, REF_ATTRS),
    ];
    for (kind, bytes) in refs {
        f.put(&urls::reference(kind, term).unwrap(), bytes);
    }
    f.put(&urls::listing(term, &subject("COMP")).unwrap(), LISTING);
    f.put(&urls::listing(term, &subject("MUSI")).unwrap(), LISTING);
    (ctx, f)
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn reference_pull_writes_terms(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    let summary = pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok);
    assert_eq!((summary.requests, summary.targets), (7, 7));
    assert_eq!(summary.rows_written, 3);
    assert_eq!(count(&pool, "select count(*) from terms").await, 3);
    assert_eq!(count(&pool, "select count(*) from raw_responses").await, 7);
    let last = ctx
        .store
        .last_ok_run("reference", Some(fall()))
        .await
        .unwrap();
    assert!(last.is_some());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn listing_pull_writes_rows_and_records_a_run(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    let summary = pull_section_listings(&f, &ctx, fall()).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    // TERMS, SUBJECTS, then one page per subject.
    assert_eq!((summary.requests, summary.targets), (4, 4));
    assert_eq!(summary.rows_written, 16, "8 rows on each of two pages");
    assert_eq!(count(&pool, "select count(*) from sections").await, 8);
    assert_eq!(
        count(
            &pool,
            "select count(*) from sections where withdrawn_at is not null"
        )
        .await,
        0
    );
    let run = ctx
        .store
        .last_ok_run("listings", Some(fall()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.rows_written, 16);
    assert!(
        count(
            &pool,
            "select count(*) from parse_stats where source = 'listing'"
        )
        .await
            > 0,
        "fill rates are recorded for the next run's guard"
    );
    assert!(
        ctx.archive.contains(&sha256(LISTING)),
        "the page was archived before it was parsed"
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn zero_rows_after_a_good_run_quarantines_and_writes_nothing(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    // Rice now answers COMP with the search form: the anchor is missing.
    f.put(
        &urls::listing(fall(), &subject("COMP")).unwrap(),
        SEARCH_FORM,
    );
    let before = count(&pool, "select count(*) from sections").await;
    let summary = pull_section_listings(&f, &ctx, fall()).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Quarantined, "{summary:?}");
    assert_eq!(summary.rows_written, 0);
    assert!(summary.note.unwrap().contains("COMP had"));
    assert_eq!(count(&pool, "select count(*) from sections").await, before);
    assert_eq!(
        count(
            &pool,
            "select count(*) from sections where withdrawn_at is not null"
        )
        .await,
        0,
        "a quarantined run never withdraws"
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from ingest_runs where outcome = 'quarantined'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from parse_issues where code = 'zero_rows'"
        )
        .await,
        1
    );
    assert!(
        ctx.archive.contains(&sha256(SEARCH_FORM)),
        "the odd page is on disk for a person to look at"
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn wrong_term_header_fails_the_run(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    // Summer 2026 is in TERMS, but the fixture page is headed Fall 2026.
    let summer = TermCode::parse("202630").unwrap();
    f.put(&urls::reference(RefKind::Terms, summer).unwrap(), REF_TERMS);
    f.put(
        &urls::reference(RefKind::Subjects, summer).unwrap(),
        SUBJECTS,
    );
    f.put(&urls::listing(summer, &subject("COMP")).unwrap(), LISTING);
    f.put(&urls::listing(summer, &subject("MUSI")).unwrap(), LISTING);
    let summary = pull_section_listings(&f, &ctx, summer).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Failed, "{summary:?}");
    assert!(summary.note.unwrap().contains("asked for 202630"));
    assert_eq!(count(&pool, "select count(*) from sections").await, 0);
    assert_eq!(
        count(
            &pool,
            "select count(*) from ingest_runs where outcome = 'failed'"
        )
        .await,
        1
    );
    // A term Rice does not list fails before any listing is fetched.
    let spring = TermCode::parse("202720").unwrap();
    f.put(&urls::reference(RefKind::Terms, spring).unwrap(), REF_TERMS);
    let calls = f.calls();
    let summary = pull_section_listings(&f, &ctx, spring).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Failed);
    assert!(summary.note.unwrap().contains("not in Rice's TERMS"));
    assert_eq!(f.calls(), calls + 1);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn five_failures_in_a_row_stop_the_run(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    f.remove(&urls::listing(fall(), &subject("COMP")).unwrap());
    f.remove(&urls::listing(fall(), &subject("MUSI")).unwrap());
    // Two failures are tolerated; the run still ends, quarantined by volume.
    let summary = pull_section_listings(&f, &ctx, fall()).await.unwrap();
    assert_eq!(summary.failures, 2);
    // Sections due for XML, every fetch a 404: the fifth ends the run.
    let summary = pull_section_xml(&f, &ctx, fall(), Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(summary.outcome, RunOutcome::Failed, "{summary:?}");
    assert_eq!(summary.failures, 5);
    assert!(summary.requests < summary.targets);
    assert!(summary.note.unwrap().contains("5 consecutive"));
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn section_xml_merges_the_feed_and_stamps_the_crn(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    let due = ctx
        .crns_due(
            fall(),
            skyspace_ingest::DueColumn::Xml,
            Duration::from_secs(0),
        )
        .await
        .unwrap();
    assert_eq!(due.len(), 8);
    for crn in &due {
        f.put(&urls::associated_sections(fall(), *crn).unwrap(), ASSOC);
    }
    let summary = pull_section_xml(&f, &ctx, fall(), Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    assert_eq!((summary.requests, summary.targets), (8, 8));
    let still_due = ctx
        .crns_due(
            fall(),
            skyspace_ingest::DueColumn::Xml,
            Duration::from_hours(1),
        )
        .await
        .unwrap();
    assert!(
        still_due.is_empty(),
        "the requested CRN is stamped even when the feed omits it"
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn poll_seats_exits_at_once_with_no_window(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    let summary = poll_seats(&f, &ctx, fall(), Duration::from_mins(1))
        .await
        .unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok);
    assert_eq!((summary.requests, summary.targets), (0, 0));
    assert!(summary.note.unwrap().contains("no poll window"));
    assert_eq!(f.calls(), 0);
    assert_eq!(count(&pool, "select count(*) from ingest_runs").await, 0);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn replay_reparses_an_archived_listing(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    sqlx::query("delete from meetings")
        .execute(&pool)
        .await
        .unwrap();
    let since = OffsetDateTime::now_utc().date().previous_day().unwrap();
    let summary = replay(&ctx, Source::Listing, since).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    assert_eq!((summary.requests, summary.targets), (0, 2));
    assert_eq!(summary.rows_written, 16);
    assert!(count(&pool, "select count(*) from meetings").await > 0);
    assert_eq!(f.calls(), 4 + 7, "replay makes no request");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn requirements_pull_writes_drafts_only(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    let year = CatalogYear(2026);
    f.put(&urls::program_index(year).unwrap(), GA_INDEX);
    let bscs = url::Url::parse(
        "https://ga.rice.edu/programs-study/departments-programs/engineering/computer-science/computer-science-bscs/",
    )
    .unwrap();
    f.put(&bscs, GA_BSCS);
    let only = vec!["computer-science-bscs".to_owned(), "nope".to_owned()];
    let summary = pull_requirements(&f, &ctx, year, &only).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    assert_eq!((summary.requests, summary.targets), (2, 2));
    assert_eq!(summary.rows_written, 1);
    let drafts = ctx.store.drafts(DraftState::Pending).await.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].slug, "computer-science-bscs");
    assert_eq!(drafts[0].source_sha256, sha256(GA_BSCS).to_vec());
    assert_eq!(count(&pool, "select count(*) from programs").await, 0);
    assert_eq!(
        count(
            &pool,
            "select count(*) from parse_issues where code = 'slug_not_on_index'"
        )
        .await,
        1
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn catalog_and_detail_pulls_write_records(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    let year = CatalogYear(2026);
    f.put(&urls::subjects_for_year(year).unwrap(), SUBJECTS);
    f.put(&urls::catalist(year, &subject("COMP")).unwrap(), CATALIST);
    f.put(
        &urls::catalist(year, &subject("MUSI")).unwrap(),
        SEARCH_FORM,
    );
    let summary = pull_catalog(&f, &ctx, year).await.unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    assert_eq!((summary.requests, summary.targets), (3, 3));
    assert!(summary.rows_written > 0);
    assert_eq!(
        count(
            &pool,
            "select count(*) from course_catalog where catalog_year = 2026"
        )
        .await,
        i64::try_from(summary.rows_written).unwrap()
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from parse_issues where code = 'no_rows'"
        )
        .await,
        1,
        "a subject page without records is an issue, not a failure"
    );

    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    let due = ctx
        .crns_due(
            fall(),
            skyspace_ingest::DueColumn::Detail,
            Duration::from_secs(0),
        )
        .await
        .unwrap();
    assert_eq!(due.len(), 8);
    for crn in &due {
        f.put(&urls::detail(fall(), *crn).unwrap(), DETAIL);
    }
    let summary = pull_course_detail(&f, &ctx, fall(), Duration::from_secs(0))
        .await
        .unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    assert_eq!(summary.rows_written, 8);
    assert_eq!(
        count(
            &pool,
            "select count(*) from sections where detail_fetched_at is not null"
        )
        .await,
        8
    );
    let still_due = ctx
        .crns_due(
            fall(),
            skyspace_ingest::DueColumn::Detail,
            Duration::from_hours(1),
        )
        .await
        .unwrap();
    assert!(still_due.is_empty());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn poll_seats_inside_a_window_records_readings_then_waits(pool: PgPool) {
    let (ctx, f) = setup(pool.clone());
    pull_reference_lists(&f, &ctx, fall()).await.unwrap();
    pull_section_listings(&f, &ctx, fall()).await.unwrap();
    let now = OffsetDateTime::now_utc();
    ctx.store
        .put_poll_window(
            fall(),
            "test",
            now - Duration::from_hours(1),
            now + Duration::from_hours(1),
            15,
        )
        .await
        .unwrap();
    let poll_set = ctx.store.poll_set(fall()).await.unwrap();
    assert!(!poll_set.is_empty(), "timed sections are in the poll set");
    for (_, crn) in &poll_set {
        f.put(&urls::enrollment(fall(), *crn).unwrap(), ENROLLMENT);
    }
    let summary = poll_seats(&f, &ctx, fall(), Duration::from_mins(1))
        .await
        .unwrap();
    assert_eq!(summary.outcome, RunOutcome::Ok, "{summary:?}");
    let expected = u32::try_from(poll_set.len()).unwrap();
    assert_eq!(summary.requests, expected);
    assert_eq!(summary.rows_written, u64::from(expected));
    assert_eq!(
        count(&pool, "select count(*) from section_seat_state").await,
        i64::try_from(poll_set.len()).unwrap()
    );
    assert_eq!(
        count(
            &pool,
            "select count(*) from raw_responses where source = 'listing'"
        )
        .await,
        2,
        "seat XML is never archived"
    );
    // Inside the interval the next run does nothing.
    let calls = f.calls();
    let summary = poll_seats(&f, &ctx, fall(), Duration::from_mins(1))
        .await
        .unwrap();
    assert_eq!(summary.requests, 0);
    assert!(summary.note.unwrap().contains("less than 15 minutes"));
    assert_eq!(f.calls(), calls);
}
