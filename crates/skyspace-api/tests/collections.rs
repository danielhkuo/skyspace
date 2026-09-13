#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Collections, schedules, the guest claim, account deletion, and events.

mod common;

use axum::http::header::SET_COOKIE;
use axum::http::{Method, StatusCode};
use common::{app, call, get, json, schedule, seed_catalog, sign_in};
use serde_json::json;
use uuid::Uuid;

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn collections_and_their_courses(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let created = call(
        &app,
        Method::POST,
        "/api/v1/collections",
        Some(&cookie),
        Some(&json!({ "name": "Maybe" })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = json(created).await;
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["courses"], json!([]));

    let duplicate = call(
        &app,
        Method::POST,
        "/api/v1/collections",
        Some(&cookie),
        Some(&json!({ "name": "maybe" })),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(json(duplicate).await["code"], "duplicate_name");

    let uri = format!("/api/v1/collections/{id}/courses/COMP/140");
    for _ in 0..2 {
        let put = call(&app, Method::PUT, &uri, Some(&cookie), None).await;
        assert_eq!(put.status(), StatusCode::NO_CONTENT, "PUT is idempotent");
    }
    let unheld = call(
        &app,
        Method::PUT,
        &format!("/api/v1/collections/{id}/courses/HIST/999"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        unheld.status(),
        StatusCode::NO_CONTENT,
        "a bookmark is a code, not a row"
    );
    let bad = call(
        &app,
        Method::PUT,
        &format!("/api/v1/collections/{id}/courses/C/1"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let list = json(get(&app, "/api/v1/collections", Some(&cookie)).await).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(
        list[0]["courses"],
        json!([{ "subject": "COMP", "number": "140" }, { "subject": "HIST", "number": "999" }])
    );

    let renamed = call(
        &app,
        Method::PATCH,
        &format!("/api/v1/collections/{id}"),
        Some(&cookie),
        Some(&json!({ "name": "Later" })),
    )
    .await;
    assert_eq!(json(renamed).await["name"], "Later");
    let removed = call(&app, Method::DELETE, &uri, Some(&cookie), None).await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let list = json(get(&app, "/api/v1/collections", Some(&cookie)).await).await;
    assert_eq!(list[0]["courses"].as_array().unwrap().len(), 1);
    let deleted = call(
        &app,
        Method::DELETE,
        &format!("/api/v1/collections/{id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        json(get(&app, "/api/v1/collections", Some(&cookie)).await).await,
        json!([])
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn schedules_round_trip_with_versions(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let created = call(
        &app,
        Method::POST,
        "/api/v1/schedules",
        Some(&cookie),
        Some(&json!({ "schedule": schedule("Draft", &[10001, 99999]) })),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let created = json(created).await;
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["version"], 1);
    assert_eq!(created["schedule"]["id"], id);
    assert_eq!(
        created["schedule"]["candidates"][0]["sections"],
        json!([10001, 99999]),
        "a CRN outside the term stays in the document"
    );
    let list = json(get(&app, "/api/v1/schedules?term=202710", Some(&cookie)).await).await;
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["term"], "202710");

    let mut edited = created["schedule"].clone();
    edited["name"] = json!("Final");
    let saved = call(
        &app,
        Method::PUT,
        &format!("/api/v1/schedules/{id}"),
        Some(&cookie),
        Some(&json!({ "version": 1, "schedule": edited })),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(json(saved).await["version"], 2);
    let stale = call(
        &app,
        Method::PUT,
        &format!("/api/v1/schedules/{id}"),
        Some(&cookie),
        Some(&json!({ "version": 1, "schedule": edited })),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(json(stale).await["code"], "stale_version");
    let no_version = call(
        &app,
        Method::PUT,
        &format!("/api/v1/schedules/{id}"),
        Some(&cookie),
        Some(&json!({ "schedule": edited })),
    )
    .await;
    assert_eq!(no_version.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(no_version).await["message"], "version");

    let mut unheld = schedule("Elsewhere", &[]);
    unheld.term = common::term("202720");
    let refused = call(
        &app,
        Method::POST,
        "/api/v1/schedules",
        Some(&cookie),
        Some(&json!({ "schedule": unheld })),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "term");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn the_claim_checks_limits_before_any_insert(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let taken = call(
        &app,
        Method::POST,
        "/api/v1/collections",
        Some(&cookie),
        Some(&json!({ "name": "Saved" })),
    )
    .await;
    assert_eq!(taken.status(), StatusCode::OK);

    let kept = Uuid::new_v4();
    let elsewhere = Uuid::new_v4();
    let heavy = Uuid::new_v4();
    let renamed = Uuid::new_v4();
    let mut spring = schedule("Spring", &[]);
    spring.term = common::term("202720");
    let mut big = schedule("Big", &[]);
    big.candidates = std::iter::repeat_n(big.candidates[0].clone(), 201).collect();
    let body = json!({
        "schedules": [
            { "clientId": kept, "schedule": schedule("Fall", &[10001]) },
            { "clientId": elsewhere, "schedule": spring },
            { "clientId": heavy, "schedule": big },
        ],
        "collections": [
            { "clientId": renamed, "name": "Saved", "courses": [{ "subject": "COMP", "number": "182" }] },
        ],
    });
    let first = call(
        &app,
        Method::POST,
        "/api/v1/account/claim",
        Some(&cookie),
        Some(&body),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first = json(first).await;
    assert_eq!(first["schedules"].as_array().unwrap().len(), 1);
    assert_eq!(first["schedules"][0]["clientId"], kept.to_string());
    assert_eq!(first["collections"][0]["renamedTo"], "Saved (2)");
    let skipped = first["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 2);
    assert!(
        skipped
            .iter()
            .any(|s| s["clientId"] == elsewhere.to_string() && s["reason"] == "term_not_loaded")
    );
    assert!(
        skipped
            .iter()
            .any(|s| s["clientId"] == heavy.to_string() && s["reason"] == "too_many_items")
    );

    // A retry returns the same ids and creates nothing new.
    let second = json(
        call(
            &app,
            Method::POST,
            "/api/v1/account/claim",
            Some(&cookie),
            Some(&body),
        )
        .await,
    )
    .await;
    assert_eq!(second["schedules"][0]["id"], first["schedules"][0]["id"]);
    assert_eq!(
        second["collections"][0]["id"],
        first["collections"][0]["id"]
    );
    assert!(second["collections"][0]["renamedTo"].is_null());
    let schedules = json(get(&app, "/api/v1/schedules?term=202710", Some(&cookie)).await).await;
    assert_eq!(schedules.as_array().unwrap().len(), 1);
    let collections = json(get(&app, "/api/v1/collections", Some(&cookie)).await).await;
    assert_eq!(collections.as_array().unwrap().len(), 2);

    // The claim alone accepts a body past 256 KiB.
    let padding = "x".repeat(400 * 1024);
    let large = json!({
        "schedules": [{ "clientId": Uuid::new_v4(), "schedule": {
            "id": Uuid::new_v4(), "name": padding, "term": "202710", "candidates": [], "busy": []
        }}],
        "collections": [],
    });
    let response = call(
        &app,
        Method::POST,
        "/api/v1/account/claim",
        Some(&cookie),
        Some(&large),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn deleting_the_account_cascades_and_clears_the_cookie(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    call(
        &app,
        Method::POST,
        "/api/v1/collections",
        Some(&cookie),
        Some(&json!({ "name": "Saved" })),
    )
    .await;
    let patched = call(
        &app,
        Method::PATCH,
        "/api/v1/account",
        Some(&cookie),
        Some(&json!({ "displayName": "Owl" })),
    )
    .await;
    assert_eq!(patched.status(), StatusCode::OK);
    let deleted = call(&app, Method::DELETE, "/api/v1/account", Some(&cookie), None).await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert!(
        deleted
            .headers()
            .get(SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        get(&app, "/api/v1/account", Some(&cookie)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let again = sign_in(&app, &state, "owl@rice.edu").await;
    assert_eq!(
        json(get(&app, "/api/v1/collections", Some(&again)).await).await,
        json!([]),
        "the new account starts empty"
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn events_take_only_known_names(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let ok = call(
        &app,
        Method::POST,
        "/api/v1/events",
        None,
        Some(&json!({ "name": "catalog_search", "term": "202710", "empty": true })),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);
    let unknown = call(
        &app,
        Method::POST,
        "/api/v1/events",
        None,
        Some(&json!({ "name": "netid_seen" })),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json(unknown).await["code"], "invalid_request");
    let identifying = call(
        &app,
        Method::POST,
        "/api/v1/events",
        None,
        Some(&json!({ "name": "class_view", "accountId": "x" })),
    )
    .await;
    assert_eq!(identifying.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let report = call(
        &app,
        Method::POST,
        "/api/v1/reports/rule",
        None,
        Some(&json!({ "requirement": null, "program": null, "catalogYear": 2027, "message": "Wrong hours." })),
    )
    .await;
    assert_eq!(report.status(), StatusCode::ACCEPTED);
    let empty = call(
        &app,
        Method::POST,
        "/api/v1/reports/rule",
        None,
        Some(&json!({ "requirement": null, "program": null, "catalogYear": null, "message": " " })),
    )
    .await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST);
}
