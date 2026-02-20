//! Lattice JSON API Server

use std::process;

use salvo::{
    affix_state::inject,
    oapi::{OpenApi, swagger_ui::SwaggerUi},
    prelude::*,
    trailing_slash::remove_slash,
};
use tracing::error;
use tracing_subscriber::EnvFilter;

use crate::{config::ServerConfig, state::State};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod config;
mod database;
mod extensions;
mod healthcheck;
mod products;
mod shutdown;
mod state;

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

    let pool = match database::connect(&config.database_url).await {
        Ok(pool) => pool,
        Err(db_error) => {
            error!("failed to connect to postgres: {db_error}");
            process::exit(1);
        }
    };

    let state = State::from_pool(pool);

    let router = Router::new()
        .hoop(CatchPanic::new())
        .hoop(remove_slash())
        .hoop(inject(state))
        .push(Router::with_path("healthcheck").get(healthcheck::handler))
        .push(Router::with_path("products").get(products::index::handler));

    let doc = OpenApi::new("Lattice API", "0.1.0").merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"));

    let server = Server::new(listener);

    let handle = server.handle();

    // Listen for shutdown signal
    tokio::spawn(async move {
        if let Err(error) = shutdown::listen(handle).await {
            error!("failed to listen for shutdown signal: {error}");
        }
    });

    // Start serving requests
    server.serve(router).await;
}
