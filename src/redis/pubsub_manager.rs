use actix::prelude::*;
use futures::StreamExt;
use redis::aio::PubSub;
use redis::Msg;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

/// Message delivered to a WebSocket/session actor when a Redis PubSub
/// message is received for its session.
pub struct PubSubMessage {
    pub payload: String,
}

impl Message for PubSubMessage {
    type Result = ();
}

/// Public message to register a session in the PubSubManager.
///
/// The manager will:
/// - choose a PubSub connection by hashing `session_id`
/// - tell that connection actor to:
///   - track this `Recipient`
///   - subscribe to the Redis channel for that session
pub struct RegisterSession {
    pub session_id: String,
    pub addr: Recipient<PubSubMessage>,
}

impl Message for RegisterSession {
    type Result = ();
}

/// Public message to unregister a session in the PubSubManager.
///
/// The manager will:
/// - choose the same PubSub connection by hashing `session_id`
/// - tell that connection actor to:
///   - remove this session from its map
///   - unsubscribe from the Redis channel (if desired)
pub struct UnregisterSession {
    pub session_id: String,
    pub ws_recv_messages: usize,
    pub ws_sent_messages: usize,
    pub redis_down_messages: usize,
    pub redis_up_messages: usize,
}

impl Message for UnregisterSession {
    type Result = ();
}

/// Public message to ensure the Redis subscription for a session exists
/// before completing WebSocket handshakes.
pub struct EnsureSubscription {
    pub session_id: String,
}

impl Message for EnsureSubscription {
    type Result = Result<(), String>;
}

/// Internal message: register a session on a specific PubSub connection actor.
struct ConnectionRegisterSession {
    pub session_id: String,
    pub addr: Recipient<PubSubMessage>,
}

impl Message for ConnectionRegisterSession {
    type Result = ();
}

/// Internal message: unregister a session on a specific PubSub connection actor.
struct ConnectionUnregisterSession {
    pub session_id: String,
    pub ws_recv_messages: usize,
    pub ws_sent_messages: usize,
    pub redis_down_messages: usize,
    pub redis_up_messages: usize,
}

impl Message for ConnectionUnregisterSession {
    type Result = ();
}

/// Internal message: ensure a Redis subscription exists for the session.
struct ConnectionEnsureSubscription {
    pub session_id: String,
}

impl Message for ConnectionEnsureSubscription {
    type Result = Result<(), String>;
}

/// Internal message: incoming Redis message for this connection.
/// The background worker that owns `PubSub` sends this into the actor.
struct DeliverPubSubMessage {
    pub msg: Msg,
}

impl Message for DeliverPubSubMessage {
    type Result = ();
}

/// Commands sent from the actor to the background worker that owns PubSub.
enum WorkerCommand {
    Subscribe { session_id: String },
    Unsubscribe { session_id: String },
}

/// Result sent back from worker after processing subscription commands.
struct SubscriptionCommandResult {
    pub session_id: String,
    pub result: Result<(), String>,
}

impl Message for SubscriptionCommandResult {
    type Result = ();
}

/// Actor that owns the mapping of sessions to recipients and controls
/// a background worker that owns a single Redis PubSub connection.
///
/// Responsibilities:
/// - Maintain `sessions: HashMap<session_id, Recipient<PubSubMessage>>`
/// - Send subscribe/unsubscribe commands to the worker
/// - Receive Redis messages from the worker via `DeliverPubSubMessage`
///   and forward them to the appropriate session recipient.
struct SessionEntry {
    recipient: Recipient<PubSubMessage>,
    downstream_forwarded: usize,
}

struct PubSubConnectionActor {
    /// Index of this connection in the pool (for logging only).
    conn_idx: usize,
    /// Map session_id -> Recipient.
    sessions: HashMap<String, SessionEntry>,
    /// Sessions with an active Redis subscription on this connection.
    ready_sessions: HashSet<String>,
    /// Sessions with a subscription request in-flight.
    pending_requests: HashSet<String>,
    /// Pending ensure requests awaiting completion.
    pending_subscriptions: HashMap<String, Vec<oneshot::Sender<Result<(), String>>>>,
    /// Sender to the worker that owns the PubSub connection.
    cmd_tx: Option<mpsc::UnboundedSender<WorkerCommand>>,
    /// Temporary storage for PubSub before `started`, moved into worker.
    pubsub: Option<PubSub>,
}

