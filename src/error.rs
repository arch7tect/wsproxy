use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("PORT must be greater than 0")]
    InvalidPort,

    #[error("REDIS_URL cannot be empty")]
    EmptyRedisUrl,

    #[error("MAX_MESSAGE_SIZE_BYTES must be greater than 0")]
    InvalidMaxMessageSize,

    #[error("Failed to load environment variables: {0}")]
    EnvError(#[from] envy::Error),
}

#[derive(Error, Debug)]
pub enum RedisError {
    #[error("Redis connection error: {0}")]
    ConnectionError(#[from] redis::RedisError),
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Missing authorization token")]
    MissingToken,

    #[error("Invalid authorization token")]
    InvalidToken,

    #[error("Authentication timeout")]
    Timeout,

    #[error("Redis error: {0}")]
    RedisError(#[from] RedisError),
}
