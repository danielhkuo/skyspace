//! Every endpoint, declared once in a table the router is built from and a
//! test walks. Handlers parse and authorise; the store runs SQL; core decides.
#![allow(
    clippy::missing_errors_doc,
    reason = "every handler fails with `ApiError`, whose table is in `error.rs`"
)]

use std::time::Duration;

use axum::Router;
use axum::http::header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{from_fn, from_fn_with_state, map_response};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use sha2::{Digest, Sha256};
use skyspace_core::term::TermCode;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::dto::{DataSource, Freshness};
use crate::error::ApiError;
use crate::freshness::stamp;
use crate::guard;
use crate::state::AppState;

pub mod account;
pub mod catalog;
pub mod collections;
pub mod events;
pub mod meta;
pub mod plans;
pub mod programs;
pub mod schedules;

/// Whether a route needs a session. There is no third level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// Anyone.
    None,
    /// A signed-in account; `401` otherwise.
    Session,
}

/// One row of the endpoint table.
pub struct Endpoint {
    /// The method.
    pub method: Method,
    /// The path template, axum style: `/api/v1/plans/{id}`.
    pub path: &'static str,
    /// Whether the handler names `Session`.
    pub auth: Auth,
    service: MethodRouter<AppState>,
}

impl Endpoint {
    fn new(
        method: Method,
        path: &'static str,
        auth: Auth,
        service: MethodRouter<AppState>,
    ) -> Self {
        Self {
            method,
            path,
            auth,
            service,
        }
    }
}

/// The claim carries a guest's whole browser store, so it alone may be large.
pub const CLAIM_PATH: &str = "/api/v1/account/claim";
const BODY_LIMIT: usize = 256 * 1024;
const CLAIM_BODY_LIMIT: usize = 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Every endpoint under `/api/v1`, plus `/health`.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one table on purpose: a reader sees every route at once"
)]
pub fn endpoints() -> Vec<Endpoint> {
    use Auth::{None, Session};
    use axum::routing::{delete, get, patch, post, put};
    const GET: Method = Method::GET;
    const POST: Method = Method::POST;
    const PUT: Method = Method::PUT;
    const PATCH: Method = Method::PATCH;
    const DELETE: Method = Method::DELETE;
    vec![
        Endpoint::new(GET, "/health", None, get(meta::health)),
        Endpoint::new(GET, "/api/v1/meta", None, get(meta::meta)),
        Endpoint::new(GET, "/api/v1/reference", None, get(catalog::reference)),
        Endpoint::new(GET, "/api/v1/sections", None, get(catalog::sections)),
        Endpoint::new(GET, "/api/v1/sections/{crn}", None, get(catalog::section)),
        Endpoint::new(GET, "/api/v1/courses", None, get(catalog::courses)),
        Endpoint::new(
            GET,
            "/api/v1/courses/{subject}/{number}",
            None,
            get(catalog::course),
        ),
        Endpoint::new(GET, "/api/v1/seats", None, get(catalog::seats)),
        Endpoint::new(GET, "/api/v1/programs", None, get(programs::list)),
        Endpoint::new(GET, "/api/v1/programs/{id}", None, get(programs::one)),
        Endpoint::new(
            POST,
            "/api/v1/reports/rule",
            None,
            post(programs::report_rule),
        ),
        Endpoint::new(GET, "/api/v1/schedules", Session, get(schedules::list)),
        Endpoint::new(POST, "/api/v1/schedules", Session, post(schedules::create)),
        Endpoint::new(GET, "/api/v1/schedules/{id}", Session, get(schedules::one)),
        Endpoint::new(PUT, "/api/v1/schedules/{id}", Session, put(schedules::save)),
        Endpoint::new(
            DELETE,
            "/api/v1/schedules/{id}",
            Session,
            delete(schedules::remove),
        ),
        Endpoint::new(GET, "/api/v1/plans", Session, get(plans::list)),
        Endpoint::new(POST, "/api/v1/plans", Session, post(plans::create)),
        Endpoint::new(GET, "/api/v1/plans/{id}", Session, get(plans::one)),
        Endpoint::new(PUT, "/api/v1/plans/{id}", Session, put(plans::save)),
        Endpoint::new(
            POST,
            "/api/v1/plans/{id}/duplicate",
            Session,
            post(plans::duplicate),
        ),
        Endpoint::new(DELETE, "/api/v1/plans/{id}", Session, delete(plans::remove)),
        Endpoint::new(
            GET,
            "/api/v1/plans/{id}/bundle",
            Session,
            get(plans::bundle),
        ),
        Endpoint::new(
            GET,
            "/api/v1/plans/{id}/report",
            Session,
            get(plans::report),
        ),
        Endpoint::new(GET, "/api/v1/collections", Session, get(collections::list)),
        Endpoint::new(
            POST,
            "/api/v1/collections",
            Session,
            post(collections::create),
        ),
        Endpoint::new(
            PATCH,
            "/api/v1/collections/{id}",
            Session,
            patch(collections::rename),
        ),
        Endpoint::new(
            DELETE,
            "/api/v1/collections/{id}",
            Session,
            delete(collections::remove),
        ),
        Endpoint::new(
            PUT,
            "/api/v1/collections/{id}/courses/{subject}/{number}",
            Session,
            put(collections::add_course),
        ),
        Endpoint::new(
            DELETE,
            "/api/v1/collections/{id}/courses/{subject}/{number}",
            Session,
            delete(collections::remove_course),
        ),
        Endpoint::new(GET, "/api/v1/auth/methods", None, get(account::methods)),
        Endpoint::new(
            POST,
            "/api/v1/auth/email/request",
            None,
            post(account::email_request),
        ),
        Endpoint::new(
            POST,
            "/api/v1/auth/email/verify",
            None,
            post(account::email_verify),
        ),
        Endpoint::new(
            GET,
            "/api/v1/auth/sso/complete",
            None,
            get(account::sso_complete),
        ),
        Endpoint::new(POST, "/api/v1/auth/logout", Session, post(account::logout)),
        Endpoint::new(GET, "/api/v1/account", Session, get(account::view)),
        Endpoint::new(PATCH, "/api/v1/account", Session, patch(account::patch)),
        Endpoint::new(POST, CLAIM_PATH, Session, post(account::claim)),
        Endpoint::new(DELETE, "/api/v1/account", Session, delete(account::remove)),
        Endpoint::new(POST, "/api/v1/events", None, post(events::record)),
    ]
}

