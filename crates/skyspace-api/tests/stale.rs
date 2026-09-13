#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Stale data: with the last `ingest_runs` row failed, catalog endpoints
//! still return the previous data and a `Freshness` with `stale: true`.

mod common;

use axum::http::StatusCode;
use common::{app, finish_run, get, json, seed_catalog};
use skyspace_store::RunOutcome;

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn a_failed_run_after_a_good_one_keeps_serving(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let before = json(get(&app, "/api/v1/sections?term=202710", None).await).await;
    assert_eq!(before["freshness"]["stale"], false);
    finish_run(&state.store, "listings", RunOutcome::Failed).await;
    let after = get(&app, "/api/v1/sections?term=202710", None).await;
    assert_eq!(after.status(), StatusCode::OK);
    let after = json(after).await;
    assert_eq!(after["data"]["total"], 2);
    // The last good run still stamps the data, and the failed one marks it.
    assert_eq!(after["freshness"]["stale"], true);
    assert_eq!(
        after["freshness"]["pulledAt"],
        before["freshness"]["pulledAt"]
    );
    let section = json(get(&app, "/api/v1/sections/10001", None).await).await;
    assert_eq!(section["freshness"]["stale"], true);
    let meta = json(get(&app, "/api/v1/meta", None).await).await;
    let listings = meta["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["job"] == "listings")
        .unwrap();
    assert_eq!(listings["stale"], true);
    assert!(!listings["lastOk"].is_null());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn no_good_run_at_all_is_stale_but_served(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    // The fixture's good run is for `listings`; the detail and reference
    // jobs have only ever failed.
    finish_run(&state.store, "reference", RunOutcome::Failed).await;
    let reference = get(&app, "/api/v1/reference?term=202710", None).await;
    assert_eq!(reference.status(), StatusCode::OK);
    let body = json(reference).await;
    assert_eq!(body["data"]["subjects"].as_array().unwrap().len(), 2);
    assert_eq!(body["freshness"]["stale"], true);
    assert_eq!(body["freshness"]["pulledAt"], "1970-01-01T00:00:00Z");
    let course = json(get(&app, "/api/v1/courses/COMP/140", None).await).await;
    assert_eq!(course["freshness"]["stale"], false);
    let meta = json(get(&app, "/api/v1/meta", None).await).await;
    let reference = meta["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["job"] == "reference")
        .unwrap();
    assert_eq!(reference["stale"], true);
    assert!(reference["lastOk"].is_null());
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn a_term_that_never_pulled_is_stale_from_the_epoch(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    state
        .store
        .upsert_terms(&[skyspace_store::TermInput {
            code: common::term("202720"),
            label: "Spring Semester 2027".to_owned(),
        }])
        .await
        .unwrap();
    let body = json(get(&app, "/api/v1/sections?term=202720", None).await).await;
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["freshness"]["stale"], true);
    assert_eq!(body["freshness"]["pulledAt"], "1970-01-01T00:00:00Z");
}
