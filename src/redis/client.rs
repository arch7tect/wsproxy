use redis::{aio::ConnectionManager, Client};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RedisError {
    #[error("Redis connection error: {0}")]
    ConnectionError(#[from] redis::RedisError),
}

#[derive(Clone)]
pub struct RedisClient {
    client: Client,
}

impl RedisClient {
    pub fn new(redis_url: &str) -> Result<Self, RedisError> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }

    pub async fn get_connection(&self) -> Result<ConnectionManager, RedisError> {
        let conn = self.client.get_connection_manager().await?;
        Ok(conn)
    }

    pub async fn get_pubsub(&self) -> Result<redis::aio::PubSub, RedisError> {
        let conn = self.client.get_async_connection().await?;
        Ok(conn.into_pubsub())
    }
}