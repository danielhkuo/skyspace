//! Catalog reads: reference lists, the section search, one section, one or
//! many courses, and seats. Every response is `Fresh<T>` with a weak `ETag`
//! that a finished pull moves for the whole term at once.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum_extra::extract::Query;
use serde::Deserialize;
use skyspace_core::catalog::{Attribute, Section};
use skyspace_core::code::{CourseCode, Crn};
use skyspace_core::term::TermCode;
use skyspace_store::{SectionQuery, SectionSort, Store};
use time::{Duration, OffsetDateTime};

use crate::dto::{
    CatalogQuery, ClassLinks, CourseView, DataSource, DropFilter, Fresh, Freshness, ReferenceBody,
    ReferenceEntry, SeatRow, SectionPage, SortKey,
};
use crate::error::ApiError;
use crate::freshness::{SEATS_IN_WINDOW, SEATS_OUTSIDE_WINDOW, threshold};
use crate::routes::{
    cached, catalog_etag, etag_matches, freshness_for, not_modified, resolve_term,
};
use crate::state::AppState;

/// Offsets past this are refused: nobody pages that far by hand.
pub const MAX_OFFSET: u32 = 10_000;
/// The largest page.
pub const MAX_LIMIT: u16 = 100;
/// The most codes one `GET /courses` may name.
pub const MAX_CODES: usize = 50;
/// The most CRNs one `GET /seats` may name.
pub const MAX_CRNS: usize = 200;
/// Extra counts run for an empty result, one per active filter.
const MAX_SUGGESTIONS: usize = 6;
const MINUTES_IN_DAY: u16 = 24 * 60;
const MAX_AGE_REFERENCE: u32 = 3600;
const MAX_AGE_SEARCH: u32 = 60;
const MAX_AGE_COURSE: u32 = 300;
const MAX_AGE_SEATS: u32 = 60;

fn invalid(field: &str) -> ApiError {
    ApiError::Invalid(field.to_owned())
}

fn sort_dedup<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

impl CatalogQuery {
    /// Normalise and check the query: reversed ranges and a far offset are
    /// refused; lists are sorted and de-duplicated; the limit is clamped.
    /// The term is checked by the handler, which can ask the store.
    ///
    /// # Errors
    /// `ApiError::Invalid` naming the field.
    pub fn validate(&mut self) -> Result<(), ApiError> {
        self.q = self
            .q
            .take()
            .map(|q| q.trim().to_owned())
            .filter(|q| !q.is_empty());
        sort_dedup(&mut self.subject);
        sort_dedup(&mut self.department);
        sort_dedup(&mut self.school);
        sort_dedup(&mut self.attr);
        sort_dedup(&mut self.level);
        sort_dedup(&mut self.days);
        sort_dedup(&mut self.part_of_term);
        if let (Some(min), Some(max)) = (self.level_min, self.level_max)
            && min > max
        {
            return Err(invalid("levelMin"));
        }
        if let (Some(min), Some(max)) = (self.credits_min, self.credits_max)
            && min > max
        {
            return Err(invalid("creditsMin"));
        }
        if self.starts_after.is_some_and(|m| m >= MINUTES_IN_DAY) {
            return Err(invalid("startsAfter"));
        }
        if self.ends_before.is_some_and(|m| m > MINUTES_IN_DAY) {
            return Err(invalid("endsBefore"));
        }
        if let (Some(start), Some(end)) = (self.starts_after, self.ends_before)
            && start >= end
        {
            return Err(invalid("startsAfter"));
        }
        if self.offset > MAX_OFFSET {
            return Err(invalid("offset"));
        }
        self.limit = self.limit.clamp(1, MAX_LIMIT);
        Ok(())
    }

