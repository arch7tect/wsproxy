use crate::state::AppState;
use actix_web::{web, HttpResponse, Responder};

/// Health check endpoint with Redis connectivity check
pub async fn health_handler(app_state: web::Data<AppState>) -> impl Responder {
    match app_state.redis_client.check_connection().await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "status": "healthy",
            "redis": "connected",
            "active_connections": app_state.get_active_connections()
        })),
        Err(e) => {
            tracing::error!(error = %e, "Redis health check failed");
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "status": "unhealthy",
                "redis": "disconnected",
                "error": e.to_string()
            }))
        }
    }
}

/// Readiness check endpoint (simple liveness probe)
pub async fn ready_handler() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ready"
    }))
}
