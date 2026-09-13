#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A hand-run migration for a development database, until the CLI's
//! `skyspace migrate` is the everyday tool:
//! `SKYSPACE_MIGRATE_URL=postgres://… cargo test -p skyspace-api --test migrate -- --ignored`

use skyspace_store::Store;

#[tokio::test]
#[ignore = "runs migrations against SKYSPACE_MIGRATE_URL on request"]
async fn migrate_the_named_database() {
    let url = std::env::var("SKYSPACE_MIGRATE_URL").expect("SKYSPACE_MIGRATE_URL");
    let store = Store::connect(&url, 1).await.unwrap();
    store.migrate().await.unwrap();
}