    /// The `ETag` input: sorted keys, sorted values, defaults omitted. The
    /// term is left out because the `ETag` already carries it.
    #[must_use]
    pub fn canonical(&self) -> String {
        let defaults = Self::default();
        let mut pairs: Vec<(&str, String)> = Vec::new();
        if let Some(q) = &self.q {
            pairs.push(("q", q.to_lowercase()));
        }
        pairs.extend(
            self.subject
                .iter()
                .map(|s| ("subject", s.as_str().to_owned())),
        );
        pairs.extend(self.department.iter().map(|d| ("department", d.clone())));
        pairs.extend(self.school.iter().map(|s| ("school", s.clone())));
        pairs.extend(self.attr.iter().map(|a| ("attr", a.code().to_owned())));
        pairs.extend(self.level.iter().map(|l| ("level", l.to_string())));
        push_opt(&mut pairs, "levelMin", self.level_min);
        push_opt(&mut pairs, "levelMax", self.level_max);
        push_opt(&mut pairs, "creditsMin", self.credits_min);
        push_opt(&mut pairs, "creditsMax", self.credits_max);
        pairs.extend(
            self.days
                .iter()
                .map(|d| ("days", format!("{d:?}").to_lowercase())),
        );
        push_opt(&mut pairs, "startsAfter", self.starts_after);
        push_opt(&mut pairs, "endsBefore", self.ends_before);
        pairs.extend(
            self.part_of_term
                .iter()
                .map(|p| ("partOfTerm", p.0.clone())),
        );
        if self.open_seats_only != defaults.open_seats_only {
            pairs.push(("openSeatsOnly", self.open_seats_only.to_string()));
        }
        if self.scheduled_only != defaults.scheduled_only {
            pairs.push(("scheduledOnly", self.scheduled_only.to_string()));
        }
        if self.sort != defaults.sort {
            pairs.push(("sort", format!("{:?}", self.sort).to_lowercase()));
        }
        if self.offset != defaults.offset {
            pairs.push(("offset", self.offset.to_string()));
        }
        if self.limit != defaults.limit {
            pairs.push(("limit", self.limit.to_string()));
        }
        pairs.sort();
        pairs.dedup();
        pairs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// The store's query. `levelMin`/`levelMax` become the hundreds they
    /// cover, intersected with `level` when both are given, because the
    /// store filters by level and not by number.
    #[must_use]
    pub fn to_store_query(&self, term: TermCode) -> SectionQuery {
        let mask = self.days.iter().fold(0u8, |m, d| m | d.bit());
        SectionQuery {
            term,
            q: self.q.clone(),
            subject: self.subject.clone(),
            department: self.department.clone(),
            school: self.school.clone(),
            attr: self.attr.clone(),
            level: self.levels(),
            credits_min: self.credits_min,
            credits_max: self.credits_max,
            day_mask: (mask != 0).then_some(mask),
            starts_after: self.starts_after,
            ends_before: self.ends_before,
            part_of_term: self.part_of_term.clone(),
            open_seats_only: self.open_seats_only,
            scheduled_only: self.scheduled_only,
            sort: match self.sort {
                SortKey::Relevance => SectionSort::Relevance,
                SortKey::CourseNumber => SectionSort::CourseNumber,
                SortKey::Credits => SectionSort::Credits,
                SortKey::OpenSeats => SectionSort::OpenSeats,
            },
            offset: self.offset,
            limit: self.limit,
        }
    }

    /// `level`, narrowed by `levelMin`/`levelMax` when either is set.
    fn levels(&self) -> Vec<u16> {
        if self.level_min.is_none() && self.level_max.is_none() {
            return self.level.clone();
        }
        let low = self.level_min.unwrap_or(0) / 100 * 100;
        let high = self.level_max.unwrap_or(9999) / 100 * 100;
        let range: Vec<u16> = (low..=high).step_by(100).collect();
        if self.level.is_empty() {
            range
        } else {
            range
                .into_iter()
                .filter(|l| self.level.contains(l))
                .collect()
        }
    }

    /// Which filters are active, as `(field, query without it)` pairs.
    fn drop_filters(&self) -> Vec<(&'static str, Self)> {
        let mut out = Vec::new();
        let mut push = |field: &'static str, edit: fn(&mut Self)| {
            let mut copy = self.clone();
            edit(&mut copy);
            out.push((field, copy));
        };
        if self.q.is_some() {
            push("q", |q| q.q = None);
        }
        if !self.subject.is_empty() {
            push("subject", |q| q.subject.clear());
        }
        if !self.department.is_empty() {
            push("department", |q| q.department.clear());
        }
        if !self.school.is_empty() {
            push("school", |q| q.school.clear());
        }
        if !self.attr.is_empty() {
            push("attr", |q| q.attr.clear());
        }
        if !self.level.is_empty() || self.level_min.is_some() || self.level_max.is_some() {
            push("level", |q| {
                q.level.clear();
                q.level_min = None;
                q.level_max = None;
            });
        }
        if self.credits_min.is_some() || self.credits_max.is_some() {
            push("credits", |q| {
                q.credits_min = None;
                q.credits_max = None;
            });
        }
        if !self.days.is_empty() {
            push("days", |q| q.days.clear());
        }
        if self.starts_after.is_some() || self.ends_before.is_some() {
            push("time", |q| {
                q.starts_after = None;
                q.ends_before = None;
            });
        }
        if !self.part_of_term.is_empty() {
            push("partOfTerm", |q| q.part_of_term.clear());
        }
        if self.open_seats_only {
            push("openSeatsOnly", |q| q.open_seats_only = false);
        }
        if self.scheduled_only {
            push("scheduledOnly", |q| q.scheduled_only = false);
        }
        out.truncate(MAX_SUGGESTIONS);
        out
    }
}