impl PubSubConnectionActor {
    fn new(conn_idx: usize, pubsub: PubSub) -> Self {
        Self {
            conn_idx,
            sessions: HashMap::new(),
            ready_sessions: HashSet::new(),
            pending_requests: HashSet::new(),
            pending_subscriptions: HashMap::new(),
            cmd_tx: None,
            pubsub: Some(pubsub),
        }
    }

    fn request_subscription(
        &mut self,
        session_id: &str,
        waiter: Option<oneshot::Sender<Result<(), String>>>,
    ) {
        if self.ready_sessions.contains(session_id) {
            if let Some(tx) = waiter {
                let _ = tx.send(Ok(()));
            }
            return;
        }

        if let Some(tx) = waiter {
            self.pending_subscriptions
                .entry(session_id.to_string())
                .or_default()
                .push(tx);
        }

        if self.pending_requests.insert(session_id.to_string()) {
            self.send_subscribe_command(session_id);
        }
    }

    fn send_subscribe_command(&mut self, session_id: &str) {
        if let Some(tx) = &self.cmd_tx {
            if let Err(e) = tx.send(WorkerCommand::Subscribe {
                session_id: session_id.to_string(),
            }) {
                warn!(
                    conn_idx = %self.conn_idx,
                    session_id = %session_id,
                    error = %e,
                    "Failed to send subscribe command to worker"
                );
                self.pending_requests.remove(session_id);
                self.notify_waiters(session_id, Err("PubSub worker unavailable".to_string()));
            }
        } else {
            warn!(
                conn_idx = %self.conn_idx,
                session_id = %session_id,
                "No worker command channel; cannot subscribe"
            );
            self.pending_requests.remove(session_id);
            self.notify_waiters(session_id, Err("PubSub worker unavailable".to_string()));
        }
    }

    fn send_unsubscribe_command(&mut self, session_id: &str) {
        if let Some(tx) = &self.cmd_tx {
            if let Err(e) = tx.send(WorkerCommand::Unsubscribe {
                session_id: session_id.to_string(),
            }) {
                warn!(
                    conn_idx = %self.conn_idx,
                    session_id = %session_id,
                    error = %e,
                    "Failed to send unsubscribe command to worker"
                );
            }
        } else {
            debug!(
                conn_idx = %self.conn_idx,
                session_id = %session_id,
                "No worker command channel; cannot unsubscribe"
            );
        }
    }

    fn notify_waiters(&mut self, session_id: &str, result: Result<(), String>) {
        if let Some(waiters) = self.pending_subscriptions.remove(session_id) {
            for waiter in waiters {
                let _ = waiter.send(result.clone());
            }
        }
    }