/// Routes that set or need a cookie are never cached by anything.
fn private(endpoint: &Endpoint) -> bool {
    endpoint.auth == Auth::Session || endpoint.path.starts_with("/api/v1/auth/")
}

fn collect(endpoints: Vec<Endpoint>) -> Router<AppState> {
    endpoints
        .into_iter()
        .fold(Router::new(), |router, e| router.route(e.path, e.service))
}

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

/// The guard and compression sit inside the body limit, so the three are
/// applied per group and the limit can differ for the claim.
fn limited(routes: Router<AppState>, state: &AppState, bytes: usize) -> Router<AppState> {
    routes
        .layer(CompressionLayer::new().compress_when(guard::SuccessOnly))
        .layer(from_fn_with_state(state.clone(), guard::csrf))
        .layer(RequestBodyLimitLayer::new(bytes))
}

/// The whole service. Tests call this; no socket is needed.
pub fn router(state: AppState) -> Router {
    let (claim, rest): (Vec<_>, Vec<_>) =
        endpoints().into_iter().partition(|e| e.path == CLAIM_PATH);
    let (private_routes, public_routes): (Vec<_>, Vec<_>) = rest.into_iter().partition(private);
    let private_routes = collect(private_routes).layer(map_response(no_store));
    let claim = collect(claim).layer(map_response(no_store));
    let api = limited(
        collect(public_routes).merge(private_routes),
        &state,
        BODY_LIMIT,
    )
    .merge(limited(claim, &state, CLAIM_BODY_LIMIT));
    // Innermost first: each `layer` call wraps everything added before it.
    api.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        REQUEST_TIMEOUT,
    ))
    .layer(CatchPanicLayer::new())
    .layer(TraceLayer::new_for_http())
    .layer(from_fn(guard::error_body))
    .layer(PropagateRequestIdLayer::x_request_id())
    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
    .with_state(state)
}

/// The term a catalog request is about: the one named when it is held,
/// else the current one. `400 term` when neither exists.
pub async fn resolve_term(
    store: &skyspace_store::Store,
    wanted: Option<TermCode>,
) -> Result<TermCode, ApiError> {
    match wanted {
        Some(term) => {
            let held = store.terms().await?.iter().any(|t| t.code == term);
            held.then_some(term)
                .ok_or_else(|| ApiError::Invalid("term".to_owned()))
        }
        None => store
            .current_term()
            .await?
            .ok_or_else(|| ApiError::Invalid("term".to_owned())),
    }
}

