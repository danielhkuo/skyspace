#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Access rules: every plan, schedule and collection endpoint as the owner,
//! as another account, and anonymously; the router walk; the CSRF guard.

mod common;

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, ORIGIN};
use axum::http::{Method, Request, StatusCode};
use common::{app, call, get, json, plan, schedule, seed_catalog, send, sign_in};
use serde_json::json;
use skyspace_api::routes::{Auth, endpoints};

struct Owned {
    plan: String,
    schedule: String,
    collection: String,
}

/// Create one of each document as `cookie` and return their ids.
async fn create_documents(app: &axum::Router, cookie: &str) -> Owned {
    let plan = json(
        call(
            app,
            Method::POST,
            "/api/v1/plans",
            Some(cookie),
            Some(&json!({ "plan": plan("Mine", vec![], &["COMP 140"]) })),
        )
        .await,
    )
    .await;
    let schedule = json(
        call(
            app,
            Method::POST,
            "/api/v1/schedules",
            Some(cookie),
            Some(&json!({ "schedule": schedule("Fall", &[10001]) })),
        )
        .await,
    )
    .await;
    let collection = json(
        call(
            app,
            Method::POST,
            "/api/v1/collections",
            Some(cookie),
            Some(&json!({ "name": "Saved" })),
        )
        .await,
    )
    .await;
    Owned {
        plan: plan["plan"]["id"].as_str().unwrap().to_owned(),
        schedule: schedule["id"].as_str().unwrap().to_owned(),
        collection: collection["id"].as_str().unwrap().to_owned(),
    }
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn owner_other_and_anonymous(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let owner = sign_in(&app, &state, "owner@rice.edu").await;
    let other = sign_in(&app, &state, "other@rice.edu").await;
    let owned = create_documents(&app, &owner).await;
    // Give the other account a plan too, so its list is non-empty but
    // never shows the owner's.
    create_documents(&app, &other).await;

    let reads = [
        (Method::GET, format!("/api/v1/plans/{}", owned.plan), None),
        (
            Method::GET,
            format!("/api/v1/plans/{}/bundle", owned.plan),
            None,
        ),
        (
            Method::GET,
            format!("/api/v1/plans/{}/report", owned.plan),
            None,
        ),
        (
            Method::GET,
            format!("/api/v1/schedules/{}", owned.schedule),
            None,
        ),
        (
            Method::PATCH,
            format!("/api/v1/collections/{}", owned.collection),
            Some(json!({ "name": "Renamed" })),
        ),
        (
            Method::PUT,
            format!("/api/v1/collections/{}/courses/COMP/182", owned.collection),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/plans/{}/duplicate", owned.plan),
            Some(json!({ "name": "Copy" })),
        ),
        (
            Method::POST,
            format!("/api/v1/plans/{}/activate", owned.plan),
            None,
        ),
    ];
    for (method, uri, body) in &reads {
        let mine = call(&app, method.clone(), uri, Some(&owner), body.as_ref()).await;
        assert!(
            mine.status().is_success(),
            "{method} {uri} as owner: {}",
            mine.status()
        );
        assert_eq!(
            mine.headers().get(CACHE_CONTROL).unwrap(),
            "private, no-store",
            "{method} {uri}"
        );
        let theirs = call(&app, method.clone(), uri, Some(&other), body.as_ref()).await;
        assert_eq!(
            theirs.status(),
            StatusCode::NOT_FOUND,
            "{method} {uri} as other"
        );
        let nobody = call(&app, method.clone(), uri, None, body.as_ref()).await;
        assert_eq!(
            nobody.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} anonymous"
        );
        assert_eq!(json(nobody).await["code"], "unauthenticated");
    }

    let lists = json(get(&app, "/api/v1/plans", Some(&other)).await).await;
    assert!(
        lists
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"] != owned.plan.as_str()),
        "the other account's list shows the owner's plan"
    );
    let schedules = json(get(&app, "/api/v1/schedules?term=202710", Some(&other)).await).await;
    assert!(
        schedules
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["id"] != owned.schedule.as_str())
    );
    let collections = json(get(&app, "/api/v1/collections", Some(&other)).await).await;
    assert!(
        collections
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["id"] != owned.collection.as_str())
    );

    // Writes and deletes by the other account are misses, and leave the
    // owner's documents intact.
    let put = call(
        &app,
        Method::PUT,
        &format!("/api/v1/schedules/{}", owned.schedule),
        Some(&other),
        Some(&json!({ "version": 1, "schedule": schedule("Stolen", &[]) })),
    )
    .await;
    assert_eq!(put.status(), StatusCode::NOT_FOUND);
    for uri in [
        format!("/api/v1/plans/{}", owned.plan),
        format!("/api/v1/schedules/{}", owned.schedule),
        format!("/api/v1/collections/{}", owned.collection),
        format!("/api/v1/collections/{}/courses/COMP/182", owned.collection),
    ] {
        let theirs = call(&app, Method::DELETE, &uri, Some(&other), None).await;
        assert_eq!(
            theirs.status(),
            StatusCode::NOT_FOUND,
            "DELETE {uri} as other"
        );
    }
    let still = get(
        &app,
        &format!("/api/v1/schedules/{}", owned.schedule),
        Some(&owner),
    )
    .await;
    assert_eq!(json(still).await["schedule"]["name"], "Fall");
    let gone = call(
        &app,
        Method::DELETE,
        &format!("/api/v1/schedules/{}", owned.schedule),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(gone.status(), StatusCode::NO_CONTENT);
}

