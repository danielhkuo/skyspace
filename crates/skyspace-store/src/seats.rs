//! Seat polling: the current state per section, the change-only history,
//! the poll set and the registration windows.

use skyspace_core::catalog::Seats;
use skyspace_core::code::Crn;
use skyspace_core::term::TermCode;
use time::OffsetDateTime;

use crate::convert::{crn_from_db, crn_to_db, timestamp_to_db};
use crate::error::StoreError;
use crate::pool::Store;

/// One registration window, during which seats are polled.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PollWindowRow {
    /// Row id.
    pub id: i64,
    /// The term being registered for.
    pub term_code: String,
    /// "Fall 2026 registration".
    pub label: String,
    /// Window start.
    pub starts_at: OffsetDateTime,
    /// Window end.
    pub ends_at: OffsetDateTime,
    /// Minutes between polls.
    pub interval_minutes: i16,
}

#[derive(sqlx::FromRow)]
struct ChangedRow {
    section_id: i64,
    changed: bool,
}

impl Store {
    /// Record one poll's readings. The current state is updated for every
    /// CRN held; a `seat_snapshots` row is appended only when the numbers
    /// changed. Returns the number of snapshots appended.
    ///
    /// # Errors
    /// `StoreError::Database` or `Input`.
    pub async fn record_seats(
        &self,
        term: TermCode,
        seats: &[(Crn, Seats)],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut changes = 0u64;
        for (crn, reading) in seats {
            let source_time = timestamp_to_db(reading.as_of)?;
            let row: Option<ChangedRow> = sqlx::query_as(
                "insert into section_seat_state (section_id, enrolled, capacity, wait_count, wait_capacity, \
                    source_time, last_polled_at, last_changed_at) \
                 select id, $3, $4, $5, $6, $7, now(), now() from sections where term_code = $1 and crn = $2 \
                 on conflict (section_id) do update set \
                    enrolled = excluded.enrolled, capacity = excluded.capacity, \
                    wait_count = excluded.wait_count, wait_capacity = excluded.wait_capacity, \
                    source_time = excluded.source_time, last_polled_at = now(), \
                    last_changed_at = case when (section_seat_state.enrolled, section_seat_state.capacity, \
                        section_seat_state.wait_count, section_seat_state.wait_capacity) \
                        is distinct from (excluded.enrolled, excluded.capacity, excluded.wait_count, excluded.wait_capacity) \
                        then now() else section_seat_state.last_changed_at end \
                 returning section_id, (last_changed_at = last_polled_at) as changed",
            )
            .bind(term.to_string())
            .bind(crn_to_db(*crn)?)
            .bind(i32::from(reading.enrolled))
            .bind(i32::from(reading.capacity))
            .bind(i32::from(reading.waitlist_count))
            .bind(i32::from(reading.waitlist_capacity))
            .bind(source_time)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(row) = row else {
                continue;
            };
            if row.changed {
                // Rice's `time-now` can repeat across polls; a second
                // change under the same stamp replaces the row and is not
                // counted, so `changes` is rows the chart gained.
                let inserted: bool = sqlx::query_scalar(
                    "insert into seat_snapshots (section_id, observed_at, enrolled, capacity, wait_count, wait_capacity) \
                     values ($1, $2, $3, $4, $5, $6) on conflict (section_id, observed_at) do update set \
                        enrolled = excluded.enrolled, capacity = excluded.capacity, \
                        wait_count = excluded.wait_count, wait_capacity = excluded.wait_capacity \
                     returning (xmax = 0) as inserted",
                )
                .bind(row.section_id)
                .bind(source_time)
                .bind(i32::from(reading.enrolled))
                .bind(i32::from(reading.capacity))
                .bind(i32::from(reading.waitlist_count))
                .bind(i32::from(reading.waitlist_capacity))
                .fetch_one(&mut *tx)
                .await?;
                changes += u64::from(inserted);
            }
        }
        tx.commit().await?;
        Ok(changes)
    }

