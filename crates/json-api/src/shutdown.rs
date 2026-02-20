//! Graceful shutdown signal handling

use std::io;

use salvo::server::ServerHandle;
use thiserror::Error;
use tokio::signal;

#[derive(Debug, Error)]
pub(crate) enum ShutdownSignalError {
    #[error("failed to install Ctrl+C handler: {0}")]
    CtrlC(#[source] io::Error),

    #[cfg(unix)]
    #[error("failed to install SIGTERM handler: {0}")]
    SigTerm(#[source] io::Error),

    #[cfg(windows)]
    #[error("failed to install Windows terminate handler: {0}")]
    Terminate(#[source] io::Error),
}

pub(crate) async fn listen(handle: ServerHandle) -> Result<(), ShutdownSignalError> {
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
