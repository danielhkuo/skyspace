//! Fetch Rice pages politely, archive the raw bytes, then parse and store.
//!
//! Path: `courses.rice.edu` / `ga.rice.edu` -> [`fetch`] (decode with
//! `encoding_rs`) -> [`archive`] (bytes + sha256) -> `skyspace-parse` ->
//! `skyspace-store`. **Archive first:** every job writes the raw response
//! before it parses, because Rice publishes no history, so a parser bug
//! must be a replay and not lost data.
//!
//! Every job takes an advisory lock, opens an `ingest_runs` row, counts
//! every request and failure, stops after five failures in a row, runs the
//! guards in [`guards`], and closes the row with an outcome the CLI maps
//! to an exit code.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod archive;
pub mod ctx;
pub mod error;
pub mod fetch;
pub mod guards;
pub mod jobs;
pub mod replay;
pub mod urls;

pub use archive::Archive;
pub use ctx::{DueColumn, JobCtx, RawRow};
pub use error::{FetchError, JobError};
pub use fetch::{
    COURSES_BASE, Fetch, FetchOutcome, Fetcher, GA_BASE, HostLimiter, MAX_BODY_BYTES, Source,
};
pub use jobs::{
    RunOutcome, Summary, exit_code, job_names, poll_seats, pull_catalog, pull_course_detail,
    pull_reference_lists, pull_requirements, pull_section_listings, pull_section_xml,
};
pub use replay::replay;
