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
    grace_period: Duration,
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

                let grace_period_secs = grace_period.as_secs() as i64;
                let _: () = conn
                    .expire(&redis_key, grace_period_secs)
                    .await
                    .map_err(crate::error::RedisError::ConnectionError)?;

                info!(
                    session_id = %session_id,
                    grace_period_secs = grace_period_secs,
                    "Auth token TTL set to grace period for reconnection"
                );

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

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_redis() -> RedisClient {
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        RedisClient::new(&redis_url).expect("Failed to create Redis client")
    }

    async fn set_test_token(redis_client: &RedisClient, session_id: &str, token: &str) {
        let mut conn = redis_client.get_connection().await.unwrap();
        let key = format!("session:{}:auth", session_id);
        let _: () = conn.set(&key, token).await.unwrap();
    }

    async fn clear_test_token(redis_client: &RedisClient, session_id: &str) {
        let mut conn = redis_client.get_connection().await.unwrap();
        let key = format!("session:{}:auth", session_id);
        let _: () = conn.del(&key).await.unwrap_or(());
    }

    #[tokio::test]
    async fn test_missing_token() {
        let result = validate_bearer_token(
            "test-session",
            None,
            &setup_redis().await,
            Duration::from_secs(5),
            Duration::from_secs(60),
        )
        .await;

        assert!(matches!(result, Err(AuthError::MissingToken)));
    }

    #[tokio::test]
    async fn test_valid_token() {
        let redis_client = setup_redis().await;
        let session_id = "test-valid-token-grace";
        let token = "valid-token-123";

        set_test_token(&redis_client, session_id, token).await;

        let result = validate_bearer_token(
            session_id,
            Some(token),
            &redis_client,
            Duration::from_secs(5),
            Duration::from_secs(60),
        )
        .await;

        assert!(result.is_ok());

        let mut conn = redis_client.get_connection().await.unwrap();
        let key = format!("session:{}:auth", session_id);
        let stored: Option<String> = conn.get(&key).await.unwrap();
        assert!(stored.is_some(), "Token should exist with TTL for grace period");

        clear_test_token(&redis_client, session_id).await;
    }

    #[tokio::test]
    async fn test_invalid_token_mismatch() {
        let redis_client = setup_redis().await;
        let session_id = "test-invalid-token";

        set_test_token(&redis_client, session_id, "correct-token").await;

        let result = validate_bearer_token(
            session_id,
            Some("wrong-token"),
            &redis_client,
            Duration::from_secs(5),
            Duration::from_secs(60),
        )
        .await;

        assert!(matches!(result, Err(AuthError::InvalidToken)));

        clear_test_token(&redis_client, session_id).await;
    }

    #[tokio::test]
    async fn test_token_not_found_in_redis() {
        let redis_client = setup_redis().await;
        let session_id = "test-nonexistent";

        clear_test_token(&redis_client, session_id).await;

        let result = validate_bearer_token(
            session_id,
            Some("some-token"),
            &redis_client,
            Duration::from_secs(5),
            Duration::from_secs(60),
        )
        .await;

        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[tokio::test]
    async fn test_auth_timeout() {
        let redis_url = "redis://127.0.0.1:9999";
        let redis_client = RedisClient::new(redis_url).expect("Failed to create Redis client");
        let session_id = "test-timeout";

        let result = validate_bearer_token(
            session_id,
            Some("token"),
            &redis_client,
            Duration::from_millis(100),
            Duration::from_secs(60),
        )
        .await;

        assert!(matches!(result, Err(AuthError::Timeout)));
    }
}
