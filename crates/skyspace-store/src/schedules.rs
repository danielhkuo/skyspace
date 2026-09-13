//! Schedules: one replaced document per schedule, with `schedule_sections`
//! rewritten in the same transaction for the seat poll set.

use skyspace_core::plan::{AccountId, ScheduleId};
use skyspace_core::schedule::TermSchedule;
use skyspace_core::term::TermCode;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::convert::{crn_to_db, term_from_db};
use crate::error::StoreError;
use crate::pool::Store;

/// One schedule, for the schedule list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleSummaryRow {
    /// The schedule.
    pub id: ScheduleId,
    /// Display name.
    pub name: String,
    /// Which term.
    pub term: TermCode,
    /// Optimistic-concurrency version.
    pub version: i32,
    /// Last write.
    pub updated_at: OffsetDateTime,
}

/// A guest schedule offered to the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestScheduleInput {
    /// The browser's id for the record, minted at creation.
    pub client_id: Uuid,
    /// The document.
    pub schedule: TermSchedule,
}

/// What the claim did with one guest record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedRow {
    /// The browser's id.
    pub client_id: Uuid,
    /// The server's id, the same one on a retry.
    pub id: Uuid,
    /// The name it was stored under when the given one was taken.
    pub renamed_to: Option<String>,
}

#[derive(sqlx::FromRow)]
struct SummaryDbRow {
    id: Uuid,
    name: String,
    term_code: String,
    version: i32,
    updated_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct BodyRow {
    version: i32,
    updated_at: OffsetDateTime,
    body: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    version: i32,
    updated_at: OffsetDateTime,
}

/// Replace the schedule's section rows with the CRNs the document names
/// that exist in its term. CRNs not held get no row and stay in the
/// document.
pub(crate) async fn write_schedule_sections(
    conn: &mut PgConnection,
    id: Uuid,
    schedule: &TermSchedule,
) -> Result<(), StoreError> {
    sqlx::query("delete from schedule_sections where schedule_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let crns: Vec<i32> = schedule
        .candidates
        .iter()
        .flat_map(|c| c.sections.iter().copied())
        .map(crn_to_db)
        .collect::<Result<_, _>>()?;
    if crns.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "insert into schedule_sections (schedule_id, section_id) \
         select $1, id from sections where term_code = $2 and crn = any($3) on conflict do nothing",
    )
    .bind(id)
    .bind(schedule.term.to_string())
    .bind(&crns)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Insert a schedule under a minted id, which replaces `schedule.id` in
/// the stored body.
pub(crate) async fn insert_schedule(
    conn: &mut PgConnection,
    account: AccountId,
    schedule: &TermSchedule,
    client_id: Option<Uuid>,
    name: &str,
) -> Result<(ScheduleId, i32, OffsetDateTime), StoreError> {
    let id = Uuid::new_v4();
    let stored = TermSchedule {
        id: ScheduleId(id),
        name: name.to_owned(),
        ..schedule.clone()
    };
    let row: VersionRow = sqlx::query_as(
        "insert into schedules (id, account_id, client_id, term_code, name, body) \
         values ($1, $2, $3, $4, $5, $6) returning version, updated_at",
    )
    .bind(id)
    .bind(account.0)
    .bind(client_id)
    .bind(stored.term.to_string())
    .bind(&stored.name)
    .bind(serde_json::to_value(&stored)?)
    .fetch_one(&mut *conn)
    .await?;
    write_schedule_sections(conn, id, &stored).await?;
    Ok((ScheduleId(id), row.version, row.updated_at))
}

/// `name`, or `name (2)`, `name (3)`, ... : the first not used by another
/// schedule of the account in the term.
pub(crate) async fn free_schedule_name(
    conn: &mut PgConnection,
    account: AccountId,
    term: TermCode,
    name: &str,
) -> Result<String, StoreError> {
    let taken: Vec<String> = sqlx::query_scalar(
        "select lower(name) from schedules where account_id = $1 and term_code = $2",
    )
    .bind(account.0)
    .bind(term.to_string())
    .fetch_all(conn)
    .await?;
    Ok(free_name(&taken, name))
}

/// The first of `name`, `name (2)`, `name (3)`, ... absent from `taken`
/// (lowercase).
pub(crate) fn free_name(taken: &[String], name: &str) -> String {
    if !taken.contains(&name.to_lowercase()) {
        return name.to_owned();
    }
    // Bounded by the number of names taken plus one: some suffix is free.
    (2u32..)
        .take(taken.len().saturating_add(2))
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| !taken.contains(&candidate.to_lowercase()))
        .unwrap_or_else(|| name.to_owned())
}

impl Store {
    /// The account's schedules for a term, most recently written first.
    ///
    /// # Errors
    /// `StoreError::Database` or `Corrupt`.
    pub async fn schedules(
        &self,
        account: AccountId,
        term: TermCode,
    ) -> Result<Vec<ScheduleSummaryRow>, StoreError> {
        let rows: Vec<SummaryDbRow> = sqlx::query_as(
            "select id, name, term_code, version, updated_at from schedules \
             where account_id = $1 and term_code = $2 order by updated_at desc",
        )
        .bind(account.0)
        .bind(term.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(ScheduleSummaryRow {
                    id: ScheduleId(r.id),
                    name: r.name,
                    term: term_from_db("schedules", &r.term_code)?,
                    version: r.version,
                    updated_at: r.updated_at,
                })
            })
            .collect()
    }

