//! Sign-in, the account, and the guest claim. The emailed code and the
//! proxy-terminated SSO both end in one place: a minted session token
//! whose SHA-256 goes to Postgres and whose value goes in the cookie.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::header::{LOCATION, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use skyspace_core::plan::AccountId;
use skyspace_store::{
    ADDRESS_WINDOW, AccountRow, GuestCollectionInput, GuestScheduleInput, PutCodeOutcome,
    TakeCodeOutcome,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth::{
    clear_session_cookie, constant_time_eq, hash_login_code, mint_login_code, mint_session_token,
    session_cookie,
};
use crate::dto::{
    AccountPatch, AccountView, AuthMethods, ClaimRequest, ClaimResult, ClaimSkip, ClaimSkipReason,
    ClaimedId, EmailRequest, EmailVerify,
};
use crate::error::ApiError;
use crate::session::{Session, token_hash};
use crate::state::AppState;

/// Sends one address may trigger per hour.
pub const SENDS_PER_IP: u16 = 20;
const IP_WINDOW: Duration = Duration::hours(1);
/// Guest schedules one claim may land.
pub const MAX_CLAIM_SCHEDULES: usize = 20;
/// Guest collections one claim may land.
pub const MAX_CLAIM_COLLECTIONS: usize = 12;
/// Items one claimed document may hold.
pub const MAX_CLAIM_ITEMS: usize = 200;

/// The address the send limit counts. With `Config::trusted_proxy`, the
/// last `X-Forwarded-For` hop: the one our own proxy appended, so a client
/// cannot choose it. Otherwise the header is ignored (anyone can send one)
/// and the socket peer counts, else loopback (tests drive the router with
/// no socket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Ok(Self(
            client_ip(state.config.trusted_proxy, &parts.headers, peer)
                .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        ))
    }
}

/// The last `X-Forwarded-For` hop when the proxy is trusted, else `peer`.
/// A trusted proxy that sent no usable header still falls back to `peer`.
fn client_ip(trusted_proxy: bool, headers: &HeaderMap, peer: Option<IpAddr>) -> Option<IpAddr> {
    let forwarded = trusted_proxy
        .then(|| headers.get("x-forwarded-for"))
        .flatten()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.rsplit(',').next())
        .and_then(|v| v.trim().parse::<IpAddr>().ok());
    forwarded.or(peer)
}

fn normalise_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn account_view(row: AccountRow) -> AccountView {
    AccountView {
        id: AccountId(row.id),
        email: row.email,
        created_at: row.created_at,
    }
}

