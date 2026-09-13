//! Prerequisites and mutual exclusion: the published observation, the folded
//! per-course fact, and the index a plan is checked against.
//!
//! Whatever checks a `PrereqExpr` is three-valued Kleene logic: `Unparsed`
//! is unknown and an unknown clause never resolves to satisfied.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::code::CourseCode;
use crate::evaluate::CourseFacts;
use crate::plan::{EntryId, Plan, TermId, TermKind};
use crate::program::CatalogYear;
use crate::term::{Taken, TermPosition};

/// A prerequisite expression as Rice prints it: `AND`/`OR` over codes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PrereqExpr {
    /// One course.
    Course(CourseCode),
    /// Every child.
    All(Vec<PrereqExpr>),
    /// At least one child.
    Any(Vec<PrereqExpr>),
    /// Rice's text, verbatim, when the grammar could not be read safely.
    Unparsed(String),
}

impl PrereqExpr {
    /// Recursive descent over Rice's grammar. Fully parenthesised or
    /// single-operator text parses. A mixed `AND`/`OR` expression without
    /// outer parentheses (CHEM 420) is returned as `Unparsed`, because Rice's
    /// own precedence is unknown and a wrong reading says "met" when it is
    /// not. Total; never panics.
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let tokens = tokenize(raw);
        let mut parser = Parser {
            tokens: &tokens,
            at: 0,
        };
        match parser.expression() {
            Some(expr) if parser.at == tokens.len() => expr,
            _ => Self::Unparsed(raw.trim().to_owned()),
        }
    }

    /// Every code named anywhere in the expression, in print order.
    #[must_use]
    pub fn codes(&self) -> Vec<CourseCode> {
        let mut out = Vec::new();
        self.collect_codes(&mut out);
        out
    }

    fn collect_codes(&self, out: &mut Vec<CourseCode>) {
        match self {
            Self::Course(code) => out.push(code.clone()),
            Self::All(children) | Self::Any(children) => {
                for child in children {
                    child.collect_codes(out);
                }
            }
            Self::Unparsed(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Code(CourseCode),
    And,
    Or,
    Open,
    Close,
    Junk,
}

fn tokenize(raw: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut words = raw.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let mut word = word;
        while let Some(rest) = word.strip_prefix('(') {
            tokens.push(Token::Open);
            word = rest;
        }
        let mut closes = 0;
        while let Some(rest) = word.strip_suffix(')') {
            closes += 1;
            word = rest;
        }
        if !word.is_empty() {
            let token = match word.to_ascii_uppercase().as_str() {
                "AND" => Token::And,
                "OR" => Token::Or,
                _ => match words.peek() {
                    Some(next) if word.chars().all(|c| c.is_ascii_alphabetic()) => {
                        let mut number = *next;
                        let mut next_closes = 0;
                        while let Some(rest) = number.strip_suffix(')') {
                            next_closes += 1;
                            number = rest;
                        }
                        match CourseCode::new(word, number) {
                            Ok(code) => {
                                words.next();
                                closes += next_closes;
                                Token::Code(code)
                            }
                            Err(_) => Token::Junk,
                        }
                    }
                    _ => CourseCode::parse(word).map_or(Token::Junk, Token::Code),
                },
            };
            tokens.push(token);
        }
        for _ in 0..closes {
            tokens.push(Token::Close);
        }
    }
    tokens
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    /// `term (op term)*` with one operator per level; mixing is a failure.
    fn expression(&mut self) -> Option<PrereqExpr> {
        let first = self.term()?;
        let mut operator: Option<Token> = None;
        let mut children = vec![first];
        while let Some(token) = self.peek() {
            let op = match token {
                Token::And | Token::Or => token.clone(),
                _ => break,
            };
            match &operator {
                None => operator = Some(op),
                Some(seen) if *seen != op => return None,
                Some(_) => {}
            }
            self.at += 1;
            children.push(self.term()?);
        }
        Some(match operator {
            None => children.pop()?,
            Some(Token::And) => PrereqExpr::All(children),
            Some(_) => PrereqExpr::Any(children),
        })
    }

    fn term(&mut self) -> Option<PrereqExpr> {
        match self.peek()? {
            Token::Code(code) => {
                let code = code.clone();
                self.at += 1;
                Some(PrereqExpr::Course(code))
            }
            Token::Open => {
                self.at += 1;
                let inner = self.expression()?;
                match self.peek() {
                    Some(Token::Close) => {
                        self.at += 1;
                        Some(inner)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// What Rice printed for one course in one catalog year. Provenance is part
/// of the type: prerequisites change between years.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrereqObservation {
    /// The course the field belongs to.
    pub course: CourseCode,
    /// Rice's text, always kept.
    pub raw: String,
    /// A parser failure is `Unparsed`, not an error and not an `Option`.
    pub parsed: PrereqExpr,
    /// The separate `Corequisite:` label, one bare code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corequisite: Option<CourseCode>,
    /// The edition the field was read from.
    pub catalog_year: CatalogYear,
}

/// A "Mutually Exclusive" sentence from a description. Rice slash-joins a
/// list: one sentence, many codes. An empty `with` means the sentence could
/// not be read and goes to review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MutualExclusion {
    /// Every code in the sentence.
    pub with: Vec<CourseCode>,
    /// Rice's sentence.
    pub raw: String,
}

/// The folded fact for one course. `Unknown` and `NoneRequired` are
/// different variants so "never seen" cannot be read as "no prerequisite".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PrereqFact {
    /// No catalog record for this course; build no warning.
    Unknown,
    /// Rice printed no field.
    NoneRequired {
        /// The edition that printed no field.
        published_for: CatalogYear,
    },
    /// Rice printed an expression.
    Requires {
        /// The edition the expression is from.
        published_for: CatalogYear,
        /// The parsed expression.
        expr: PrereqExpr,
        /// Rice's text.
        published: String,
        /// The `Corequisite:` code, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        corequisite: Option<CourseCode>,
    },
}

/// The row crossing the wasm boundary, as `PlanBundle::prerequisites`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Prerequisite {
    /// The course.
    pub course: CourseCode,
    /// What we know.
    pub fact: PrereqFact,
}

/// One folded exclusion, deduplicated by `(blocked, blocker)` with the
/// newest catalog year winning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Exclusion {
    /// The course that cannot be taken.
    pub blocked: CourseCode,
    /// The course that blocks it.
    pub blocker: CourseCode,
    /// The edition the sentence is from.
    pub published_for: CatalogYear,
    /// Rice's sentence, always shown; extracted prose is never paraphrased.
    pub published: String,
}

/// Pure and order-independent. Per course keep the newest catalog year's
/// observation. Rows come back sorted by course.
#[must_use]
pub fn fold_prerequisites(observations: &[PrereqObservation]) -> Vec<Prerequisite> {
    let mut newest: BTreeMap<CourseCode, &PrereqObservation> = BTreeMap::new();
    for observation in observations {
        let replace = newest
            .get(&observation.course)
            .is_none_or(|held| held.catalog_year < observation.catalog_year);
        if replace {
            newest.insert(observation.course.clone(), observation);
        }
    }
    newest
        .into_values()
        .map(|observation| Prerequisite {
            course: observation.course.clone(),
            fact: if observation.raw.trim().is_empty() {
                PrereqFact::NoneRequired {
                    published_for: observation.catalog_year,
                }
            } else {
                PrereqFact::Requires {
                    published_for: observation.catalog_year,
                    expr: observation.parsed.clone(),
                    published: observation.raw.clone(),
                    corequisite: observation.corequisite.clone(),
                }
            },
        })
        .collect()
}

static UNKNOWN: PrereqFact = PrereqFact::Unknown;

/// Linear scan of a few hundred plan rows. Returns `Unknown`, not `Option`,
/// so a caller cannot collapse "never seen" into "none required".
#[must_use]
pub fn prereq_fact<'a>(rows: &'a [Prerequisite], course: &CourseCode) -> &'a PrereqFact {
    rows.iter()
        .find(|row| row.course == *course)
        .map_or(&UNKNOWN, |row| &row.fact)
}

