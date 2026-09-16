//! General Announcements: the program index and one program page's
//! `table.sc_courselist` rows, read into a draft for review.

use std::collections::BTreeMap;

use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};
use serde::{Deserialize, Serialize};
use skyspace_core::program::{
    CatalogYear, CourseFilter, CourseSelector, CreditScope, Program, ProgramId, ProgramKind,
    Requirement, RequirementBody, RequirementId, Review, SourceRef,
};
use skyspace_core::term::{CreditRange, Credits};
use skyspace_core::{CourseCode, Subject};
use uuid::Uuid;

use crate::text::{clean, code_run, credit_range, credits_min, element_text, re};
use crate::{IssueCode, ParseError, ParseReport, Parsed, sel};

const INDEX: &str = "program index";
const PAGE: &str = "program page";
const GA_ORIGIN: &str = "https://ga.rice.edu";

/// One program page found on the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramLink {
    /// The last path segment: `computer-science-bscs`.
    pub slug: String,
    /// The absolute page URL.
    pub url: String,
    /// The link text; the "Academic Credential Details" table gives the
    /// full title, the others only the credential.
    pub title: String,
    /// The School cell: `EN`.
    pub school: Option<String>,
    /// The Department cell.
    pub department: Option<String>,
}

/// What one program page said, before anyone has reviewed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramDraft {
    /// The H1.
    pub title: String,
    /// The H1's parenthetical: `BSCS`, `BMus`. Never read from the slug.
    pub credential: String,
    /// The page this was extracted from. Shown on every rule in the product.
    pub source_url: String,
    /// The `Total Credit Hours` sum row, when printed.
    pub total_credits: Option<Credits>,
    /// One per `areaheader` row.
    pub areas: Vec<AreaDraft>,
    /// Slash cross-lists: `(STAT 310, ECON 307)` means the second code
    /// fills a rule written for the first.
    pub aliases: Vec<(CourseCode, CourseCode)>,
    /// `table.sc_footnotes` bodies, each a self-check under the root.
    pub footnotes: Vec<String>,
}

/// One area of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaDraft {
    /// The `areaheader` text.
    pub title: String,
    /// The rules the parser could express.
    pub rules: Vec<Requirement>,
    /// Rows the parser could not express, verbatim, kept in this area so
    /// the self-check appears in the right sidebar group. Each becomes
    /// `RequirementBody::Unverifiable`, never a silent omission.
    pub unparsed: Vec<String>,
}

impl ProgramDraft {
    /// The only way to build a `Program`. The caller supplies the `Review`
    /// a person made, so an unreviewed draft cannot reach a student's plan.
    /// The root is an `All` of one `All` per area; every `unparsed` row and
    /// every footnote becomes `RequirementBody::Unverifiable`.
    #[must_use]
    pub fn into_program(
        self,
        id: ProgramId,
        catalog_year: CatalogYear,
        kind: ProgramKind,
        slug: String,
        review: Review,
    ) -> Program {
        let source = SourceRef {
            url: self.source_url.clone(),
            anchor: None,
        };
        let mut children: Vec<Requirement> =
            self.areas
                .into_iter()
                .enumerate()
                .map(|(index, area)| {
                    let mut of = area.rules;
                    of.extend(area.unparsed.into_iter().enumerate().map(|(row, text)| {
                        Requirement {
                            id: placeholder_id(&slug, index, 10_000 + row, &text),
                            label: text.clone(),
                            hours: None,
                            source: source.clone(),
                            body: RequirementBody::Unverifiable { text },
                        }
                    }));
                    Requirement {
                        id: placeholder_id(&slug, index, 0, &area.title),
                        label: area.title,
                        hours: None,
                        source: source.clone(),
                        body: RequirementBody::All { of },
                    }
                })
                .collect();
        children.extend(
            self.footnotes
                .into_iter()
                .enumerate()
                .map(|(row, text)| Requirement {
                    id: placeholder_id(&slug, usize::MAX, row, &text),
                    label: text.clone(),
                    hours: None,
                    source: source.clone(),
                    body: RequirementBody::Unverifiable { text },
                }),
        );
        let root = Requirement {
            id: placeholder_id(&slug, usize::MAX, usize::MAX, "root"),
            label: self.title.clone(),
            hours: None,
            source: source.clone(),
            body: RequirementBody::All { of: children },
        };
        Program {
            id,
            catalog_year,
            slug,
            kind,
            name: self.title,
            credential: self.credential,
            total_credits: self.total_credits,
            source,
            review,
            root,
            retired_requirements: Vec::new(),
        }
    }
}

