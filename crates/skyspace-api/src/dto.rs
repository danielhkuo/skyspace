//! Wire types shaped by HTTP. Identifiers and domain types come from
//! `skyspace-core` and cross the wire unchanged; this module defines only
//! what the transport adds: envelopes, freshness, pages, query strings.
//!
//! Every type derives `TS` and `#[serde(rename_all = "camelCase")]`. Request
//! bodies add `deny_unknown_fields`; query strings do not, because a shared
//! URL may carry tracking parameters. Every `OffsetDateTime` and `Uuid` field
//! carries `ts(type = "string")`: API timestamps are RFC 3339, core's
//! `Timestamp` is Unix seconds, and the two never meet.

use serde::{Deserialize, Serialize};
use skyspace_core::catalog::{Attribute, Course, Seats, Section};
use skyspace_core::code::{CourseCode, Crn, Subject};
use skyspace_core::plan::{Plan, PlanId, ScheduleId};
use skyspace_core::program::{CatalogYear, ProgramId, ProgramKind, RequirementId};
use skyspace_core::schedule::TermSchedule;
use skyspace_core::term::{Credits, PartOfTermCode, Season, TermCode};
use time::OffsetDateTime;
use uuid::Uuid;

/// A catalog response with the age of the data behind it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Fresh<T> {
    /// The payload.
    pub data: T,
    /// When the pull behind it last ran.
    pub freshness: Freshness,
}

/// When the data was pulled and whether that is later than its own schedule allows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Freshness {
    /// Which job produced the data.
    pub source: DataSource,
    /// Rice's own `time-now` for seats; `None` for listings.
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub rice_as_of: Option<OffsetDateTime>,
    /// `ingest_runs.finished_at` of the last good run.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub pulled_at: OffsetDateTime,
    /// Past that job's own staleness threshold.
    pub stale: bool,
}

/// Which pull a freshness stamp refers to. Not `skyspace_ingest::Source`,
/// which labels an archived document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum DataSource {
    /// The nightly subject listing.
    SectionListing,
    /// The weekly detail page.
    SectionDetail,
    /// The live enrollment feed.
    Seats,
    /// The weekly reference lists.
    Reference,
    /// The General Announcements.
    GeneralAnnouncements,
}

/// `GET /health`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HealthBody {
    /// Always `true` when the process answers.
    pub ok: bool,
    /// Whether the database answered a trivial query.
    pub database: bool,
    /// `ENGINE_VERSION`.
    pub engine_version: String,
}

/// `GET /api/v1/meta`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MetaBody {
    /// `terms.is_current`; `None` until a person sets it.
    pub current_term: Option<TermCode>,
    /// Every term Rice publishes that we hold.
    pub terms: Vec<TermSummary>,
    /// Age of the last good run of every job.
    pub jobs: Vec<JobFreshness>,
    /// `skyspace_core::ENGINE_VERSION`; the browser compares its wasm build.
    pub engine_version: String,
}

/// One term as `TERMS` publishes it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TermSummary {
    /// Rice's code.
    pub code: TermCode,
    /// Rice's own label, "Fall Semester 2026", never synthesised.
    pub label: String,
    /// `None` for a quadmester.
    pub season: Option<Season>,
    /// The one current term.
    pub is_current: bool,
}

/// How old one job's data is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct JobFreshness {
    /// The job name as `ingest_runs.job` records it.
    pub job: String,
    /// The last successful finish, if any.
    #[serde(with = "time::serde::rfc3339::option")]
    #[cfg_attr(feature = "ts", ts(type = "string | null"))]
    pub last_ok: Option<OffsetDateTime>,
    /// Past the job's threshold, or never run.
    pub stale: bool,
}

/// `GET /api/v1/reference?term=`: the lists the catalog rail offers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReferenceBody {
    /// Subject codes with sections this term.
    pub subjects: Vec<ReferenceEntry>,
    /// Departments.
    pub departments: Vec<ReferenceEntry>,
    /// Schools.
    pub schools: Vec<ReferenceEntry>,
    /// Parts of term for this term.
    pub parts_of_term: Vec<ReferenceEntry>,
    /// The four attributes.
    pub attributes: Vec<ReferenceEntry>,
}

/// One code and its label.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ReferenceEntry {
    /// Rice's code.
    pub code: String,
    /// Rice's label.
    pub label: String,
}

