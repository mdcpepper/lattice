//! Lattice JSON API Server

use std::process;

use salvo::{
    affix_state::inject,
    oapi::{
        OpenApi,
        security::{Http, HttpAuthScheme, SecurityScheme},
        swagger_ui::SwaggerUi,
    },
    prelude::*,
    trailing_slash::remove_slash,
};
use tracing::{error, info};

use lattice_app::{
    auth::{OpenBaoClient, OpenBaoConfig},
    context::AppContext,
};

use crate::{config::ServerConfig, observability::Observability, state::State};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod auth;
mod carts;
mod config;
mod extensions;
mod healthcheck;
mod observability;
mod products;
mod promotions;
mod router;
mod shutdown;
mod state;

#[cfg(test)]
mod test_helpers;

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
            reason = "logging not initialised yet, must use eprintln for config errors"
        )]
        {
            eprintln!("Configuration error: {e}");
        }

        process::exit(1);
    });

    let observability = Observability::init(&config).unwrap_or_else(|source| {
        #[expect(
            clippy::print_stderr,
            reason = "logging may be unavailable if subscriber initialisation fails"
        )]
        {
            eprintln!("Observability initialisation error: {source}");
        }

        process::exit(1);
    });

    let addr = config.socket_addr();

    info!("Starting server on {addr}");

    // Bind server
    let listener = TcpListener::new(addr).bind().await;

    let openbao = OpenBaoClient::new(OpenBaoConfig {
        addr: config.auth.addr,
        token: config.auth.token,
        transit_key: config.auth.transit_key,
    });

    let app = match AppContext::from_database_url(&config.database.database_url, openbao).await {
        Ok(app) => app,
        Err(init_error) => {
            error!("failed to initialise app context: {init_error}");

            process::exit(1);
        }
    };

    let api_router = Router::new()
        .hoop(auth::middleware::handler)
        .push(router::app_router());

    let router = Router::new()
        .hoop(observability::request_logging)
        .hoop(CatchPanic::new())
        .hoop(remove_slash())
        .hoop(inject(State::from_app_context(app)))
        .push(Router::with_path("metrics").get(observability::metrics_handler))
        .push(Router::with_path("healthcheck").get(healthcheck::handler))
        .push(api_router);

    let doc = OpenApi::new("Lattice API", "0.3.0")
        .add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
        )
        .merge_router(&router);

    let router = router
        .push(doc.into_router("/api-doc/openapi.json"))
        .push(SwaggerUi::new("/api-doc/openapi.json").into_router("docs"));

    let server = Server::new(listener);

    let handle = server.handle();

    tokio::spawn(async move {
        if let Err(error) = shutdown::listen(handle).await {
            error!("failed to listen for shutdown signal: {error}");
        }
    });

    server.serve(router).await;

    observability.shutdown();
}
