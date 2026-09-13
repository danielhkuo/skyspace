//! Sessions and the emailed sign-in code. Only SHA-256 digests are stored.

use std::net::IpAddr;

use skyspace_core::plan::AccountId;
use time::{Duration, OffsetDateTime};

use crate::error::StoreError;
use crate::pool::Store;

/// A session joined to its account, for the `Session` extractor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// Whose.
    pub account_id: AccountId,
    /// The account's address.
    pub email: String,
    /// When the cookie stops working.
    pub expires_at: OffsetDateTime,
    /// Last request, written at most once an hour.
    pub last_seen_at: OffsetDateTime,
}

#[derive(sqlx::FromRow)]
struct SessionDbRow {
    account_id: uuid::Uuid,
    email: String,
    expires_at: OffsetDateTime,
    last_seen_at: OffsetDateTime,
}

/// What storing a sign-in code did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutCodeOutcome {
    /// The code is live; this many sends in the current 15-minute window,
    /// this one included.
    Stored {
        /// Sends in the window.
        sends_in_window: u16,
    },
    /// The address already had three sends this window; nothing changed.
    Limited {
        /// When the window opened, for `Retry-After`.
        window_started_at: OffsetDateTime,
    },
}

/// What presenting a sign-in code did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeCodeOutcome {
    /// Right code; the row is gone.
    Accepted,
    /// Wrong code; after five the row is burnt and `attempts_left` is 0.
    Wrong {
        /// Tries left before the code burns.
        attempts_left: u8,
    },
    /// The code was past its expiry; the row is gone.
    Expired,
    /// No live code for the address.
    Missing,
}

/// Sends allowed per address per window.
pub const SENDS_PER_ADDRESS: i16 = 3;
/// The per-address window.
pub const ADDRESS_WINDOW: Duration = Duration::minutes(15);
/// Wrong attempts before a code burns.
pub const MAX_ATTEMPTS: i16 = 5;

#[derive(sqlx::FromRow)]
struct CodeRow {
    code_sha256: Vec<u8>,
    expires_at: OffsetDateTime,
    attempts: i16,
}