    fn start_worker(&mut self, ctx: &mut Context<Self>) {
        let conn_idx = self.conn_idx;
        let addr = ctx.address();

        // Take ownership of PubSub from the actor.
        let mut pubsub = self
            .pubsub
            .take()
            .expect("PubSub must be set in PubSubConnectionActor::new");

        // Create a command channel from actor -> worker.
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WorkerCommand>();
        self.cmd_tx = Some(cmd_tx);

        info!(
            conn_idx = %conn_idx,
            "Starting PubSub worker for connection actor"
        );

        ctx.spawn(
            async move {
                use tokio::sync::mpsc::error::TryRecvError;
                use std::time::Duration;
                use tokio::time::timeout;

                loop {
                    // 1) Drain all pending commands without blocking.
                    loop {
                        match cmd_rx.try_recv() {
                            Ok(WorkerCommand::Subscribe { session_id }) => {
                                let channel = format!("session:{}:down", session_id);
                                let result = match pubsub.subscribe(&channel).await {
                                    Ok(_) => {
                                        debug!(
                                            conn_idx = %conn_idx,
                                            session_id = %session_id,
                                            channel = %channel,
                                            "Subscribed to Redis channel"
                                        );
                                        Ok(())
                                    }
                                    Err(e) => {
                                        error!(
                                            conn_idx = %conn_idx,
                                            session_id = %session_id,
                                            error = %e,
                                            "Failed to subscribe to Redis channel"
                                        );
                                        Err(e.to_string())
                                    }
                                };
                                let _ = addr.try_send(SubscriptionCommandResult {
                                    session_id,
                                    result,
                                });
                            }
                            Ok(WorkerCommand::Unsubscribe { session_id }) => {
                                let channel = format!("session:{}:down", session_id);
                                if let Err(e) = pubsub.unsubscribe(&channel).await {
                                    error!(
                                        conn_idx = %conn_idx,
                                        session_id = %session_id,
                                        error = %e,
                                        "Failed to unsubscribe from Redis channel"
                                    );
                                } else {
                                    debug!(
                                        conn_idx = %conn_idx,
                                        session_id = %session_id,
                                        channel = %channel,
                                        "Unsubscribed from Redis channel"
                                    );
                                }
                            }
                            Err(TryRecvError::Empty) => {
                                // No more commands right now.
                                break;
                            }
                            Err(TryRecvError::Disconnected) => {
                                warn!(
                                    conn_idx = %conn_idx,
                                    "Command channel closed; stopping PubSub worker"
                                );
                                return;
                            }
                        }
                    }

                    // 2) Wait for the next Redis message with a timeout.
                    let msg_result = {
                        let mut stream = pubsub.on_message();
                        timeout(Duration::from_millis(100), stream.next()).await
                    };

                    match msg_result {
                        Ok(Some(msg)) => {
                            if let Err(e) = addr.try_send(DeliverPubSubMessage { msg }) {
                                warn!(
                                    conn_idx = %conn_idx,
                                    error = %e,
                                    "Failed to deliver Redis message to connection actor; stopping worker"
                                );
                                break;
                            }
                        }
                        Ok(None) => {
                            // Stream ended.
                            warn!(
                                conn_idx = %conn_idx,
                                "Redis PubSub stream ended; stopping worker"
                            );
                            break;
                        }
                        Err(_elapsed) => {
                            // Timeout elapsed, no message; loop again to handle commands.
                            continue;
                        }
                    }
                }
            }
                .into_actor(self),
        );
    }

    /// Handle an incoming Redis message: parse channel, find `session_id`,
    /// and forward payload to the corresponding `Recipient`.
    fn handle_redis_msg(&mut self, msg: Msg) {
        let channel = msg.get_channel_name();

        let payload: String = match msg.get_payload() {
            Ok(p) => p,
            Err(e) => {
                error!(
                    conn_idx = %self.conn_idx,
                    error = %e,
                    channel = %channel,
                    "Failed to parse Redis payload"
                );
                return;
            }
        };

        let Some(session_id) = extract_session_id(channel) else {
            warn!(
                conn_idx = %self.conn_idx,
                channel = %channel,
                "Failed to extract session_id from channel"
            );
            return;
        };

        debug!(
            conn_idx = %self.conn_idx,
            session_id = %session_id,
            payload = %payload,
            "Received Redis message, routing to session"
        );

        if let Some(entry) = self.sessions.get_mut(&session_id) {
            entry.downstream_forwarded += 1;
            if let Err(e) = entry.recipient.try_send(PubSubMessage { payload }) {
                warn!(
                    conn_idx = %self.conn_idx,
                    session_id = %session_id,
                    error = %e,
                    "Failed to send PubSubMessage to session actor"
                );
            } else {
                debug!(
                    conn_idx = %self.conn_idx,
                    session_id = %session_id,
                    "Message forwarded to session actor"
                );
            }
        } else {
            debug!(
                conn_idx = %self.conn_idx,
                session_id = %session_id,
                "No session recipient registered for this message"
            );
        }
    }
}

