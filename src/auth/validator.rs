use crate::error::AuthError;
use crate::redis::RedisClient;
use redis::AsyncCommands;
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub async fn validate_bearer_token(
    session_id: &str,
    provided_token: Option<&str>,
    redis_client: &RedisClient,
    timeout: Duration,
) -> Result<(), AuthError> {
    let provided_token = provided_token.ok_or(AuthError::MissingToken)?;

    let redis_key = format!("session:{}:auth", session_id);

    let result = tokio::time::timeout(timeout, async {
        let mut conn = redis_client.get_connection().await?;

        let stored_token: Option<String> = conn
            .get(&redis_key)
            .await
            .map_err(crate::error::RedisError::ConnectionError)?;

        match stored_token {
            Some(token) if token == provided_token => {
                debug!(session_id = %session_id, "Token validation successful");

                let _: () = conn
                    .del(&redis_key)
                    .await
                    .map_err(crate::error::RedisError::ConnectionError)?;

                info!(session_id = %session_id, "Auth token deleted after successful validation");

                Ok(())
            }
            Some(_) => {
                warn!(session_id = %session_id, "Token mismatch");
                Err(AuthError::InvalidToken)
            }
            None => {
                warn!(session_id = %session_id, "No token found in Redis");
                Err(AuthError::InvalidToken)
            }
        }
    })
    .await;

    match result {
        Ok(inner_result) => inner_result,
        Err(_) => {
            error!(session_id = %session_id, "Authentication timeout");
            Err(AuthError::Timeout)
        }
    }
}
