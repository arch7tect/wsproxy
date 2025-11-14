use actix::Message;

/// Message from Redis Pub/Sub to WebSocket session
#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct RedisMessage {
    pub payload: String,
}

/// Shutdown message to gracefully close session
#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown;