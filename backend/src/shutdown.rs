//! SIGTERM / SIGINT → `CancellationToken`.

use tokio_util::sync::CancellationToken;
use tracing::info;

/// Spawn a task that cancels the returned token on SIGTERM or SIGINT.
pub fn install_signal_handler() -> CancellationToken {
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        let ctrl_c = async {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(%err, "failed to listen for SIGINT");
                std::future::pending::<()>().await;
            }
        };
        #[cfg(unix)]
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(err) => {
                    tracing::error!(%err, "failed to listen for SIGTERM");
                    std::future::pending::<()>().await;
                }
            }
        };
        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => info!("SIGINT received, shutting down"),
            _ = terminate => info!("SIGTERM received, shutting down"),
        }
        cancel.cancel();
    });
    token
}
