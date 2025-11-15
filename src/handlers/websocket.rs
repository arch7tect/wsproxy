use crate::auth::validate_bearer_token;
use crate::error::AuthError;
use crate::state::AppState;
use crate::ws::WsSession;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use tracing::{error, info, warn};

/// WebSocket upgrade handler
/// URL format: /{agent_id}/ws/{session_id}
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String, String)>,
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let (agent_id, session_id) = path.into_inner();

    info!(
        agent_id = %agent_id,
        session_id = %session_id,
        "New WebSocket connection request"
    );

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    match validate_bearer_token(
        &session_id,
        auth_header,
        &app_state.redis_client,
        app_state.config.auth_timeout(),
    )
    .await
    {
        Ok(()) => {
            info!(session_id = %session_id, "Authentication successful");
        }
        Err(AuthError::MissingToken) => {
            warn!(session_id = %session_id, "Missing authorization token");
            return Ok(HttpResponse::Unauthorized().body("Missing authorization token"));
        }
        Err(AuthError::InvalidToken) => {
            warn!(session_id = %session_id, "Invalid authorization token");
            return Ok(HttpResponse::Forbidden().body("Invalid authorization token"));
        }
        Err(AuthError::Timeout) => {
            error!(session_id = %session_id, "Authentication timeout");
            return Ok(HttpResponse::GatewayTimeout().body("Authentication timeout"));
        }
        Err(AuthError::RedisError(e)) => {
            error!(session_id = %session_id, error = %e, "Redis error during authentication");
            return Ok(HttpResponse::InternalServerError().body("Internal server error"));
        }
    }

    let count = app_state.increment_connections();
    info!(active_connections = count, "New connection");

    let mut session = WsSession::new(
        session_id.clone(),
        agent_id.clone(),
        app_state.redis_client.clone(),
        app_state.shutdown_token.clone(),
        app_state.config.ws_ping_interval(),
        app_state.config.ws_ping_timeout(),
        app_state.config.max_message_size(),
    );

    let app_state_clone = app_state.clone();
    session.set_on_disconnect(move || {
        let count = app_state_clone.decrement_connections();
        info!(active_connections = count, "Connection closed");
    });

    let resp = ws::start(session, &req, stream)?;

    Ok(resp)
}
