use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Setup signal handlers for graceful shutdown
pub fn setup_shutdown_handler() -> CancellationToken {
    let token = CancellationToken::new();
    let token_clone = token.clone();

    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
                token_clone.cancel();
            }
            Err(err) => {
                warn!("Error setting up signal handler: {}", err);
            }
        }
    });

    token
}

/// Wait for graceful shutdown to complete
pub async fn wait_for_shutdown(
    shutdown_token: CancellationToken,
    active_connections: Arc<AtomicUsize>,
    grace_period: Duration,
) {
    shutdown_token.cancelled().await;

    info!("Shutdown initiated, waiting for active connections to finish...");

    let start = tokio::time::Instant::now();
    let check_interval = Duration::from_millis(100);

    loop {
        let active = active_connections.load(Ordering::SeqCst);

        if active == 0 {
            info!("All connections closed gracefully");
            break;
        }

        if start.elapsed() >= grace_period {
            warn!(
                "Grace period expired, forcefully closing {} remaining connections",
                active
            );
            break;
        }

        info!("Waiting for {} active connections to close...", active);
        sleep(check_interval).await;
    }

    info!("Shutdown complete");
}