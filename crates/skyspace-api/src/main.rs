//! The binary: read the configuration, open the pool, serve until a signal.
//! Migrations are not run here; `skyspace migrate` runs them, so two web
//! workers cannot race on the same one.

use std::net::SocketAddr;

use skyspace_api::config::Config;
use skyspace_api::{AppState, Mailer, router};
use skyspace_store::Store;

const POOL_SIZE: u32 = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("skyspace-api: {error}");
            std::process::exit(1);
        }
    };
    skyspace_api::observability::init(config.log_json)?;
    let store = Store::connect(&config.database_url, POOL_SIZE).await?;
    let mailer = Mailer::from_config(config.smtp.as_ref())?;
    let bind = config.bind;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(bind = %bind, "listening");
    let app = router(AppState::new(store, config, mailer));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// Resolves on `ctrl-c` or `SIGTERM`.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %error, "ctrl-c handler failed");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::warn!(error = %error, "sigterm handler failed"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutting down");
}
