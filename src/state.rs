use crate::config::Config;
use crate::redis::RedisClient;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Application-wide shared state
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub redis_client: RedisClient,
    pub shutdown_token: CancellationToken,
    pub active_connections: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new(
        config: Config,
        redis_client: RedisClient,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            redis_client,
            shutdown_token,
            active_connections: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn increment_connections(&self) -> usize {
        self.active_connections.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn decrement_connections(&self) -> usize {
        self.active_connections.fetch_sub(1, Ordering::SeqCst) - 1
    }

    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::SeqCst)
    }
}
