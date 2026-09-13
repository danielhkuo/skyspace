//! Schedules: one replaced document each, sized but never inspected.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::extract::Query;
use skyspace_core::plan::ScheduleId;
use skyspace_core::schedule::TermSchedule;
use uuid::Uuid;

use crate::dto::{ScheduleEnvelope, ScheduleSummary, ScheduleWrite};
use crate::error::{ApiError, ConflictKind};
use crate::routes::catalog::TermQuery;
use crate::routes::resolve_term;
use crate::session::Session;
use crate::state::AppState;

/// Candidates one schedule may hold; thirty hours of options is the point,
/// a thousand is abuse.
pub const MAX_CANDIDATES: usize = 200;
/// Busy blocks one schedule may hold.
pub const MAX_BUSY: usize = 100;
const MAX_NAME_CHARS: usize = 200;

/// The size checks: the server never reads the document beyond them.
fn validate(schedule: &TermSchedule) -> Result<(), ApiError> {
    let name = schedule.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::Invalid("name".to_owned()));
    }
    if schedule.candidates.len() > MAX_CANDIDATES {
        return Err(ApiError::Invalid("candidates".to_owned()));
    }
    if schedule.busy.len() > MAX_BUSY {
        return Err(ApiError::Invalid("busy".to_owned()));
    }
    Ok(())
}

/// The term must be held: `schedules.term_code` references `terms`.
async fn check_term(
    store: &skyspace_store::Store,
    schedule: &TermSchedule,
) -> Result<(), ApiError> {
    resolve_term(store, Some(schedule.term)).await.map(|_| ())
}

/// `GET /api/v1/schedules?term=`.
pub async fn list(
    session: Session,
    State(state): State<AppState>,
    Query(query): Query<TermQuery>,
) -> Result<Json<Vec<ScheduleSummary>>, ApiError> {
    let term = resolve_term(&state.store, query.term).await?;
    let rows = state.store.schedules(session.account_id, term).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ScheduleSummary {
                id: r.id,
                name: r.name,
                term: r.term,
                version: r.version,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

/// `POST /api/v1/schedules`: the store mints the id.
pub async fn create(
    session: Session,
    State(state): State<AppState>,
    Json(body): Json<ScheduleWrite>,
) -> Result<Json<ScheduleEnvelope>, ApiError> {
    validate(&body.schedule)?;
    check_term(&state.store, &body.schedule).await?;
    let (id, version, updated_at) = state
        .store
        .create_schedule(session.account_id, &body.schedule, None)
        .await?;
    Ok(Json(ScheduleEnvelope {
        id,
        version,
        updated_at,
        schedule: TermSchedule {
            id,
            ..body.schedule
        },
    }))
}

/// `GET /api/v1/schedules/{id}`.
pub async fn one(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduleEnvelope>, ApiError> {
    let id = ScheduleId(id);
    let (version, updated_at, schedule) = state
        .store
        .schedule(session.account_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ScheduleEnvelope {
        id,
        version,
        updated_at,
        schedule,
    }))
}

/// `PUT /api/v1/schedules/{id}`: replaces the document when `version` is
/// still current; `409 stale_version` otherwise.
pub async fn save(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ScheduleWrite>,
) -> Result<Json<ScheduleEnvelope>, ApiError> {
    let version = body
        .version
        .ok_or_else(|| ApiError::Invalid("version".to_owned()))?;
    validate(&body.schedule)?;
    check_term(&state.store, &body.schedule).await?;
    let id = ScheduleId(id);
    if state
        .store
        .schedule(session.account_id, id)
        .await?
        .is_none()
    {
        return Err(ApiError::NotFound);
    }
    let (version, updated_at) = state
        .store
        .save_schedule(session.account_id, id, version, &body.schedule)
        .await?
        .ok_or(ApiError::Conflict(ConflictKind::StaleVersion))?;
    Ok(Json(ScheduleEnvelope {
        id,
        version,
        updated_at,
        schedule: TermSchedule {
            id,
            ..body.schedule
        },
    }))
}

/// `DELETE /api/v1/schedules/{id}`.
pub async fn remove(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let gone = state
        .store
        .delete_schedule(session.account_id, ScheduleId(id))
        .await?;
    gone.then_some(StatusCode::NO_CONTENT)
        .ok_or(ApiError::NotFound)
}
