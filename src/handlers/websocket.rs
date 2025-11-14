use crate::state::AppState;
use crate::ws::WsSession;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use tracing::info;

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

    let count = app_state.increment_connections();
    info!(active_connections = count, "New connection");

    let mut session = WsSession::new(
        session_id.clone(),
        agent_id.clone(),
        app_state.redis_client.clone(),
        app_state.shutdown_token.clone(),
        app_state.config.ws_ping_interval(),
        app_state.config.ws_ping_timeout(),
    );

    let app_state_clone = app_state.clone();
    session.set_on_disconnect(move || {
        let count = app_state_clone.decrement_connections();
        info!(active_connections = count, "Connection closed");
    });

    let resp = ws::start(session, &req, stream)?;

    Ok(resp)
}