/// `GET /api/v1/courses/{subject}/{number}?term=`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CourseView {
    /// The catalog record for the term's academic year.
    pub course: Course,
    /// Every section this term; `detail: None` = not loaded yet, not "none".
    pub sections: Vec<Section>,
    /// Outbound links.
    pub links: ClassLinks,
    /// Whether any non-withdrawn section exists this term.
    pub offered: bool,
}

/// Where the class page links out to.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClassLinks {
    /// The `courses.rice.edu` course page.
    pub rice_course_page: String,
    /// The syllabus, behind NetID.
    pub esther_syllabus: Option<String>,
    /// Always `None` today: no evaluations deep link is confirmed to exist.
    pub esther_evaluations: Option<String>,
}

/// One row of `GET /api/v1/seats`. `seats: None` = outside the poll set; the
/// interface renders "not polled", never zero seats.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SeatRow {
    /// The CRN.
    pub crn: Crn,
    /// Live counts, when polled.
    pub seats: Option<Seats>,
}

/// One program in `GET /api/v1/programs`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProgramSummary {
    /// Stable across catalog years.
    pub id: ProgramId,
    /// GA slug.
    pub slug: String,
    /// What sort of program.
    pub kind: ProgramKind,
    /// Display name.
    pub name: String,
    /// `BSCS`, `BMus`; empty for the university requirements.
    pub credential: String,
    /// The catalog years with a published version.
    pub catalog_years: Vec<CatalogYear>,
    /// The declared total, when the page prints one.
    pub total_credits: Option<Credits>,
}

/// `POST /api/v1/reports/rule`: a student's complaint about an encoded rule.
/// Not `skyspace_core::RequirementReport`, which is a rule's outcome.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RequirementErrorReport {
    /// The rule, when the report is about one.
    pub requirement: Option<RequirementId>,
    /// The program.
    pub program: Option<ProgramId>,
    /// The catalog year.
    pub catalog_year: Option<CatalogYear>,
    /// What is wrong, in the student's words.
    pub message: String,
}

/// One schedule in `GET /api/v1/schedules?term=`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScheduleSummary {
    /// The id.
    pub id: ScheduleId,
    /// Display name.
    pub name: String,
    /// Which term.
    pub term: TermCode,
    /// Write version.
    pub version: i32,
    /// Last write.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: OffsetDateTime,
}

/// A schedule with its write version. A stale version on `PUT` is `409 stale_version`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScheduleEnvelope {
    /// The id.
    pub id: ScheduleId,
    /// Increments on every accepted write.
    pub version: i32,
    /// Last write.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: OffsetDateTime,
    /// The document.
    pub schedule: TermSchedule,
}

/// `PUT /api/v1/schedules/{id}` and `POST /api/v1/schedules` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScheduleWrite {
    /// The version the client last saw; ignored on create.
    pub version: Option<i32>,
    /// The document.
    pub schedule: TermSchedule,
}

/// One plan in `GET /api/v1/plans`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanSummary {
    /// The id.
    pub id: PlanId,
    /// Display name.
    pub name: String,
    /// The catalog year the plan follows.
    pub catalog_year: CatalogYear,
    /// The one active plan per account.
    pub is_active: bool,
    /// Write version.
    pub version: i32,
    /// Last write.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: OffsetDateTime,
}

/// A plan with its write version. The wire type is the core type: two plan
/// shapes would let the browser's evaluation and the server's differ.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanEnvelope {
    /// Increments on every accepted write.
    pub version: i32,
    /// Last write.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub updated_at: OffsetDateTime,
    /// The document.
    pub plan: Plan,
}

/// `PUT /api/v1/plans/{id}` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanWrite {
    /// The version the client last saw.
    pub version: i32,
    /// The document.
    pub plan: Plan,
}

/// `POST /api/v1/plans` body: the store mints the id.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanCreate {
    /// The document; its `id` is replaced by the minted one.
    pub plan: Plan,
}

/// `POST /api/v1/plans/{id}/duplicate` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlanDuplicate {
    /// The copy's name.
    pub name: String,
}

/// A saved-course collection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Collection {
    /// The id.
    pub id: skyspace_core::plan::CollectionId,
    /// Display name, unique per account (case-insensitive).
    pub name: String,
    /// Course codes, in the order they were saved.
    pub courses: Vec<CourseCode>,
}

/// `POST /api/v1/collections` and `PATCH /api/v1/collections/{id}` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CollectionWrite {
    /// The name.
    pub name: String,
}

/// `GET /api/v1/auth/methods`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AuthMethods {
    /// Whether the proxy-terminated single sign-on is configured.
    pub sso: bool,
    /// Whether the emailed code is available.
    pub email_code: bool,
    /// The email domains that may sign in.
    pub allowed_email_domains: Vec<String>,
}

