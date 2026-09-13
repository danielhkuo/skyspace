#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Route tests for the catalog: shape, the empty result, the unknown term,
//! the `ETag`, and the counts the rail shows.

mod common;

use axum::body::Body;
use axum::http::header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH};
use axum::http::{Method, Request, StatusCode};
use common::{app, get, json, seed_catalog, seed_program, send};

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn search_returns_sections_on_the_fixture_term(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let response = get(&app, "/api/v1/sections?q=COMP%20140&term=202710", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CACHE_CONTROL).unwrap(),
        "public, max-age=60"
    );
    assert!(response.headers().contains_key(ETAG));
    let body = json(response).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["rows"][0]["listing"]["crn"], 10001);
    assert_eq!(body["data"]["applied"]["term"], "202710");
    assert_eq!(body["data"]["applied"]["scheduledOnly"], true);
    assert_eq!(body["data"]["suggestions"], serde_json::json!([]));
    assert_eq!(body["freshness"]["source"], "section_listing");
    assert_eq!(body["freshness"]["stale"], false);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn term_defaults_to_the_current_one(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let body = json(get(&app, "/api/v1/sections", None).await).await;
    assert_eq!(body["data"]["applied"]["term"], "202710");
    assert_eq!(body["data"]["total"], 2);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn empty_result_carries_suggestions(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let response = get(&app, "/api/v1/sections?q=COMP%20182&subject=MATH", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["rows"], serde_json::json!([]));
    assert_eq!(body["data"]["hasMore"], false);
    let suggestions = body["data"]["suggestions"].as_array().unwrap();
    let fields: Vec<&str> = suggestions
        .iter()
        .map(|s| s["field"].as_str().unwrap())
        .collect();
    assert!(fields.contains(&"subject"), "{fields:?}");
    assert!(fields.contains(&"q"), "{fields:?}");
    assert!(
        suggestions
            .iter()
            .all(|s| s["wouldMatch"].as_u64().unwrap() > 0)
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn unknown_term_is_invalid_request(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let response = get(&app, "/api/v1/sections?term=202720", None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["code"], "invalid_request");
    assert_eq!(body["message"], "term");
    assert!(!body["requestId"].as_str().unwrap().is_empty());
    let malformed = get(&app, "/api/v1/sections?term=abc", None).await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(malformed).await["code"], "invalid_request");
    let reversed = get(&app, "/api/v1/sections?levelMin=400&levelMax=300", None).await;
    assert_eq!(json(reversed).await["message"], "levelMin");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn etag_answers_304_until_a_pull_moves_it(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let first = get(&app, "/api/v1/sections?subject=COMP", None).await;
    let etag = first
        .headers()
        .get(ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(etag.starts_with("W/\"202710-"));
    let again = send(
        &app,
        Request::builder()
            .uri("/api/v1/sections?subject=COMP")
            .header(IF_NONE_MATCH, &etag)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(again.headers().get(ETAG).unwrap(), etag.as_str());
    // A finished pull moves the data version for the term.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    common::finish_run(&state.store, "listings", skyspace_store::RunOutcome::Ok).await;
    let moved = send(
        &app,
        Request::builder()
            .uri("/api/v1/sections?subject=COMP")
            .header(IF_NONE_MATCH, &etag)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(moved.status(), StatusCode::OK);
    assert_ne!(moved.headers().get(ETAG).unwrap(), etag.as_str());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn scheduled_only_hides_untimed_rows_and_says_how_many(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let on = json(get(&app, "/api/v1/sections", None).await).await;
    assert_eq!(on["data"]["total"], 2);
    assert_eq!(on["data"]["unscheduledHidden"], 1);
    assert_eq!(on["data"]["courseCount"], 2);
    let off = json(get(&app, "/api/v1/sections?scheduledOnly=false", None).await).await;
    assert_eq!(off["data"]["total"], 3);
    assert_eq!(off["data"]["unscheduledHidden"], 0);
    let limited = json(get(&app, "/api/v1/sections?scheduledOnly=false&limit=2", None).await).await;
    assert_eq!(limited["data"]["rows"].as_array().unwrap().len(), 2);
    assert_eq!(limited["data"]["hasMore"], true);
    assert_eq!(limited["data"]["limit"], 2);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn single_section_and_course_routes(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let section = get(&app, "/api/v1/sections/10001?term=202710", None).await;
    assert_eq!(section.status(), StatusCode::OK);
    assert_eq!(
        section.headers().get(CACHE_CONTROL).unwrap(),
        "public, max-age=300"
    );
    assert_eq!(
        json(section).await["data"]["listing"]["code"]["subject"],
        "COMP"
    );
    let missing = get(&app, "/api/v1/sections/99999", None).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(missing).await["code"], "not_found");

    let course = json(get(&app, "/api/v1/courses/COMP/140", None).await).await;
    assert_eq!(course["data"]["course"]["title"], "Computational Thinking");
    assert_eq!(course["data"]["offered"], true);
    assert_eq!(course["data"]["sections"].as_array().unwrap().len(), 1);
    assert!(
        course["data"]["links"]["riceCoursePage"]
            .as_str()
            .unwrap()
            .contains("p_subj=COMP")
    );
    assert!(course["data"]["links"]["estherEvaluations"].is_null());
    let unheld = get(&app, "/api/v1/courses/COMP/100", None).await;
    assert_eq!(unheld.status(), StatusCode::NOT_FOUND);
    let bad = get(&app, "/api/v1/courses/C/x", None).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn multi_code_courses_flag_offered(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let body = json(
        get(
            &app,
            "/api/v1/courses?code=COMP+140&code=COMP+999&code=NOPE+100&term=202710",
            None,
        )
        .await,
    )
    .await;
    let views = body["data"].as_array().unwrap();
    assert_eq!(views.len(), 2);
    let offered: Vec<(String, bool)> = views
        .iter()
        .map(|v| {
            (
                v["course"]["code"]["number"].as_str().unwrap().to_owned(),
                v["offered"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        offered,
        vec![("140".to_owned(), true), ("999".to_owned(), false)]
    );
    let none = get(&app, "/api/v1/courses", None).await;
    assert_eq!(none.status(), StatusCode::BAD_REQUEST);
    let too_many = format!(
        "/api/v1/courses?{}",
        (0..51)
            .map(|i| format!("code=COMP+{i}"))
            .collect::<Vec<_>>()
            .join("&")
    );
    assert_eq!(
        get(&app, &too_many, None).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn seats_are_batch_only_and_mark_unpolled_rows(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let response = get(&app, "/api/v1/seats?term=202710&crn=30003&crn=10001", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let rows = body["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["crn"], 10001);
    assert!(rows[0]["seats"].is_null());
    assert_eq!(rows[1]["crn"], 30003);
    assert_eq!(rows[1]["seats"]["enrolled"], 10);
    assert_eq!(body["freshness"]["source"], "seats");
    assert_eq!(body["freshness"]["riceAsOf"], "2023-11-14T22:13:20Z");
    assert_eq!(body["freshness"]["stale"], true);
    let too_many = format!(
        "/api/v1/seats?{}",
        (0..201)
            .map(|i| format!("crn={i}"))
            .collect::<Vec<_>>()
            .join("&")
    );
    assert_eq!(
        get(&app, &too_many, None).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn reference_lists_what_the_term_holds(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let response = get(&app, "/api/v1/reference?term=202710", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CACHE_CONTROL).unwrap(),
        "public, max-age=3600"
    );
    let body = json(response).await;
    let subjects: Vec<&str> = body["data"]["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["code"].as_str().unwrap())
        .collect();
    assert_eq!(subjects, vec!["COMP", "MATH"]);
    assert_eq!(body["data"]["attributes"].as_array().unwrap().len(), 4);
    assert_eq!(body["freshness"]["source"], "reference");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn meta_and_health_answer_without_data(pool: sqlx::PgPool) {
    let (app, _) = app(pool);
    let health = get(&app, "/health", None).await;
    assert_eq!(health.status(), StatusCode::OK);
    let body = json(health).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["database"], true);
    let meta = get(&app, "/api/v1/meta", None).await;
    assert_eq!(meta.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    let body = json(meta).await;
    assert!(body["currentTerm"].is_null());
    assert_eq!(body["terms"], serde_json::json!([]));
    assert_eq!(body["engineVersion"], skyspace_core::ENGINE_VERSION);
    assert!(
        body["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["stale"] == true)
    );
    // No current term and none named: the search cannot pick one.
    let response = get(&app, "/api/v1/sections", None).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let unknown = send(
        &app,
        Request::builder()
            .method(Method::GET)
            .uri("/api/v1/nothing")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let body = json(unknown).await;
    assert_eq!(body["code"], "not_found");
    assert!(!body["requestId"].as_str().unwrap().is_empty());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn the_program_list_carries_an_etag_and_answers_304(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    seed_program(&state.store).await;
    let first = get(&app, "/api/v1/programs", None).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first.headers().get(CACHE_CONTROL).unwrap(),
        "public, max-age=300"
    );
    let etag = first
        .headers()
        .get(ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(etag.starts_with("W/\"programs-2027-"), "{etag}");
    let body = json(first).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["slug"], "example-bs");
    assert_eq!(body[0]["catalogYears"], serde_json::json!([2027]));
    let again = send(
        &app,
        Request::builder()
            .uri("/api/v1/programs")
            .header(IF_NONE_MATCH, &etag)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(again.headers().get(ETAG).unwrap(), etag.as_str());
    // A filter that changes the list changes the tag.
    let filtered = get(&app, "/api/v1/programs?q=nothing-matches", None).await;
    assert_eq!(filtered.status(), StatusCode::OK);
    assert_ne!(filtered.headers().get(ETAG).unwrap(), etag.as_str());
    assert_eq!(json(filtered).await, serde_json::json!([]));
}