/// Where a course sits on the board. `term` is `None` for incoming credit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Placed {
    /// When.
    pub when: Taken,
    /// Which column, or `None` for incoming credit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<TermId>,
    /// Which card.
    pub entry: EntryId,
}

/// Earliest appearance of each course, built once per check. Manual cards
/// enter only with a `rice_equivalent`; codes are canonicalised, so a
/// cross-list is found under either code.
#[derive(Debug, Clone, Default)]
pub struct TakenIndex {
    earliest: BTreeMap<CourseCode, Placed>,
}

impl TakenIndex {
    /// Index every card on the board.
    #[must_use]
    pub fn build(plan: &Plan, facts: &CourseFacts) -> Self {
        Self::build_without(plan, facts, None)
    }

    /// Index every card except `skip`, for the drag preview.
    #[must_use]
    pub fn build_without(plan: &Plan, facts: &CourseFacts, skip: Option<EntryId>) -> Self {
        let mut index = Self::default();
        for card in &plan.incoming_credit {
            if let Some(code) = &card.rice_equivalent
                && Some(card.id) != skip
            {
                index.record(
                    facts.canonical(code),
                    Placed {
                        when: Taken::Incoming,
                        term: None,
                        entry: card.id,
                    },
                );
            }
        }
        for term in &plan.terms {
            let when = Taken::Term(term.position);
            match &term.kind {
                TermKind::Rice { courses, .. } => {
                    for course in courses {
                        if Some(course.id) != skip {
                            index.record(
                                facts.canonical(&course.course),
                                Placed {
                                    when,
                                    term: Some(term.id),
                                    entry: course.id,
                                },
                            );
                        }
                    }
                }
                TermKind::Away { cards } => {
                    for card in cards {
                        if let Some(code) = &card.rice_equivalent
                            && Some(card.id) != skip
                        {
                            index.record(
                                facts.canonical(code),
                                Placed {
                                    when,
                                    term: Some(term.id),
                                    entry: card.id,
                                },
                            );
                        }
                    }
                }
                TermKind::Off => {}
            }
        }
        index
    }