/// `POST /api/v1/auth/email/request` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EmailRequest {
    /// The address; must be on an allowed domain.
    pub email: String,
}

/// `POST /api/v1/auth/email/verify` body.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EmailVerify {
    /// The address.
    pub email: String,
    /// The six-digit code.
    pub code: String,
}

/// `GET /api/v1/account`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountView {
    /// The account id.
    pub id: skyspace_core::plan::AccountId,
    /// The verified address.
    pub email: String,
    /// When the account was created.
    #[serde(with = "time::serde::rfc3339")]
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub created_at: OffsetDateTime,
}

/// `PATCH /api/v1/account` body. Nothing is editable at launch except a
/// display name, so the body is a placeholder that rejects unknown fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountPatch {
    /// Reserved; ignored.
    pub display_name: Option<String>,
}

/// `GET /api/v1/sections` query string. Field for field the wire type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CatalogQuery {
    /// Defaults to `terms.is_current`.
    pub term: Option<TermCode>,
    /// Keyword: code, title, instructor.
    pub q: Option<String>,
    /// Subject codes; FWIS and LPAP are subjects, not attributes.
    pub subject: Vec<Subject>,
    /// Departments; detail-only column.
    pub department: Vec<String>,
    /// Schools; detail-only column.
    pub school: Vec<String>,
    /// AD, GRP1, GRP2, GRP3.
    pub attr: Vec<Attribute>,
    /// `CourseNumber::level` values: 100, 200, ...
    pub level: Vec<u16>,
    /// Lowest number, inclusive; for a rule's `NumberRange` selector.
    pub level_min: Option<u16>,
    /// Highest number, inclusive.
    pub level_max: Option<u16>,
    /// Hundredths, inclusive.
    pub credits_min: Option<u16>,
    /// Hundredths, inclusive.
    pub credits_max: Option<u16>,
    /// Meeting days.
    pub days: Vec<Day>,
    /// Minutes from midnight.
    pub starts_after: Option<u16>,
    /// Minutes from midnight.
    pub ends_before: Option<u16>,
    /// `cls-ses` labels.
    pub part_of_term: Vec<PartOfTermCode>,
    /// Outside the poll set is excluded, not treated as full.
    pub open_seats_only: bool,
    /// On by default: 58% of rows have no meeting time.
    pub scheduled_only: bool,
    /// Sort key.
    pub sort: SortKey,
    /// Page offset, capped at 10,000.
    pub offset: u32,
    /// Page size, clamped to 100.
    pub limit: u16,
}

impl Default for CatalogQuery {
    fn default() -> Self {
        Self {
            term: None,
            q: None,
            subject: vec![],
            department: vec![],
            school: vec![],
            attr: vec![],
            level: vec![],
            level_min: None,
            level_max: None,
            credits_min: None,
            credits_max: None,
            days: vec![],
            starts_after: None,
            ends_before: None,
            part_of_term: vec![],
            open_seats_only: false,
            scheduled_only: true,
            sort: SortKey::Relevance,
            offset: 0,
            limit: 25,
        }
    }
}

/// A weekday in a URL. `DaySet` is a bit set, unreadable in a query string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Day {
    /// Monday.
    Mon,
    /// Tuesday.
    Tue,
    /// Wednesday.
    Wed,
    /// Thursday.
    Thu,
    /// Friday.
    Fri,
    /// Saturday.
    Sat,
    /// Sunday.
    Sun,
}

impl Day {
    /// The bit in `DaySet`.
    #[must_use]
    pub const fn bit(self) -> u8 {
        match self {
            Self::Mon => 1 << 0,
            Self::Tue => 1 << 1,
            Self::Wed => 1 << 2,
            Self::Thu => 1 << 3,
            Self::Fri => 1 << 4,
            Self::Sat => 1 << 5,
            Self::Sun => 1 << 6,
        }
    }
}

/// How to order a search. No grade sort: Rice publishes no grades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SortKey {
    /// Exact code, then code prefix, then title rank.
    Relevance,
    /// Subject then number.
    CourseNumber,
    /// Minimum credit hours.
    Credits,
    /// Most open seats first.
    OpenSeats,
}