/// The stamp for one job's data. `stale` is true past the job's own
/// threshold since the last good run, and also when the most recent run of
/// the job failed: the number is served, but it is older than it should be.
pub async fn freshness_for(
    store: &skyspace_store::Store,
    job: &str,
    term: Option<TermCode>,
    source: DataSource,
    rice_as_of: Option<time::OffsetDateTime>,
    stale_after: time::Duration,
) -> Result<Freshness, ApiError> {
    let now = time::OffsetDateTime::now_utc();
    let last_ok = store
        .last_ok_run(job, term)
        .await?
        .and_then(|r| r.finished_at);
    let mut freshness = stamp(source, last_ok, rice_as_of, stale_after, now);
    freshness.stale |= last_run_failed(store, job).await?;
    Ok(freshness)
}

/// Whether the most recent run of `job`, any term, ended badly.
pub async fn last_run_failed(store: &skyspace_store::Store, job: &str) -> Result<bool, ApiError> {
    Ok(store
        .meta()
        .await?
        .jobs
        .iter()
        .find(|j| j.job == job)
        .is_some_and(|j| run_failed(j.last_run_outcome.as_deref())))
}

/// `failed` and `quarantined` both mean the catalog was not refreshed.
#[must_use]
pub fn run_failed(outcome: Option<&str>) -> bool {
    matches!(outcome, Some("failed" | "quarantined"))
}

/// `W/"{term}-{data_version}-{hash}"`: a finished pull moves `data_version`
/// and invalidates every catalog `ETag` for the term at once.
#[must_use]
pub fn catalog_etag(term: TermCode, data_version: i64, key: &str) -> String {
    format!("W/\"{term}-{data_version}-{}\"", short_hash(key))
}

/// The first 16 hex digits of the SHA-256 of `text`.
#[must_use]
pub fn short_hash(text: &str) -> String {
    crate::auth::hex(&Sha256::digest(text.as_bytes())[..8])
}

/// Whether the request's `If-None-Match` names this `ETag`.
#[must_use]
pub fn etag_matches(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(IF_NONE_MATCH)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .any(|candidate| candidate == etag || candidate == "*")
}

fn with_cache_headers(mut response: Response, max_age: u32, etag: &str) -> Response {
    let cache = HeaderValue::from_str(&format!("public, max-age={max_age}"))
        .unwrap_or_else(|_| HeaderValue::from_static("public"));
    let tag = HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("W/\"-\""));
    response.headers_mut().insert(CACHE_CONTROL, cache);
    response.headers_mut().insert(ETAG, tag);
    response
}

/// An empty `304` carrying the same cache headers the full response would.
#[must_use]
pub fn not_modified(max_age: u32, etag: &str) -> Response {
    with_cache_headers(StatusCode::NOT_MODIFIED.into_response(), max_age, etag)
}

/// A public, cacheable JSON response: `304` on a matching `If-None-Match`,
/// else the body with `Cache-Control` and `ETag`.
pub fn cached<T: serde::Serialize>(
    headers: &HeaderMap,
    max_age: u32,
    etag: &str,
    body: &T,
) -> Response {
    if etag_matches(headers, etag) {
        return not_modified(max_age, etag);
    }
    with_cache_headers(axum::Json(body).into_response(), max_age, etag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etags_are_weak_and_stable() {
        let term = TermCode::parse("202710").unwrap();
        let a = catalog_etag(term, 5, "q=comp");
        assert!(a.starts_with("W/\"202710-5-"));
        assert_eq!(a, catalog_etag(term, 5, "q=comp"));
        assert_ne!(a, catalog_etag(term, 6, "q=comp"));
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_str(&a).unwrap());
        assert!(etag_matches(&headers, &a));
        assert!(!etag_matches(&headers, "W/\"other\""));
    }

    #[test]
    fn every_endpoint_has_a_distinct_method_and_path() {
        let all = endpoints();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(
                    !(a.method == b.method && a.path == b.path),
                    "{} {} listed twice",
                    a.method,
                    a.path
                );
            }
        }
        assert!(all.iter().any(|e| e.path == CLAIM_PATH));
    }
}
