//! Every SQL statement in Skyspace, in one crate, because `skyspace-ingest`
//! and `skyspace-api` touch the same tables.
//!
//! Rows are column-shaped structs converted into `skyspace-core` types with
//! `TryFrom`; a core type never derives `sqlx::FromRow`. Queries use
//! `sqlx::query_as` with plain SQL, not the `query!` macro, so a build needs
//! no live database; the price is that a column typo fails at run time,
//! which is why every query has a test against a throwaway database.
//!
//! Every per-account method takes the `AccountId` first and puts it in the
//! `WHERE` clause, so a row another account owns reads as missing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod accounts;
mod catalog;
mod collections;
mod convert;
mod courses;
mod error;
mod feedback;
mod ingest;
mod plans;
mod pool;
mod programs;
mod schedules;
mod seats;
mod sections;
mod sessions;

#[cfg(test)]
mod testing;

pub use accounts::AccountRow;
pub use catalog::{
    JobRow, MetaRows, PartOfTermRow, SectionPageRows, SectionQuery, SectionSort, SubjectRow,
    TermRow,
};
pub use collections::{CollectionRow, GuestCollectionInput};
pub use convert::sha256;
pub use error::StoreError;
pub use ingest::{
    CanaryRow, IssueSeverity, JobLock, RawResponseInput, RawSource, RunOutcome, RunRow,
    RunSummaryRow, SectionXmlRow, TermInput,
};
pub use plans::{DeleteOutcome, PlanSummaryRow, default_limits};
pub use pool::{MIGRATOR, Page, Store};
pub use programs::{
    DraftId, DraftInput, DraftRow, DraftState, ProgramSummaryRow, VersionSource, review_now,
};
pub use schedules::{ClaimedRow, GuestScheduleInput, ScheduleSummaryRow};
pub use seats::PollWindowRow;
pub use sections::SectionRow;
pub use sessions::{
    ADDRESS_WINDOW, MAX_ATTEMPTS, PutCodeOutcome, SENDS_PER_ADDRESS, SessionRow, TakeCodeOutcome,
};
