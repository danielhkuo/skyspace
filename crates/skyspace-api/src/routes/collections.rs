//! Saved-course collections. A bookmark is a course code, so it survives a
//! term where the course is not offered.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use skyspace_core::code::CourseCode;
use skyspace_core::plan::CollectionId;
use skyspace_store::{CollectionRow, StoreError};
use uuid::Uuid;

use crate::dto::{Collection, CollectionWrite};
use crate::error::{ApiError, ConflictKind};
use crate::session::Session;
use crate::state::AppState;

const MAX_NAME_CHARS: usize = 100;

fn validate_name(name: &str) -> Result<&str, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::Invalid("name".to_owned()));
    }
    Ok(name)
}

/// The store's `DuplicateName` is the one store error that is the
/// student's, not ours.
fn map_store(error: StoreError) -> ApiError {
    match error {
        StoreError::DuplicateName => ApiError::Conflict(ConflictKind::DuplicateName),
        other => other.into(),
    }
}

fn view(row: CollectionRow) -> Collection {
    Collection {
        id: row.id,
        name: row.name,
        courses: row.courses,
    }
}

fn parse_code(subject: &str, number: &str) -> Result<CourseCode, ApiError> {
    CourseCode::new(subject, number).map_err(|_| ApiError::Invalid("code".to_owned()))
}

/// `GET /api/v1/collections`.
pub async fn list(
    session: Session,
    State(state): State<AppState>,
) -> Result<Json<Vec<Collection>>, ApiError> {
    let rows = state.store.collections(session.account_id).await?;
    Ok(Json(rows.into_iter().map(view).collect()))
}

/// `POST /api/v1/collections`: `409 duplicate_name` on a taken name.
pub async fn create(
    session: Session,
    State(state): State<AppState>,
    Json(body): Json<CollectionWrite>,
) -> Result<Json<Collection>, ApiError> {
    let name = validate_name(&body.name)?;
    let row = state
        .store
        .create_collection(session.account_id, name, None)
        .await
        .map_err(map_store)?;
    Ok(Json(view(row)))
}

/// `PATCH /api/v1/collections/{id}`: rename.
pub async fn rename(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CollectionWrite>,
) -> Result<Json<Collection>, ApiError> {
    let name = validate_name(&body.name)?;
    let row = state
        .store
        .rename_collection(session.account_id, CollectionId(id), name)
        .await
        .map_err(map_store)?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(view(row)))
}

/// `DELETE /api/v1/collections/{id}`.
pub async fn remove(
    session: Session,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let gone = state
        .store
        .delete_collection(session.account_id, CollectionId(id))
        .await?;
    gone.then_some(StatusCode::NO_CONTENT)
        .ok_or(ApiError::NotFound)
}

/// `PUT /api/v1/collections/{id}/courses/{subject}/{number}`: idempotent.
pub async fn add_course(
    session: Session,
    State(state): State<AppState>,
    Path((id, subject, number)): Path<(Uuid, String, String)>,
) -> Result<StatusCode, ApiError> {
    let code = parse_code(&subject, &number)?;
    let owned = state
        .store
        .add_course(session.account_id, CollectionId(id), &code)
        .await?;
    owned
        .then_some(StatusCode::NO_CONTENT)
        .ok_or(ApiError::NotFound)
}

/// `DELETE /api/v1/collections/{id}/courses/{subject}/{number}`.
pub async fn remove_course(
    session: Session,
    State(state): State<AppState>,
    Path((id, subject, number)): Path<(Uuid, String, String)>,
) -> Result<StatusCode, ApiError> {
    let code = parse_code(&subject, &number)?;
    let owned = state
        .store
        .remove_course(session.account_id, CollectionId(id), &code)
        .await?;
    owned
        .then_some(StatusCode::NO_CONTENT)
        .ok_or(ApiError::NotFound)
}