    fn record(&mut self, code: CourseCode, placed: Placed) {
        let earlier = self
            .earliest
            .get(&code)
            .is_none_or(|held| placed.when < held.when);
        if earlier {
            self.earliest.insert(code, placed);
        }
    }

    /// The earliest placement of a course already canonicalised.
    #[must_use]
    pub fn earliest(&self, course: &CourseCode) -> Option<Placed> {
        self.earliest.get(course).copied()
    }
}

/// Three-valued truth for a prerequisite expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Truth {
    /// Every named course is in place.
    #[default]
    Satisfied,
    /// A named course is absent, in the same term, or later.
    Missing,
    /// An `Unparsed` clause decides.
    Unknown,
}

impl Truth {
    const fn all(self, other: Self) -> Self {
        match (self, other) {
            (Self::Missing, _) | (_, Self::Missing) => Self::Missing,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Satisfied, Self::Satisfied) => Self::Satisfied,
        }
    }

    const fn any(self, other: Self) -> Self {
        match (self, other) {
            (Self::Satisfied, _) | (_, Self::Satisfied) => Self::Satisfied,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Missing, Self::Missing) => Self::Missing,
        }
    }
}

/// The outcome of checking one expression against one target term.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrereqResult {
    /// The Kleene verdict.
    pub truth: Truth,
    /// Named courses absent from the plan.
    pub missing: Vec<CourseCode>,
    /// Named courses placed in the target term itself.
    pub same_term: Vec<CourseCode>,
    /// Named courses placed later than the target term, with their column.
    pub later: Vec<(CourseCode, TermId)>,
}