fn push_opt(pairs: &mut Vec<(&str, String)>, key: &'static str, value: Option<u16>) {
    if let Some(v) = value {
        pairs.push((key, v.to_string()));
    }
}

/// `?term=` on the single-item routes.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TermQuery {
    /// Defaults to the current term.
    pub term: Option<TermCode>,
}

/// `?term=&code=…&code=…` on the multi-code course route.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodesQuery {
    /// Defaults to the current term.
    pub term: Option<TermCode>,
    /// `ELEC 303`, `ELEC+303` or `ELEC303`; up to fifty.
    pub code: Vec<String>,
}

/// `?term=&crn=…&crn=…` on the seats route.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SeatsQuery {
    /// Defaults to the current term.
    pub term: Option<TermCode>,
    /// Up to two hundred.
    pub crn: Vec<u32>,
}

/// The stamp for data the nightly listing pull feeds.
async fn listing_freshness(store: &Store, term: TermCode) -> Result<Freshness, ApiError> {
    freshness_for(
        store,
        "listings",
        Some(term),
        DataSource::SectionListing,
        None,
        threshold("listings"),
    )
    .await
}

fn class_links(term: TermCode, code: &CourseCode) -> ClassLinks {
    ClassLinks {
        rice_course_page: format!(
            "https://courses.rice.edu/courses/courses/!SWKSCAT.cat?p_action=CATALIST&p_acyr_code={}&p_subj={}&p_crse_numb={}",
            term.academic_year(),
            code.subject.as_str(),
            code.number.as_str()
        ),
        esther_syllabus: None,
        esther_evaluations: None,
    }
}

fn course_view(
    term: TermCode,
    course: skyspace_core::catalog::Course,
    sections: Vec<Section>,
) -> CourseView {
    let links = class_links(term, &course.code);
    let offered = !sections.is_empty();
    CourseView {
        course,
        sections,
        links,
        offered,
    }
}

/// `GET /api/v1/reference?term=`: subjects and parts of term seen in the
/// term, the four attributes, and the department and school lists, which
/// stay empty until the detail pull has covered a term.
pub async fn reference(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TermQuery>,
) -> Result<Response, ApiError> {
    let term = resolve_term(&state.store, query.term).await?;
    let data_version = state.store.data_version(term).await?;
    let etag = catalog_etag(term, data_version, "reference");
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_REFERENCE, &etag));
    }
    let subjects = state
        .store
        .subjects(term)
        .await?
        .into_iter()
        .map(|s| ReferenceEntry {
            code: s.subject.as_str().to_owned(),
            label: s.subject.as_str().to_owned(),
        })
        .collect();
    let parts_of_term = state
        .store
        .parts_of_term(term)
        .await?
        .into_iter()
        .map(|p| ReferenceEntry {
            label: p.label.unwrap_or_else(|| p.code.0.clone()),
            code: p.code.0,
        })
        .collect();
    let attributes = [
        (Attribute::AnalyzingDiversity, "Analyzing Diversity"),
        (Attribute::DistributionOne, "Distribution Group I"),
        (Attribute::DistributionTwo, "Distribution Group II"),
        (Attribute::DistributionThree, "Distribution Group III"),
    ]
    .into_iter()
    .map(|(a, label)| ReferenceEntry {
        code: a.code().to_owned(),
        label: label.to_owned(),
    })
    .collect();
    let body = Fresh {
        data: ReferenceBody {
            subjects,
            departments: Vec::new(),
            schools: Vec::new(),
            parts_of_term,
            attributes,
        },
        freshness: freshness_for(
            &state.store,
            "reference",
            None,
            DataSource::Reference,
            None,
            threshold("reference"),
        )
        .await?,
    };
    Ok(cached(&headers, MAX_AGE_REFERENCE, &etag, &body))
}

