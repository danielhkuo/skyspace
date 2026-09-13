//! Every URL a job requests, built with `url` so no query string is
//! hand-escaped. The GA archive path for a past catalog year lives here too.

use skyspace_core::code::{Crn, Subject};
use skyspace_core::program::CatalogYear;
use skyspace_core::term::TermCode;
use skyspace_parse::RefKind;
use url::Url;

use crate::fetch::{COURSES_BASE, GA_BASE};

/// The catalog year whose General Announcements are the live site. Earlier
/// years are mirrored under `/archive/<YYYY>-<YYYY+1>/`; 2025 is PDF only.
pub const GA_LIVE_YEAR: u16 = 2026;

fn courses(program: &str, pairs: &[(&str, &str)]) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(&format!("{COURSES_BASE}.{program}"))?;
    url.query_pairs_mut().extend_pairs(pairs);
    Ok(url)
}

/// `!SWKSCAT.info?action=<KIND>&term=&year=`.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn reference(kind: RefKind, term: TermCode) -> Result<Url, url::ParseError> {
    let action = match kind {
        RefKind::Terms => "TERMS",
        RefKind::Subjects => "SUBJECTS",
        RefKind::Departments => "DEPARTMENTS",
        RefKind::Schools => "SCHOOLS",
        RefKind::Sessions => "SESSIONS",
        RefKind::Years => "YEARS",
        RefKind::Attrs => "ATTRS",
    };
    courses(
        "info",
        &[
            ("action", action),
            ("term", &term.to_string()),
            ("year", &term.academic_year().to_string()),
        ],
    )
}

/// `SUBJECTS` for an academic year, for the catalog pull.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn subjects_for_year(year: CatalogYear) -> Result<Url, url::ParseError> {
    courses(
        "info",
        &[("action", "SUBJECTS"), ("year", &year.0.to_string())],
    )
}

/// `!SWKSCAT.cat?p_action=QUERY&p_term=&p_subj=`. The subject filter is
/// required: without one Rice returns the search form.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn listing(term: TermCode, subject: &Subject) -> Result<Url, url::ParseError> {
    courses(
        "cat",
        &[
            ("p_action", "QUERY"),
            ("p_term", &term.to_string()),
            ("p_subj", subject.as_str()),
        ],
    )
}

/// `!SWKSCAT.cat?p_action=CATALIST&p_acyr_code=&p_subj=`.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn catalist(year: CatalogYear, subject: &Subject) -> Result<Url, url::ParseError> {
    courses(
        "cat",
        &[
            ("p_action", "CATALIST"),
            ("p_acyr_code", &year.0.to_string()),
            ("p_subj", subject.as_str()),
        ],
    )
}

/// `!SWKSCAT.info?action=ASSOCIATED-SECTIONS&crn=&term=`.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn associated_sections(term: TermCode, crn: Crn) -> Result<Url, url::ParseError> {
    courses(
        "info",
        &[
            ("action", "ASSOCIATED-SECTIONS"),
            ("crn", &crn.to_string()),
            ("term", &term.to_string()),
        ],
    )
}

/// `!SWKSCAT.cat?p_action=COURSE&p_term=&p_crn=`.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn detail(term: TermCode, crn: Crn) -> Result<Url, url::ParseError> {
    courses(
        "cat",
        &[
            ("p_action", "COURSE"),
            ("p_term", &term.to_string()),
            ("p_crn", &crn.to_string()),
        ],
    )
}

/// `!SWKSCAT.live?action=ENROLLMENT&crn=&term=`.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn enrollment(term: TermCode, crn: Crn) -> Result<Url, url::ParseError> {
    courses(
        "live",
        &[
            ("action", "ENROLLMENT"),
            ("crn", &crn.to_string()),
            ("term", &term.to_string()),
        ],
    )
}

/// The GA root for a catalog year: the live site for `GA_LIVE_YEAR` and
/// later, the archive mirror before it.
#[must_use]
pub fn ga_root(year: CatalogYear) -> String {
    if year.0 >= GA_LIVE_YEAR {
        GA_BASE.to_owned()
    } else {
        format!("{GA_BASE}/archive/{}-{}", year.0, year.0 + 1)
    }
}

/// The program index for a catalog year.
///
/// # Errors
/// `url::ParseError` only if the base constant is malformed.
pub fn program_index(year: CatalogYear) -> Result<Url, url::ParseError> {
    Url::parse(&format!(
        "{}/programs-study/departments-programs/",
        ga_root(year)
    ))
}

/// The catalog year a GA URL belongs to, from its archive segment; the
/// live year when it has none.
#[must_use]
pub fn catalog_year_of_ga_url(url: &str) -> CatalogYear {
    url.split("/archive/")
        .nth(1)
        .and_then(|rest| rest.split('-').next())
        .and_then(|year| year.parse::<u16>().ok())
        .map_or(CatalogYear(GA_LIVE_YEAR), CatalogYear)
}

/// One query parameter of a URL, for reading a term, CRN or year back out
/// of an archived `raw_responses.url`.
#[must_use]
pub fn query_param(url: &str, key: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

#[cfg(test)]
mod tests {
    use skyspace_core::code::{Crn, Subject};
    use skyspace_core::program::CatalogYear;
    use skyspace_core::term::TermCode;
    use skyspace_parse::RefKind;

    use super::{catalog_year_of_ga_url, ga_root, listing, query_param, reference};

    #[test]
    fn urls_keep_the_doubled_segment_and_escape_nothing_by_hand() {
        let term = TermCode::parse("202710").unwrap();
        let url = listing(term, &Subject::new("COMP").unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://courses.rice.edu/courses/courses/!SWKSCAT.cat?p_action=QUERY&p_term=202710&p_subj=COMP"
        );
        let url = reference(RefKind::Terms, term).unwrap();
        assert!(url.as_str().contains("action=TERMS&term=202710&year=2027"));
        assert_eq!(query_param(url.as_str(), "term").as_deref(), Some("202710"));
        assert_eq!(
            super::associated_sections(term, Crn(42)).unwrap().query(),
            Some("action=ASSOCIATED-SECTIONS&crn=00042&term=202710")
        );
    }

    #[test]
    fn ga_archive_years() {
        assert_eq!(ga_root(CatalogYear(2026)), "https://ga.rice.edu");
        assert_eq!(
            ga_root(CatalogYear(2024)),
            "https://ga.rice.edu/archive/2024-2025"
        );
        assert_eq!(
            catalog_year_of_ga_url("https://ga.rice.edu/archive/2023-2024/x/"),
            CatalogYear(2023)
        );
        assert_eq!(
            catalog_year_of_ga_url("https://ga.rice.edu/x/"),
            CatalogYear(2026)
        );
    }
}