    /// One schedule with its version and last write.
    ///
    /// # Errors
    /// `StoreError::Database` or `Json`.
    pub async fn schedule(
        &self,
        account: AccountId,
        id: ScheduleId,
    ) -> Result<Option<(i32, OffsetDateTime, TermSchedule)>, StoreError> {
        let row: Option<BodyRow> = sqlx::query_as(
            "select version, updated_at, body from schedules where account_id = $1 and id = $2",
        )
        .bind(account.0)
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| Ok((r.version, r.updated_at, serde_json::from_value(r.body)?)))
            .transpose()
    }

    /// Store a new schedule under a minted id, which replaces
    /// `schedule.id` in the stored body. The term must be held.
    ///
    /// # Errors
    /// `StoreError::Database` (including a foreign-key failure for a term
    /// not held), `Input` or `Json`.
    pub async fn create_schedule(
        &self,
        account: AccountId,
        schedule: &TermSchedule,
        client_id: Option<Uuid>,
    ) -> Result<(ScheduleId, i32, OffsetDateTime), StoreError> {
        let mut tx = self.pool.begin().await?;
        let created =
            insert_schedule(&mut tx, account, schedule, client_id, &schedule.name).await?;
        tx.commit().await?;
        Ok(created)
    }

    /// Replace the document when `version` is still current, rewriting
    /// `schedule_sections` in the same transaction. `None` means stale.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`.
    pub async fn save_schedule(
        &self,
        account: AccountId,
        id: ScheduleId,
        version: i32,
        schedule: &TermSchedule,
    ) -> Result<Option<(i32, OffsetDateTime)>, StoreError> {
        let stored = TermSchedule {
            id,
            ..schedule.clone()
        };
        let mut tx = self.pool.begin().await?;
        let row: Option<VersionRow> = sqlx::query_as(
            "update schedules set body = $4, name = $5, term_code = $6, version = version + 1, updated_at = now() \
             where id = $1 and account_id = $2 and version = $3 returning version, updated_at",
        )
        .bind(id.0)
        .bind(account.0)
        .bind(version)
        .bind(serde_json::to_value(&stored)?)
        .bind(&stored.name)
        .bind(stored.term.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        write_schedule_sections(&mut tx, id.0, &stored).await?;
        tx.commit().await?;
        Ok(Some((row.version, row.updated_at)))
    }

    /// Delete a schedule. `false` when it is not the account's.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_schedule(
        &self,
        account: AccountId,
        id: ScheduleId,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from schedules where account_id = $1 and id = $2")
            .bind(account.0)
            .bind(id.0)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Claim guest schedules in one transaction. A record already claimed
    /// under its `client_id` returns the same id again; a taken name gets a
    /// suffix reported in `renamed_to`. A schedule whose term is not held
    /// is skipped and absent from the result; the caller checks the limits
    /// before calling.
    ///
    /// # Errors
    /// `StoreError::Database`, `Input` or `Json`. Nothing lands on error.
    pub async fn claim_schedules(
        &self,
        account: AccountId,
        guests: &[GuestScheduleInput],
    ) -> Result<Vec<ClaimedRow>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut out = Vec::with_capacity(guests.len());
        for guest in guests {
            let existing: Option<Uuid> = sqlx::query_scalar(
                "select id from schedules where account_id = $1 and client_id = $2",
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
            let held: bool =
                sqlx::query_scalar("select exists (select 1 from terms where code = $1)")
                    .bind(guest.schedule.term.to_string())
                    .fetch_one(&mut *tx)
                    .await?;
            if !held {
                continue;
            }
            let name =
                free_schedule_name(&mut tx, account, guest.schedule.term, &guest.schedule.name)
                    .await?;
            let renamed_to = (name != guest.schedule.name).then(|| name.clone());
            let id = Uuid::new_v4();
            let stored = TermSchedule {
                id: ScheduleId(id),
                name: name.clone(),
                ..guest.schedule.clone()
            };
            let id: Uuid = sqlx::query_scalar(
                "insert into schedules (id, account_id, client_id, term_code, name, body) \
                 values ($1, $2, $3, $4, $5, $6) \
                 on conflict (account_id, client_id) where client_id is not null \
                 do update set client_id = excluded.client_id returning id",
            )
            .bind(id)
            .bind(account.0)
            .bind(guest.client_id)
            .bind(stored.term.to_string())
            .bind(&name)
            .bind(serde_json::to_value(&stored)?)
            .fetch_one(&mut *tx)
            .await?;
            write_schedule_sections(&mut tx, id, &stored).await?;
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
    use super::free_name;

    #[test]
    fn free_name_suffixes_from_two() {
        let taken = vec!["fall".to_owned(), "fall (2)".to_owned()];
        assert_eq!(free_name(&taken, "Fall"), "Fall (3)");
        assert_eq!(free_name(&taken, "Spring"), "Spring");
    }
}

#[cfg(test)]
mod sql_tests {
    use skyspace_core::code::Crn;
    use skyspace_core::plan::ScheduleId;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::GuestScheduleInput;
    use crate::pool::Store;
    use crate::testing::{fall, listing, schedule, seed_account, seed_term, timed};

    async fn section_rows(store: &Store, id: Uuid) -> Vec<i32> {
        sqlx::query_scalar(
            "select s.crn from schedule_sections ss join sections s on s.id = ss.section_id where ss.schedule_id = $1 order by s.crn",
        )
        .bind(id)
        .fetch_all(store.pool())
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn create_read_save_delete(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[
                    listing(10001, "COMP 140", "CT", vec![timed("MWF", 540, 590)]),
                    listing(10002, "COMP 140", "CT", vec![]),
                ],
            )
            .await
            .unwrap();
        let account = seed_account(&store, "a@rice.edu").await;
        let other = seed_account(&store, "b@rice.edu").await;
        let (id, version, _) = store
            .create_schedule(
                account,
                &schedule("Plan A", &[10001, 77777]),
                Some(Uuid::new_v4()),
            )
            .await
            .unwrap();
        assert_eq!(version, 1);
        assert_eq!(section_rows(&store, id.0).await, vec![10001]);
        let list = store.schedules(account, fall()).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert!(store.schedules(other, fall()).await.unwrap().is_empty());
        let (_, _, stored) = store.schedule(account, id).await.unwrap().unwrap();
        assert_eq!(stored.id, id);
        assert_eq!(stored.candidates[0].sections, vec![Crn(10001), Crn(77777)]);
        assert!(store.schedule(other, id).await.unwrap().is_none());

        let mut edited = schedule("Plan A2", &[10002]);
        edited.id = ScheduleId(Uuid::new_v4());
        let (v2, _) = store
            .save_schedule(account, id, 1, &edited)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v2, 2);
        assert!(
            store
                .save_schedule(account, id, 1, &edited)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .save_schedule(other, id, 2, &edited)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(section_rows(&store, id.0).await, vec![10002]);
        assert_eq!(
            store.schedule(account, id).await.unwrap().unwrap().2.name,
            "Plan A2"
        );

        assert!(!store.delete_schedule(other, id).await.unwrap());
        assert!(store.delete_schedule(account, id).await.unwrap());
        assert!(store.schedule(account, id).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn claim_is_idempotent_and_renames(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(fall(), &[listing(10001, "COMP 140", "CT", vec![])])
            .await
            .unwrap();
        let account = seed_account(&store, "a@rice.edu").await;
        store
            .create_schedule(account, &schedule("Fall", &[]), None)
            .await
            .unwrap();
        let client = Uuid::new_v4();
        let mut unknown_term = schedule("Elsewhere", &[]);
        unknown_term.term = crate::testing::term("209910");
        let guests = vec![
            GuestScheduleInput {
                client_id: client,
                schedule: schedule("Fall", &[10001]),
            },
            GuestScheduleInput {
                client_id: Uuid::new_v4(),
                schedule: unknown_term,
            },
        ];
        let first = store.claim_schedules(account, &guests).await.unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].client_id, client);
        assert_eq!(first[0].renamed_to.as_deref(), Some("Fall (2)"));
        assert_eq!(section_rows(&store, first[0].id).await, vec![10001]);
        let again = store.claim_schedules(account, &guests[..1]).await.unwrap();
        assert_eq!(again[0].id, first[0].id);
        assert_eq!(again[0].renamed_to, None);
        assert_eq!(store.schedules(account, fall()).await.unwrap().len(), 2);
    }
}