fn cookie_header(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Mint a session for the account and return the `Set-Cookie` value.
async fn open_session(state: &AppState, account: AccountId) -> Result<HeaderValue, ApiError> {
    let token = mint_session_token();
    let ttl = Duration::days(i64::from(state.config.session_ttl_days));
    let now = OffsetDateTime::now_utc();
    state
        .store
        .create_session(account, &token.sha256, now + ttl)
        .await?;
    Ok(cookie_header(&session_cookie(
        &token.cookie_value,
        ttl.whole_seconds(),
        secure(state),
    )))
}

fn secure(state: &AppState) -> bool {
    state.config.public_origin.starts_with("https://")
}

fn cleared(state: &AppState) -> HeaderValue {
    cookie_header(&clear_session_cookie(secure(state)))
}

/// `GET /api/v1/auth/methods`.
#[allow(clippy::unused_async, reason = "axum handlers are async functions")]
pub async fn methods(State(state): State<AppState>) -> Json<AuthMethods> {
    Json(AuthMethods {
        sso: state.config.sso.is_some(),
        email_code: true,
        allowed_email_domains: state.config.allowed_email_domains.clone(),
    })
}

/// `POST /api/v1/auth/email/request`: always `202`, so the route cannot
/// tell who has an account; `429` only for the send limits.
pub async fn email_request(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<EmailRequest>,
) -> Result<StatusCode, ApiError> {
    let email = normalise_email(&body.email);
    if !state.config.email_allowed(&email) {
        return Ok(StatusCode::ACCEPTED);
    }
    let now = OffsetDateTime::now_utc();
    if state
        .store
        .count_recent_ip_sends(ip, now, IP_WINDOW)
        .await?
        >= SENDS_PER_IP
    {
        return Err(ApiError::RateLimited {
            retry_after_seconds: retry_after(IP_WINDOW),
        });
    }
    let code = mint_login_code();
    let expires = now + Duration::minutes(i64::from(state.config.login_code_ttl_minutes));
    match state
        .store
        .put_login_code(&email, &hash_login_code(&email, &code), expires, now)
        .await?
    {
        PutCodeOutcome::Limited { window_started_at } => Err(ApiError::RateLimited {
            retry_after_seconds: retry_after(window_started_at + ADDRESS_WINDOW - now),
        }),
        PutCodeOutcome::Stored { .. } => {
            state.store.record_ip_send(ip, now, IP_WINDOW).await?;
            state.mailer.send_login_code(&email, &code).await?;
            Ok(StatusCode::ACCEPTED)
        }
    }
}

fn retry_after(left: Duration) -> u32 {
    u32::try_from(left.whole_seconds().max(1)).unwrap_or(u32::MAX)
}

/// `POST /api/v1/auth/email/verify`: the right code opens a session; a
/// wrong one counts, and the fifth burns the code.
pub async fn email_verify(
    State(state): State<AppState>,
    Json(body): Json<EmailVerify>,
) -> Result<Response, ApiError> {
    let email = normalise_email(&body.email);
    let now = OffsetDateTime::now_utc();
    let outcome = state
        .store
        .take_login_code(&email, &hash_login_code(&email, &body.code), now)
        .await?;
    match outcome {
        TakeCodeOutcome::Accepted => {}
        TakeCodeOutcome::Wrong { .. } | TakeCodeOutcome::Expired | TakeCodeOutcome::Missing => {
            return Err(ApiError::Invalid("code".to_owned()));
        }
    }
    let account = state.store.create_account(&email).await?;
    let cookie = open_session(&state, account.account_id()).await?;
    let mut response = Json(account_view(account)).into_response();
    response.headers_mut().insert(SET_COOKIE, cookie);
    Ok(response)
}

/// `GET /api/v1/auth/sso/complete`: trusts the proxy's identity header only
/// with the shared secret, and only when SSO is configured at all.
pub async fn sso_complete(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let sso = state.config.sso.as_ref().ok_or(ApiError::NotFound)?;
    let secret = headers
        .get(sso.secret_header.as_str())
        .map_or(&[][..], HeaderValue::as_bytes);
    if !constant_time_eq(secret, sso.shared_secret.as_bytes()) {
        return Err(ApiError::Unauthenticated);
    }
    let email = headers
        .get(sso.identity_header.as_str())
        .and_then(|v| v.to_str().ok())
        .map(normalise_email)
        .filter(|e| state.config.email_allowed(e))
        .ok_or_else(|| ApiError::Invalid("email".to_owned()))?;
    let account = state.store.create_account(&email).await?;
    let cookie = open_session(&state, account.account_id()).await?;
    let mut response = StatusCode::FOUND.into_response();
    response
        .headers_mut()
        .insert(LOCATION, HeaderValue::from_static("/"));
    response.headers_mut().insert(SET_COOKIE, cookie);
    Ok(response)
}

/// `POST /api/v1/auth/logout`: deletes the row, so a copied cookie is dead.
pub async fn logout(
    _session: Session,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(hash) = token_hash(&headers) {
        state.store.delete_session(&hash).await?;
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(SET_COOKIE, cleared(&state));
    Ok(response)
}

async fn load_view(state: &AppState, session: &Session) -> Result<Json<AccountView>, ApiError> {
    let row = state
        .store
        .account(session.account_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(account_view(row)))
}

/// `GET /api/v1/account`.
pub async fn view(
    session: Session,
    State(state): State<AppState>,
) -> Result<Json<AccountView>, ApiError> {
    load_view(&state, &session).await
}

/// `PATCH /api/v1/account`: nothing is editable at launch; the body only
/// rejects unknown fields.
pub async fn patch(
    session: Session,
    State(state): State<AppState>,
    Json(_): Json<AccountPatch>,
) -> Result<Json<AccountView>, ApiError> {
    load_view(&state, &session).await
}

/// `DELETE /api/v1/account`: plans, schedules, collections and sessions
/// cascade.
pub async fn remove(session: Session, State(state): State<AppState>) -> Result<Response, ApiError> {
    state.store.delete_account(session.account_id).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(SET_COOKIE, cleared(&state));
    Ok(response)
}

fn skip(client_id: Uuid, reason: ClaimSkipReason) -> ClaimSkip {
    ClaimSkip { client_id, reason }
}

fn claimed(row: skyspace_store::ClaimedRow) -> ClaimedId {
    ClaimedId {
        client_id: row.client_id,
        id: row.id,
        renamed_to: row.renamed_to,
    }
}

/// `POST /api/v1/account/claim`: limits are checked before any insert; an
/// over-limit document is skipped and named while the rest still land.
pub async fn claim(
    session: Session,
    State(state): State<AppState>,
    Json(body): Json<ClaimRequest>,
) -> Result<Json<ClaimResult>, ApiError> {
    let held = state.store.terms().await?;
    let mut skipped = Vec::new();
    let mut schedules = Vec::new();
    for (i, guest) in body.schedules.into_iter().enumerate() {
        let items = guest.schedule.candidates.len() + guest.schedule.busy.len();
        if i >= MAX_CLAIM_SCHEDULES || items > MAX_CLAIM_ITEMS {
            skipped.push(skip(guest.client_id, ClaimSkipReason::TooManyItems));
        } else if !held.iter().any(|t| t.code == guest.schedule.term) {
            skipped.push(skip(guest.client_id, ClaimSkipReason::TermNotLoaded));
        } else {
            schedules.push(GuestScheduleInput {
                client_id: guest.client_id,
                schedule: guest.schedule,
            });
        }
    }
    let mut collections = Vec::new();
    for (i, guest) in body.collections.into_iter().enumerate() {
        if i >= MAX_CLAIM_COLLECTIONS || guest.courses.len() > MAX_CLAIM_ITEMS {
            skipped.push(skip(guest.client_id, ClaimSkipReason::TooManyItems));
        } else {
            collections.push(GuestCollectionInput {
                client_id: guest.client_id,
                name: guest.name,
                courses: guest.courses,
            });
        }
    }
    let schedules = state
        .store
        .claim_schedules(session.account_id, &schedules)
        .await?;
    let collections = state
        .store
        .claim_collections(session.account_id, &collections)
        .await?;
    Ok(Json(ClaimResult {
        schedules: schedules.into_iter().map(claimed).collect(),
        collections: collections.into_iter().map(claimed).collect(),
        skipped,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(forwarded: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_str(forwarded).unwrap());
        headers
    }

    #[test]
    fn forwarded_for_is_read_only_behind_a_trusted_proxy_and_from_the_last_hop() {
        let peer = Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)));
        let spoofed = headers("9.9.9.9, 203.0.113.9");
        assert_eq!(client_ip(false, &spoofed, peer), peer);
        assert_eq!(
            client_ip(true, &spoofed, peer),
            Some(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)))
        );
        assert_eq!(client_ip(true, &headers("not an ip"), peer), peer);
        assert_eq!(client_ip(true, &HeaderMap::new(), None), None);
        assert_eq!(client_ip(false, &spoofed, None), None);
    }
}
