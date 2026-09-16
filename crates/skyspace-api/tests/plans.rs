#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Plans: create, bundle, report through the engine, stale `PUT`, the last
//! plan, duplicates, and the abuse guard.

mod common;

use axum::http::{Method, StatusCode};
use common::{app, call, get, json, plan, seed_catalog, seed_program, sign_in};
use serde_json::{Value, json};
use skyspace_core::plan::{NonCourseClaim, PlanTerm, TermId, TermKind};
use skyspace_core::program::RequirementId;
use skyspace_core::term::{Season, TermPosition};

async fn create_plan(
    app: &axum::Router,
    cookie: &str,
    body: &Value,
) -> axum::http::Response<axum::body::Body> {
    call(app, Method::POST, "/api/v1/plans", Some(cookie), Some(body)).await
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn create_bundle_and_report(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let program = seed_program(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    assert_eq!(
        json(get(&app, "/api/v1/plans", Some(&cookie)).await).await,
        json!([])
    );
    let created = create_plan(
        &app,
        &cookie,
        &json!({ "plan": plan("Four years", vec![program], &["COMP 140"]) }),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    let envelope = json(created).await;
    assert_eq!(envelope["version"], 1);
    assert_eq!(envelope["plan"]["name"], "Four years");
    let id = envelope["plan"]["id"].as_str().unwrap().to_owned();
    assert_ne!(id, "00000000-0000-0000-0000-000000000000");

    let list = json(get(&app, "/api/v1/plans", Some(&cookie)).await).await;
    assert_eq!(list[0]["id"], id);
    assert_eq!(list[0]["isActive"], true);
    assert_eq!(list[0]["catalogYear"], 2027);

    let bundle = get(&app, &format!("/api/v1/plans/{id}/bundle"), Some(&cookie)).await;
    assert_eq!(bundle.status(), StatusCode::OK);
    let bundle = json(bundle).await;
    assert_eq!(bundle["plan"]["id"], id);
    assert_eq!(bundle["programs"].as_array().unwrap().len(), 1);
    assert_eq!(bundle["programs"][0]["slug"], "example-bs");
    assert!(bundle["facts"].is_object());

    let report = get(&app, &format!("/api/v1/plans/{id}/report"), Some(&cookie)).await;
    assert_eq!(report.status(), StatusCode::OK);
    let report = json(report).await;
    assert_eq!(report["plan"], id);
    assert_eq!(report["engineVersion"], skyspace_core::ENGINE_VERSION);
    let programs = report["programs"].as_array().unwrap();
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0]["name"], "Example Major");
    assert_eq!(
        programs[0]["progress"]["requirementsMet"],
        programs[0]["progress"]["requirementsCheckable"],
        "COMP 140 fills the one rule: {}",
        programs[0]
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn put_with_a_stale_version_is_409(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let envelope =
        json(create_plan(&app, &cookie, &json!({ "plan": plan("P", vec![], &[]) })).await).await;
    let id = envelope["plan"]["id"].as_str().unwrap().to_owned();
    let mut edited = envelope["plan"].clone();
    edited["name"] = json!("Renamed");
    let saved = call(
        &app,
        Method::PUT,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        Some(&json!({ "version": 1, "plan": edited })),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let saved = json(saved).await;
    assert_eq!(saved["version"], 2);
    assert_eq!(saved["plan"]["name"], "Renamed");
    assert_eq!(saved["plan"]["id"], id);

    let stale = call(
        &app,
        Method::PUT,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        Some(&json!({ "version": 1, "plan": saved["plan"] })),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let body = json(stale).await;
    assert_eq!(body["code"], "stale_version");
    assert_eq!(body["message"], "This changed in another tab.");
    let current = json(get(&app, &format!("/api/v1/plans/{id}"), Some(&cookie)).await).await;
    assert_eq!(current["version"], 2);

    let unknown = call(
        &app,
        Method::PUT,
        &format!("/api/v1/plans/{}", uuid::Uuid::new_v4()),
        Some(&cookie),
        Some(&json!({ "version": 1, "plan": saved["plan"] })),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
    let extra_field = call(
        &app,
        Method::PUT,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        Some(&json!({ "version": 2, "plan": saved["plan"], "owner": "me" })),
    )
    .await;
    assert_eq!(extra_field.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json(extra_field).await["code"], "invalid_request");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn the_last_plan_cannot_be_deleted_and_a_duplicate_can(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let first = json(
        create_plan(
            &app,
            &cookie,
            &json!({ "plan": plan("Only", vec![], &["COMP 140"]) }),
        )
        .await,
    )
    .await;
    let id = first["plan"]["id"].as_str().unwrap().to_owned();
    let refused = call(
        &app,
        Method::DELETE,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::CONFLICT);
    assert_eq!(json(refused).await["code"], "last_plan");

    let copy = call(
        &app,
        Method::POST,
        &format!("/api/v1/plans/{id}/duplicate"),
        Some(&cookie),
        Some(&json!({ "name": "Copy" })),
    )
    .await;
    assert_eq!(copy.status(), StatusCode::OK);
    let copy = json(copy).await;
    assert_eq!(copy["plan"]["name"], "Copy");
    assert_ne!(copy["plan"]["id"], id);
    assert_eq!(copy["plan"]["terms"], first["plan"]["terms"]);
    let copy_id = copy["plan"]["id"].as_str().unwrap().to_owned();
    let list = json(get(&app, "/api/v1/plans", Some(&cookie)).await).await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["id"], id, "the original stays active and first");

    let deleted = call(
        &app,
        Method::DELETE,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let list = json(get(&app, "/api/v1/plans", Some(&cookie)).await).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], copy_id);
    assert_eq!(list[0]["isActive"], true, "the copy became active");
    let again = call(
        &app,
        Method::DELETE,
        &format!("/api/v1/plans/{id}"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(again.status(), StatusCode::NOT_FOUND);
    let blank = call(
        &app,
        Method::POST,
        &format!("/api/v1/plans/{copy_id}/duplicate"),
        Some(&cookie),
        Some(&json!({ "name": "   " })),
    )
    .await;
    assert_eq!(blank.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(blank).await["message"], "name");
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn the_abuse_guard_names_the_field(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let mut many_terms = serde_json::to_value(plan("Long", vec![], &[])).unwrap();
    let term = many_terms["terms"][0].clone();
    many_terms["terms"] = json!(vec![term.clone(); 41]);
    let refused = create_plan(&app, &cookie, &json!({ "plan": many_terms })).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "terms");

    let codes: Vec<&str> = std::iter::repeat_n("COMP 140", 401).collect();
    let many_cards = plan("Wide", vec![], &codes);
    let refused = create_plan(&app, &cookie, &json!({ "plan": many_cards })).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "courses");

    // Forty terms and four hundred cards are fine; an `off` term and a
    // Rice term with no published code are accepted.
    let mut wide = plan("Edge", vec![], &codes[..400]);
    for i in 1..40 {
        let position = TermPosition {
            academic_year: 2027 + i / 3,
            season: Season::Fall,
        };
        let kind = if i == 1 {
            TermKind::Rice {
                code: None,
                courses: Vec::new(),
            }
        } else {
            TermKind::Off
        };
        wide.terms.push(PlanTerm {
            id: TermId(uuid::Uuid::new_v4()),
            position,
            label: None,
            kind,
            non_course: Vec::new(),
        });
    }
    let accepted = create_plan(&app, &cookie, &json!({ "plan": wide })).await;
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(
        json(accepted).await["plan"]["terms"]
            .as_array()
            .unwrap()
            .len(),
        40
    );
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn the_abuse_guard_counts_every_vector_in_the_document(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let ids = |n: usize| -> Vec<RequirementId> {
        (0..n)
            .map(|_| RequirementId(uuid::Uuid::new_v4()))
            .collect()
    };
    // One card, five thousand fills: under the card cap, over the id cap.
    let mut stuffed = plan("Stuffed", vec![], &["COMP 140"]);
    if let TermKind::Rice { courses, .. } = &mut stuffed.terms[0].kind {
        courses[0].fills = ids(5000);
    }
    let refused = create_plan(&app, &cookie, &json!({ "plan": stuffed })).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "fills");

    // The cap is over the whole document: non-course claims on the term
    // plus fills on a card, neither over the cap alone, name the larger.
    let mut claims = plan("Claims", vec![], &["COMP 140"]);
    if let TermKind::Rice { courses, .. } = &mut claims.terms[0].kind {
        courses[0].fills = ids(1600);
    }
    claims.terms[0].non_course = ids(2500)
        .into_iter()
        .map(|requirement| NonCourseClaim {
            requirement,
            label: "x".to_owned(),
        })
        .collect();
    let refused = create_plan(&app, &cookie, &json!({ "plan": claims })).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "nonCourse");

    // Fills spread over two cards are summed; the field is still `fills`.
    let mut split = plan("Split", vec![], &["COMP 140", "COMP 182"]);
    if let TermKind::Rice { courses, .. } = &mut split.terms[0].kind {
        courses[0].fills = ids(2001);
        courses[1].fills = ids(2000);
    }
    let refused = create_plan(&app, &cookie, &json!({ "plan": split })).await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(refused).await["message"], "fills");

    // Exactly the cap is accepted.
    let mut full = plan("Full", vec![], &["COMP 140"]);
    if let TermKind::Rice { courses, .. } = &mut full.terms[0].kind {
        courses[0].fills = ids(4000);
    }
    let accepted = create_plan(&app, &cookie, &json!({ "plan": full })).await;
    assert_eq!(accepted.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "../skyspace-store/migrations")]
async fn a_later_plan_becomes_active_only_when_asked(pool: sqlx::PgPool) {
    let (app, state) = app(pool);
    seed_catalog(&state.store).await;
    let program = seed_program(&state.store).await;
    let cookie = sign_in(&app, &state, "owl@rice.edu").await;
    let first = json(
        create_plan(
            &app,
            &cookie,
            &json!({ "plan": plan("First", vec![program], &[]) }),
        )
        .await,
    )
    .await;
    let second = json(
        create_plan(
            &app,
            &cookie,
            &json!({ "plan": plan("Second", vec![program], &[]) }),
        )
        .await,
    )
    .await;
    let list = json(get(&app, "/api/v1/plans", Some(&cookie)).await).await;
    assert_eq!(list[0]["id"], first["plan"]["id"]);
    assert_eq!(list[0]["isActive"], true);
    assert_eq!(list[1]["isActive"], false);

    let second_id = second["plan"]["id"].as_str().unwrap();
    let activated = call(
        &app,
        Method::POST,
        &format!("/api/v1/plans/{second_id}/activate"),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(activated.status(), StatusCode::NO_CONTENT);
    let list = json(get(&app, "/api/v1/plans", Some(&cookie)).await).await;
    assert_eq!(list[0]["id"], second_id);
    assert_eq!(list[0]["isActive"], true);
    assert_eq!(list[1]["isActive"], false);

    let missing = call(
        &app,
        Method::POST,
        "/api/v1/plans/00000000-0000-0000-0000-000000000000/activate",
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}
