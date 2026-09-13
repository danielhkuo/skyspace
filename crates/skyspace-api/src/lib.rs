//! The Skyspace HTTP server. `routes/` parses and authorises,
//! `skyspace-store` runs SQL, `skyspace-core` decides.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod auth;
pub mod config;
pub mod dto;
pub mod error;
pub mod freshness;
pub mod guard;
pub mod observability;
pub mod routes;
pub mod session;
pub mod state;

pub use routes::router;
pub use state::{AppState, MailError, Mailer, SentCode};
