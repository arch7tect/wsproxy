use crate::redis::{PubSubManager, PubSubMessage, RedisClient, RegisterSession, UnregisterSession};
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
    /// PubSub manager
    pub pubsub_manager: Addr<PubSubManager>,
    /// Cancellation token for shutdown
    pub shutdown_token: CancellationToken,
    /// Last heartbeat timestamp
    pub hb: Instant,
    /// Heartbeat interval
    pub hb_interval: Duration,
    /// Heartbeat timeout
    pub hb_timeout: Duration,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Connection counter callback
    pub on_disconnect: Option<Box<dyn Fn() + Send>>,
    /// Count of messages forwarded from PubSub to the WebSocket
    pub ws_sent: usize,
    /// Count of messages received from WebSocket/published upstream
    pub ws_recv: usize,
    /// Count of upstream publishes that succeeded
    pub redis_published_up: usize,
    /// Count of upstream publishes that failed
    pub redis_publish_failures: usize,
}

impl WsSession {
    pub fn new(
        session_id: String,
        agent_id: String,
        redis_client: RedisClient,
        pubsub_manager: Addr<PubSubManager>,
        shutdown_token: CancellationToken,
        hb_interval: Duration,
        hb_timeout: Duration,
        max_message_size: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            agent_id,
            redis_client,
            pubsub_manager,
            shutdown_token,
            hb: Instant::now(),
            hb_interval,
            hb_timeout,
            max_message_size,
            on_disconnect: None,
            ws_sent: 0,
            ws_recv: 0,
            redis_published_up: 0,
            redis_publish_failures: 0,
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

    /// Register with PubSubManager
    fn register_with_pubsub_manager(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let session_id = self.session_id.clone();
        let pubsub_manager = self.pubsub_manager.clone();
        let addr = ctx.address().recipient();

        info!(session_id=%session_id, "Registering with PubSubManager");

        pubsub_manager.do_send(RegisterSession {
            session_id,
            addr,
        });
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
        self.register_with_pubsub_manager(ctx);

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
            ws_sent=%self.ws_sent,
            ws_recv=%self.ws_recv,
            redis_published_up=%self.redis_published_up,
            redis_publish_failures=%self.redis_publish_failures,
            "WebSocket session stopped"
        );

        self.pubsub_manager.do_send(UnregisterSession {
            session_id: self.session_id.clone(),
            ws_recv_messages: self.ws_recv,
            ws_sent_messages: self.ws_sent,
            redis_down_messages: self.ws_sent,
            redis_up_messages: self.redis_published_up,
        });

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
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.hb = Instant::now();

                if text.len() > self.max_message_size {
                    warn!(
                        session_id=%self.session_id,
                        message_size=text.len(),
                        max_size=self.max_message_size,
                        "Message exceeds maximum size"
                    );
                    ctx.text(
                        serde_json::json!({
                            "error": "Message too large",
                            "max_size": self.max_message_size
                        })
                        .to_string(),
                    );
                    return;
                }

                if let Err(e) = serde_json::from_str::<serde_json::Value>(&text) {
                    warn!(
                        session_id=%self.session_id,
                        error=%e,
                        "Invalid JSON message from client"
                    );
                    ctx.text(
                        serde_json::json!({
                            "error": "Invalid JSON format"
                        })
                        .to_string(),
                    );
                    return;
                }

                debug!(
                    session_id=%self.session_id,
                    message_size=text.len(),
                    "Publishing client message to upstream channel"
                );
                self.ws_recv += 1;

                let channel = format!("session:{}:up", self.session_id);
                let redis_client = self.redis_client.clone();
                let text_clone = text.to_string();
                let session_id = self.session_id.clone();
                let publish_channel = channel.clone();

                ctx.spawn(
                    async move {
                        (session_id, channel.clone(), redis_client.publish(&publish_channel, &text_clone).await)
                    }
                    .into_actor(self)
                    .map(move |(session_id, channel, result), act, _ctx| {
                        match result {
                            Ok(_) => {
                                act.redis_published_up += 1;
                            }
                            Err(e) => {
                                act.redis_publish_failures += 1;
                                error!(
                                    session_id=%session_id,
                                    channel=%channel,
                                    error=%e,
                                    "Failed to publish message to Redis"
                                );
                            }
                        }
                    }),
                );
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

/// Handle messages from PubSubManager
impl Handler<PubSubMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: PubSubMessage, ctx: &mut Self::Context) {
        debug!(
            session_id=%self.session_id,
            payload=%msg.payload,
            "Received message from PubSubManager, forwarding to WebSocket"
        );
        self.ws_sent += 1;
        ctx.text(msg.payload);
    }
}

/// Handle messages from Redis Pub/Sub (legacy, kept for compatibility)
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
