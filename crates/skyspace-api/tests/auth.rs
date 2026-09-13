#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! The emailed-code flow: always `202`, five wrong attempts burn a code,
//! the send limits answer `429` with `Retry-After`, a verified code opens a
//! session, and sign-out closes it.

mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::header::{CACHE_CONTROL, RETRY_AFTER, SET_COOKIE};
use axum::http::{Method, Request, StatusCode};
use common::{app, app_with, call, cookie_pair, get, json, send, sign_in};
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

    // Another address is not affected.
    assert_eq!(
        request_code(&app, "elsewhere@rice.edu").await.status(),
        StatusCode::ACCEPTED
    );
}

/// A code request from `peer` carrying `forwarded` as `X-Forwarded-For`.
async fn request_code_from(
    app: &axum::Router,
    email: &str,
    peer: Option<IpAddr>,
    forwarded: Option<&str>,
) -> StatusCode {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/email/request")
        .header("origin", common::ORIGIN_VALUE)
        .header("content-type", "application/json");
    if let Some(forwarded) = forwarded {
        builder = builder.header("x-forwarded-for", forwarded);
    }
    let mut request = builder
        .body(Body::from(
            serde_json::to_vec(&json!({ "email": email })).unwrap(),
        ))
        .unwrap();
    if let Some(peer) = peer {
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(peer, 40000)));
    }
    send(app, request).await.status()
}

/// Twenty-five requests from one peer, `i` in the address so the
/// per-address limit never fires and `forwarded(i)` as the header.
async fn statuses(
    app: &axum::Router,
    peer: IpAddr,
    forwarded: impl Fn(usize) -> String,
) -> Vec<StatusCode> {
    let mut out = Vec::new();
    for i in 0..25 {
        let email = format!("u{}-{i}@rice.edu", peer.to_string().replace('.', "-"));
        out.push(request_code_from(app, &email, Some(peer), Some(&forwarded(i))).await);
    }
    out
}

fn tally(statuses: &[StatusCode]) -> (usize, usize) {
    let accepted = statuses
        .iter()
        .filter(|s| **s == StatusCode::ACCEPTED)
        .count();
    let limited = statuses
        .iter()
        .filter(|s| **s == StatusCode::TOO_MANY_REQUESTS)
        .count();
    (accepted, limited)
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn forwarded_for_is_ignored_without_a_trusted_proxy(pool: sqlx::PgPool) {
    let (app, _) = app(pool);
    let peer = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));
    // A rotating header from one socket peer still hits the per-IP cap.
    let spoofed = statuses(&app, peer, |i| format!("9.9.9.{i}")).await;
    assert_eq!(tally(&spoofed), (20, 5), "{spoofed:?}");
    // The cap is keyed on the peer: another peer is unaffected, and no
    // header at all is the same peer.
    let other = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 8));
    assert_eq!(
        request_code_from(&app, "other@rice.edu", Some(other), Some("9.9.9.1")).await,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        request_code_from(&app, "same@rice.edu", Some(peer), None).await,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn a_trusted_proxy_supplies_the_client_as_the_last_forwarded_hop(pool: sqlx::PgPool) {
    let (app, _) = app_with(pool, |c| c.trusted_proxy = true);
    let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
    // The client-chosen first hop rotates; the proxy-appended last hop is
    // what counts, so the cap fires.
    let fixed_last = statuses(&app, peer, |i| format!("9.9.9.{i}, 203.0.113.9")).await;
    assert_eq!(tally(&fixed_last), (20, 5), "{fixed_last:?}");
    // A different last hop is a different client.
    assert_eq!(
        request_code_from(
            &app,
            "next@rice.edu",
            Some(peer),
            Some("9.9.9.1, 203.0.113.10")
        )
        .await,
        StatusCode::ACCEPTED
    );
    // A trusted proxy that sent no header falls back to the peer.
    assert_eq!(
        request_code_from(&app, "bare@rice.edu", Some(peer), None).await,
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
