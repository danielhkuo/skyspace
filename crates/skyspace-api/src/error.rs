//! One error type, one status and one body for everything.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::dto::{ErrorBody, ErrorCode};

/// The kind of conflict a `409` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Another tab wrote first.
    StaleVersion,
    /// Cannot delete the last plan.
    LastPlan,
    /// A collection with that name exists.
    DuplicateName,
}

/// Everything a handler can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// Names the field; safe to show.
    #[error("{0}")]
    Invalid(String),
    /// No valid session cookie.
    #[error("not signed in")]
    Unauthenticated,
    /// The row does not exist, or belongs to someone else.
    #[error("not found")]
    NotFound,
    /// A product rule refused the write.
    #[error("conflict")]
    Conflict(ConflictKind),
    /// Too many attempts; the body says when to retry.
    #[error("too many attempts")]
    RateLimited {
        /// Seconds until the window opens.
        retry_after_seconds: u32,
    },
    /// Anything else. Logged with its source chain; never shown.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    /// The status the variant maps to.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Unauthenticated => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The code the client switches on.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Invalid(_) => ErrorCode::InvalidRequest,
            Self::Unauthenticated => ErrorCode::Unauthenticated,
            Self::NotFound => ErrorCode::NotFound,
            Self::Conflict(ConflictKind::StaleVersion) => ErrorCode::StaleVersion,
            Self::Conflict(ConflictKind::LastPlan) => ErrorCode::LastPlan,
            Self::Conflict(ConflictKind::DuplicateName) => ErrorCode::DuplicateName,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Internal(_) => ErrorCode::Internal,
        }
    }

    /// The one-sentence message. Never a database message.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Invalid(field) => field.clone(),
            Self::Unauthenticated => "Sign in to continue.".to_owned(),
            Self::NotFound => "Not found.".to_owned(),
            Self::Conflict(ConflictKind::StaleVersion) => "This changed in another tab.".to_owned(),
            Self::Conflict(ConflictKind::LastPlan) => {
                "Create another plan before deleting this one.".to_owned()
            }
            Self::Conflict(ConflictKind::DuplicateName) => {
                "A collection with that name already exists.".to_owned()
            }
            Self::RateLimited {
                retry_after_seconds,
            } => format!("Try again in {} minutes.", retry_after_seconds.div_ceil(60)),
            Self::Internal(_) => "Something went wrong.".to_owned(),
        }
    }

    /// Build the body with the request id the middleware supplies.
    #[must_use]
    pub fn body(&self, request_id: &str) -> ErrorBody {
        ErrorBody {
            code: self.code(),
            message: self.message(),
            request_id: request_id.to_owned(),
            retry_after_seconds: match self {
                Self::RateLimited {
                    retry_after_seconds,
                } => Some(*retry_after_seconds),
                _ => None,
            },
        }
    }
}

impl IntoResponse for ApiError {
    /// The request id is filled in by `guard::error_body` from the
    /// `x-request-id` header; here it is a placeholder.
    fn into_response(self) -> Response {
        if let Self::Internal(error) = &self {
            tracing::error!(error = ?error, "request failed");
        }
        let status = self.status();
        let body = self.body("");
        let mut response = (status, axum::Json(body)).into_response();
        if let Self::RateLimited {
            retry_after_seconds,
        } = self
            && let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, value);
        }
        response
    }
}

impl From<skyspace_store::StoreError> for ApiError {
    /// A store error is always `500`: a missing row is an `Option` turned
    /// into `NotFound` at the call site, so a query returning nothing for
    /// the wrong reason cannot quietly become a `404`.
    fn from(error: skyspace_store::StoreError) -> Self {
        Self::Internal(anyhow::Error::new(error))
    }
}

impl From<crate::state::MailError> for ApiError {
    /// A refused message is ours to fix, not the student's: `500`.
    fn from(error: crate::state::MailError) -> Self {
        Self::Internal(anyhow::Error::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_codes_match_the_table() {
        assert_eq!(
            ApiError::Invalid("term".into()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(ApiError::Unauthenticated.code(), ErrorCode::Unauthenticated);
        assert_eq!(
            ApiError::Conflict(ConflictKind::LastPlan).code(),
            ErrorCode::LastPlan
        );
        assert_eq!(
            ApiError::RateLimited {
                retry_after_seconds: 90
            }
            .message(),
            "Try again in 2 minutes."
        );
        assert_eq!(
            ApiError::Internal(anyhow::anyhow!("secret")).message(),
            "Something went wrong."
        );
    }
}