/// One page of sections.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SectionPage {
    /// The rows.
    pub rows: Vec<Section>,
    /// Rows matching the query, across every page.
    pub total: u32,
    /// The offset served.
    pub offset: u32,
    /// The limit served.
    pub limit: u16,
    /// Whether another page exists.
    pub has_more: bool,
    /// Rows `scheduledOnly` removed, so nothing vanishes silently.
    pub unscheduled_hidden: u32,
    /// Distinct courses across the whole match, not the page.
    pub course_count: u32,
    /// The query after validation; this is what puts filter state in the URL.
    pub applied: CatalogQuery,
    /// Populated only when `total` is zero.
    pub suggestions: Vec<DropFilter>,
}

/// "Dropping this filter gives N results."
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DropFilter {
    /// The query field.
    pub field: String,
    /// How many rows would match without it.
    pub would_match: u32,
}

/// `POST /api/v1/account/claim` body. `client_id` is minted at creation, not at claim.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClaimRequest {
    /// Guest schedules.
    pub schedules: Vec<GuestSchedule>,
    /// Guest collections.
    pub collections: Vec<GuestCollection>,
}

/// A schedule built as a guest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GuestSchedule {
    /// The browser's id for the document.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub client_id: Uuid,
    /// The document.
    pub schedule: TermSchedule,
}

/// A collection built as a guest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GuestCollection {
    /// The browser's id for the document.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub client_id: Uuid,
    /// The name.
    pub name: String,
    /// The saved codes.
    pub courses: Vec<CourseCode>,
}

/// What the claim did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClaimResult {
    /// Schedules now owned by the account.
    pub schedules: Vec<ClaimedId>,
    /// Collections now owned by the account.
    pub collections: Vec<ClaimedId>,
    /// Documents that did not land, with the reason.
    pub skipped: Vec<ClaimSkip>,
}

/// A claimed document's server id.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClaimedId {
    /// The browser's id.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub client_id: Uuid,
    /// The server's id.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub id: Uuid,
    /// The name it was given when the original collided.
    pub renamed_to: Option<String>,
}

/// A document the claim skipped.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClaimSkip {
    /// The browser's id.
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub client_id: Uuid,
    /// Why.
    pub reason: ClaimSkipReason,
}

/// Why a claim skipped a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ClaimSkipReason {
    /// Already claimed by another account.
    AlreadyClaimed,
    /// Over the per-document item limit.
    TooManyItems,
    /// The schedule's term is not in the database.
    TermNotLoaded,
}

/// `POST /api/v1/events` body: anonymous counters only.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EventBody {
    /// A name from the closed list.
    pub name: EventName,
    /// The term in view, if any.
    pub term: Option<TermCode>,
    /// Whether a search came back empty.
    pub empty: Option<bool>,
}

/// The closed list of usage events. Unknown names are rejected by serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum EventName {
    /// A catalog search ran.
    CatalogSearch,
    /// A class page opened.
    ClassView,
    /// A schedule was saved.
    ScheduleSave,
    /// A plan was saved.
    PlanSave,
    /// A plan was exported to PDF.
    PlanExport,
    /// A program was added to a plan.
    ProgramAdd,
}

/// The one error body every non-2xx response carries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ErrorBody {
    /// The client switches on this, never on `message`.
    pub code: ErrorCode,
    /// One sentence; never a database message.
    pub message: String,
    /// The `x-request-id`; a bug report quotes it.
    pub request_id: String,
    /// `429` only.
    pub retry_after_seconds: Option<u32>,
}

/// Every error code the API returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ErrorCode {
    /// 400.
    InvalidRequest,
    /// 401.
    Unauthenticated,
    /// 404.
    NotFound,
    /// 409: another tab wrote first.
    StaleVersion,
    /// 409: cannot delete the last plan.
    LastPlan,
    /// 409: a collection with that name exists.
    DuplicateName,
    /// 408, from the timeout layer.
    Timeout,
    /// 413, from the body-limit layer.
    PayloadTooLarge,
    /// 429.
    RateLimited,
    /// 500.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_query_defaults_hide_unscheduled_rows() {
        let query = CatalogQuery::default();
        assert!(query.scheduled_only);
        assert_eq!(query.limit, 25);
        assert_eq!(query.sort, SortKey::Relevance);
        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("\"scheduledOnly\":true"));
    }

    #[test]
    fn day_bits_match_day_set() {
        use skyspace_core::catalog::DaySet;
        assert_eq!(Day::Thu.bit(), DaySet::THURSDAY.bits());
        assert_eq!(Day::Sun.bit(), DaySet::SUNDAY.bits());
    }

    #[test]
    fn error_codes_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::StaleVersion).unwrap(),
            "\"stale_version\""
        );
    }
}