/// A deterministic placeholder: UUID v5 of a fingerprint, so re-parsing the
/// same page yields the same ids. The store mints real ids at approval.
fn placeholder_id(slug: &str, area: usize, row: usize, text: &str) -> RequirementId {
    let fingerprint = format!(
        "skyspace-parse|{slug}|{area}|{row}|{}",
        clean(text).to_ascii_lowercase()
    );
    RequirementId(Uuid::new_v5(&Uuid::NAMESPACE_URL, fingerprint.as_bytes()))
}

/// Every program page linked from `programs-study/departments-programs/`.
/// The page holds four sortings of one list; links are merged by URL and
/// the longest title kept.
///
/// # Errors
/// [`ParseError::SelectorMissing`] when the page has no `table.grid`.
pub fn parse_program_index(html: &str) -> Result<Parsed<Vec<ProgramLink>>, ParseError> {
    let doc = Html::parse_document(html);
    let grid = sel("table.grid")?;
    if doc.select(&grid).next().is_none() {
        return Err(ParseError::SelectorMissing {
            selector: "table.grid",
            document: INDEX,
        });
    }
    let row = sel("table.grid tr")?;
    let cell = sel("td")?;
    let header = sel("span.table-header-text")?;
    let link = sel("a[href]")?;
    let mut report = ParseReport::default();
    let mut links: BTreeMap<String, ProgramLink> = BTreeMap::new();
    for tr in doc.select(&row) {
        report.saw_row();
        let cells: Vec<(String, ElementRef<'_>)> = tr
            .select(&cell)
            .map(|td| {
                let label = td
                    .select(&header)
                    .next()
                    .map(element_text)
                    .unwrap_or_default();
                (label, td)
            })
            .collect();
        let value = |name: &str| -> Option<String> {
            cells
                .iter()
                .find(|(label, _)| label == name)
                .map(|(_, td)| cell_value(*td, &header))
                .filter(|text| !text.is_empty() && text != "-")
        };
        let school = value("School");
        let department = value("Department");
        let mut kept = false;
        for (label, td) in &cells {
            if !matches!(
                label.as_str(),
                "Program Title" | "Program" | "Undergraduate" | "Graduate"
            ) {
                continue;
            }
            for a in td.select(&link) {
                let Some(href) = a.value().attr("href") else {
                    continue;
                };
                let Some(slug) = program_slug(href) else {
                    continue;
                };
                kept = true;
                let found = ProgramLink {
                    slug,
                    url: absolute(href),
                    title: element_text(a),
                    school: school.clone(),
                    department: department.clone(),
                };
                merge_link(&mut links, found);
            }
        }
        if kept {
            report.kept_row();
        }
    }
    if links.is_empty() {
        return Err(ParseError::SelectorMissing {
            selector: "table.grid a[href]",
            document: INDEX,
        });
    }
    let value: Vec<ProgramLink> = links.into_values().collect();
    for link in &value {
        report.filled("title");
        if link.school.is_some() {
            report.filled("school");
        }
        if link.department.is_some() {
            report.filled("department");
        }
    }
    Ok(Parsed { value, report })
}

/// The cell's text without its small-screen header label.
fn cell_value(td: ElementRef<'_>, header: &Selector) -> String {
    let header_text = td
        .select(header)
        .next()
        .map(element_text)
        .unwrap_or_default();
    let all = element_text(td);
    clean(all.strip_prefix(&header_text).unwrap_or(&all))
}

fn merge_link(links: &mut BTreeMap<String, ProgramLink>, found: ProgramLink) {
    match links.get_mut(&found.url) {
        Some(held) => {
            if found.title.len() > held.title.len() {
                held.title = found.title;
            }
            if held.school.is_none() {
                held.school = found.school;
            }
            if held.department.is_none() {
                held.department = found.department;
            }
        }
        None => {
            links.insert(found.url.clone(), found);
        }
    }
}

/// The slug of a program page: three segments under
/// `/programs-study/departments-programs/`, on the live site or an archive.
fn program_slug(href: &str) -> Option<String> {
    let path = href.strip_prefix(GA_ORIGIN).unwrap_or(href);
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let (_, rest) = path.split_once("/programs-study/departments-programs/")?;
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    match segments.as_slice() {
        [_school, _department, program] => Some((*program).to_owned()),
        _ => None,
    }
}

fn absolute(href: &str) -> String {
    if href.starts_with('/') {
        format!("{GA_ORIGIN}{href}")
    } else {
        href.to_owned()
    }
}

/// One program page as a draft. Rows are classified by their class tokens
/// (`areaheader`, `areasubheader`, `orclass`, `listsum`) and by whether
/// `td.codecol` holds a code; anything unclassified lands in the area's
/// `unparsed` list and is reported, never dropped.
///
/// # Errors
/// [`ParseError::SelectorMissing`] when the page has no `table.sc_courselist`.
pub fn parse_program_page(
    html: &str,
    source_url: &str,
) -> Result<Parsed<ProgramDraft>, ParseError> {
    let doc = Html::parse_document(html);
    let table = sel("table.sc_courselist")?;
    if doc.select(&table).next().is_none() {
        return Err(ParseError::SelectorMissing {
            selector: "table.sc_courselist",
            document: PAGE,
        });
    }
    let title = doc
        .select(&sel("h1.page-title")?)
        .next()
        .or_else(|| doc.select(&sel("h1").ok()?).next())
        .map(element_text)
        .unwrap_or_default();
    let credential = credential(&title)?;
    let footnotes: Vec<String> = doc
        .select(&sel("table.sc_footnotes td.notecol")?)
        .map(element_text)
        .filter(|t| !t.is_empty())
        .collect();
    let mut page = Page::new(source_url)?;
    for tr in doc.select(&sel("table.sc_courselist tr")?) {
        page.row(tr);
    }
    page.close_group();
    let mut report = page.report;
    if !title.is_empty() {
        report.filled("title");
    }
    if !credential.is_empty() {
        report.filled("credential");
    }
    if page.total_credits.is_some() {
        report.filled("total_credits");
    }
    Ok(Parsed {
        value: ProgramDraft {
            title,
            credential,
            source_url: source_url.to_owned(),
            total_credits: page.total_credits,
            areas: page.areas,
            aliases: page.aliases,
            footnotes,
        },
        report,
    })
}

/// The last parenthetical of the H1: `(BSCS)` in
/// `Bachelor of Science in Computer Science (BSCS) Degree`.
fn credential(title: &str) -> Result<String, ParseError> {
    let paren = re(r"\(([^()]+)\)")?;
    Ok(paren
        .captures_iter(title)
        .last()
        .and_then(|caps| caps.get(1))
        .map(|m| clean(m.as_str()))
        .unwrap_or_default())
}

/// A rule row that collects the course rows under it.
struct Group {
    label: String,
    hours: Option<CreditRange>,
    kind: GroupKind,
    options: Vec<Requirement>,
    filter: CourseFilter,
    id: RequirementId,
}

enum GroupKind {
    Select(u8),
    Credits(Credits),
    /// A bare heading such as "Math Courses": the course rows under it are
    /// all required, and the heading is their label, not a rule of its own.
    All,
}

/// A codeless row that reads as a heading rather than a sentence: a few
/// words, no terminal punctuation, no hours. "Math Courses" is one;
/// "Note: University Graduation Requirements include 31 credit hours." and
/// "Select 1 course from the following:" are not.
fn is_heading(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text.split_whitespace().count() <= 6
        && !text.ends_with(['.', ':', ';', '!', '?'])
        && !text.contains(':')
        && !text.chars().any(|c| c.is_ascii_digit())
}

/// What one `<tr>` turned out to be.
enum Row {
    AreaHeader(String),
    Subtotal,
    Rule {
        text: String,
        hours: Option<CreditRange>,
    },
    Course {
        codes: Vec<CourseCode>,
        title: String,
        hours: Option<CreditRange>,
        indented: bool,
    },
    Or {
        codes: Vec<CourseCode>,
    },
    Comment {
        text: String,
        hours: Option<CreditRange>,
    },
    Total(Option<Credits>),
    Skip,
}

/// Selectors and patterns compiled once per page.
struct Shapes {
    hours: Selector,
    codecol: Selector,
    comment: Selector,
    indent: Selector,
    td: Selector,
    select_rule: Regex,
    credits_rule: Regex,
    semesters: Regex,
    between: Regex,
    exception: Regex,
}

impl Shapes {
    fn new() -> Result<Self, ParseError> {
        Ok(Self {
            hours: sel("td.hourscol")?,
            codecol: sel("td.codecol")?,
            comment: sel("span.courselistcomment")?,
            indent: sel("td.codecol div.blockindent")?,
            td: sel("td")?,
            select_rule: re(
                r"(?i)^select\s+(\d+|one|two|three|four|five|six)\s+(?:courses?\b|of the following|from the following)",
            )?,
            credits_rule: re(r"(?i)^select\s+(?:a minimum of\s+)?(\d+)\s+credit hours")?,
            semesters: re(r"(?i)\(minimum of (\d+) semesters?\)")?,
            between: re(
                r"(?i)any course between\s+([A-Z]{2,8})\s*(\d{1,4})\s+and\s+([A-Z]{2,8})\s*(\d{1,4})",
            )?,
            exception: re(
                r"(?i)with the exception of\s+([A-Z]{2,8}\s*\d{1,4}(?:\s*[,/]?\s*(?:and\s+)?[A-Z]{2,8}\s*\d{1,4})*)",
            )?,
        })
    }
}

/// Parse state for one page.
struct Page<'a> {
    source: SourceRef,
    slug: String,
    shapes: Shapes,
    report: ParseReport,
    areas: Vec<AreaDraft>,
    aliases: Vec<(CourseCode, CourseCode)>,
    group: Option<Group>,
    in_summary: bool,
    row_index: usize,
    total_credits: Option<Credits>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl Page<'_> {
    fn new(source_url: &str) -> Result<Self, ParseError> {
        Ok(Self {
            source: SourceRef {
                url: source_url.to_owned(),
                anchor: None,
            },
            slug: program_slug(source_url).unwrap_or_else(|| source_url.to_owned()),
            shapes: Shapes::new()?,
            report: ParseReport::default(),
            areas: Vec::new(),
            aliases: Vec::new(),
            group: None,
            in_summary: false,
            row_index: 0,
            total_credits: None,
            _marker: std::marker::PhantomData,
        })
    }

    fn row(&mut self, tr: ElementRef<'_>) {
        let row = self.classify(tr);
        if matches!(row, Row::Skip) {
            return;
        }
        self.report.saw_row();
        self.row_index += 1;
        match row {
            Row::AreaHeader(title) => {
                self.close_group();
                self.in_summary = false;
                self.areas.push(AreaDraft {
                    title,
                    rules: Vec::new(),
                    unparsed: Vec::new(),
                });
                self.report.kept_row();
            }
            Row::Subtotal => {
                self.close_group();
                self.in_summary = true;
            }
            Row::Total(credits) => {
                self.close_group();
                if credits.is_some() {
                    self.total_credits = credits;
                    self.report.kept_row();
                }
            }
            Row::Rule { text, hours } => {
                self.close_group();
                self.rule(text, hours);
            }
            Row::Course {
                codes,
                title,
                hours,
                indented,
            } => self.course(&codes, title, hours, indented),
            Row::Or { codes } => self.or_row(&codes),
            Row::Comment { text, hours } => {
                if self.in_summary {
                    return;
                }
                self.close_group();
                self.comment(text, hours);
            }
            Row::Skip => {}
        }
    }

    fn classify(&mut self, tr: ElementRef<'_>) -> Row {
        let tokens: Vec<String> = tr.value().classes().map(str::to_owned).collect();
        let has = |token: &str| tokens.iter().any(|t| t == token);
        let at = format!("{} row {}", self.slug, self.row_index + 1);
        let hours_text = tr
            .select(&self.shapes.hours)
            .next()
            .map(element_text)
            .unwrap_or_default();
        let hours = if hours_text.is_empty() {
            None
        } else {
            credit_range(&hours_text, &mut self.report, &at)
        };
        let comment_text = tr
            .select(&self.shapes.comment)
            .next()
            .map(text_without_sup)
            .unwrap_or_default();
        if has("listsum") {
            let label = tr
                .select(&self.shapes.td)
                .next()
                .map(element_text)
                .unwrap_or_default();
            return if label.starts_with("Total Credit Hours") {
                Row::Total(credits_min(&hours_text))
            } else {
                Row::Skip
            };
        }
        if has("areaheader") {
            return if comment_text.starts_with("Total Credit Hours Required") {
                Row::Subtotal
            } else {
                Row::AreaHeader(comment_text)
            };
        }
        if has("areasubheader") {
            return Row::Rule {
                text: comment_text,
                hours,
            };
        }
        let codes: Vec<CourseCode> = tr
            .select(&self.shapes.codecol)
            .next()
            .map(|td| codes_in_cell(&text_without_sup(td)))
            .unwrap_or_default();
        if has("orclass") {
            return if codes.is_empty() {
                Row::Comment {
                    text: element_text(tr),
                    hours,
                }
            } else {
                Row::Or { codes }
            };
        }
        if !codes.is_empty() {
            let title = tr
                .select(&self.shapes.td)
                .filter(|td| !self.shapes.codecol.matches(td) && !self.shapes.hours.matches(td))
                .map(text_without_sup)
                .find(|text| !text.is_empty())
                .unwrap_or_default();
            let indented = tr.select(&self.shapes.indent).next().is_some();
            return Row::Course {
                codes,
                title,
                hours,
                indented,
            };
        }
        if !comment_text.is_empty() {
            return if comment_text.starts_with("Total Credit Hours Required") {
                Row::Subtotal
            } else {
                Row::Comment {
                    text: comment_text,
                    hours,
                }
            };
        }
        if tr.select(&self.shapes.codecol).next().is_some() {
            let text = element_text(tr);
            if !text.is_empty() {
                return Row::Comment { text, hours };
            }
        }
        Row::Skip
    }

    fn area(&mut self) -> &mut AreaDraft {
        if self.areas.is_empty() {
            self.areas.push(AreaDraft {
                title: String::new(),
                rules: Vec::new(),
                unparsed: Vec::new(),
            });
        }
        let last = self.areas.len() - 1;
        &mut self.areas[last]
    }

    fn id_for(&self, text: &str) -> RequirementId {
        placeholder_id(
            &self.slug,
            self.areas.len().saturating_sub(1),
            self.row_index,
            text,
        )
    }

    fn unparsed(&mut self, text: String) {
        let at = format!("{} row {}", self.slug, self.row_index);
        self.report.issue(
            IssueCode::UnclassifiedRequirementRow,
            format!("{at}: {text:?}"),
        );
        self.report.kept_row();
        self.area().unparsed.push(text);
    }

    fn rule(&mut self, text: String, hours: Option<CreditRange>) {
        let id = self.id_for(&text);
        if let Some(caps) = self.shapes.select_rule.captures(&text) {
            let count = caps
                .get(1)
                .and_then(|m| count_word(m.as_str()))
                .unwrap_or(1);
            self.group = Some(Group {
                label: text,
                hours,
                kind: GroupKind::Select(count),
                options: Vec::new(),
                filter: CourseFilter::default(),
                id,
            });
            return;
        }
        if let Some(caps) = self.shapes.credits_rule.captures(&text) {
            let minimum = caps
                .get(1)
                .and_then(|m| Credits::parse(m.as_str()).ok())
                .unwrap_or(Credits::ZERO);
            self.group = Some(Group {
                label: text,
                hours,
                kind: GroupKind::Credits(minimum),
                options: Vec::new(),
                filter: CourseFilter::default(),
                id,
            });
            return;
        }
        self.unparsed(text);
    }

    fn course_requirement(
        &mut self,
        codes: &[CourseCode],
        title: String,
        hours: Option<CreditRange>,
    ) -> Requirement {
        let semesters = self
            .shapes
            .semesters
            .captures(&title)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<u8>().ok())
            .unwrap_or(1);
        let first = codes[0].clone();
        for other in &codes[1..] {
            self.aliases.push((first.clone(), other.clone()));
        }
        Requirement {
            id: self.id_for(&format!("{first} {title}")),
            label: title,
            hours,
            source: self.source.clone(),
            body: RequirementBody::Course {
                filter: CourseFilter {
                    include: vec![CourseSelector::Code { code: first }],
                    exclude: Vec::new(),
                },
                semesters,
            },
        }
    }

    fn course(
        &mut self,
        codes: &[CourseCode],
        title: String,
        hours: Option<CreditRange>,
        indented: bool,
    ) {
        self.report.kept_row();
        let first = codes[0].clone();
        if matches!(self.group.as_ref().map(|g| &g.kind), Some(GroupKind::All)) {
            let requirement = self.course_requirement(codes, title, hours);
            if let Some(group) = self.group.as_mut() {
                group.options.push(requirement);
            }
            return;
        }
        if indented && self.group.is_some() {
            if let Some(GroupKind::Credits(_)) = self.group.as_ref().map(|g| &g.kind) {
                for other in &codes[1..] {
                    self.aliases.push((first.clone(), other.clone()));
                }
                if let Some(group) = self.group.as_mut() {
                    group
                        .filter
                        .include
                        .push(CourseSelector::Code { code: first });
                }
            } else {
                let requirement = self.course_requirement(codes, title, hours);
                if let Some(group) = self.group.as_mut() {
                    group.options.push(requirement);
                }
            }
            return;
        }
        self.close_group();
        let requirement = self.course_requirement(codes, title, hours);
        self.area().rules.push(requirement);
    }

    /// `or MATH 105` adds a selector to the rule above it.
    fn or_row(&mut self, codes: &[CourseCode]) {
        let first = codes[0].clone();
        let aliases: Vec<(CourseCode, CourseCode)> = codes[1..]
            .iter()
            .map(|other| (first.clone(), other.clone()))
            .collect();
        let target = match self.group.as_mut() {
            Some(group) => match group.kind {
                GroupKind::Credits(_) => {
                    group
                        .filter
                        .include
                        .push(CourseSelector::Code { code: first });
                    self.aliases.extend(aliases);
                    self.report.kept_row();
                    return;
                }
                GroupKind::Select(_) | GroupKind::All => group.options.last_mut(),
            },
            None => self.areas.last_mut().and_then(|area| area.rules.last_mut()),
        };
        match target.map(|r| &mut r.body) {
            Some(RequirementBody::Course { filter, .. }) => {
                filter.include.push(CourseSelector::Code { code: first });
                self.aliases.extend(aliases);
                self.report.kept_row();
            }
            _ => self.unparsed(format!(
                "or {}",
                codes
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" / ")
            )),
        }
    }

    /// A codeless row: the "any course between X and Y with the exception
    /// of Z" form becomes a range rule; everything else is kept verbatim.
    fn comment(&mut self, text: String, hours: Option<CreditRange>) {
        if hours.is_none() && is_heading(&text) {
            let id = self.id_for(&text);
            self.group = Some(Group {
                label: text,
                hours: None,
                kind: GroupKind::All,
                options: Vec::new(),
                filter: CourseFilter::default(),
                id,
            });
            return;
        }
        let Some(caps) = self.shapes.between.captures(&text) else {
            self.unparsed(text);
            return;
        };
        let field = |i: usize| caps.get(i).map_or("", |m| m.as_str());
        let (Ok(low_subject), Ok(high_subject)) = (Subject::new(field(1)), Subject::new(field(3)))
        else {
            self.unparsed(text);
            return;
        };
        let (Ok(low), Ok(high)) = (field(2).parse::<u16>(), field(4).parse::<u16>()) else {
            self.unparsed(text);
            return;
        };
        if low_subject != high_subject || low > high {
            self.unparsed(text);
            return;
        }
        let exclude: Vec<CourseSelector> = self
            .shapes
            .exception
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| code_run(&m.as_str().replace(" and ", " ")).0)
            .unwrap_or_default()
            .into_iter()
            .map(|code| CourseSelector::Code { code })
            .collect();
        let requirement = Requirement {
            id: self.id_for(&text),
            label: text,
            hours,
            source: self.source.clone(),
            body: RequirementBody::Course {
                filter: CourseFilter {
                    include: vec![CourseSelector::NumberRange {
                        subject: Some(low_subject),
                        low,
                        high,
                    }],
                    exclude,
                },
                semesters: 1,
            },
        };
        self.report.kept_row();
        self.area().rules.push(requirement);
    }

    /// Finish an open rule row. A rule that collected no options is not a
    /// rule the engine can check, so it goes to `unparsed` instead.
    fn close_group(&mut self) {
        let Some(group) = self.group.take() else {
            return;
        };
        let empty = match group.kind {
            GroupKind::Select(_) | GroupKind::All => group.options.is_empty(),
            GroupKind::Credits(_) => group.filter.include.is_empty(),
        };
        if empty {
            self.unparsed(group.label);
            return;
        }
        let body = match group.kind {
            GroupKind::Select(count) => RequirementBody::Select {
                count,
                of: group.options,
            },
            GroupKind::Credits(minimum) => RequirementBody::Credits {
                minimum,
                scope: CreditScope::Any,
                from: group.filter,
            },
            GroupKind::All => RequirementBody::All { of: group.options },
        };
        self.report.kept_row();
        let source = self.source.clone();
        self.area().rules.push(Requirement {
            id: group.id,
            label: group.label,
            hours: group.hours,
            source,
            body,
        });
    }
}

/// The codes in a `td.codecol`: `STAT 310 / ECON 307`, `or MATH 105`, or a
/// bare `COMP 303` that GA printed without a link. Links are not required,
/// because the 2026-27 BSCS page prints one code as plain text.
fn codes_in_cell(text: &str) -> Vec<CourseCode> {
    let text = text
        .strip_prefix("or ")
        .or_else(|| text.strip_prefix("OR "))
        .unwrap_or(text);
    code_run(text).0
}

/// `1` or `one` as a count.
fn count_word(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        digits => digits.parse().ok(),
    }
}

/// The element's text with footnote markers (`<sup>`) left out.
fn text_without_sup(element: ElementRef<'_>) -> String {
    let mut out = String::new();
    let mut stack: Vec<_> = element.children().collect();
    stack.reverse();
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(text) => out.push_str(&text.text),
            Node::Element(el) if el.name() == "sup" => {}
            Node::Element(_) => {
                let mut children: Vec<_> = node.children().collect();
                children.reverse();
                stack.extend(children);
            }
            _ => {}
        }
    }
    clean(&out)
}
