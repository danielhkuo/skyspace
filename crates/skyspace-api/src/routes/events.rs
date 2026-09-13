//! Anonymous usage counters: a name from a closed list, the term, and
//! whether a search was empty. No account, session or address.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use crate::dto::EventBody;
use crate::error::ApiError;
use crate::state::AppState;

/// `POST /api/v1/events`. Unknown names never reach here: serde rejects them.
pub async fn record(
    State(state): State<AppState>,
    Json(body): Json<EventBody>,
) -> Result<StatusCode, ApiError> {
    let name = serde_json::to_value(body.name)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .ok_or_else(|| ApiError::Invalid("name".to_owned()))?;
    let detail = match body.empty {
        Some(empty) => serde_json::json!({ "empty": empty }),
        None => serde_json::json!({}),
    };
    state.store.record_event(&name, body.term, detail).await?;
    Ok(StatusCode::NO_CONTENT)
}
