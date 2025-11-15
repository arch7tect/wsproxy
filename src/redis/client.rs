use crate::error::RedisError;
use redis::{aio::ConnectionManager, AsyncCommands, Client};

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

    pub async fn publish(&self, channel: &str, message: &str) -> Result<(), RedisError> {
        let mut conn = self.get_connection().await?;
        conn.publish::<_, _, ()>(channel, message).await?;
        Ok(())
    }

    pub async fn check_connection(&self) -> Result<(), RedisError> {
        let mut conn = self.get_connection().await?;
        redis::cmd("PING").query_async::<_, ()>(&mut conn).await?;
        Ok(())
    }
}
