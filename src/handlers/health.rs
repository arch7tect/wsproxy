use crate::state::AppState;
use actix_web::{web, HttpResponse, Responder};

/// Basic health check endpoint
pub async fn health_handler(app_state: web::Data<AppState>) -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "active_connections": app_state.get_active_connections()
    }))
}

/// Readiness check endpoint
pub async fn ready_handler() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ready"
    }))
}