/// The hard-coded public list. The table's `Auth::None` rows must equal it,
/// so adding a public route is a deliberate edit here.
const PUBLIC: &[(&str, &str)] = &[
    ("GET", "/health"),
    ("GET", "/api/v1/meta"),
    ("GET", "/api/v1/reference"),
    ("GET", "/api/v1/sections"),
    ("GET", "/api/v1/sections/{crn}"),
    ("GET", "/api/v1/courses"),
    ("GET", "/api/v1/courses/{subject}/{number}"),
    ("GET", "/api/v1/seats"),
    ("GET", "/api/v1/programs"),
    ("GET", "/api/v1/programs/{id}"),
    ("POST", "/api/v1/reports/rule"),
    ("GET", "/api/v1/auth/methods"),
    ("POST", "/api/v1/auth/email/request"),
    ("POST", "/api/v1/auth/email/verify"),
    ("GET", "/api/v1/auth/sso/complete"),
    ("POST", "/api/v1/events"),
];

fn fill(path: &str) -> String {
    path.replace("{id}", "00000000-0000-0000-0000-000000000001")
        .replace("{crn}", "10001")
        .replace("{subject}", "COMP")
        .replace("{number}", "140")
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn every_route_outside_the_public_list_answers_401_without_a_cookie(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let table = endpoints();
    let listed: Vec<(String, &str)> = table
        .iter()
        .filter(|e| e.auth == Auth::None)
        .map(|e| (e.method.to_string(), e.path))
        .collect();
    let expected: Vec<(String, &str)> = PUBLIC.iter().map(|(m, p)| ((*m).to_owned(), *p)).collect();
    assert_eq!(listed, expected, "the public list changed");
    for endpoint in table.iter().filter(|e| e.auth == Auth::Session) {
        let uri = fill(endpoint.path);
        let response = call(&app, endpoint.method.clone(), &uri, None, Some(&json!({}))).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{} {uri} without a cookie",
            endpoint.method
        );
        let body = json(response).await;
        assert_eq!(body["code"], "unauthenticated");
        assert!(!body["requestId"].as_str().unwrap().is_empty());
    }
    // An expired or unknown cookie is the same as none.
    let stale = get(
        &app,
        "/api/v1/plans",
        Some("skyspace_session=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn csrf_guard_wants_origin_and_json(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owner@rice.edu").await;
    let body = serde_json::to_vec(&json!({ "name": "Saved" })).unwrap();
    let without_origin = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/collections")
            .header(CONTENT_TYPE, "application/json")
            .header("cookie", &cookie)
            .body(Body::from(body.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(without_origin.status(), StatusCode::BAD_REQUEST);
    let parsed = json(without_origin).await;
    assert_eq!(parsed["code"], "invalid_request");
    assert_eq!(parsed["message"], "origin");
    let wrong_origin = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/collections")
            .header(ORIGIN, "https://evil.example")
            .header(CONTENT_TYPE, "application/json")
            .header("cookie", &cookie)
            .body(Body::from(body.clone()))
            .unwrap(),
    )
    .await;
    assert_eq!(wrong_origin.status(), StatusCode::BAD_REQUEST);
    let form = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/collections")
            .header(ORIGIN, common::ORIGIN_VALUE)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("cookie", &cookie)
            .body(Body::from("name=Saved"))
            .unwrap(),
    )
    .await;
    assert_eq!(form.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(form).await["message"], "contentType");
    // GET needs neither.
    assert_eq!(
        get(&app, "/api/v1/collections", Some(&cookie))
            .await
            .status(),
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn layer_errors_are_json_too(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owner@rice.edu").await;
    let huge = vec![b' '; 300 * 1024];
    let too_large = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/collections")
            .header(ORIGIN, common::ORIGIN_VALUE)
            .header(CONTENT_TYPE, "application/json")
            .header("cookie", &cookie)
            .body(Body::from(huge))
            .unwrap(),
    )
    .await;
    assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json(too_large).await["code"], "payload_too_large");
    let malformed = send(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/v1/collections")
            .header(ORIGIN, common::ORIGIN_VALUE)
            .header(CONTENT_TYPE, "application/json")
            .header("cookie", &cookie)
            .body(Body::from("{\"nope\": 1}"))
            .unwrap(),
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = json(malformed).await;
    assert_eq!(body["code"], "invalid_request");
    assert!(body["message"].as_str().unwrap().contains("nope"));
    let wrong_method = call(&app, Method::DELETE, "/api/v1/meta", None, None).await;
    assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(json(wrong_method).await["code"], "invalid_request");
}
