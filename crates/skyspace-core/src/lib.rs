//! Pure degree-requirement and scheduling engine for Skyspace.
//!
//! This crate performs no input or output. It takes a plan, the programs it
//! claims and the facts about the courses involved, and returns a report.
//! That makes it testable, and it lets the same code run on the server and in
//! the browser through WebAssembly, so the two cannot disagree about a
//! progress total.
//!
//! Nothing here may block, read a file, read a clock, draw a random number or
//! open a socket. The dependency tree is frozen in `ci/core-deps.txt`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![allow(clippy::module_name_repetitions)]

pub mod catalog;
pub mod code;
pub mod evaluate;
pub mod plan;
pub mod prereq;
pub mod program;
pub mod schedule;
pub mod term;
pub mod warn;

use serde::{Deserialize, Serialize};

pub use code::{CodeError, CourseCode, CourseNumber, Crn, SectionNumber, Subject};
pub use evaluate::{
    CourseFacts, CourseInfo, Outcome, PlanBundle, ProgramReport, Progress, Report,
    RequirementReport, evaluate, evaluate_program,
};
pub use plan::{
    AccountId, BusyId, CollectionId, EntryId, Plan, PlanId, PlanTerm, PlannedCourse, ScheduleId,
    TermId, TermKind,
};
pub use program::{CatalogYear, Program, ProgramId, Requirement, RequirementId};
pub use schedule::{Conflict, ScheduledMeeting, find_conflicts};
pub use term::{Credits, Season, TermCode, TermPosition};
pub use warn::{PlacementPreview, Warning, preview_placement, warnings};

/// Unix seconds. Core carries a time; it cannot read one. The caller passes
/// the current term in on every call. The `ts` attribute stops ts-rs writing
/// `bigint`, which would break `JSON.stringify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, type = "number"))]
pub struct Timestamp(pub i64);

/// The browser compares this with the server's on start-up, so a cached wasm
/// bundle cannot show a different progress total from the one in the PDF.
/// Bump the crate version when the engine's output changes, and only then.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
