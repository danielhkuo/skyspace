//! `/health` for the deploy and `/api/v1/meta` for the browser's first paint.

use axum::Json;
use axum::extract::State;
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use time::OffsetDateTime;

use crate::dto::{HealthBody, MetaBody, TermSummary};
use crate::error::ApiError;
use crate::freshness::job_rows;
use crate::routes::run_failed;
use crate::state::AppState;

fn no_cache(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// `GET /health`: `ok` whenever the process answers; `database` reports a
/// trivial query.
pub async fn health(State(state): State<AppState>) -> Response {
    let database = state.store.current_term().await.is_ok();
    no_cache(
        Json(HealthBody {
            ok: true,
            database,
            engine_version: skyspace_core::ENGINE_VERSION.to_owned(),
        })
        .into_response(),
    )
}

/// `GET /api/v1/meta`: terms, the current one, and every job's age.
pub async fn meta(State(state): State<AppState>) -> Result<Response, ApiError> {
    let rows = state.store.meta().await?;
    let now = OffsetDateTime::now_utc();
    let last_ok: Vec<(String, Option<OffsetDateTime>)> = rows
        .jobs
        .iter()
        .map(|j| (j.job.clone(), j.last_ok))
        .collect();
    let body = MetaBody {
        current_term: rows.current_term,
        terms: rows
            .terms
            .into_iter()
            .map(|t| TermSummary {
                code: t.code,
                label: t.label,
                season: t.season,
                is_current: t.is_current,
            })
            .collect(),
        jobs: job_rows(&last_ok, now)
            .into_iter()
            .map(|mut job| {
                let last = rows.jobs.iter().find(|j| j.job == job.job);
                job.stale |= last.is_some_and(|j| run_failed(j.last_run_outcome.as_deref()));
                job
            })
            .collect(),
        engine_version: skyspace_core::ENGINE_VERSION.to_owned(),
    };
    Ok(no_cache(Json(body).into_response()))
}