/// `GET /api/v1/sections`: one page of the search with the rail's counts,
/// and, when nothing matched, what dropping each filter would give.
pub async fn sections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CatalogQuery>,
) -> Result<Response, ApiError> {
    let mut query = query;
    query.validate()?;
    let term = resolve_term(&state.store, query.term).await?;
    query.term = Some(term);
    let data_version = state.store.data_version(term).await?;
    let etag = catalog_etag(term, data_version, &query.canonical());
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_SEARCH, &etag));
    }
    let rows = state
        .store
        .search_sections(&query.to_store_query(term))
        .await?;
    let suggestions = if rows.total == 0 && query.offset == 0 {
        suggest(&state.store, &query, term).await?
    } else {
        Vec::new()
    };
    let freshness = listing_freshness(&state.store, term).await?;
    let body = Fresh {
        data: SectionPage {
            has_more: rows.page.next.is_some(),
            rows: rows.page.items,
            total: rows.total,
            offset: query.offset,
            limit: query.limit,
            unscheduled_hidden: rows.unscheduled_hidden,
            course_count: rows.course_count,
            applied: query,
            suggestions,
        },
        freshness,
    };
    Ok(cached(&headers, MAX_AGE_SEARCH, &etag, &body))
}

/// One count per active filter, at most six, for the empty state.
async fn suggest(
    store: &Store,
    query: &CatalogQuery,
    term: TermCode,
) -> Result<Vec<DropFilter>, ApiError> {
    let mut out = Vec::new();
    for (field, without) in query.drop_filters() {
        let mut probe = without.to_store_query(term);
        probe.limit = 1;
        probe.offset = 0;
        let would_match = store.search_sections(&probe).await?.total;
        if would_match > 0 {
            out.push(DropFilter {
                field: field.to_owned(),
                would_match,
            });
        }
    }
    Ok(out)
}

/// `GET /api/v1/sections/{crn}?term=`.
pub async fn section(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(crn): Path<u32>,
    Query(query): Query<TermQuery>,
) -> Result<Response, ApiError> {
    let term = resolve_term(&state.store, query.term).await?;
    let data_version = state.store.data_version(term).await?;
    let etag = catalog_etag(term, data_version, &format!("crn={crn}"));
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_COURSE, &etag));
    }
    let section = state
        .store
        .section(term, Crn(crn))
        .await?
        .ok_or(ApiError::NotFound)?;
    let body = Fresh {
        data: section,
        freshness: listing_freshness(&state.store, term).await?,
    };
    Ok(cached(&headers, MAX_AGE_COURSE, &etag, &body))
}

/// `GET /api/v1/courses/{subject}/{number}?term=`.
pub async fn course(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((subject, number)): Path<(String, String)>,
    Query(query): Query<TermQuery>,
) -> Result<Response, ApiError> {
    let code = CourseCode::new(&subject, &number).map_err(|_| invalid("code"))?;
    let term = resolve_term(&state.store, query.term).await?;
    let data_version = state.store.data_version(term).await?;
    let etag = catalog_etag(term, data_version, &format!("code={code}"));
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_COURSE, &etag));
    }
    let (course, sections) = state
        .store
        .course(term, &code)
        .await?
        .ok_or(ApiError::NotFound)?;
    let body = Fresh {
        data: course_view(term, course, sections),
        freshness: listing_freshness(&state.store, term).await?,
    };
    Ok(cached(&headers, MAX_AGE_COURSE, &etag, &body))
}

/// `GET /api/v1/courses?code=ELEC+303&code=STAT+310&term=`: up to fifty
/// codes at once, each with an `offered` flag; codes we hold no record for
/// are absent.
pub async fn courses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CodesQuery>,
) -> Result<Response, ApiError> {
    if query.code.is_empty() || query.code.len() > MAX_CODES {
        return Err(invalid("code"));
    }
    let mut codes = query
        .code
        .iter()
        .map(|c| CourseCode::parse(c).map_err(|_| invalid("code")))
        .collect::<Result<Vec<_>, _>>()?;
    sort_dedup(&mut codes);
    let term = resolve_term(&state.store, query.term).await?;
    let data_version = state.store.data_version(term).await?;
    let key = codes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let etag = catalog_etag(term, data_version, &format!("codes={key}"));
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_COURSE, &etag));
    }
    let views: Vec<CourseView> = state
        .store
        .courses_by_code(term, &codes)
        .await?
        .into_iter()
        .map(|(course, sections, _)| course_view(term, course, sections))
        .collect();
    let body = Fresh {
        data: views,
        freshness: listing_freshness(&state.store, term).await?,
    };
    Ok(cached(&headers, MAX_AGE_COURSE, &etag, &body))
}

