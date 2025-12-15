use crate::error::ConfigError;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    // Server
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    // Redis
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    #[serde(default = "default_pubsub_pool_size")]
    pub pubsub_pool_size: usize,

    // WebSocket
    #[serde(default = "default_ws_ping_interval")]
    pub ws_ping_interval_secs: u64,

    #[serde(default = "default_ws_ping_timeout")]
    pub ws_ping_timeout_secs: u64,

    // Shutdown
    #[serde(default = "default_shutdown_grace_period")]
    pub shutdown_grace_period_secs: u64,

    // JWT Authentication
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    #[serde(default = "default_jwt_algorithm")]
    pub jwt_algorithm: String,

    // Messages
    #[serde(default = "default_max_message_size")]
    pub max_message_size_bytes: usize,

    // Logging
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let config = envy::from_env::<Config>()?;
        config.validate()?;

        Ok(config)
    }

    pub fn ws_ping_interval(&self) -> Duration {
        Duration::from_secs(self.ws_ping_interval_secs)
    }

    pub fn ws_ping_timeout(&self) -> Duration {
        Duration::from_secs(self.ws_ping_timeout_secs)
    }

    pub fn shutdown_grace_period(&self) -> Duration {
        Duration::from_secs(self.shutdown_grace_period_secs)
    }

    pub fn max_message_size(&self) -> usize {
        self.max_message_size_bytes
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::InvalidPort);
        }

        if self.redis_url.is_empty() {
            return Err(ConfigError::EmptyRedisUrl);
        }

        if self.max_message_size_bytes == 0 {
            return Err(ConfigError::InvalidMaxMessageSize);
        }

        if self.jwt_secret.is_empty() || self.jwt_secret.len() < 32 {
            return Err(ConfigError::JwtSecretInvalid);
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            redis_url: default_redis_url(),
            pubsub_pool_size: default_pubsub_pool_size(),
            ws_ping_interval_secs: default_ws_ping_interval(),
            ws_ping_timeout_secs: default_ws_ping_timeout(),
            shutdown_grace_period_secs: default_shutdown_grace_period(),
            jwt_secret: default_jwt_secret(),
            jwt_algorithm: default_jwt_algorithm(),
            max_message_size_bytes: default_max_message_size(),
            log_level: default_log_level(),
        }
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_pubsub_pool_size() -> usize {
    512  // Optimized for 10,000+ connections (20:1 ratio)
}

fn default_ws_ping_interval() -> u64 {
    15
}

fn default_ws_ping_timeout() -> u64 {
    30
}

fn default_shutdown_grace_period() -> u64 {
    120  // Extended for high scale (10,000+ connections)
}

fn default_jwt_secret() -> String {
    String::new()  // Must be provided via environment
}

fn default_jwt_algorithm() -> String {
    "HS256".to_string()
}

fn default_max_message_size() -> usize {
    1024 * 1024
}

fn default_log_level() -> String {
    "info,wsproxy=debug,actix_web=info,actix_server=info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.ws_ping_interval(), Duration::from_secs(15));
        assert_eq!(config.max_message_size(), 1024 * 1024);
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        config.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_max_message_size_validation() {
        let mut config = Config::default();
        assert_eq!(config.max_message_size(), 1024 * 1024);

        config.max_message_size_bytes = 0;
        assert!(config.validate().is_err());

        config.max_message_size_bytes = 100;
        assert!(config.validate().is_ok());
        assert_eq!(config.max_message_size(), 100);
    }
}
