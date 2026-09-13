//! The `Session` extractor: the only authentication mechanism. No auth
//! middleware and no `MaybeSession`; a handler that names `Session` needs
//! one, and a handler that does not is public.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::header::COOKIE;
use axum::http::request::Parts;
use skyspace_core::plan::AccountId;
use time::{Duration, OffsetDateTime};

use crate::auth::{cookie_from_header, hash_cookie_value};
use crate::error::ApiError;
use crate::state::AppState;

/// `last_seen_at` is written at most this often.
const TOUCH_INTERVAL: Duration = Duration::hours(1);

/// A signed-in account. Every per-account store call takes `account_id`
/// from here, never from the path, query or body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// Whose request this is.
    pub account_id: AccountId,
    /// The verified address.
    pub email: String,
}

/// The SHA-256 of the session cookie in `headers`, when one is present and
/// shaped like a token we minted.
#[must_use]
pub fn token_hash(headers: &HeaderMap) -> Option<[u8; 32]> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(cookie_from_header)
        .and_then(hash_cookie_value)
}

impl<S> FromRequestParts<S> for Session
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        let hash = token_hash(&parts.headers).ok_or(ApiError::Unauthenticated)?;
        let row = state
            .store
            .session_by_token(&hash)
            .await?
            .ok_or(ApiError::Unauthenticated)?;
        let now = OffsetDateTime::now_utc();
        if row.expires_at <= now {
            state.store.delete_session(&hash).await?;
            return Err(ApiError::Unauthenticated);
        }
        if now - row.last_seen_at >= TOUCH_INTERVAL {
            let ttl = Duration::days(i64::from(state.config.session_ttl_days));
            state.store.touch_session(&hash, now, now + ttl).await?;
        }
        Ok(Self {
            account_id: row.account_id,
            email: row.email,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::mint_session_token;
    use axum::http::HeaderValue;

    #[test]
    fn token_hash_reads_the_session_cookie_only() {
        let token = mint_session_token();
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("other=1"));
        assert_eq!(token_hash(&headers), None);
        headers.append(
            COOKIE,
            HeaderValue::from_str(&format!("skyspace_session={}", token.cookie_value)).unwrap(),
        );
        assert_eq!(token_hash(&headers), Some(token.sha256));
    }
}
