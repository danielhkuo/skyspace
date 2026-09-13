//! Accounts.

use skyspace_core::plan::AccountId;
use time::OffsetDateTime;

use crate::error::StoreError;
use crate::pool::Store;

/// One `accounts` row.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AccountRow {
    /// The account.
    pub id: uuid::Uuid,
    /// The verified address.
    pub email: String,
    /// When the account was created.
    pub created_at: OffsetDateTime,
    /// Last visit, written at most once an hour.
    pub last_seen_at: OffsetDateTime,
}

impl AccountRow {
    /// The id as core names it.
    #[must_use]
    pub fn account_id(&self) -> AccountId {
        AccountId(self.id)
    }
}

const COLUMNS: &str = "id, email, created_at, last_seen_at";

impl Store {
    /// The account for an address, matched case-insensitively.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn account_by_email(&self, email: &str) -> Result<Option<AccountRow>, StoreError> {
        let row: Option<AccountRow> = sqlx::query_as(&format!(
            "select {COLUMNS} from accounts where lower(email) = lower($1)"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// The account by id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn account(&self, account: AccountId) -> Result<Option<AccountRow>, StoreError> {
        let row: Option<AccountRow> =
            sqlx::query_as(&format!("select {COLUMNS} from accounts where id = $1"))
                .bind(account.0)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row)
    }

    /// Create an account for a verified address, or return the existing
    /// one for that address.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn create_account(&self, email: &str) -> Result<AccountRow, StoreError> {
        let row: AccountRow = sqlx::query_as(&format!(
            "insert into accounts (email) values (lower($1)) \
             on conflict (email) do update set last_seen_at = now() returning {COLUMNS}"
        ))
        .bind(email)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Write `last_seen_at`. `false` when the account is unknown.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn touch_account(
        &self,
        account: AccountId,
        now: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query("update accounts set last_seen_at = $2 where id = $1")
            .bind(account.0)
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Delete an account; plans, schedules, collections and sessions cascade.
    /// `false` when the account is unknown.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_account(&self, account: AccountId) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from accounts where id = $1")
            .bind(account.0)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};

    use crate::pool::Store;
    use crate::testing::plan;

    #[sqlx::test]
    async fn accounts_round_trip_and_cascade(pool: PgPool) {
        let store = Store::from_pool(pool);
        assert!(
            store
                .account_by_email("a@rice.edu")
                .await
                .unwrap()
                .is_none()
        );
        let created = store.create_account("A@rice.edu").await.unwrap();
        assert_eq!(created.email, "a@rice.edu");
        let same = store.create_account("a@rice.edu").await.unwrap();
        assert_eq!(same.id, created.id);
        let found = store.account_by_email("A@RICE.EDU").await.unwrap().unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(
            store
                .account(created.account_id())
                .await
                .unwrap()
                .unwrap()
                .email,
            "a@rice.edu"
        );
        let later = OffsetDateTime::now_utc() + Duration::hours(1);
        assert!(
            store
                .touch_account(created.account_id(), later)
                .await
                .unwrap()
        );
        assert_eq!(
            store
                .account(created.account_id())
                .await
                .unwrap()
                .unwrap()
                .last_seen_at,
            later
        );
        store
            .create_plan(created.account_id(), &plan("P", 2026, Vec::new(), &[]))
            .await
            .unwrap();
        assert!(store.delete_account(created.account_id()).await.unwrap());
        assert!(!store.delete_account(created.account_id()).await.unwrap());
        assert!(store.account(created.account_id()).await.unwrap().is_none());
        let plans: i64 = sqlx::query_scalar("select count(*) from plans")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(plans, 0);
    }
}
