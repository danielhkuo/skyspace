//! Feedback on a rule, and product counters.

use skyspace_core::program::{CatalogYear, ProgramId, RequirementId};
use skyspace_core::term::TermCode;
use time::OffsetDateTime;

use crate::courses::year_to_db;
use crate::error::StoreError;
use crate::pool::Store;

impl Store {
    /// Record a report on a rule. Returns the row id. An unknown requirement
    /// or program id is stored as null rather than refused, so a report
    /// about a rule that was just retired is not lost.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn record_requirement_report(
        &self,
        requirement: Option<RequirementId>,
        program: Option<ProgramId>,
        catalog_year: Option<CatalogYear>,
        message: &str,
    ) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into requirement_reports (requirement_id, program_id, catalog_year, message) values (\
                (select id from requirements where id = $1), \
                (select id from programs where id = $2), $3, $4) returning id",
        )
        .bind(requirement.map(|r| r.0))
        .bind(program.map(|p| p.0))
        .bind(catalog_year.map(year_to_db).transpose()?)
        .bind(message)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// Reports recorded since `since`: the rate-limit source for the
    /// logged-out report route.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn recent_requirement_reports(
        &self,
        since: OffsetDateTime,
    ) -> Result<u64, StoreError> {
        let count: i64 =
            sqlx::query_scalar("select count(*) from requirement_reports where created_at >= $1")
                .bind(since)
                .fetch_one(&self.pool)
                .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Record a usage event. No account, session or address is stored.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn record_event(
        &self,
        name: &str,
        term: Option<TermCode>,
        detail: serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query("insert into usage_events (name, term_code, detail) values ($1, $2, $3)")
            .bind(name)
            .bind(term.map(|t| t.to_string()))
            .bind(detail)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use skyspace_core::program::{CatalogYear, ProgramId, RequirementId};
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use crate::pool::Store;
    use crate::programs::VersionSource;
    use crate::testing::{all, course_rule, fall, program, seed_term};

    #[sqlx::test]
    async fn reports_and_events(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let now = OffsetDateTime::now_utc();
        assert_eq!(
            store
                .recent_requirement_reports(now - Duration::hours(1))
                .await
                .unwrap(),
            0
        );
        let program_id = store
            .publish_program(
                &program(
                    "example-bs",
                    2026,
                    all("Root", vec![course_rule("COMP 140")]),
                ),
                "alice",
                VersionSource::Ga,
            )
            .await
            .unwrap();
        let rule = store
            .program(program_id, CatalogYear(2026))
            .await
            .unwrap()
            .unwrap()
            .root
            .flatten()[1]
            .id;
        let known = store
            .record_requirement_report(
                Some(rule),
                Some(program_id),
                Some(CatalogYear(2026)),
                "wrong hours",
            )
            .await
            .unwrap();
        let unknown = store
            .record_requirement_report(
                Some(RequirementId(Uuid::new_v4())),
                Some(ProgramId(Uuid::new_v4())),
                None,
                "gone",
            )
            .await
            .unwrap();
        assert!(unknown > known);
        let (req, prog): (Option<Uuid>, Option<Uuid>) = sqlx::query_as(
            "select requirement_id, program_id from requirement_reports where id = $1",
        )
        .bind(unknown)
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!((req, prog), (None, None));
        assert_eq!(
            store
                .recent_requirement_reports(now - Duration::hours(1))
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .recent_requirement_reports(now + Duration::hours(1))
                .await
                .unwrap(),
            0
        );
        store
            .record_event("search_empty", Some(fall()), serde_json::json!({"q": "x"}))
            .await
            .unwrap();
        store
            .record_event("page_view", None, serde_json::json!({}))
            .await
            .unwrap();
        let events: i64 = sqlx::query_scalar("select count(*) from usage_events")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(events, 2);
    }
}
