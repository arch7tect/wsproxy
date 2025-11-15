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
    ConnectionError(redis::RedisError),

    #[error("Redis command error: {0}")]
    CommandError(redis::RedisError),

    #[error("Redis type conversion error: {0}")]
    TypeError(redis::RedisError),

    #[error("Redis I/O error: {0}")]
    IoError(redis::RedisError),
}

impl From<redis::RedisError> for RedisError {
    fn from(err: redis::RedisError) -> Self {
        match err.kind() {
            redis::ErrorKind::IoError => RedisError::IoError(err),
            redis::ErrorKind::TypeError => RedisError::TypeError(err),
            redis::ErrorKind::ResponseError => RedisError::CommandError(err),
            redis::ErrorKind::ExecAbortError => RedisError::CommandError(err),
            redis::ErrorKind::BusyLoadingError => RedisError::CommandError(err),
            redis::ErrorKind::NoScriptError => RedisError::CommandError(err),
            redis::ErrorKind::InvalidClientConfig => RedisError::ConnectionError(err),
            redis::ErrorKind::Moved
            | redis::ErrorKind::Ask
            | redis::ErrorKind::TryAgain
            | redis::ErrorKind::ClusterDown
            | redis::ErrorKind::CrossSlot
            | redis::ErrorKind::MasterDown
            | redis::ErrorKind::ReadOnly => RedisError::CommandError(err),
            _ => RedisError::ConnectionError(err),
        }
    }
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

impl From<redis::RedisError> for AuthError {
    fn from(err: redis::RedisError) -> Self {
        AuthError::RedisError(RedisError::from(err))
    }
}
