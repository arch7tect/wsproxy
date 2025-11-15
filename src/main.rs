mod auth;
mod config;
pub mod error;
mod handlers;
mod redis;
mod shutdown;
mod state;
mod ws;

use actix_web::{web, App, HttpServer};
use config::Config;
use redis::RedisClient;
use shutdown::{setup_shutdown_handler, wait_for_shutdown};
use state::AppState;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = Config::from_env().expect("Failed to load configuration");

    init_logging(&config);

    info!("Starting wsproxy...");
    info!(config=?config, "Configuration loaded");

    let shutdown_token = setup_shutdown_handler();

    let redis_client = RedisClient::new(&config.redis_url).expect("Failed to create Redis client");
    info!("Redis client created");

    let app_state = AppState::new(config.clone(), redis_client, shutdown_token.clone());
    let app_state_clone = app_state.clone();

    let bind_address = format!("{}:{}", config.host, config.port);
    info!(bind_address=%bind_address, "Starting HTTP server");

    let server = HttpServer::new(move || {
        App::new()
            .wrap(tracing_actix_web::TracingLogger::default())
            .app_data(web::Data::new(app_state_clone.clone()))
            .route("/health", web::get().to(handlers::health_handler))
            .route("/ready", web::get().to(handlers::ready_handler))
            .route(
                "/{agent_id}/ws/{session_id}",
                web::get().to(handlers::websocket_handler),
            )
    })
    .bind(&bind_address)?
    .run();

    let server_handle = server.handle();
    let server_task = tokio::spawn(server);

    info!(bind_address=%bind_address, "wsproxy is running");
    info!("Press Ctrl+C to stop");

    shutdown_token.cancelled().await;
    info!("Shutdown signal received, stopping server...");

    server_handle.stop(true).await;

    wait_for_shutdown(
        shutdown_token,
        app_state.active_connections,
        app_state.config.shutdown_grace_period(),
    )
    .await;

    server_task.await.ok();

    info!("wsproxy stopped");

    Ok(())
}

fn init_logging(config: &Config) {
    use tracing_subscriber::{fmt, registry, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    registry()
        .with(filter)
        .with(fmt::layer().with_target(true).with_level(true))
        .init();

    let rust_log = std::env::var("RUST_LOG").ok();
    tracing::info!(rust_log=?rust_log, "RUST_LOG");
}