/// Evaluate `expr` for a course placed in `target`, against the index.
#[must_use]
pub fn evaluate_prereq(
    expr: &PrereqExpr,
    target: TermPosition,
    index: &TakenIndex,
    facts: &CourseFacts,
) -> PrereqResult {
    match expr {
        PrereqExpr::Unparsed(_) => PrereqResult {
            truth: Truth::Unknown,
            ..PrereqResult::default()
        },
        PrereqExpr::Course(code) => course_result(code, target, index, facts),
        PrereqExpr::All(children) => {
            let mut out = PrereqResult::default();
            for child in children {
                let r = evaluate_prereq(child, target, index, facts);
                out.truth = out.truth.all(r.truth);
                out.missing.extend(r.missing);
                out.same_term.extend(r.same_term);
                out.later.extend(r.later);
            }
            out
        }
        PrereqExpr::Any(children) => {
            let results: Vec<PrereqResult> = children
                .iter()
                .map(|child| evaluate_prereq(child, target, index, facts))
                .collect();
            let truth = results
                .iter()
                .fold(Truth::Missing, |acc, r| acc.any(r.truth));
            if truth == Truth::Satisfied {
                return PrereqResult::default();
            }
            // Nothing in the branch is met. One warning, not one per option:
            // the nearest miss speaks for the group (same term, then later,
            // then absent), because a warning names a single course.
            let first = results
                .iter()
                .find(|r| !r.same_term.is_empty())
                .or_else(|| results.iter().find(|r| !r.later.is_empty()))
                .or_else(|| results.iter().find(|r| !r.missing.is_empty()));
            match first {
                Some(r) => PrereqResult { truth, ..r.clone() },
                None => PrereqResult {
                    truth,
                    ..PrereqResult::default()
                },
            }
        }
    }
}

