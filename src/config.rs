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

    // WebSocket
    #[serde(default = "default_ws_ping_interval")]
    pub ws_ping_interval_secs: u64,

    #[serde(default = "default_ws_ping_timeout")]
    pub ws_ping_timeout_secs: u64,

    // Shutdown
    #[serde(default = "default_shutdown_grace_period")]
    pub shutdown_grace_period_secs: u64,

    // Logging
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Result<Self, anyhow::Error> {
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

    fn validate(&self) -> Result<(), anyhow::Error> {
        if self.port == 0 {
            anyhow::bail!("PORT must be greater than 0");
        }

        if self.redis_url.is_empty() {
            anyhow::bail!("REDIS_URL cannot be empty");
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
            ws_ping_interval_secs: default_ws_ping_interval(),
            ws_ping_timeout_secs: default_ws_ping_timeout(),
            shutdown_grace_period_secs: default_shutdown_grace_period(),
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

fn default_ws_ping_interval() -> u64 {
    15
}

fn default_ws_ping_timeout() -> u64 {
    30
}

fn default_shutdown_grace_period() -> u64 {
    30
}

fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert_eq!(config.ws_ping_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        assert!(config.validate().is_ok());

        config.port = 0;
        assert!(config.validate().is_err());
    }
}