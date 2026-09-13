//! The one error type callers see. `sqlx` types never cross the crate
//! boundary except wrapped here.

use core::fmt::Display;

/// Why a store call failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The database refused or the connection failed.
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    /// A migration could not be applied.
    #[error("migration: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A stored row does not convert into its core type. Names the column so
    /// the fix is one `psql` query away.
    #[error("corrupt row in {table}.{column}: {detail}")]
    Corrupt {
        /// The table the row came from.
        table: &'static str,
        /// The column that failed.
        column: &'static str,
        /// What was wrong with it.
        detail: String,
    },
    /// A `jsonb` document does not deserialise into its core type.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A caller-supplied value cannot be stored: a credit total past
    /// `smallint`, a timestamp outside the calendar.
    #[error("invalid input: {0}")]
    Input(String),
    /// A name the account already uses for a collection.
    #[error("that name is already in use")]
    DuplicateName,
}

/// Build a `Corrupt` error for one column.
pub(crate) fn corrupt(
    table: &'static str,
    column: &'static str,
    detail: impl Display,
) -> StoreError {
    StoreError::Corrupt {
        table,
        column,
        detail: detail.to_string(),
    }
}

/// True when `error` is a unique violation on the named constraint or index.
pub(crate) fn violates(error: &sqlx::Error, constraint: &str) -> bool {
    match error {
        sqlx::Error::Database(db) => {
            db.is_unique_violation() && db.constraint() == Some(constraint)
        }
        _ => false,
    }
}
