//! Two middlewares and a compression predicate. The CSRF guard refuses a
//! mutating request a plain HTML form could send; the error rewriter turns
//! any non-JSON response of status 400 or more, from a layer or the router,
//! into the one `ErrorBody` the client parses everywhere.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::header::{CONTENT_TYPE, ORIGIN};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tower_http::compression::predicate::Predicate;

use crate::dto::{ErrorBody, ErrorCode};
use crate::error::ApiError;
use crate::state::AppState;

/// The header `SetRequestIdLayer` fills.
pub const REQUEST_ID: &str = "x-request-id";

/// How much of an error body the rewriter is willing to read. Every error
/// body of ours is a sentence; anything larger is replaced wholesale.
const MAX_ERROR_BODY: usize = 64 * 1024;

/// `Origin == config.public_origin` and `Content-Type: application/json` on
/// every `POST`, `PUT`, `PATCH` and `DELETE`, which a plain form cannot send.
pub async fn csrf(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !is_mutating(request.method()) {
        return next.run(request).await;
    }
    let origin_ok = request
        .headers()
        .get(ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|origin| origin == state.config.public_origin);
    if !origin_ok {
        return ApiError::Invalid("origin".to_owned()).into_response();
    }
    let json = request
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.trim_start().starts_with("application/json"));
    if !json {
        return ApiError::Invalid("contentType".to_owned()).into_response();
    }
    next.run(request).await
}

fn is_mutating(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Rewrite any response of status 400 or more into an `ErrorBody` carrying
/// the request id: a JSON body keeps its code and message and gains the id;
/// anything else gets a code derived from the status.
pub async fn error_body(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let response = next.run(request).await;
    if response.status().is_client_error() || response.status().is_server_error() {
        rewrite(response, &request_id).await
    } else {
        response
    }
}

async fn rewrite(response: Response, request_id: &str) -> Response {
    let (mut parts, body) = response.into_parts();
    let is_json = parts
        .headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/json"));
    let bytes = to_bytes(body, MAX_ERROR_BODY).await.unwrap_or_default();
    let body = if is_json {
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| fill_request_id(value, request_id))
            .unwrap_or_else(|| serialise(&from_status(parts.status, &bytes, request_id)))
    } else {
        serialise(&from_status(parts.status, &bytes, request_id))
    };
    parts
        .headers
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(body))
}

fn fill_request_id(mut value: serde_json::Value, request_id: &str) -> Option<Vec<u8>> {
    let object = value.as_object_mut()?;
    object.insert(
        "requestId".to_owned(),
        serde_json::Value::String(request_id.to_owned()),
    );
    serde_json::to_vec(&value).ok()
}

fn serialise(body: &ErrorBody) -> Vec<u8> {
    serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec())
}

/// The code and message a bare status maps to. A short text body from a
/// rejection ("missing field `name`") is kept as the message because it
/// names the field; nothing else is repeated.
#[must_use]
pub fn from_status(status: StatusCode, text: &[u8], request_id: &str) -> ErrorBody {
    let (code, default) = match status {
        StatusCode::REQUEST_TIMEOUT => (ErrorCode::Timeout, "The request took too long."),
        StatusCode::PAYLOAD_TOO_LARGE => (ErrorCode::PayloadTooLarge, "The request is too large."),
        StatusCode::NOT_FOUND => (ErrorCode::NotFound, "Not found."),
        StatusCode::UNAUTHORIZED => (ErrorCode::Unauthenticated, "Sign in to continue."),
        StatusCode::TOO_MANY_REQUESTS => (ErrorCode::RateLimited, "Try again later."),
        s if s.is_client_error() => (ErrorCode::InvalidRequest, "The request is malformed."),
        _ => (ErrorCode::Internal, "Something went wrong."),
    };
    let message = match code {
        ErrorCode::InvalidRequest => std::str::from_utf8(text)
            .ok()
            .map(str::trim)
            .filter(|t| !t.is_empty() && t.len() <= 300 && !t.starts_with('<'))
            .unwrap_or(default),
        _ => default,
    };
    ErrorBody {
        code,
        message: message.to_owned(),
        request_id: request_id.to_owned(),
        retry_after_seconds: None,
    }
}

/// Compress successful responses only, so the error rewriter outside the
/// compression layer always reads plain bytes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SuccessOnly;

impl Predicate for SuccessOnly {
    fn should_compress<B>(&self, response: &axum::http::Response<B>) -> bool
    where
        B: axum::body::HttpBody,
    {
        response.status().is_success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_map_to_codes() {
        assert_eq!(
            from_status(StatusCode::REQUEST_TIMEOUT, b"", "r").code,
            ErrorCode::Timeout
        );
        assert_eq!(
            from_status(StatusCode::PAYLOAD_TOO_LARGE, b"", "r").code,
            ErrorCode::PayloadTooLarge
        );
        assert_eq!(
            from_status(StatusCode::METHOD_NOT_ALLOWED, b"", "r").code,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            from_status(StatusCode::INTERNAL_SERVER_ERROR, b"secret", "r").message,
            "Something went wrong."
        );
        let body = from_status(StatusCode::BAD_REQUEST, b"missing field `name`", "r");
        assert_eq!(body.message, "missing field `name`");
        assert_eq!(body.request_id, "r");
    }

    #[test]
    fn json_bodies_gain_the_request_id() {
        let value = serde_json::json!({"code": "not_found", "message": "x", "requestId": ""});
        let out = fill_request_id(value, "abc").unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(parsed["requestId"], "abc");
        assert_eq!(parsed["code"], "not_found");
    }
}
