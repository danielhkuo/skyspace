//! Published programs and the logged-out rule report.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum_extra::extract::Query;
use serde::Deserialize;
use skyspace_core::program::{CatalogYear, ProgramId};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::dto::{DataSource, Fresh, ProgramSummary, RequirementErrorReport};
use crate::error::ApiError;
use crate::freshness::threshold;
use crate::routes::{cached, freshness_for, short_hash};
use crate::state::AppState;

const MAX_AGE_PROGRAM: u32 = 3600;
const MAX_AGE_PROGRAM_LIST: u32 = 300;
/// Reports accepted per hour across everyone; counted on the table itself.
pub const REPORTS_PER_HOUR: u64 = 60;
const MAX_REPORT_CHARS: usize = 4000;

/// `?catalogYear=&credential=&q=` on the program list.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProgramsQuery {
    /// Defaults to the General Announcements edition the current term falls
    /// in (`TermCode::catalog_year`): 2026 for Fall 2026, not the academic year.
    pub catalog_year: Option<CatalogYear>,
    /// `BSCS`, `BA`; matched ignoring case.
    pub credential: Option<String>,
    /// A substring of the name, slug or credential.
    pub q: Option<String>,
}

/// `?catalogYear=` on one program.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct YearQuery {
    /// Defaults to the General Announcements edition the current term falls
    /// in (`TermCode::catalog_year`): 2026 for Fall 2026, not the academic year.
    pub catalog_year: Option<CatalogYear>,
}

async fn resolve_year(
    store: &skyspace_store::Store,
    wanted: Option<CatalogYear>,
) -> Result<CatalogYear, ApiError> {
    match wanted {
        Some(year) => Ok(year),
        None => store
            .current_term()
            .await?
            .map(|t| CatalogYear(t.catalog_year()))
            .ok_or_else(|| ApiError::Invalid("catalogYear".to_owned())),
    }
}

fn matches_filter(summary: &skyspace_store::ProgramSummaryRow, query: &ProgramsQuery) -> bool {
    let credential_ok = query
        .credential
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .is_none_or(|c| summary.credential.eq_ignore_ascii_case(c));
    let needle = query
        .q
        .as_deref()
        .map(|q| q.trim().to_lowercase())
        .filter(|q| !q.is_empty());
    let q_ok = needle.is_none_or(|q| {
        summary.name.to_lowercase().contains(&q)
            || summary.slug.to_lowercase().contains(&q)
            || summary.credential.to_lowercase().contains(&q)
    });
    credential_ok && q_ok
}

/// `GET /api/v1/programs?catalogYear=&credential=&q=`: every published
/// program for the year, with the years each is published for. The
/// `ETag` hashes the list itself: a publish for any year changes it.
pub async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProgramsQuery>,
) -> Result<Response, ApiError> {
    let year = resolve_year(&state.store, query.catalog_year).await?;
    let mut out = Vec::new();
    for row in state.store.programs(year).await? {
        if !matches_filter(&row, &query) {
            continue;
        }
        let catalog_years = state.store.program_versions(row.id).await?;
        out.push(ProgramSummary {
            id: row.id,
            slug: row.slug,
            kind: row.kind,
            name: row.name,
            credential: row.credential,
            catalog_years,
            total_credits: row.total_credits,
        });
    }
    let etag = format!(
        "W/\"programs-{}-{}\"",
        year.0,
        short_hash(&serde_json::to_string(&out).unwrap_or_default())
    );
    Ok(cached(&headers, MAX_AGE_PROGRAM_LIST, &etag, &out))
}

/// `GET /api/v1/programs/{id}?catalogYear=`: exactly that year's version.
pub async fn one(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<YearQuery>,
) -> Result<Response, ApiError> {
    let year = resolve_year(&state.store, query.catalog_year).await?;
    let program = state
        .store
        .program(ProgramId(id), year)
        .await?
        .ok_or(ApiError::NotFound)?;
    let etag = format!(
        "W/\"{id}-{}-{}\"",
        year.0,
        short_hash(&program.review.published_at.0.to_string())
    );
    let body = Fresh {
        data: program,
        freshness: freshness_for(
            &state.store,
            "requirements",
            None,
            DataSource::GeneralAnnouncements,
            None,
            threshold("requirements"),
        )
        .await?,
    };
    Ok(cached(&headers, MAX_AGE_PROGRAM, &etag, &body))
}

/// `POST /api/v1/reports/rule`: a complaint about an encoded rule, logged
/// out. The hourly limit is a count over the table, so it survives a restart.
pub async fn report_rule(
    State(state): State<AppState>,
    axum::Json(report): axum::Json<RequirementErrorReport>,
) -> Result<StatusCode, ApiError> {
    let message = report.message.trim();
    if message.is_empty() || message.chars().count() > MAX_REPORT_CHARS {
        return Err(ApiError::Invalid("message".to_owned()));
    }
    let now = OffsetDateTime::now_utc();
    let recent = state
        .store
        .recent_requirement_reports(now - Duration::hours(1))
        .await?;
    if recent >= REPORTS_PER_HOUR {
        return Err(ApiError::RateLimited {
            retry_after_seconds: 3600,
        });
    }
    state
        .store
        .record_requirement_report(
            report.requirement,
            report.program,
            report.catalog_year,
            message,
        )
        .await?;
    Ok(StatusCode::ACCEPTED)
}
