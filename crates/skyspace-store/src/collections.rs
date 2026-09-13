//! Saved-course collections, and their half of the guest claim.

use skyspace_core::code::CourseCode;
use skyspace_core::plan::{AccountId, CollectionId};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{StoreError, violates};
use crate::pool::Store;
use crate::schedules::{ClaimedRow, free_name};

/// One collection with its courses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRow {
    /// The collection.
    pub id: CollectionId,
    /// Display name, unique per account ignoring case.
    pub name: String,
    /// Saved courses, oldest first.
    pub courses: Vec<CourseCode>,
}

/// A guest collection offered to the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestCollectionInput {
    /// The browser's id for the record.
    pub client_id: Uuid,
    /// Display name.
    pub name: String,
    /// Saved courses.
    pub courses: Vec<CourseCode>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    name: String,
    courses: Vec<String>,
}

impl TryFrom<Row> for CollectionRow {
    type Error = StoreError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let courses = row
            .courses
            .iter()
            .map(|text| {
                CourseCode::parse(text)
                    .map_err(|e| crate::error::corrupt("collection_courses", "subject", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            id: CollectionId(row.id),
            name: row.name,
            courses,
        })
    }
}

/// `select` one collection per row with its courses aggregated.
const SELECT: &str = "select c.id, c.name, coalesce(\
        (select array_agg(cc.subject || ' ' || cc.number order by cc.added_at, cc.subject, cc.number) \
         from collection_courses cc where cc.collection_id = c.id), '{}') as courses \
     from collections c";

fn map_duplicate(error: sqlx::Error) -> StoreError {
    if violates(&error, "collections_name") {
        StoreError::DuplicateName
    } else {
        error.into()
    }
}

async fn load_one(
    conn: &mut PgConnection,
    account: AccountId,
    id: Uuid,
) -> Result<Option<CollectionRow>, StoreError> {
    let row: Option<Row> =
        sqlx::query_as(&format!("{SELECT} where c.account_id = $1 and c.id = $2"))
            .bind(account.0)
            .bind(id)
            .fetch_optional(conn)
            .await?;
    row.map(CollectionRow::try_from).transpose()
}

async fn add_courses(
    conn: &mut PgConnection,
    id: Uuid,
    courses: &[CourseCode],
) -> Result<(), StoreError> {
    for code in courses {
        sqlx::query(
            "insert into collection_courses (collection_id, subject, number) values ($1, $2, $3) on conflict do nothing",
        )
        .bind(id)
        .bind(code.subject.as_str())
        .bind(code.number.as_str())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

impl Store {
    /// The account's collections, oldest first, each with its courses.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn collections(&self, account: AccountId) -> Result<Vec<CollectionRow>, StoreError> {
        let rows: Vec<Row> = sqlx::query_as(&format!(
            "{SELECT} where c.account_id = $1 order by c.created_at, c.id"
        ))
        .bind(account.0)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(CollectionRow::try_from).collect()
    }

    /// Create an empty collection.
    ///
    /// # Errors
    /// `StoreError::DuplicateName` when the account already has one under
    /// that name (ignoring case); otherwise `Database`.
    pub async fn create_collection(
        &self,
        account: AccountId,
        name: &str,
        client_id: Option<Uuid>,
    ) -> Result<CollectionRow, StoreError> {
        let id: Uuid = sqlx::query_scalar(
            "insert into collections (account_id, client_id, name) values ($1, $2, $3) returning id",
        )
        .bind(account.0)
        .bind(client_id)
        .bind(name)
        .fetch_one(&self.pool)
        .await
        .map_err(map_duplicate)?;
        Ok(CollectionRow {
            id: CollectionId(id),
            name: name.to_owned(),
            courses: Vec::new(),
        })
    }

    /// Rename a collection. `None` when it is not the account's.
    ///
    /// # Errors
    /// `StoreError::DuplicateName` or `Database`.
    pub async fn rename_collection(
        &self,
        account: AccountId,
        id: CollectionId,
        name: &str,
    ) -> Result<Option<CollectionRow>, StoreError> {
        let mut conn = self.pool.acquire().await?;
        let done =
            sqlx::query("update collections set name = $3 where account_id = $1 and id = $2")
                .bind(account.0)
                .bind(id.0)
                .bind(name)
                .execute(&mut *conn)
                .await
                .map_err(map_duplicate)?;
        if done.rows_affected() == 0 {
            return Ok(None);
        }
        load_one(&mut conn, account, id.0).await
    }

    /// Delete a collection and its courses. `false` when it is not the
    /// account's.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_collection(
        &self,
        account: AccountId,
        id: CollectionId,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from collections where account_id = $1 and id = $2")
            .bind(account.0)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Save a course. Idempotent: a repeated tap is not an error. `false`
    /// when the collection is not the account's.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn add_course(
        &self,
        account: AccountId,
        id: CollectionId,
        code: &CourseCode,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let owned: bool = sqlx::query_scalar(
            "select exists (select 1 from collections where account_id = $1 and id = $2)",
        )
        .bind(account.0)
        .bind(id.0)
        .fetch_one(&mut *tx)
        .await?;
        if !owned {
            return Ok(false);
        }
        add_courses(&mut tx, id.0, core::slice::from_ref(code)).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Remove a course. `false` when the collection is not the account's;
    /// `true` whether or not the course was in it.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn remove_course(
        &self,
        account: AccountId,
        id: CollectionId,
        code: &CourseCode,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await?;
        let owned: bool = sqlx::query_scalar(
            "select exists (select 1 from collections where account_id = $1 and id = $2)",
        )
        .bind(account.0)
        .bind(id.0)
        .fetch_one(&mut *tx)
        .await?;
        if !owned {
            return Ok(false);
        }
        sqlx::query("delete from collection_courses where collection_id = $1 and subject = $2 and number = $3")
            .bind(id.0)
            .bind(code.subject.as_str())
            .bind(code.number.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Claim guest collections in one transaction. A record already
    /// claimed under its `client_id` returns the same id again and its
    /// courses are left as they are; a taken name gets a suffix reported in
    /// `renamed_to`. The caller checks the limits before calling.
    ///
    /// # Errors
    /// `StoreError::Database`. Nothing lands on error.
    pub async fn claim_collections(
        &self,
        account: AccountId,
        guests: &[GuestCollectionInput],
    ) -> Result<Vec<ClaimedRow>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut out = Vec::with_capacity(guests.len());
        for guest in guests {
            let existing: Option<Uuid> = sqlx::query_scalar(
                "select id from collections where account_id = $1 and client_id = $2",
            )
            .bind(account.0)
            .bind(guest.client_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(id) = existing {
                out.push(ClaimedRow {
                    client_id: guest.client_id,
                    id,
                    renamed_to: None,
                });
                continue;
            }
            let taken: Vec<String> =
                sqlx::query_scalar("select lower(name) from collections where account_id = $1")
                    .bind(account.0)
                    .fetch_all(&mut *tx)
                    .await?;
            let name = free_name(&taken, &guest.name);
            let renamed_to = (name != guest.name).then(|| name.clone());
            let id: Uuid = sqlx::query_scalar(
                "insert into collections (account_id, client_id, name) values ($1, $2, $3) \
                 on conflict (account_id, client_id) where client_id is not null \
                 do update set client_id = excluded.client_id returning id",
            )
            .bind(account.0)
            .bind(guest.client_id)
            .bind(&name)
            .fetch_one(&mut *tx)
            .await?;
            add_courses(&mut tx, id, &guest.courses).await?;
            out.push(ClaimedRow {
                client_id: guest.client_id,
                id,
                renamed_to,
            });
        }
        tx.commit().await?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::GuestCollectionInput;
    use crate::error::StoreError;
    use crate::pool::Store;
    use crate::testing::{code, seed_account};

    #[sqlx::test]
    async fn create_rename_courses_delete(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let other = seed_account(&store, "b@rice.edu").await;
        let saved = store
            .create_collection(account, "Saved", None)
            .await
            .unwrap();
        assert!(matches!(
            store.create_collection(account, "saved", None).await,
            Err(StoreError::DuplicateName)
        ));
        store.create_collection(other, "Saved", None).await.unwrap();
        let later = store
            .create_collection(account, "Later", Some(Uuid::new_v4()))
            .await
            .unwrap();

        assert!(
            store
                .add_course(account, saved.id, &code("COMP 140"))
                .await
                .unwrap()
        );
        assert!(
            store
                .add_course(account, saved.id, &code("COMP 140"))
                .await
                .unwrap()
        );
        assert!(
            store
                .add_course(account, saved.id, &code("MATH 101"))
                .await
                .unwrap()
        );
        assert!(
            !store
                .add_course(other, saved.id, &code("COMP 140"))
                .await
                .unwrap()
        );
        let list = store.collections(account).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, saved.id);
        assert_eq!(list[0].courses, vec![code("COMP 140"), code("MATH 101")]);
        assert!(list[1].courses.is_empty());

        assert!(
            store
                .remove_course(account, saved.id, &code("COMP 140"))
                .await
                .unwrap()
        );
        assert!(
            !store
                .remove_course(other, saved.id, &code("MATH 101"))
                .await
                .unwrap()
        );
        assert_eq!(
            store.collections(account).await.unwrap()[0].courses,
            vec![code("MATH 101")]
        );

        let renamed = store
            .rename_collection(account, later.id, "Soon")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(renamed.name, "Soon");
        assert!(matches!(
            store.rename_collection(account, later.id, "SAVED").await,
            Err(StoreError::DuplicateName)
        ));
        assert!(
            store
                .rename_collection(other, later.id, "Mine")
                .await
                .unwrap()
                .is_none()
        );

        assert!(!store.delete_collection(other, saved.id).await.unwrap());
        assert!(store.delete_collection(account, saved.id).await.unwrap());
        assert_eq!(store.collections(account).await.unwrap().len(), 1);
    }

    #[sqlx::test]
    async fn claim_is_idempotent_and_renames(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        store
            .create_collection(account, "Saved", None)
            .await
            .unwrap();
        let client = Uuid::new_v4();
        let guests = vec![
            GuestCollectionInput {
                client_id: client,
                name: "saved".to_owned(),
                courses: vec![code("COMP 140"), code("COMP 140"), code("MATH 101")],
            },
            GuestCollectionInput {
                client_id: Uuid::new_v4(),
                name: "Other".to_owned(),
                courses: Vec::new(),
            },
        ];
        let first = store.claim_collections(account, &guests).await.unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].renamed_to.as_deref(), Some("saved (2)"));
        assert_eq!(first[1].renamed_to, None);
        let again = store.claim_collections(account, &guests).await.unwrap();
        assert_eq!(again[0].id, first[0].id);
        assert_eq!(again[1].id, first[1].id);
        assert_eq!(again[0].renamed_to, None);
        let list = store.collections(account).await.unwrap();
        assert_eq!(list.len(), 3);
        let claimed = list.iter().find(|c| c.id.0 == first[0].id).unwrap();
        assert_eq!(claimed.name, "saved (2)");
        assert_eq!(claimed.courses.len(), 2);
    }
}