    /// The sections the seats job polls: live sections with a timed class
    /// meeting, on a saved schedule, or in a saved collection.
    ///
    /// # Errors
    /// `StoreError::Database`, or `Corrupt` for a negative CRN.
    pub async fn poll_set(&self, term: TermCode) -> Result<Vec<(i64, Crn)>, StoreError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i64,
            crn: i32,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "select s.id, s.crn from sections s join courses c on c.id = s.course_id \
             where s.term_code = $1 and s.withdrawn_at is null \
               and (exists (select 1 from meetings m \
                            where m.section_id = s.id and m.kind = 'class' and m.start_time is not null) \
                    or exists (select 1 from schedule_sections ss where ss.section_id = s.id) \
                    or exists (select 1 from collection_courses cc \
                               where cc.subject = c.subject and cc.number = c.number)) \
             order by s.id",
        )
        .bind(term.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| Ok((row.id, crn_from_db("sections", row.crn)?)))
            .collect()
    }

    /// The poll window open at `now` for the term, if any.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn poll_windows_active(
        &self,
        term: TermCode,
        now: OffsetDateTime,
    ) -> Result<Option<PollWindowRow>, StoreError> {
        let row: Option<PollWindowRow> = sqlx::query_as(
            "select id, term_code, label, starts_at, ends_at, interval_minutes from poll_windows \
             where term_code = $1 and starts_at <= $2 and ends_at > $2 order by starts_at limit 1",
        )
        .bind(term.to_string())
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Add a poll window. Returns the row id.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn put_poll_window(
        &self,
        term: TermCode,
        label: &str,
        starts_at: OffsetDateTime,
        ends_at: OffsetDateTime,
        interval_minutes: i16,
    ) -> Result<i64, StoreError> {
        let id: i64 = sqlx::query_scalar(
            "insert into poll_windows (term_code, label, starts_at, ends_at, interval_minutes) \
             values ($1, $2, $3, $4, $5) returning id",
        )
        .bind(term.to_string())
        .bind(label)
        .bind(starts_at)
        .bind(ends_at)
        .bind(interval_minutes)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use skyspace_core::code::Crn;
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};

    use crate::pool::Store;
    use crate::testing::{
        code, fall, listing, schedule, seats, seed_account, seed_term, timed, unparsed,
    };

    /// Rice's `time-now` repeating across two polls does not lose the
    /// later reading or over-count the chart's rows.
    #[sqlx::test]
    async fn repeated_source_time_replaces_the_snapshot(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[listing(
                    10001,
                    "COMP 140",
                    "CT",
                    vec![timed("MWF", 540, 590)],
                )],
            )
            .await
            .unwrap();
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(10001), seats(5, 30, 1_700_000_000))])
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(10001), seats(6, 30, 1_700_000_000))])
                .await
                .unwrap(),
            0
        );
        let (rows, enrolled): (i64, i32) =
            sqlx::query_as("select count(*), max(enrolled) from seat_snapshots")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_eq!((rows, enrolled), (1, 6));
    }

    #[sqlx::test]
    async fn snapshots_only_on_change(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[listing(
                    10001,
                    "COMP 140",
                    "CT",
                    vec![timed("MWF", 540, 590)],
                )],
            )
            .await
            .unwrap();
        let count = || async {
            sqlx::query_scalar::<_, i64>("select count(*) from seat_snapshots")
                .fetch_one(store.pool())
                .await
                .unwrap()
        };
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(10001), seats(10, 30, 1_700_000_000))])
                .await
                .unwrap(),
            1
        );
        assert_eq!(count().await, 1);
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(10001), seats(10, 30, 1_700_000_900))])
                .await
                .unwrap(),
            0
        );
        assert_eq!(count().await, 1);
        let polled: OffsetDateTime =
            sqlx::query_scalar("select last_polled_at from section_seat_state")
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert!(polled > OffsetDateTime::now_utc() - Duration::minutes(1));
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(10001), seats(11, 30, 1_700_001_800))])
                .await
                .unwrap(),
            1
        );
        assert_eq!(count().await, 2);
        assert_eq!(
            store
                .record_seats(fall(), &[(Crn(99999), seats(1, 1, 1))])
                .await
                .unwrap(),
            0
        );
        let rows = store.seats(fall(), &[Crn(10001)]).await.unwrap();
        assert_eq!(rows[0].1.enrolled, 11);
        assert_eq!(rows[0].1.as_of.0, 1_700_001_800);
    }

    #[sqlx::test]
    async fn poll_set_is_default_view_plus_saved(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        store
            .upsert_sections(
                fall(),
                &[
                    listing(10001, "COMP 140", "scheduled", vec![timed("MWF", 540, 590)]),
                    listing(10002, "COMP 182", "unscheduled, unsaved", vec![unparsed()]),
                    listing(10003, "MUSI 251", "unscheduled, in a collection", vec![]),
                    listing(
                        10004,
                        "COMP 215",
                        "unscheduled, on a schedule",
                        vec![unparsed()],
                    ),
                    listing(
                        10005,
                        "COMP 310",
                        "scheduled but withdrawn",
                        vec![timed("TR", 600, 660)],
                    ),
                ],
            )
            .await
            .unwrap();
        store
            .withdraw_missing(
                fall(),
                &skyspace_core::code::Subject::new("COMP").unwrap(),
                &[Crn(10001), Crn(10002), Crn(10004)],
            )
            .await
            .unwrap();
        let account = seed_account(&store, "a@rice.edu").await;
        let collection = store
            .create_collection(account, "Saved", None)
            .await
            .unwrap();
        store
            .add_course(account, collection.id, &code("MUSI 251"))
            .await
            .unwrap();
        store
            .create_schedule(account, &schedule("Plan A", &[10004]), None)
            .await
            .unwrap();
        let set = store.poll_set(fall()).await.unwrap();
        let crns: Vec<Crn> = set.iter().map(|(_, c)| *c).collect();
        assert_eq!(crns, vec![Crn(10001), Crn(10003), Crn(10004)]);
    }

    #[sqlx::test]
    async fn poll_windows(pool: PgPool) {
        let store = Store::from_pool(pool);
        seed_term(&store, "202710").await;
        let now = OffsetDateTime::now_utc();
        assert!(
            store
                .poll_windows_active(fall(), now)
                .await
                .unwrap()
                .is_none()
        );
        store
            .put_poll_window(
                fall(),
                "Fall 2026 registration",
                now - Duration::days(1),
                now + Duration::days(1),
                15,
            )
            .await
            .unwrap();
        let window = store
            .poll_windows_active(fall(), now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(window.label, "Fall 2026 registration");
        assert_eq!(window.interval_minutes, 15);
        assert!(
            store
                .poll_windows_active(fall(), now + Duration::days(2))
                .await
                .unwrap()
                .is_none()
        );
    }
}
