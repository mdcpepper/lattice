//! Lattice JSON API Server

use std::{io, process};

use salvo::{
    oapi::{OpenApi, swagger_ui::SwaggerUi},
    prelude::*,
    server::ServerHandle,
    trailing_slash::remove_slash,
};
use thiserror::Error;
use tokio::signal;
use tracing_subscriber::EnvFilter;

use crate::{config::ServerConfig, handlers::healthcheck};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod config;
mod handlers;

/// Lattice JSON API Server entry point
///
/// # Panics
///
/// Panics if the server fails to bind or serve requests
#[tokio::main]
pub async fn main() {
    // Load configuration from .env and CLI arguments
    let config = ServerConfig::load().unwrap_or_else(|e| {
        #[expect(
            clippy::print_stderr,
            reason = "logging not initialized yet, must use eprintln for config errors"
        )]
        {
            eprintln!("Configuration error: {e}");
        }

        process::exit(1);
    });

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level)),
        )
        .init();

    let addr = config.socket_addr();
    tracing::info!("Starting server on {addr}");

    // Bind server
    let listener = TcpListener::new(addr).bind().await;

    // Create router
    let router = Router::new()
        .hoop(CatchPanic::new())
        .hoop(remove_slash())
        .push(Router::with_path("healthcheck").get(healthcheck::handler));

    // Create OpenAPI documentation
    let doc = OpenApi::new("Lattice API", "0.1.0").merge_router(&router);

    // Add documentation routes
    let router = router
        .unshift(doc.into_router("/api-doc/openapi.json"))
        .unshift(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"));

    let server = Server::new(listener);

    let handle = server.handle();

    // Listen for shutdown signal
    tokio::spawn(async move {
        if let Err(error) = listen_shutdown_signal(handle).await {
            tracing::error!("failed to listen for shutdown signal: {error}");
        }
    });

    // Start serving requests
    server.serve(router).await;
}

#[derive(Debug, Error)]
enum ShutdownSignalError {
    #[error("failed to install Ctrl+C handler: {0}")]
    CtrlC(#[source] io::Error),
    #[cfg(unix)]
    #[error("failed to install SIGTERM handler: {0}")]
    SigTerm(#[source] io::Error),
    #[cfg(windows)]
    #[error("failed to install Windows terminate handler: {0}")]
    Terminate(#[source] io::Error),
}

async fn listen_shutdown_signal(handle: ServerHandle) -> Result<(), ShutdownSignalError> {
    // Wait Shutdown Signal
    let ctrl_c = async {
        // Handle Ctrl+C signal
        signal::ctrl_c().await.map_err(ShutdownSignalError::CtrlC)
    };

    #[cfg(unix)]
    let terminate = async {
        // Handle SIGTERM on Unix systems
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .map_err(ShutdownSignalError::SigTerm)?
            .recv()
            .await;
        Ok::<(), ShutdownSignalError>(())
    };

    #[cfg(windows)]
    let terminate = async {
        // Handle Ctrl+C on Windows (alternative implementation)
        signal::windows::ctrl_c()
            .map_err(ShutdownSignalError::Terminate)?
            .recv()
            .await;
        Ok::<(), ShutdownSignalError>(())
    };

    // Wait for either signal to be received
    tokio::select! {
        result = ctrl_c => {
            result?;
            tracing::info!("ctrl_c signal received");
        }
        result = terminate => {
            result?;
            tracing::info!("terminate signal received");
        }
    };

    // Graceful Shutdown Server
    handle.stop_graceful(None);
    Ok(())
}
