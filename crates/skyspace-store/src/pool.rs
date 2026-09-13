//! The `Store` handle: a connection pool and the migrator.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::StoreError;

/// Every SQL statement in the project runs through this handle. Cheap to
/// clone: it holds one pool.
#[derive(Clone)]
pub struct Store {
    pub(crate) pool: PgPool,
}

/// One page of results, with the cursor for the next page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// The rows on this page.
    pub items: Vec<T>,
    /// Opaque cursor for the next page, or `None` at the end.
    pub next: Option<String>,
}

/// The embedded `migrations/` directory.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl Store {
    /// Open a pool against `url` with at most `max_connections` connections.
    ///
    /// # Errors
    /// `StoreError::Database` when the first connection fails.
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// Wrap a pool the caller already has. `#[sqlx::test]` hands one in.
    #[must_use]
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Runs `migrations/`. Called by `skyspace migrate`, never at server
    /// start, so two web workers cannot race on the same migration.
    ///
    /// # Errors
    /// `StoreError::Migrate` when a migration fails or the applied history
    /// disagrees with the embedded files.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// The underlying pool, for the advisory lock a job holds across its run.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::Store;

    /// `#[sqlx::test]` applied the migrations from scratch; a second run is
    /// a no-op and the version table lists all eight.
    #[sqlx::test]
    async fn migrations_apply_from_scratch(pool: PgPool) {
        let store = Store::from_pool(pool);
        store.migrate().await.unwrap();
        let applied: i64 = sqlx::query_scalar("select count(*) from _sqlx_migrations")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(applied, 8);
        let tables: i64 = sqlx::query_scalar(
            "select count(*) from information_schema.tables where table_schema = 'public' and table_name <> '_sqlx_migrations'",
        )
        .fetch_one(store.pool())
        .await
        .unwrap();
        assert_eq!(tables, 35);
    }
}