impl Store {
    /// Open a session for an account under a token's digest.
    ///
    /// # Errors
    /// `StoreError::Database`, including a unique violation for a digest
    /// already in use.
    pub async fn create_session(
        &self,
        account: AccountId,
        token_sha256: &[u8; 32],
        expires_at: OffsetDateTime,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "insert into sessions (token_sha256, account_id, expires_at) values ($1, $2, $3)",
        )
        .bind(token_sha256.to_vec())
        .bind(account.0)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The session under a token's digest, with its account. Expiry is
    /// returned, not enforced: the extractor compares it with its clock.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn session_by_token(
        &self,
        token_sha256: &[u8; 32],
    ) -> Result<Option<SessionRow>, StoreError> {
        let row: Option<SessionDbRow> = sqlx::query_as(
            "select s.account_id, a.email, s.expires_at, s.last_seen_at \
             from sessions s join accounts a on a.id = s.account_id where s.token_sha256 = $1",
        )
        .bind(token_sha256.to_vec())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| SessionRow {
            account_id: AccountId(r.account_id),
            email: r.email,
            expires_at: r.expires_at,
            last_seen_at: r.last_seen_at,
        }))
    }

    /// Write `last_seen_at` and extend `expires_at`. `false` when the
    /// session is unknown.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn touch_session(
        &self,
        token_sha256: &[u8; 32],
        now: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<bool, StoreError> {
        let done = sqlx::query(
            "update sessions set last_seen_at = $2, expires_at = $3 where token_sha256 = $1",
        )
        .bind(token_sha256.to_vec())
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Sign out: delete the session. `false` when it was already gone.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_session(&self, token_sha256: &[u8; 32]) -> Result<bool, StoreError> {
        let done = sqlx::query("delete from sessions where token_sha256 = $1")
            .bind(token_sha256.to_vec())
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Delete sessions past their expiry. Returns the number deleted.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn delete_expired_sessions(&self, now: OffsetDateTime) -> Result<u64, StoreError> {
        let done = sqlx::query("delete from sessions where expires_at <= $1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected())
    }

    /// Store a sign-in code for an address, replacing any live one. The
    /// send counter bumps inside a live 15-minute window and resets outside
    /// it; the fourth send in a window is refused and nothing changes.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn put_login_code(
        &self,
        email: &str,
        code_sha256: &[u8; 32],
        expires_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<PutCodeOutcome, StoreError> {
        let window_start = now - ADDRESS_WINDOW;
        let sends: Option<i16> = sqlx::query_scalar(
            "insert into login_codes (email, code_sha256, expires_at, attempts, sends_in_window, window_started_at) \
             values (lower($1), $2, $3, 0, 1, $4) \
             on conflict (email) do update set \
                code_sha256 = excluded.code_sha256, expires_at = excluded.expires_at, attempts = 0, \
                sends_in_window = case when login_codes.window_started_at > $5 \
                    then login_codes.sends_in_window + 1 else 1 end, \
                window_started_at = case when login_codes.window_started_at > $5 \
                    then login_codes.window_started_at else $4 end \
             where login_codes.window_started_at <= $5 or login_codes.sends_in_window < $6 \
             returning sends_in_window",
        )
        .bind(email)
        .bind(code_sha256.to_vec())
        .bind(expires_at)
        .bind(now)
        .bind(window_start)
        .bind(SENDS_PER_ADDRESS)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(sends) = sends {
            return Ok(PutCodeOutcome::Stored {
                sends_in_window: u16::try_from(sends).unwrap_or(0),
            });
        }
        let started: OffsetDateTime =
            sqlx::query_scalar("select window_started_at from login_codes where email = lower($1)")
                .bind(email)
                .fetch_one(&self.pool)
                .await?;
        Ok(PutCodeOutcome::Limited {
            window_started_at: started,
        })
    }

    /// Present a code. The right code consumes the row; a wrong one counts
    /// an attempt, and the fifth wrong attempt burns the row.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn take_login_code(
        &self,
        email: &str,
        code_sha256: &[u8; 32],
        now: OffsetDateTime,
    ) -> Result<TakeCodeOutcome, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row: Option<CodeRow> = sqlx::query_as(
            "select code_sha256, expires_at, attempts from login_codes where email = lower($1) for update",
        )
        .bind(email)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(TakeCodeOutcome::Missing);
        };
        let delete = sqlx::query("delete from login_codes where email = lower($1)").bind(email);
        let outcome = if row.expires_at <= now {
            delete.execute(&mut *tx).await?;
            TakeCodeOutcome::Expired
        } else if row.code_sha256.as_slice() == code_sha256 {
            delete.execute(&mut *tx).await?;
            TakeCodeOutcome::Accepted
        } else {
            let attempts = row.attempts.saturating_add(1);
            if attempts >= MAX_ATTEMPTS {
                delete.execute(&mut *tx).await?;
                TakeCodeOutcome::Wrong { attempts_left: 0 }
            } else {
                sqlx::query("update login_codes set attempts = $2 where email = lower($1)")
                    .bind(email)
                    .bind(attempts)
                    .execute(&mut *tx)
                    .await?;
                TakeCodeOutcome::Wrong {
                    attempts_left: u8::try_from(MAX_ATTEMPTS - attempts).unwrap_or(0),
                }
            }
        };
        tx.commit().await?;
        Ok(outcome)
    }

    /// Sends from an address in the window ending at `now`; 0 outside one.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn count_recent_ip_sends(
        &self,
        ip: IpAddr,
        now: OffsetDateTime,
        window: Duration,
    ) -> Result<u16, StoreError> {
        let sends: Option<i16> = sqlx::query_scalar(
            "select sends from login_ip_windows where ip = $1::inet and window_started_at > $2",
        )
        .bind(ip.to_string())
        .bind(now - window)
        .fetch_optional(&self.pool)
        .await?;
        Ok(sends.and_then(|s| u16::try_from(s).ok()).unwrap_or(0))
    }

    /// Count one send from an address, opening a new window when the old
    /// one has ended. Returns sends in the window, this one included.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn record_ip_send(
        &self,
        ip: IpAddr,
        now: OffsetDateTime,
        window: Duration,
    ) -> Result<u16, StoreError> {
        let sends: i16 = sqlx::query_scalar(
            "insert into login_ip_windows (ip, sends, window_started_at) values ($1::inet, 1, $2) \
             on conflict (ip) do update set \
                sends = case when login_ip_windows.window_started_at > $3 then login_ip_windows.sends + 1 else 1 end, \
                window_started_at = case when login_ip_windows.window_started_at > $3 \
                    then login_ip_windows.window_started_at else $2 end \
             returning sends",
        )
        .bind(ip.to_string())
        .bind(now)
        .bind(now - window)
        .fetch_one(&self.pool)
        .await?;
        Ok(u16::try_from(sends).unwrap_or(u16::MAX))
    }

    /// The nightly sweep: delete codes that expired and address windows
    /// that opened before `older_than`. Returns rows deleted.
    ///
    /// # Errors
    /// `StoreError::Database`.
    pub async fn expire_login_codes(&self, older_than: OffsetDateTime) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let codes = sqlx::query("delete from login_codes where expires_at < $1")
            .bind(older_than)
            .execute(&mut *tx)
            .await?;
        let ips = sqlx::query("delete from login_ip_windows where window_started_at < $1")
            .bind(older_than)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(codes.rows_affected() + ips.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};

    use super::{PutCodeOutcome, TakeCodeOutcome};
    use crate::convert::sha256;
    use crate::pool::Store;
    use crate::testing::seed_account;

    #[sqlx::test]
    async fn sessions_by_hash(pool: PgPool) {
        let store = Store::from_pool(pool);
        let account = seed_account(&store, "a@rice.edu").await;
        let token = sha256(b"token");
        let now = OffsetDateTime::now_utc();
        store
            .create_session(account, &token, now + Duration::days(30))
            .await
            .unwrap();
        assert!(store.create_session(account, &token, now).await.is_err());
        let session = store.session_by_token(&token).await.unwrap().unwrap();
        assert_eq!(session.account_id, account);
        assert_eq!(session.email, "a@rice.edu");
        assert!(
            store
                .session_by_token(&sha256(b"other"))
                .await
                .unwrap()
                .is_none()
        );
        let later = now + Duration::hours(2);
        assert!(
            store
                .touch_session(&token, later, later + Duration::days(30))
                .await
                .unwrap()
        );
        assert_eq!(
            store
                .session_by_token(&token)
                .await
                .unwrap()
                .unwrap()
                .last_seen_at,
            later
        );
        assert_eq!(store.delete_expired_sessions(now).await.unwrap(), 0);
        assert_eq!(
            store
                .delete_expired_sessions(later + Duration::days(31))
                .await
                .unwrap(),
            1
        );
        store
            .create_session(account, &token, now + Duration::days(30))
            .await
            .unwrap();
        assert!(store.delete_session(&token).await.unwrap());
        assert!(!store.delete_session(&token).await.unwrap());
    }

    #[sqlx::test]
    async fn login_codes_burn_after_five_wrong_attempts(pool: PgPool) {
        let store = Store::from_pool(pool);
        let now = OffsetDateTime::now_utc();
        let code = sha256(b"123456");
        let wrong = sha256(b"000000");
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &code, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Missing
        );
        let stored = store
            .put_login_code("A@rice.edu", &code, now + Duration::minutes(10), now)
            .await
            .unwrap();
        assert_eq!(stored, PutCodeOutcome::Stored { sends_in_window: 1 });
        for left in [4, 3, 2, 1] {
            assert_eq!(
                store
                    .take_login_code("a@rice.edu", &wrong, now)
                    .await
                    .unwrap(),
                TakeCodeOutcome::Wrong {
                    attempts_left: left
                }
            );
        }
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &wrong, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Wrong { attempts_left: 0 }
        );
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &code, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Missing
        );

        store
            .put_login_code("a@rice.edu", &code, now + Duration::minutes(10), now)
            .await
            .unwrap();
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &code, now + Duration::minutes(11))
                .await
                .unwrap(),
            TakeCodeOutcome::Expired
        );
        store
            .put_login_code("a@rice.edu", &code, now + Duration::minutes(10), now)
            .await
            .unwrap();
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &wrong, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Wrong { attempts_left: 4 }
        );
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &code, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Accepted
        );
        assert_eq!(
            store
                .take_login_code("a@rice.edu", &code, now)
                .await
                .unwrap(),
            TakeCodeOutcome::Missing
        );
    }

    #[sqlx::test]
    async fn send_limits_per_address_and_ip(pool: PgPool) {
        let store = Store::from_pool(pool);
        let now = OffsetDateTime::now_utc();
        let code = sha256(b"1");
        let expires = now + Duration::minutes(10);
        for expected in [1, 2, 3] {
            assert_eq!(
                store
                    .put_login_code("a@rice.edu", &code, expires, now)
                    .await
                    .unwrap(),
                PutCodeOutcome::Stored {
                    sends_in_window: expected
                }
            );
        }
        assert!(matches!(
            store
                .put_login_code("a@rice.edu", &code, expires, now + Duration::minutes(1))
                .await
                .unwrap(),
            PutCodeOutcome::Limited { .. }
        ));
        let later = now + Duration::minutes(16);
        assert_eq!(
            store
                .put_login_code("a@rice.edu", &code, later + Duration::minutes(10), later)
                .await
                .unwrap(),
            PutCodeOutcome::Stored { sends_in_window: 1 }
        );

        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));
        let hour = Duration::hours(1);
        assert_eq!(store.count_recent_ip_sends(ip, now, hour).await.unwrap(), 0);
        assert_eq!(store.record_ip_send(ip, now, hour).await.unwrap(), 1);
        assert_eq!(
            store
                .record_ip_send(ip, now + Duration::minutes(5), hour)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .count_recent_ip_sends(ip, now + Duration::minutes(6), hour)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            store
                .count_recent_ip_sends(ip, now + Duration::hours(2), hour)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .record_ip_send(ip, now + Duration::hours(2), hour)
                .await
                .unwrap(),
            1
        );

        assert_eq!(
            store
                .expire_login_codes(now + Duration::days(1))
                .await
                .unwrap(),
            2
        );
    }
}
