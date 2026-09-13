//! Plans: the student's document, its bundle, and the server-side report.
//! Any number of terms is accepted; the only cap is an abuse guard.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use skyspace_core::evaluate::{PlanBundle, Report, evaluate};
use skyspace_core::plan::{Plan, PlanId, TermKind};
use skyspace_store::DeleteOutcome;
use uuid::Uuid;

use crate::dto::{PlanCreate, PlanDuplicate, PlanEnvelope, PlanSummary, PlanWrite};
use crate::error::{ApiError, ConflictKind};
use crate::session::Session;
use crate::state::AppState;

/// Terms one plan may hold: eight years of quarters is still under it.
pub const MAX_TERMS: usize = 40;
/// Cards one plan may hold, incoming credit included.
pub const MAX_CARDS: usize = 400;
const MAX_NAME_CHARS: usize = 200;

fn card_count(plan: &Plan) -> usize {
    plan.incoming_credit.len()
        + plan
            .terms
            .iter()
            .map(|t| match &t.kind {
                TermKind::Rice { courses, .. } => courses.len(),
                TermKind::Away { cards } => cards.len(),
                TermKind::Off => 0,
            })
            .sum::<usize>()
}

fn validate_name(name: &str) -> Result<(), ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::Invalid("name".to_owned()));
    }
    Ok(())
}

/// The abuse guard, naming the field. Nothing else about the document is
/// checked here: the engine reports on it.
fn validate(plan: &Plan) -> Result<(), ApiError> {
    validate_name(&plan.name)?;
    if plan.terms.len() > MAX_TERMS {
        return Err(ApiError::Invalid("terms".to_owned()));
    }
    if card_count(plan) > MAX_CARDS {
        return Err(ApiError::Invalid("courses".to_owned()));
    }
    Ok(())
}

async fn envelope(
    state: &AppState,
    session: &Session,
    id: PlanId,
) -> Result<Json<PlanEnvelope>, ApiError> {
    let (version, updated_at, plan) = state
        .store
        .plan(session.account_id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(PlanEnvelope {
        version,
        updated_at,
        plan,
    }))
}

async fn load_bundle(
    state: &AppState,
    session: &Session,
    id: Uuid,
) -> Result<PlanBundle, ApiError> {
    state
        .store
        .plan_bundle(session.account_id, PlanId(id))
        .await?
        .ok_or(ApiError::NotFound)
}

/// `GET /api/v1/plans`: active first.
pub async fn list(
    session: Session,
    State(state): State<AppState>,
) -> Result<Json<Vec<PlanSummary>>, ApiError> {
    let rows = state.store.plans(session.account_id).await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| PlanSummary {
                id: r.id,
                name: r.name,
                catalog_year: r.catalog_year,
                is_active: r.is_active,
                version: r.version,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

/// `POST /api/v1/plans`: the store mints the id; the first plan is active.
pub async fn create(
    session: Session,
    State(state): State<AppState>,
    Json(body): Json<PlanCreate>,
) -> Result<Json<PlanEnvelope>, ApiError> {
    validate(&body.plan)?;
    let (id, _) = state
        .store
        .create_plan(session.account_id, &body.plan)
        .await?;
    envelope(&state, &session, id).await
}

/// `GET /api/v1/plans/{id}`.
pub async fn one(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PlanEnvelope>, ApiError> {
    envelope(&state, &session, PlanId(id)).await
}

/// `PUT /api/v1/plans/{id}`: one statement guarded by the version; zero
/// rows updated is `409 stale_version`.
pub async fn save(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PlanWrite>,
) -> Result<Json<PlanEnvelope>, ApiError> {
    validate(&body.plan)?;
    let id = PlanId(id);
    if state.store.plan(session.account_id, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let (version, updated_at) = state
        .store
        .save_plan(session.account_id, id, body.version, &body.plan)
        .await?
        .ok_or(ApiError::Conflict(ConflictKind::StaleVersion))?;
    Ok(Json(PlanEnvelope {
        version,
        updated_at,
        plan: Plan { id, ..body.plan },
    }))
}

/// `POST /api/v1/plans/{id}/duplicate`.
pub async fn duplicate(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PlanDuplicate>,
) -> Result<Json<PlanEnvelope>, ApiError> {
    validate_name(&body.name)?;
    let (copy, _) = state
        .store
        .duplicate_plan(session.account_id, PlanId(id), body.name.trim())
        .await?
        .ok_or(ApiError::NotFound)?;
    envelope(&state, &session, copy).await
}

/// `DELETE /api/v1/plans/{id}`: `409 last_plan` for the account's last one.
pub async fn remove(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    match state
        .store
        .delete_plan(session.account_id, PlanId(id))
        .await?
    {
        DeleteOutcome::Deleted => Ok(StatusCode::NO_CONTENT),
        DeleteOutcome::NotFound => Err(ApiError::NotFound),
        DeleteOutcome::LastPlan => Err(ApiError::Conflict(ConflictKind::LastPlan)),
    }
}

/// `GET /api/v1/plans/{id}/bundle`: everything the browser's engine needs.
pub async fn bundle(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PlanBundle>, ApiError> {
    Ok(Json(load_bundle(&state, &session, id).await?))
}

/// `GET /api/v1/plans/{id}/report`: the engine over the stored plan, for
/// the first paint and the PDF.
pub async fn report(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Report>, ApiError> {
    let bundle = load_bundle(&state, &session, id).await?;
    Ok(Json(evaluate(&bundle)))
}
