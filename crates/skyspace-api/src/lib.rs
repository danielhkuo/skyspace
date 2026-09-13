//! The Skyspace HTTP server. `routes/` parses and authorises,
//! `skyspace-store` runs SQL, `skyspace-core` decides.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod auth;
pub mod config;
pub mod dto;
pub mod freshness;
