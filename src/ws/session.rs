use crate::redis::{subscribe_to_channel, RedisClient};
use crate::ws::messages::{RedisMessage, Shutdown};
use actix::prelude::*;
use actix_web_actors::ws;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket session actor
pub struct WsSession {
    /// Unique ID for this session instance
    pub id: Uuid,
    /// Session ID from URL path
    pub session_id: String,
    /// Agent ID from URL path
    pub agent_id: String,
    /// Redis client
    pub redis_client: RedisClient,
    /// Cancellation token for shutdown
    pub shutdown_token: CancellationToken,
    /// Last heartbeat timestamp
    pub hb: Instant,
    /// Heartbeat interval
    pub hb_interval: Duration,
    /// Heartbeat timeout
    pub hb_timeout: Duration,
    /// Connection counter callback
    pub on_disconnect: Option<Box<dyn Fn() + Send>>,
}

impl WsSession {
    pub fn new(
        session_id: String,
        agent_id: String,
        redis_client: RedisClient,
        shutdown_token: CancellationToken,
        hb_interval: Duration,
        hb_timeout: Duration,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            agent_id,
            redis_client,
            shutdown_token,
            hb: Instant::now(),
            hb_interval,
            hb_timeout,
            on_disconnect: None,
        }
    }

    pub fn set_on_disconnect<F>(&mut self, f: F)
    where
        F: Fn() + Send + 'static,
    {
        self.on_disconnect = Some(Box::new(f));
    }

    /// Start heartbeat process
    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let interval = self.hb_interval;
        let timeout = self.hb_timeout;

        ctx.run_interval(interval, move |act, ctx| {
            if Instant::now().duration_since(act.hb) > timeout {
                warn!(
                    session_id=%act.session_id,
                    "WebSocket heartbeat timeout, closing connection"
                );
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    /// Subscribe to Redis Pub/Sub channel
    fn subscribe_to_redis(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let session_id = self.session_id.clone();
        let redis_client = self.redis_client.clone();
        let shutdown_token = self.shutdown_token.clone();
        let addr = ctx.address();

        ctx.spawn(
            async move {
                match redis_client.get_pubsub().await {
                    Ok(pubsub) => {
                        let channel = format!("session:{}:down", session_id);
                        info!(channel=%channel, "Starting Redis subscription");

                        subscribe_to_channel(channel, addr, pubsub, shutdown_token).await;
                    }
                    Err(e) => {
                        error!(error=%e, "Failed to get Redis pubsub connection");
                    }
                }
            }
            .into_actor(self),
        );
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            id=%self.id, session_id=%self.session_id, agent_id=%self.agent_id,
            "WebSocket session started"
        );

        self.start_heartbeat(ctx);
        self.subscribe_to_redis(ctx);

        let shutdown_token = self.shutdown_token.clone();
        ctx.spawn(
            async move {
                shutdown_token.cancelled().await;
            }
            .into_actor(self)
            .map(|_, _act, ctx| {
                info!("Shutdown signal received, closing WebSocket session");
                ctx.stop();
            }),
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            id=%self.id, session_id=%self.session_id,
            "WebSocket session stopped"
        );

        if let Some(ref callback) = self.on_disconnect {
            callback();
        }
    }
}

/// Handle messages from WebSocket client
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(msg)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                debug!(text=%text, "Received text message from client");
                // Phase 1: upstream message handling not implemented yet
            }
            Ok(ws::Message::Binary(_)) => {
                warn!("Binary messages not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                info!(reason=?reason, "Client closed connection");
                ctx.close(reason);
                ctx.stop();
            }
            Err(e) => {
                error!(error=%e, "WebSocket protocol error");
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// Handle messages from Redis Pub/Sub
impl Handler<RedisMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: RedisMessage, ctx: &mut Self::Context) {
        debug!(payload=?msg.payload, "Forwarding message from Redis to WebSocket");
        ctx.text(msg.payload);
    }
}

/// Handle shutdown message
impl Handler<Shutdown> for WsSession {
    type Result = ();

    fn handle(&mut self, _msg: Shutdown, ctx: &mut Self::Context) {
        info!("Received shutdown message, closing session");
        ctx.stop();
    }
}