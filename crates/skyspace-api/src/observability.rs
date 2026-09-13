//! The log subscriber. Fields are structured, never a formatted sentence,
//! and no line carries a NetID, an email address, a plan name or a course
//! list: identifiers only.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::{SubscriberInitExt, TryInitError};

/// The filter when `SKYSPACE_LOG` is unset.
pub const DEFAULT_FILTER: &str = "info,skyspace=debug";

/// Install the log subscriber. Reads `SKYSPACE_LOG`, default
/// `info,skyspace=debug`. JSON in production, compact in development.
///
/// # Errors
/// Returns an error when a subscriber is already installed.
pub fn init(json: bool) -> Result<(), TryInitError> {
    let filter =
        EnvFilter::try_from_env("SKYSPACE_LOG").unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
    } else {
        registry
            .with(tracing_subscriber::fmt::layer().compact())
            .try_init()
    }
}