fn course_result(
    code: &CourseCode,
    target: TermPosition,
    index: &TakenIndex,
    facts: &CourseFacts,
) -> PrereqResult {
    let canonical = facts.canonical(code);
    let Some(placed) = index.earliest(&canonical) else {
        return PrereqResult {
            truth: Truth::Missing,
            missing: vec![code.clone()],
            ..PrereqResult::default()
        };
    };
    match placed.when {
        Taken::Incoming => PrereqResult::default(),
        Taken::Term(when) if when < target => PrereqResult::default(),
        Taken::Term(when) if when == target => PrereqResult {
            truth: Truth::Missing,
            same_term: vec![code.clone()],
            ..PrereqResult::default()
        },
        Taken::Term(_) => match placed.term {
            Some(term) => PrereqResult {
                truth: Truth::Missing,
                later: vec![(code.clone(), term)],
                ..PrereqResult::default()
            },
            None => PrereqResult {
                truth: Truth::Missing,
                missing: vec![code.clone()],
                ..PrereqResult::default()
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(raw: &str) -> CourseCode {
        CourseCode::parse(raw).unwrap()
    }

    #[test]
    fn prereq_parses_rice_grammar() {
        let expr = PrereqExpr::parse("COMP 182 AND COMP 215 AND (ELEC 303 OR STAT 310)");
        assert_eq!(
            expr,
            PrereqExpr::All(vec![
                PrereqExpr::Course(code("COMP 182")),
                PrereqExpr::Course(code("COMP 215")),
                PrereqExpr::Any(vec![
                    PrereqExpr::Course(code("ELEC 303")),
                    PrereqExpr::Course(code("STAT 310")),
                ]),
            ])
        );
        assert_eq!(
            PrereqExpr::parse("COMP 140"),
            PrereqExpr::Course(code("COMP 140"))
        );
        assert_eq!(
            PrereqExpr::parse("(CHEM 111 AND CHEM 112) AND (MATH 101 OR MATH 102)"),
            PrereqExpr::All(vec![
                PrereqExpr::All(vec![
                    PrereqExpr::Course(code("CHEM 111")),
                    PrereqExpr::Course(code("CHEM 112")),
                ]),
                PrereqExpr::Any(vec![
                    PrereqExpr::Course(code("MATH 101")),
                    PrereqExpr::Course(code("MATH 102")),
                ]),
            ])
        );
        let chem_420 =
            "MATH 212 AND (PHYS 102 OR PHYS 112) AND CHEM 310 OR (CHEM 311 AND CHEM 312)";
        assert_eq!(
            PrereqExpr::parse(chem_420),
            PrereqExpr::Unparsed(chem_420.to_owned())
        );
        assert_eq!(
            PrereqExpr::parse("COMP 140 AND"),
            PrereqExpr::Unparsed("COMP 140 AND".to_owned())
        );
        assert_eq!(
            PrereqExpr::parse("permission of instructor"),
            PrereqExpr::Unparsed("permission of instructor".to_owned())
        );
        assert_eq!(PrereqExpr::parse(""), PrereqExpr::Unparsed(String::new()));
        assert_eq!(
            PrereqExpr::parse("(COMP 140)"),
            PrereqExpr::Course(code("COMP 140"))
        );
    }

    #[test]
    fn fold_is_order_independent() {
        let older = PrereqObservation {
            course: code("COMP 382"),
            raw: "COMP 182".to_owned(),
            parsed: PrereqExpr::parse("COMP 182"),
            corequisite: None,
            catalog_year: CatalogYear(2025),
        };
        let newer = PrereqObservation {
            course: code("COMP 382"),
            raw: "COMP 182 AND COMP 215".to_owned(),
            parsed: PrereqExpr::parse("COMP 182 AND COMP 215"),
            corequisite: None,
            catalog_year: CatalogYear(2026),
        };
        let none = PrereqObservation {
            course: code("COMP 140"),
            raw: String::new(),
            parsed: PrereqExpr::Unparsed(String::new()),
            corequisite: None,
            catalog_year: CatalogYear(2026),
        };
        let a = fold_prerequisites(&[older.clone(), newer.clone(), none.clone()]);
        let b = fold_prerequisites(&[none, newer.clone(), older]);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].course, code("COMP 140"));
        assert_eq!(
            a[0].fact,
            PrereqFact::NoneRequired {
                published_for: CatalogYear(2026)
            }
        );
        assert_eq!(
            a[1].fact,
            PrereqFact::Requires {
                published_for: CatalogYear(2026),
                expr: newer.parsed,
                published: newer.raw,
                corequisite: None,
            }
        );
        assert_eq!(prereq_fact(&a, &code("MATH 101")), &PrereqFact::Unknown);
    }

    #[test]
    fn payload_enums_round_trip() {
        let fact = PrereqFact::Requires {
            published_for: CatalogYear(2026),
            expr: PrereqExpr::parse("COMP 140"),
            published: "COMP 140".to_owned(),
            corequisite: None,
        };
        let json = serde_json::to_string(&fact).unwrap();
        assert!(json.starts_with(r#"{"kind":"requires","value":{"#));
        assert_eq!(serde_json::from_str::<PrereqFact>(&json).unwrap(), fact);
        let unknown = serde_json::to_string(&PrereqFact::Unknown).unwrap();
        assert_eq!(unknown, r#"{"kind":"unknown"}"#);
        let expr = PrereqExpr::All(vec![PrereqExpr::Unparsed("x".to_owned())]);
        let json = serde_json::to_string(&expr).unwrap();
        assert_eq!(serde_json::from_str::<PrereqExpr>(&json).unwrap(), expr);
    }

    #[test]
    fn kleene_table_holds() {
        use Truth::{Missing, Satisfied, Unknown};
        assert_eq!(Satisfied.all(Unknown), Unknown);
        assert_eq!(Missing.all(Unknown), Missing);
        assert_eq!(Satisfied.any(Unknown), Satisfied);
        assert_eq!(Missing.any(Unknown), Unknown);
        assert_eq!(Unknown.all(Unknown), Unknown);
        assert_eq!(Missing.any(Missing), Missing);
    }
}