impl Actor for PubSubConnectionActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            conn_idx = %self.conn_idx,
            "PubSubConnectionActor started"
        );
        self.start_worker(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!(
            conn_idx = %self.conn_idx,
            "PubSubConnectionActor stopped"
        );
    }
}

impl Handler<ConnectionRegisterSession> for PubSubConnectionActor {
    type Result = ();

    fn handle(
        &mut self,
        msg: ConnectionRegisterSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let session_id = msg.session_id;
        let addr = msg.addr;

        // Insert into local sessions map synchronously.
        self.sessions.insert(
            session_id.clone(),
            SessionEntry {
                recipient: addr,
                downstream_forwarded: 0,
            },
        );
        self.request_subscription(&session_id, None);

        info!(
            conn_idx = %self.conn_idx,
            session_id = %session_id,
            total_sessions = %self.sessions.len(),
            "Session registered on connection actor"
        );
    }
}

impl Handler<ConnectionUnregisterSession> for PubSubConnectionActor {
    type Result = ();

    fn handle(
        &mut self,
        msg: ConnectionUnregisterSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let session_id = msg.session_id;
        let ws_recv_messages = msg.ws_recv_messages;
        let ws_sent_messages = msg.ws_sent_messages;
        let redis_down_messages = msg.redis_down_messages;
        let redis_up_messages = msg.redis_up_messages;

        if let Some(entry) = self.sessions.remove(&session_id) {
            info!(
                conn_idx = %self.conn_idx,
                session_id = %session_id,
                total_sessions = %self.sessions.len(),
                redis_forwarded = %entry.downstream_forwarded,
                ws_recv = %ws_recv_messages,
                ws_sent = %ws_sent_messages,
                redis_down_messages = %redis_down_messages,
                redis_up_messages = %redis_up_messages,
                "Session unregistered from connection actor"
            );
            self.ready_sessions.remove(&session_id);
            self.pending_requests.remove(&session_id);
            self.notify_waiters(&session_id, Err("Subscription cancelled".to_string()));
            self.send_unsubscribe_command(&session_id);
        } else {
            debug!(
                conn_idx = %self.conn_idx,
                session_id = %session_id,
                "Unregister requested for a non-existing session"
            );
        }
    }
}

impl Handler<DeliverPubSubMessage> for PubSubConnectionActor {
    type Result = ();

    fn handle(&mut self, msg: DeliverPubSubMessage, _ctx: &mut Self::Context) -> Self::Result {
        self.handle_redis_msg(msg.msg);
    }
}

impl Handler<SubscriptionCommandResult> for PubSubConnectionActor {
    type Result = ();

    fn handle(&mut self, msg: SubscriptionCommandResult, _ctx: &mut Self::Context) -> Self::Result {
        self.pending_requests.remove(&msg.session_id);
        match &msg.result {
            Ok(_) => {
                self.ready_sessions.insert(msg.session_id.clone());
            }
            Err(err) => {
                self.ready_sessions.remove(&msg.session_id);
                warn!(
                    conn_idx = %self.conn_idx,
                    session_id = %msg.session_id,
                    error = %err,
                    "Subscription command failed"
                );
            }
        }
        self.notify_waiters(&msg.session_id, msg.result.clone());
    }
}

impl Handler<ConnectionEnsureSubscription> for PubSubConnectionActor {
    type Result = ResponseFuture<Result<(), String>>;

    fn handle(&mut self, msg: ConnectionEnsureSubscription, _ctx: &mut Self::Context) -> Self::Result {
        if self.ready_sessions.contains(&msg.session_id) {
            return Box::pin(async { Ok(()) });
        }

        let (tx, rx) = oneshot::channel();
        self.request_subscription(&msg.session_id, Some(tx));

        Box::pin(async move {
            match rx.await {
                Ok(res) => res,
                Err(_) => Err("Subscription wait channel dropped".to_string()),
            }
        })
    }
}

