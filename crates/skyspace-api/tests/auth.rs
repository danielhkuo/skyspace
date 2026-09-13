#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The emailed-code flow: always `202`, five wrong attempts burn a code,
//! the send limits answer `429` with `Retry-After`, a verified code opens a
//! session, and sign-out closes it.

mod common;

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, RETRY_AFTER, SET_COOKIE};
use axum::http::{Method, Request, StatusCode};
use common::{app, call, cookie_pair, get, json, send, sign_in};
use serde_json::json;

async fn request_code(app: &axum::Router, email: &str) -> axum::http::Response<Body> {
    call(
        app,
        Method::POST,
        "/api/v1/auth/email/request",
        None,
        Some(&json!({ "email": email })),
    )
    .await
}

async fn verify(app: &axum::Router, email: &str, code: &str) -> axum::http::Response<Body> {
    call(
        app,
        Method::POST,
        "/api/v1/auth/email/verify",
        None,
        Some(&json!({ "email": email, "code": code })),
    )
    .await
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn email_request_always_answers_202(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    let allowed = request_code(&app, "Owl@Rice.edu").await;
    assert_eq!(allowed.status(), StatusCode::ACCEPTED);
    assert_eq!(
        allowed.headers().get(CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    let outside = request_code(&app, "someone@gmail.com").await;
    assert_eq!(outside.status(), StatusCode::ACCEPTED);
    let nonsense = request_code(&app, "not an address").await;
    assert_eq!(nonsense.status(), StatusCode::ACCEPTED);
    let sent = state.mailer.captured();
    assert_eq!(sent.len(), 1, "only the allowed domain is mailed");
    assert_eq!(sent[0].to, "owl@rice.edu");
    assert_eq!(sent[0].code.len(), 6);
    let methods = json(get(&app, "/api/v1/auth/methods", None).await).await;
    assert_eq!(methods["sso"], false);
    assert_eq!(methods["emailCode"], true);
    assert_eq!(methods["allowedEmailDomains"], json!(["rice.edu"]));
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn five_wrong_codes_burn_the_right_one(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    assert_eq!(
        request_code(&app, "owl@rice.edu").await.status(),
        StatusCode::ACCEPTED
    );
    let code = state.mailer.captured()[0].code.clone();
    let wrong = if code == "000000" { "111111" } else { "000000" };
    for _ in 0..5 {
        let response = verify(&app, "owl@rice.edu", wrong).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json(response).await;
        assert_eq!(body["code"], "invalid_request");
        assert_eq!(body["message"], "code");
    }
    let burnt = verify(&app, "owl@rice.edu", &code).await;
    assert_eq!(burnt.status(), StatusCode::BAD_REQUEST);
    assert!(burnt.headers().get(SET_COOKIE).is_none());
    // A fresh code works, and four wrong tries before it do not burn it.
    request_code(&app, "owl@rice.edu").await;
    let code = state.mailer.captured()[1].code.clone();
    for _ in 0..4 {
        assert_eq!(
            verify(&app, "owl@rice.edu", wrong).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        verify(&app, "owl@rice.edu", &code).await.status(),
        StatusCode::OK
    );
    // A code is single use.
    assert_eq!(
        verify(&app, "owl@rice.edu", &code).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn send_limits_answer_429_with_retry_after(pool: sqlx::PgPool) {
    let (app, _) = app(pool);
    for _ in 0..3 {
        assert_eq!(
            request_code(&app, "owl@rice.edu").await.status(),
            StatusCode::ACCEPTED
        );
    }
    let fourth = request_code(&app, "owl@rice.edu").await;
    assert_eq!(fourth.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u32 = fourth
        .headers()
        .get(RETRY_AFTER)
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=900).contains(&retry), "{retry}");
    let body = json(fourth).await;
    assert_eq!(body["code"], "rate_limited");
    assert_eq!(body["retryAfterSeconds"], retry);

    // Twenty sends from one address in an hour, across many mailboxes.
    for i in 0..6 {
        for _ in 0..3 {
            let response = send(
                &app,
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/email/request")
                    .header("origin", common::ORIGIN_VALUE)
                    .header("content-type", "application/json")
                    .header("x-forwarded-for", "203.0.113.9")
                    .body(Body::from(
                        serde_json::to_vec(&json!({ "email": format!("s{i}@rice.edu") })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::ACCEPTED, "send {i}");
        }
    }
    for (n, expected) in [
        (0, StatusCode::ACCEPTED),
        (0, StatusCode::ACCEPTED),
        (0, StatusCode::TOO_MANY_REQUESTS),
    ] {
        let response = send(
            &app,
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/email/request")
                .header("origin", common::ORIGIN_VALUE)
                .header("content-type", "application/json")
                .header("x-forwarded-for", "203.0.113.9")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "email": format!("t{n}@rice.edu") })).unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), expected);
        if expected == StatusCode::TOO_MANY_REQUESTS {
            assert!(response.headers().contains_key(RETRY_AFTER));
        }
    }
    // Another address is not affected.
    assert_eq!(
        request_code(&app, "elsewhere@rice.edu").await.status(),
        StatusCode::ACCEPTED
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn verify_sets_the_cookie_and_logout_clears_it(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    request_code(&app, "Owl@rice.edu").await;
    let code = state.mailer.captured()[0].code.clone();
    let verified = verify(&app, "owl@rice.edu", &code).await;
    assert_eq!(verified.status(), StatusCode::OK);
    let set_cookie = verified
        .headers()
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(set_cookie.starts_with("skyspace_session="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Lax"));
    assert!(set_cookie.contains("Path=/"));
    assert!(
        !set_cookie.contains("Secure"),
        "the test origin is plain http"
    );
    let cookie = cookie_pair(&verified);
    let body = json(verified).await;
    assert_eq!(body["email"], "owl@rice.edu");

    let account = get(&app, "/api/v1/account", Some(&cookie)).await;
    assert_eq!(account.status(), StatusCode::OK);
    assert_eq!(
        account.headers().get(CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    let view = json(account).await;
    assert_eq!(view["email"], "owl@rice.edu");
    assert_eq!(view["id"], body["id"]);

    // Signing in again is the same account with a new token.
    let again = sign_in(&app, &state, "owl@rice.edu").await;
    assert_ne!(again, cookie);
    assert_eq!(
        json(get(&app, "/api/v1/account", Some(&again)).await).await["id"],
        body["id"]
    );

    let logout = call(
        &app,
        Method::POST,
        "/api/v1/auth/logout",
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert!(
        logout
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let after = get(&app, "/api/v1/account", Some(&cookie)).await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
    // The other token is untouched.
    assert_eq!(
        get(&app, "/api/v1/account", Some(&again)).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn sso_is_absent_until_configured(pool: sqlx::PgPool) {
    let (app, _) = app(pool);
    let response = get(&app, "/api/v1/auth/sso/complete", None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