/// `GET /api/v1/seats?term=&crn=…`: one row per CRN asked for, `seats`
/// absent outside the poll set. The threshold follows the poll window.
pub async fn seats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SeatsQuery>,
) -> Result<Response, ApiError> {
    if query.crn.len() > MAX_CRNS {
        return Err(invalid("crn"));
    }
    let mut crns: Vec<Crn> = query.crn.iter().map(|c| Crn(*c)).collect();
    sort_dedup(&mut crns);
    let term = resolve_term(&state.store, query.term).await?;
    let polled = state.store.seats(term, &crns).await?;
    let rice_as_of = polled
        .iter()
        .map(|(_, s)| s.as_of.0)
        .max()
        .and_then(|secs| OffsetDateTime::from_unix_timestamp(secs).ok());
    let key = crns
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let version = rice_as_of.map_or(0, OffsetDateTime::unix_timestamp);
    let etag = catalog_etag(term, version, &format!("crns={key}"));
    if etag_matches(&headers, &etag) {
        return Ok(not_modified(MAX_AGE_SEATS, &etag));
    }
    let rows: Vec<SeatRow> = crns
        .iter()
        .map(|crn| SeatRow {
            crn: *crn,
            seats: polled.iter().find(|(c, _)| c == crn).map(|(_, s)| *s),
        })
        .collect();
    let now = OffsetDateTime::now_utc();
    let in_window = state.store.poll_windows_active(term, now).await?.is_some();
    let stale_after: Duration = if in_window {
        SEATS_IN_WINDOW
    } else {
        SEATS_OUTSIDE_WINDOW
    };
    let body = Fresh {
        data: rows,
        freshness: freshness_for(
            &state.store,
            "seats",
            Some(term),
            DataSource::Seats,
            rice_as_of,
            stale_after,
        )
        .await?,
    };
    Ok(cached(&headers, MAX_AGE_SEATS, &etag, &body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::Day;

    #[test]
    fn validate_refuses_reversed_ranges_and_far_offsets() {
        let mut q = CatalogQuery {
            level_min: Some(400),
            level_max: Some(300),
            ..CatalogQuery::default()
        };
        assert!(matches!(q.validate(), Err(ApiError::Invalid(f)) if f == "levelMin"));
        let mut q = CatalogQuery {
            offset: 10_001,
            ..CatalogQuery::default()
        };
        assert!(matches!(q.validate(), Err(ApiError::Invalid(f)) if f == "offset"));
        let mut q = CatalogQuery {
            limit: 500,
            q: Some("  ".into()),
            subject: vec![
                skyspace_core::code::Subject::new("MATH").unwrap(),
                skyspace_core::code::Subject::new("COMP").unwrap(),
                skyspace_core::code::Subject::new("COMP").unwrap(),
            ],
            ..CatalogQuery::default()
        };
        q.validate().unwrap();
        assert_eq!(q.limit, MAX_LIMIT);
        assert_eq!(q.q, None);
        assert_eq!(q.subject.len(), 2);
        assert_eq!(q.subject[0].as_str(), "COMP");
    }

    #[test]
    fn canonical_omits_defaults_and_sorts() {
        assert_eq!(CatalogQuery::default().canonical(), "");
        let q = CatalogQuery {
            q: Some("Comp".into()),
            attr: vec![Attribute::DistributionTwo],
            days: vec![Day::Wed, Day::Mon],
            scheduled_only: false,
            ..CatalogQuery::default()
        };
        assert_eq!(
            q.canonical(),
            "attr=GRP2&days=mon&days=wed&q=comp&scheduledOnly=false"
        );
    }

    #[test]
    fn level_range_becomes_hundreds() {
        let term = TermCode::parse("202710").unwrap();
        let q = CatalogQuery {
            level_min: Some(300),
            level_max: Some(499),
            days: vec![Day::Mon, Day::Wed],
            ..CatalogQuery::default()
        };
        let store = q.to_store_query(term);
        assert_eq!(store.level, vec![300, 400]);
        assert_eq!(store.day_mask, Some(0b101));
        let q = CatalogQuery {
            level: vec![100, 400],
            level_min: Some(300),
            ..CatalogQuery::default()
        };
        assert_eq!(q.to_store_query(term).level, vec![400]);
    }

    #[test]
    fn drop_filters_names_each_active_filter() {
        let q = CatalogQuery {
            q: Some("x".into()),
            open_seats_only: true,
            ..CatalogQuery::default()
        };
        let fields: Vec<_> = q.drop_filters().into_iter().map(|(f, _)| f).collect();
        assert_eq!(fields, vec!["q", "openSeatsOnly", "scheduledOnly"]);
    }
}