/// Hash function: stable mapping from session_id to connection index.
fn hash_to_index(session_id: &str, pool_size: usize) -> usize {
    let size = pool_size.max(1);
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    (hasher.finish() as usize) % size
}

/// Manager actor that owns a pool of PubSub connection actors.
///
/// Responsibilities:
/// - Initialize N Redis PubSub connections (each in its own actor)
/// - Route `RegisterSession` / `UnregisterSession` to the correct connection
///   based on consistent hashing of `session_id`.
pub struct PubSubManager {
    /// Addresses of PubSub connection actors.
    connections: Vec<Addr<PubSubConnectionActor>>,
    /// Logical pool size (used in hashing); updated after initialization.
    pool_size: usize,
    /// Redis client factory.
    redis_client: crate::redis::RedisClient,
    /// Set to true after PoolInitialized is processed.
    initialized: bool,
    /// Sessions registered before pool initialization is complete.
    pending_sessions: Vec<RegisterSession>,
}

impl PubSubManager {
    pub fn new(redis_client: crate::redis::RedisClient, pool_size: usize) -> Self {
        let pool_size = pool_size.max(1);
        Self {
            connections: Vec::with_capacity(pool_size),
            pool_size,
            redis_client,
            initialized: false,
            pending_sessions: Vec::new(),
        }
    }

    fn get_connection_index(&self, session_id: &str) -> usize {
        hash_to_index(session_id, self.pool_size)
    }

    async fn initialize_connections(
        redis_client: crate::redis::RedisClient,
        configured_size: usize,
    ) -> Vec<Addr<PubSubConnectionActor>> {
        info!(
            pool_size = %configured_size,
            "Initializing PubSub connection actors"
        );

        let mut addrs = Vec::with_capacity(configured_size);

        for i in 0..configured_size {
            match redis_client.get_pubsub().await {
                Ok(pubsub) => {
                    let actor = PubSubConnectionActor::new(i, pubsub).start();
                    info!(conn_idx = %i, "PubSubConnectionActor created");
                    addrs.push(actor);
                }
                Err(e) => {
                    error!(
                        conn_idx = %i,
                        error = %e,
                        "Failed to create PubSub connection; skipping this slot"
                    );
                }
            }
        }

        info!(
            created = %addrs.len(),
            expected = %configured_size,
            "PubSub connection actor pool initialized"
        );

        addrs
    }

    fn do_register_session(&mut self, msg: RegisterSession) {
        if self.connections.is_empty() {
            warn!(
                session_id = %msg.session_id,
                "No PubSub connection actors available; cannot register session"
            );
            return;
        }

        let idx = self.get_connection_index(&msg.session_id);
        if let Some(conn) = self.connections.get(idx) {
            let _ = conn.do_send(ConnectionRegisterSession {
                session_id: msg.session_id,
                addr: msg.addr,
            });
        } else {
            warn!(
                session_id = %msg.session_id,
                conn_idx = %idx,
                pool_len = %self.connections.len(),
                "Calculated connection index is out of bounds; session not registered"
            );
        }
    }

}

/// Internal message: manager has finished creating connection actors.
struct PoolInitialized {
    connections: Vec<Addr<PubSubConnectionActor>>,
}

impl Message for PoolInitialized {
    type Result = ();
}

impl Actor for PubSubManager {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("PubSubManager started");

        let redis_client = self.redis_client.clone();
        let configured_size = self.pool_size;
        let addr = ctx.address();

        // Initialize connection actors asynchronously.
        ctx.spawn(
            async move {
                let connections =
                    PubSubManager::initialize_connections(redis_client.clone(), configured_size)
                        .await;
                addr.do_send(PoolInitialized { connections });
            }
                .into_actor(self),
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("PubSubManager stopped");
    }
}

impl Handler<PoolInitialized> for PubSubManager {
    type Result = ();

