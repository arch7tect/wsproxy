use actix_web::{HttpResponse, Responder};

/// Basic health check endpoint
pub async fn health_handler() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy"
    }))
}

/// Readiness check endpoint
pub async fn ready_handler() -> impl Responder {
    // Phase 1: no Redis health check yet
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ready"
    }))
}