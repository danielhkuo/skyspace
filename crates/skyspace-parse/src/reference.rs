//! The seven `!SWKSCAT.info` reference lists: `TERMS`, `SUBJECTS`,
//! `DEPARTMENTS`, `SCHOOLS`, `SESSIONS`, `YEARS`, `ATTRS`. Every one is a
//! root element of `<X code="..."><VAL>..</VAL><OPT>label</OPT>label</X>`
//! entries; `TERMS` and `YEARS` name the current one on the root.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::ParseError;

/// Which reference list a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    /// `action=TERMS`: every term code with its label and the current one.
    Terms,
    /// `action=SUBJECTS`: the closed subject list for a term.
    Subjects,
    /// `action=DEPARTMENTS`: departments, a different list from subjects.
    Departments,
    /// `action=SCHOOLS`: schools.
    Schools,
    /// `action=SESSIONS`: part-of-term codes and labels for one term.
    Sessions,
    /// `action=YEARS`: academic years, `2026-2027`, and the current one.
    Years,
    /// `action=ATTRS`: the four course attributes.
    Attrs,
}

impl RefKind {
    /// Root element, entry element, and the root attribute naming the
    /// current entry, if the list has one.
    const fn shape(
        self,
    ) -> (
        &'static str,
        &'static str,
        Option<&'static str>,
        &'static str,
    ) {
        match self {
            Self::Terms => ("TERMS", "TERM", Some("currentTerm"), "TERMS/TERM"),
            Self::Subjects => ("SUBJECTS", "SUBJECT", None, "SUBJECTS/SUBJECT"),
            Self::Departments => ("DEPARTMENTS", "DEPARTMENT", None, "DEPARTMENTS/DEPARTMENT"),
            Self::Schools => ("SCHOOLS", "SCHOOL", None, "SCHOOLS/SCHOOL"),
            Self::Sessions => ("SESSIONS", "SESSION", None, "SESSIONS/SESSION"),
            Self::Years => ("YEARS", "YEAR", Some("currentYear"), "YEARS/YEAR"),
            Self::Attrs => ("ATTRS", "ATTR", None, "ATTRS/ATTR"),
        }
    }
}

/// One entry of a reference list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceEntry {
    /// Rice's code: `202710`, `COMP`, `GRP3`.
    pub code: String,
    /// Rice's `<OPT>` text, never synthesised.
    pub label: String,
    /// True for the entry the root's `currentTerm` / `currentYear` names.
    pub current: bool,
}

/// Every entry of one reference list, in Rice's order.
///
/// # Errors
/// [`ParseError::SelectorMissing`] when the root element is not the one
/// `kind` names or holds no entries, so an empty list never looks like a
/// good pull; [`ParseError::Xml`] when the text is not well-formed XML.
pub fn parse_reference_list(xml: &str, kind: RefKind) -> Result<Vec<ReferenceEntry>, ParseError> {
    let (root_name, entry_name, current_attr, path) = kind.shape();
    let missing = ParseError::SelectorMissing {
        selector: path,
        document: "reference list",
    };
    let mut reader = Reader::from_str(xml);
    let mut depth = 0u32;
    let mut current: Option<String> = None;
    let mut root_ok = false;
    let mut entries = Vec::new();
    let mut open: Option<ReferenceEntry> = None;
    let mut in_opt = false;
    loop {
        let event = reader.read_event().map_err(xml_error)?;
        match event {
            Event::Start(start) => {
                depth += 1;
                let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
                if depth == 1 {
                    if name != root_name {
                        return Err(missing);
                    }
                    root_ok = true;
                    if let Some(attr) = current_attr {
                        current = attribute(&start, attr);
                    }
                } else if depth == 2 && name == entry_name {
                    let code = attribute(&start, "code").unwrap_or_default();
                    open = Some(ReferenceEntry {
                        current: current.as_deref() == Some(code.as_str()),
                        code,
                        label: String::new(),
                    });
                } else if depth == 3 && name == "OPT" {
                    in_opt = true;
                }
            }
            Event::Text(text) if in_opt => {
                let text = text
                    .xml_content()
                    .map_err(|e| xml_error(quick_xml::Error::from(e)))?;
                if let Some(entry) = open.as_mut() {
                    entry.label.push_str(&text);
                }
            }
            Event::GeneralRef(reference) if in_opt => {
                let resolved = if reference.is_char_ref() {
                    reference.resolve_char_ref().map_err(xml_error)?
                } else {
                    let name = reference
                        .decode()
                        .map_err(|e| xml_error(quick_xml::Error::from(e)))?;
                    predefined_entity(&name)
                };
                if let (Some(entry), Some(ch)) = (open.as_mut(), resolved) {
                    entry.label.push(ch);
                }
            }
            Event::End(_) => {
                if depth == 3 {
                    in_opt = false;
                } else if depth == 2
                    && let Some(entry) = open.take()
                {
                    entries.push(ReferenceEntry {
                        label: entry.label.trim().to_owned(),
                        ..entry
                    });
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_ok || entries.is_empty() {
        return Err(missing);
    }
    Ok(entries)
}

fn attribute(start: &quick_xml::events::BytesStart<'_>, name: &str) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|attr| attr.key.as_ref() == name.as_bytes())
        .and_then(|attr| attr.unescape_value().ok())
        .map(std::borrow::Cow::into_owned)
}

/// The five entities XML predefines; the reader hands them over by name.
fn predefined_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => None,
    }
}

fn xml_error(error: quick_xml::Error) -> ParseError {
    ParseError::Xml(quick_xml::DeError::from(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_and_char_refs_are_resolved() {
        let xml = r#"<SUBJECTS term="202710"><SUBJECT code="AAAS"><VAL>AAAS</VAL><OPT>African &amp; African &#65;mer &lt;Studies&gt; (AAAS)</OPT>African &amp; African Amer Studies</SUBJECT></SUBJECTS>"#;
        let entries = parse_reference_list(xml, RefKind::Subjects).unwrap();
        assert_eq!(entries[0].label, "African & African Amer <Studies> (AAAS)");
        assert_eq!(entries[0].code, "AAAS");
        assert!(!entries[0].current);
    }
}