    fn handle(&mut self, msg: PoolInitialized, _ctx: &mut Self::Context) -> Self::Result {
        self.connections = msg.connections;
        self.pool_size = self.connections.len().max(1);
        self.initialized = true;

        info!(
            pool_size = %self.connections.len(),
            "Connection actor pool initialized in PubSubManager"
        );

        if !self.pending_sessions.is_empty() {
            info!(
                pending = %self.pending_sessions.len(),
                "Replaying pending session registrations after pool initialization"
            );
        }

        let pending = std::mem::take(&mut self.pending_sessions);
        for msg in pending {
            self.do_register_session(msg);
        }

    }
}

impl Handler<RegisterSession> for PubSubManager {
    type Result = ();

    fn handle(&mut self, msg: RegisterSession, _ctx: &mut Self::Context) -> Self::Result {
        if !self.initialized || self.connections.is_empty() {
            debug!(
                session_id = %msg.session_id,
                "Connection pool not ready yet; queuing session registration"
            );
            self.pending_sessions.push(msg);
            return;
        }

        self.do_register_session(msg);
    }
}

impl Handler<UnregisterSession> for PubSubManager {
    type Result = ();

    fn handle(&mut self, msg: UnregisterSession, _ctx: &mut Self::Context) -> Self::Result {
        if self.connections.is_empty() {
            debug!(
                session_id = %msg.session_id,
                "No PubSub connection actors available; nothing to unregister"
            );
            return;
        }

        let idx = self.get_connection_index(&msg.session_id);
        if let Some(conn) = self.connections.get(idx) {
            let _ = conn.do_send(ConnectionUnregisterSession {
                session_id: msg.session_id,
                ws_recv_messages: msg.ws_recv_messages,
                ws_sent_messages: msg.ws_sent_messages,
                redis_down_messages: msg.redis_down_messages,
                redis_up_messages: msg.redis_up_messages,
            });
        } else {
            warn!(
                session_id = %msg.session_id,
                conn_idx = %idx,
                pool_len = %self.connections.len(),
                "Calculated connection index is out of bounds; session not unregistered"
            );
        }
    }
}

impl Handler<EnsureSubscription> for PubSubManager {
    type Result = ResponseActFuture<Self, Result<(), String>>;

    fn handle(&mut self, msg: EnsureSubscription, _ctx: &mut Self::Context) -> Self::Result {
        if !self.initialized || self.connections.is_empty() {
            let err = Err("PubSub connection pool is not ready".to_string());
            return Box::pin(async move { err }.into_actor(self));
        }

        let idx = self.get_connection_index(&msg.session_id);
        if let Some(conn) = self.connections.get(idx) {
            let fut = conn
                .send(ConnectionEnsureSubscription {
                    session_id: msg.session_id,
                })
                .into_actor(self)
                .map(|res, _act, _ctx| match res {
                    Ok(inner) => inner,
                    Err(_) => Err("PubSub connection actor unavailable".to_string()),
                });
            Box::pin(fut)
        } else {
            Box::pin(
                async move { Err("PubSub connection index out of bounds".to_string()) }
                    .into_actor(self),
            )
        }
    }
}

/// Helper: extract session_id from Redis channel name.
///
/// Expected format: "session:{session_id}:down".
fn extract_session_id(channel: &str) -> Option<String> {
    let parts: Vec<&str> = channel.split(':').collect();
    if parts.len() == 3 && parts[0] == "session" && parts[2] == "down" {
        Some(parts[1].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_id() {
        assert_eq!(
            extract_session_id("session:test-123:down"),
            Some("test-123".to_string())
        );
        assert_eq!(
            extract_session_id("session:abc-def-ghi:down"),
            Some("abc-def-ghi".to_string())
        );
        assert_eq!(extract_session_id("invalid:format"), None);
        assert_eq!(extract_session_id("session:only-two"), None);
    }

    #[test]
    fn test_hash_distribution_is_stable() {
        let pool_size = 4;

        let idx1 = hash_to_index("session-1", pool_size);
        let idx2 = hash_to_index("session-1", pool_size);
        assert_eq!(idx1, idx2, "Same session should hash to same index");

        let idx3 = hash_to_index("session-2", pool_size);
        assert!(idx3 < pool_size, "Index should be within pool size");
    }
}
