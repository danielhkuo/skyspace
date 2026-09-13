//! Parsers that turn one saved Rice page into `skyspace-core` types.
//!
//! Every entry point takes `&str` and returns a value plus a [`ParseReport`]:
//! the evidence that the parse was sane. Nothing here reads a file, a clock
//! or a socket, so `skyspace replay` can re-run any parser over archived
//! bytes and a test needs no network. The dependency tree is frozen in
//! `ci/parse-deps.txt`.
//!
//! Two rules hold throughout. **Regex never parses markup**: structure comes
//! from CSS selectors, text inside a cell from a regex. **A missing anchor is
//! an error**: a parser that finds none of the nodes it is built around
//! returns [`ParseError::SelectorMissing`], never an empty `Ok`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod catalog;
mod detail;
mod enrollment;
mod labels;
mod listing;
mod program;
mod reference;
mod text;
mod xml;

use scraper::Selector;
use serde::Serialize;
use skyspace_core::TermCode;

pub use catalog::parse_catalog_subject;
pub use detail::parse_section_detail;
pub use enrollment::{parse_enrollment, rfc3339_to_unix};
pub use labels::{DETAIL_LABELS, DetailField, attribute_from_text, lookup_label};
pub use listing::parse_subject_listing;
pub use program::{AreaDraft, ProgramDraft, ProgramLink, parse_program_index, parse_program_page};
pub use reference::{RefKind, ReferenceEntry, parse_reference_list};
pub use xml::{SectionXml, parse_associated_sections};

/// A parse result together with the evidence that the parse was sane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Parsed<T> {
    /// What the page said.
    pub value: T,
    /// How the parser got there.
    pub report: ParseReport,
}

/// Counts and issues from one parse. Ingest compares these against history:
/// a field whose fill rate drops is a renamed cell class, not a quiet run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ParseReport {
    /// Rows the parser looked at.
    pub rows_seen: u32,
    /// Rows that became a value.
    pub rows_kept: u32,
    /// How many kept rows filled each named field.
    pub field_filled: Vec<(&'static str, u32)>,
    /// Everything odd, with the text that caused it.
    pub issues: Vec<ParseIssue>,
}

impl ParseReport {
    /// Count one row looked at.
    pub fn saw_row(&mut self) {
        self.rows_seen = self.rows_seen.saturating_add(1);
    }

    /// Count one row kept.
    pub fn kept_row(&mut self) {
        self.rows_kept = self.rows_kept.saturating_add(1);
    }

    /// Count one filled field, by name.
    pub fn filled(&mut self, field: &'static str) {
        match self
            .field_filled
            .iter_mut()
            .find(|(name, _)| *name == field)
        {
            Some((_, count)) => *count = count.saturating_add(1),
            None => self.field_filled.push((field, 1)),
        }
    }

    /// Record an issue.
    pub fn issue(&mut self, code: IssueCode, detail: impl Into<String>) {
        self.issues.push(ParseIssue {
            code,
            detail: detail.into(),
        });
    }

    /// The fill count for a field, zero when never filled.
    #[must_use]
    pub fn fill_count(&self, field: &str) -> u32 {
        self.field_filled
            .iter()
            .find(|(name, _)| *name == field)
            .map_or(0, |(_, count)| *count)
    }

    /// True when any issue carries `code`.
    #[must_use]
    pub fn has_issue(&self, code: IssueCode) -> bool {
        self.issues.iter().any(|issue| issue.code == code)
    }
}

/// One thing the parser could not do cleanly, with the text that caused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseIssue {
    /// What kind of problem.
    pub code: IssueCode,
    /// The offending text, and where it was.
    pub detail: String,
}

/// An enum, not strings, so a new code forces a decision at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    /// A `<b>` label outside `DETAIL_LABELS`: Rice added a field.
    UnknownDetailLabel,
    /// A row without a CRN, code or another required key; the row is dropped.
    RowMissingKey,
    /// Meeting text that did not read as days and times; kept as `Unparsed`.
    UnreadableMeeting,
    /// A CourseLeaf row matching no known kind; kept in `unparsed`.
    UnclassifiedRequirementRow,
    /// Credit hours with more than two decimals; rounded to hundredths.
    SubCentCredits,
    /// A subject code that does not read as one.
    UnknownSubject,
    /// An attribute outside the closed `ATTRS` vocabulary.
    UnknownAttribute,
    /// A "Mutually Exclusive" sentence whose codes could not be read.
    UnreadableExclusion,
    /// A `Prerequisite(s):` field the grammar could not read; stored as `Unparsed`.
    UnparsedPrerequisite,
    /// A final-exam label or code outside the six Rice publishes.
    UnknownFinalExam,
}

/// Why a page could not be parsed at all. Row-level trouble is a
/// [`ParseIssue`]; this is for a page whose shape is wrong.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The anchoring selector matched nothing: Rice changed its HTML, or
    /// returned the search form. Never `Ok(vec![])`.
    #[error("selector `{selector}` matched no nodes in {document}")]
    SelectorMissing {
        /// The CSS selector, or element path, that found nothing.
        selector: &'static str,
        /// Which page kind was being read.
        document: &'static str,
    },
    /// A selector or regex in our own source is not valid.
    #[error("invalid selector `{selector}`")]
    BadSelector {
        /// The offending text.
        selector: &'static str,
    },
    /// The page header names a different term: Rice fell back to the
    /// current term for a code it does not know.
    #[error("asked for {wanted}, page is {got:?}")]
    WrongTerm {
        /// The term the caller asked for.
        wanted: TermCode,
        /// What the page said.
        got: String,
    },
    /// The XML did not deserialise.
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::DeError),
    /// Rice's `time-now` was not `YYYY-MM-DDTHH:MM:SS±HH:MM`.
    #[error("timestamp `{0}` is not RFC 3339")]
    Timestamp(String),
}

/// Build a selector without `unwrap`: the library error borrows the input,
/// so it is discarded and the selector text carried instead.
///
/// # Errors
/// [`ParseError::BadSelector`] when `css` is not valid CSS.
pub fn sel(css: &'static str) -> Result<Selector, ParseError> {
    Selector::parse(css).map_err(|_| ParseError::BadSelector { selector: css })
}
