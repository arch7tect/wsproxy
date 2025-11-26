use futures::StreamExt;
use http::{header::AUTHORIZATION, HeaderValue};
use redis::{aio::ConnectionManager, AsyncCommands};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
};
use tokio_util::sync::CancellationToken;
use rand::{rngs::StdRng, Rng, SeedableRng};

// tracing
use tracing::{info, warn, error, debug};
use tracing_subscriber::{EnvFilter, fmt};

fn init_tracing() {
    // Configure from RUST_LOG, e.g.:
    // RUST_LOG=info
    // RUST_LOG=debug,load_test=trace
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();
}

#[derive(Debug, Clone, Default)]
struct LatencyStats {
    latencies: Vec<Duration>,
    errors: usize,
    sent: usize,
    received: usize,
    drain_starts: usize,
    drain_completed: usize,
    drain_closed: usize,
    drain_timeouts: usize,
    drain_missing_msgs: usize,
    pub publish_attempts: usize,
    pub publish_success: usize,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
            errors: 0,
            sent: 0,
            received: 0,
            drain_starts: 0,
            drain_completed: 0,
            drain_closed: 0,
            drain_timeouts: 0,
            drain_missing_msgs: 0,
            publish_attempts: 0,
            publish_success: 0,
        }
    }

    fn add_latency(&mut self, latency: Duration) {
        self.latencies.push(latency);
    }

    fn add_error(&mut self) {
        self.errors += 1;
    }

    /// Merge stats from another worker into this one.
    fn merge(&mut self, other: &LatencyStats) {
        self.latencies.extend_from_slice(&other.latencies);
        self.errors += other.errors;
        self.sent += other.sent;
        self.received += other.received;
        self.drain_starts += other.drain_starts;
        self.drain_completed += other.drain_completed;
        self.drain_closed += other.drain_closed;
        self.drain_timeouts += other.drain_timeouts;
        self.drain_missing_msgs += other.drain_missing_msgs;
        self.publish_attempts += other.publish_attempts;
        self.publish_success += other.publish_success;
    }

    fn calculate_percentile(&self, percentile: f64) -> Option<Duration> {
        if self.latencies.is_empty() {
            return None;
        }

        let mut sorted = self.latencies.clone();
        sorted.sort();

        let p = percentile.clamp(0.0, 100.0);
        let rank = p / 100.0 * (sorted.len() as f64 - 1.0);
        let idx = rank.round() as usize;

        sorted.get(idx).copied()
    }

    fn average(&self) -> Option<Duration> {
        if self.latencies.is_empty() {
            return None;
        }

        let total_nanos: u128 = self.latencies.iter().map(|d| d.as_nanos()).sum();
        let avg_nanos = total_nanos / self.latencies.len() as u128;

        Some(Duration::from_nanos(avg_nanos as u64))
    }

    fn min(&self) -> Option<Duration> {
        self.latencies.iter().min().copied()
    }

    fn max(&self) -> Option<Duration> {
        self.latencies.iter().max().copied()
    }
}

#[derive(Debug, Default)]
struct WorkerErrorStats {
    total_worker_errors: usize,
    io_timeouts: usize,
    http_504: usize,
    http_500: usize,
    http_other: usize,
    join_errors: usize,
    other: usize,
}

impl WorkerErrorStats {
    fn classify(&mut self, lower: &str) {
        if lower.contains("operation timed out") || lower.contains("timed out") {
            self.io_timeouts += 1;
        } else if lower.contains("504") {
            self.http_504 += 1;
        } else if lower.contains("500") {
            self.http_500 += 1;
        } else if lower.contains("http error") {
            self.http_other += 1;
        } else {
            self.other += 1;
        }
    }

    fn add_worker_error_msg(&mut self, msg: &str) {
        self.total_worker_errors += 1;
        let lower = msg.to_lowercase();
        self.classify(&lower);
    }

    fn add_join_error(&mut self, msg: &str) {
        self.total_worker_errors += 1;
        self.join_errors += 1;
        let lower = msg.to_lowercase();
        self.classify(&lower);
    }
}

struct Config {
    redis_url: String,
    ws_host: String,
    ws_port: u16,
    num_workers: usize,
    warmup_secs: u64,
    test_duration_secs: u64,
    message_interval_secs: u64,
    drain_timeout_secs: u64,
    shutdown_timeout_secs: u64,
    // Redis connection "pool" settings (for logging / URL params)
    redis_max_connections: u32,
    redis_connection_timeout_ms: u64,
    redis_response_timeout_ms: u64,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        CliArgs::parse().map(|args| Self {
            redis_url: args.redis_url,
            ws_host: args.ws_host,
            ws_port: args.ws_port,
            num_workers: args.workers,
            warmup_secs: args.warmup,
            test_duration_secs: args.test_duration,
            message_interval_secs: args.message_interval,
            drain_timeout_secs: args.drain_timeout,
            shutdown_timeout_secs: args.shutdown_timeout,
            redis_max_connections: args.redis_max_connections,
            redis_connection_timeout_ms: args.redis_connection_timeout_ms,
            redis_response_timeout_ms: args.redis_response_timeout_ms,
        })
    }
}

#[derive(Debug)]
struct CliArgs {
    workers: usize,
    warmup: u64,
    test_duration: u64,
    message_interval: u64,
    drain_timeout: u64,
    shutdown_timeout: u64,
    ws_host: String,
    ws_port: u16,
    redis_url: String,
    redis_max_connections: u32,
    redis_connection_timeout_ms: u64,
    redis_response_timeout_ms: u64,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            workers: 100,
            warmup: 5,
            test_duration: 30,
            message_interval: 1,
            drain_timeout: 30,
            shutdown_timeout: 10,
            ws_host: "127.0.0.1".to_string(),
            ws_port: 4040,
            redis_url: "redis://127.0.0.1:6379".to_string(),
            redis_max_connections: 30,
            redis_connection_timeout_ms: 5000,
            redis_response_timeout_ms: 3000,
        }
    }
}

impl CliArgs {
    fn parse() -> Result<Self, String> {
        let mut args = CliArgs::default();
        let mut iter = std::env::args().skip(1).peekable();
        let mut required = RequiredFlags::default();

        if iter.peek().is_none() {
            return Err(Self::usage());
        }

        while let Some(arg) = iter.next() {
            if arg == "--help" || arg == "-h" {
                return Err(Self::usage());
            }

            if !arg.starts_with("--") {
                return Err(format!("Unexpected argument: {arg}\n{}", Self::usage()));
            }

            let (key, value_opt) = if let Some((k, v)) = arg.split_once('=') {
                (k.to_string(), Some(v.to_string()))
            } else {
                (arg, iter.next())
            };

            let value = value_opt.ok_or_else(|| {
                format!("Missing value for argument {key}\n{}", Self::usage())
            })?;

            match key.as_str() {
                "--workers" => {
                    args.workers = parse_value(&value, "workers")?;
                    required.workers = true;
                }
                "--warmup" => {
                    args.warmup = parse_value(&value, "warmup")?;
                    required.warmup = true;
                }
                "--test-duration" => {
                    args.test_duration = parse_value(&value, "test-duration")?;
                    required.test_duration = true;
                }
                "--message-interval" => {
                    args.message_interval = parse_value(&value, "message-interval")?;
                    required.message_interval = true;
                }
                "--drain-timeout" => {
                    args.drain_timeout = parse_value(&value, "drain-timeout")?;
                    required.drain_timeout = true;
                }
                "--shutdown-timeout" => {
                    args.shutdown_timeout = parse_value(&value, "shutdown-timeout")?;
                }
                "--ws-host" => {
                    args.ws_host = value;
                    required.ws_host = true;
                }
                "--ws-port" => {
                    args.ws_port = parse_value(&value, "ws-port")?;
                    required.ws_port = true;
                }
                "--redis-url" => args.redis_url = value,
                "--redis-max-connections" => {
                    args.redis_max_connections = parse_value(&value, "redis-max-connections")?
                }
                "--redis-connection-timeout-ms" => {
                    args.redis_connection_timeout_ms =
                        parse_value(&value, "redis-connection-timeout-ms")?
                }
                "--redis-response-timeout-ms" => {
                    args.redis_response_timeout_ms =
                        parse_value(&value, "redis-response-timeout-ms")?
                }
                _ => {
                    return Err(format!("Unknown argument: {key}\n{}", Self::usage()));
                }
            }
        }

        if let Some(missing) = required.missing() {
            return Err(format!(
                "Missing required argument: {missing}\n{}",
                Self::usage()
            ));
        }

        Ok(args)
    }

    fn usage() -> String {
        format!(
            "Usage: cargo run --example load_test -- \\
            --workers <num> --warmup <secs> --test-duration <secs> \\
            --message-interval <secs> --drain-timeout <secs> \\
            --ws-host <host> --ws-port <port> \\
            [--redis-url <url>] [--redis-max-connections <num>] \\
            [--redis-connection-timeout-ms <ms>] [--redis-response-timeout-ms <ms>] \\
            [--shutdown-timeout <secs>]"
        )
    }
}

fn parse_value<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("Invalid value for {name}: {value}\n{}", CliArgs::usage()))
}

#[derive(Default)]
struct RequiredFlags {
    workers: bool,
    warmup: bool,
    test_duration: bool,
    message_interval: bool,
    drain_timeout: bool,
    ws_host: bool,
    ws_port: bool,
}

impl RequiredFlags {
    fn missing(&self) -> Option<&'static str> {
        if !self.workers {
            return Some("--workers");
        }
        if !self.warmup {
            return Some("--warmup");
        }
        if !self.test_duration {
            return Some("--test-duration");
        }
        if !self.message_interval {
            return Some("--message-interval");
        }
        if !self.drain_timeout {
            return Some("--drain-timeout");
        }
        if !self.ws_host {
            return Some("--ws-host");
        }
        if !self.ws_port {
            return Some("--ws-port");
        }
        None
    }
}

type WorkerError = Box<dyn std::error::Error + Send + Sync>;
type WorkerResult = Result<LatencyStats, WorkerError>;

/// Single worker:
/// - Registers session auth in Redis
/// - Opens WebSocket
/// - Starts sending messages to Redis channel at a fixed interval
/// - Measures end-to-end latency of messages received on WebSocket
/// - Tracks sent/received counts for delivery ratio
async fn worker(
    worker_id: usize,
    config: Arc<Config>,
    redis_pool: Arc<ConnectionManager>,
    warmup_complete: Arc<tokio::sync::Notify>,
    stop_send_token: CancellationToken,
    shutdown_token: CancellationToken,
) -> WorkerResult {
    let session_id = format!("load-test-{}", uuid::Uuid::new_v4());
    let agent_id = format!("agent-{}", worker_id);
    let auth_token = format!("token-{}", worker_id);

    // Auth connection (shared manager cloned per worker)
    let mut redis_conn = redis_pool.as_ref().clone();

    let auth_key = format!("session:{}:auth", session_id);
    let _: () = redis_conn.set(&auth_key, &auth_token).await?;

    // WebSocket URL
    let ws_url = format!(
        "ws://{}:{}/{}/ws/{}",
        config.ws_host, config.ws_port, agent_id, session_id
    );

    let mut request = ws_url.into_client_request()?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", auth_token))?,
    );

    let (ws, _) = connect_async(request).await?;
    let (_write, mut read) = ws.split();

    // Redis channel where worker publishes messages
    let redis_channel = format!("session:{}:down", session_id);

    // Wait until "warmup" phase is complete globally
    warmup_complete.notified().await;

    // Common time origin for this worker
    let origin = Instant::now();
    let message_interval = Duration::from_secs(config.message_interval_secs);
    let max_delay_ms =
        message_interval
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;

    // Use a dedicated Redis connection for publishing
    let mut redis_pub_conn = redis_conn.clone();

    // Global sent/received counters for this worker (shared between tasks)
    let sent_counter = Arc::new(AtomicUsize::new(0));
    let sent_counter_for_send = sent_counter.clone();
    let received_counter = Arc::new(AtomicUsize::new(0));
    let received_counter_for_recv = received_counter.clone();
    let publish_attempts = Arc::new(AtomicUsize::new(0));
    let publish_success = Arc::new(AtomicUsize::new(0));
    let publish_attempts_send = publish_attempts.clone();
    let publish_success_send = publish_success.clone();

    // Per-worker shutdown coordination
    let worker_shutdown = CancellationToken::new();
    let worker_shutdown_for_send = worker_shutdown.clone();
    let worker_shutdown_for_recv = worker_shutdown.clone();
    let receiver_stop_token = CancellationToken::new();
    let receiver_stop = receiver_stop_token.clone();

    // SEND LOOP
    let redis_channel_send = redis_channel.clone();
    let shutdown_for_send = shutdown_token.clone();
    let shutdown_for_recv = shutdown_token.clone();

    let send_task = tokio::spawn(async move {
        let mut rng = StdRng::from_entropy();
        let stop_send = stop_send_token.clone();
        loop {
            tokio::select! {
                _ = worker_shutdown_for_send.cancelled() => {
                    break;
                }
                _ = shutdown_for_send.cancelled() => {
                    break;
                }
                _ = stop_send.cancelled() => {
                    break;
                }
                _ = time::sleep(if max_delay_ms == 0 {
                    Duration::from_secs(0)
                } else {
                    let jitter_ms = rng.gen_range(0..=max_delay_ms);
                    Duration::from_millis(jitter_ms)
                }) => {
                    publish_attempts_send.fetch_add(1, Ordering::Relaxed);
                    let timestamp = origin.elapsed().as_nanos();
                    let message_id = uuid::Uuid::new_v4().to_string();

                    let message = json!({
                        "type": "data",
                        "message_id": message_id,
                        "timestamp": timestamp,
                    });

                    if let Err(e) = redis_pub_conn
                        .publish::<_, _, i32>(&redis_channel_send, message.to_string())
                        .await
                    {
                        warn!(worker = worker_id, error = %e, "Failed to publish to Redis");
                    } else {
                        // Count only successfully published messages
                        publish_success_send.fetch_add(1, Ordering::Relaxed);
                        sent_counter_for_send.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    });

    // RECEIVE LOOP
    let receive_task = tokio::spawn(async move {
        let mut stats = LatencyStats::new();

        loop {
            tokio::select! {
                _ = worker_shutdown_for_recv.cancelled() => {
                    break;
                }
                _ = receiver_stop.cancelled() => {
                    break;
                }
                _ = shutdown_for_recv.cancelled() => {
                    break;
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            let receive_time = origin.elapsed();

                            if let Ok(msg_value) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(sent_nanos) = msg_value
                                    .get("timestamp")
                                    .and_then(|t| t.as_u64())
                                {
                                    let sent_duration = Duration::from_nanos(sent_nanos);
                                    let latency = receive_time.saturating_sub(sent_duration);
                                    stats.add_latency(latency);
                                    stats.received += 1;
                                    received_counter_for_recv.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            debug!(worker = worker_id, "WebSocket closed by server");
                            worker_shutdown_for_recv.cancel();
                            break;
                        }
                        Some(Err(e)) => {
                            warn!(worker = worker_id, error = %e, "WebSocket read error");
                            stats.add_error();
                            worker_shutdown_for_recv.cancel();
                            break;
                        }
                        None => {
                            debug!(worker = worker_id, "WebSocket stream ended");
                            worker_shutdown_for_recv.cancel();
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        stats
    });

    let mut final_stats = LatencyStats::new();

    // Wait for sender to finish (measurement stop)
    match send_task.await {
        Ok(_) => {}
        Err(e) => {
            error!(worker = worker_id, error = %e, "Send task join error");
            final_stats.add_error();
        }
    }

    let expected_sent = sent_counter.load(Ordering::Relaxed);

    // Allow receiver to drain remaining messages with a generous timeout.
    let drain_timeout = Duration::from_secs(config.drain_timeout_secs);
    let drain_start = Instant::now();
    let mut drain_started = false;
    let mut drain_completed = false;
    let mut drain_closed = false;
    let mut drain_timed_out = false;
    let mut pending_gap = 0usize;
    loop {
        let received = received_counter.load(Ordering::Relaxed);
        if received >= expected_sent {
            if drain_started {
                drain_completed = true;
            }
            break;
        }

        if worker_shutdown.is_cancelled() {
            warn!(
                worker = worker_id,
                received,
                expected = expected_sent,
                elapsed_ms = drain_start.elapsed().as_millis(),
                "Stopping drain because WebSocket closed early"
            );
            drain_closed = true;
            break;
        }

        if !drain_started {
            drain_started = true;
            pending_gap = expected_sent.saturating_sub(received_counter.load(Ordering::Relaxed));
        }

        if drain_start.elapsed() >= drain_timeout {
            warn!(
                worker = worker_id,
                received,
                expected = expected_sent,
                elapsed_ms = drain_start.elapsed().as_millis(),
                "Worker did not drain all messages within timeout"
            );
            drain_timed_out = true;
            break;
        }

        time::sleep(Duration::from_millis(50)).await;
    }

    receiver_stop_token.cancel();
    worker_shutdown.cancel();

    match receive_task.await {
        Ok(stats) => {
            final_stats = stats;
        }
        Err(e) => {
            error!(worker = worker_id, error = %e, "Receive task join error");
            final_stats.add_error();
        }
    }

    final_stats.sent = expected_sent;
    final_stats.publish_attempts = publish_attempts.load(Ordering::Relaxed);
    final_stats.publish_success = publish_success.load(Ordering::Relaxed);
    if drain_started {
        final_stats.drain_starts += 1;
    }
    if drain_completed {
        final_stats.drain_completed += 1;
    }
    if drain_closed {
        final_stats.drain_closed += 1;
    }
    if drain_timed_out {
        final_stats.drain_timeouts += 1;
        final_stats.drain_missing_msgs += pending_gap;
    } else if drain_closed {
        let received = received_counter.load(Ordering::Relaxed);
        final_stats.drain_missing_msgs += expected_sent.saturating_sub(received);
    }

    // Clean up auth key, ignore errors.
    let _: Result<(), redis::RedisError> = redis_conn.del(&auth_key).await;

    Ok(final_stats)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = Arc::new(Config::from_args().unwrap_or_else(|e| {
        error!("{}", e);
        std::process::exit(1);
    }));

    info!("Load Test Configuration:");
    info!("  Workers: {}", config.num_workers);
    info!("  Warmup: {}s", config.warmup_secs);
    info!("  Test Duration: {}s", config.test_duration_secs);
    info!("  Message Interval: {}s", config.message_interval_secs);
    info!("  Drain Timeout: {}s", config.drain_timeout_secs);
    info!("  Shutdown Timeout: {}s", config.shutdown_timeout_secs);
    info!("  WebSocket: {}:{}", config.ws_host, config.ws_port);
    info!("  Redis: {}", config.redis_url);
    info!(
        "  Redis Pool: max_conn={}, conn_timeout={}ms, resp_timeout={}ms",
        config.redis_max_connections,
        config.redis_connection_timeout_ms,
        config.redis_response_timeout_ms
    );

    // Build Redis connection URL with timeout parameters (best-effort)
    info!("Connecting to Redis: {}", config.redis_url);

    let mut redis_url = config.redis_url.clone();
    let separator = if redis_url.contains('?') { "&" } else { "?" };
    redis_url.push_str(&format!(
        "{}connection_timeout={}ms&response_timeout={}ms",
        separator,
        config.redis_connection_timeout_ms,
        config.redis_response_timeout_ms
    ));

    let redis_client = redis::Client::open(redis_url.as_str())?;
    let redis_pool = Arc::new(redis_client.get_connection_manager().await?);

    info!(
        "Redis connection manager created with timeouts: conn={}ms, resp={}ms",
        config.redis_connection_timeout_ms, config.redis_response_timeout_ms
    );

    let warmup_complete = Arc::new(tokio::sync::Notify::new());
    let stop_send_token = CancellationToken::new();
    let shutdown_token = CancellationToken::new();

    info!("Starting {} workers...", config.num_workers);

    let mut handles = Vec::with_capacity(config.num_workers);

    for worker_id in 0..config.num_workers {
        let config = Arc::clone(&config);
        let redis_pool = Arc::clone(&redis_pool);
        let warmup_complete = Arc::clone(&warmup_complete);
        let stop_send = stop_send_token.clone();
        let shutdown = shutdown_token.clone();

        let handle = tokio::spawn(async move {
            worker(
                worker_id,
                config,
                redis_pool,
                warmup_complete,
                stop_send,
                shutdown,
            )
                .await
        });

        handles.push(handle);
    }

    // Let workers connect and register sessions (no traffic yet).
    time::sleep(Duration::from_secs(2)).await;
    info!(
        "Workers started. Warmup phase (no traffic yet): {}s",
        config.warmup_secs
    );

    time::sleep(Duration::from_secs(config.warmup_secs)).await;
    info!(
        "Warmup complete. Starting measurement phase (traffic) for {}s",
        config.test_duration_secs
    );
    warmup_complete.notify_waiters();

    // Measurement window
    time::sleep(Duration::from_secs(config.test_duration_secs)).await;
    info!("Measurement window complete. Stopping publishers...");
    stop_send_token.cancel();

    info!(
        "Waiting up to {}s for workers to shutdown...",
        config.shutdown_timeout_secs
    );

    // Wait for all workers to finish with a timeout
    let shutdown_timeout = Duration::from_secs(config.shutdown_timeout_secs);
    let shutdown_start = Instant::now();

    let mut aggregated_stats = LatencyStats::new();
    let mut worker_error_stats = WorkerErrorStats::default();

    for (idx, handle) in handles.into_iter().enumerate() {
        let remaining_time = shutdown_timeout.saturating_sub(shutdown_start.elapsed());

        match tokio::time::timeout(remaining_time, handle).await {
            Ok(join_result) => match join_result {
                Ok(worker_result) => match worker_result {
                    Ok(stats) => {
                        aggregated_stats.merge(&stats);
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        error!(worker = idx, error = %msg, "Worker error");
                        worker_error_stats.add_worker_error_msg(&msg);
                        aggregated_stats.add_error();
                    }
                },
                Err(e) => {
                    let msg = e.to_string();
                    error!(worker = idx, error = %msg, "Worker join error");
                    worker_error_stats.add_join_error(&msg);
                    aggregated_stats.add_error();
                }
            },
            Err(_) => {
                let msg = "worker did not shutdown within timeout".to_string();
                warn!(worker = idx, "Worker did not shutdown within timeout");
                worker_error_stats.add_worker_error_msg(&msg);
                aggregated_stats.add_error();
            }
        }
    }

    info!("All workers finished in {:?}", shutdown_start.elapsed());

    let stats = aggregated_stats;

    info!("=== Load Test Results ===");
    info!("Total measurements (received): {}", stats.latencies.len());
    info!("Errors (WS + worker-level): {}", stats.errors);

    // Delivery metrics
    info!("Delivery Metrics:");
    info!("  Total sent:     {}", stats.sent);
    info!("  Total received: {}", stats.received);
    if stats.sent > 0 {
        let ratio = stats.received as f64 / stats.sent as f64 * 100.0;
        info!("  Delivery ratio: {:.2}%", ratio);
        let delta = stats.sent as i64 - stats.received as i64;
        if delta != 0 {
            warn!(
                "  Imbalance detected: {} messages {}",
                delta.abs(),
                if delta > 0 { "missing" } else { "extra" }
            );
        }
    } else {
        info!("  Delivery ratio: N/A (no sent messages recorded)");
    }

    if !stats.latencies.is_empty() {
        info!("Latency Statistics:");
        if let Some(min) = stats.min() {
            info!("  Min:     {:?}", min);
        }
        if let Some(avg) = stats.average() {
            info!("  Average: {:?}", avg);
        }
        if let Some(p50) = stats.calculate_percentile(50.0) {
            info!("  P50:     {:?}", p50);
        }
        if let Some(p95) = stats.calculate_percentile(95.0) {
            info!("  P95:     {:?}", p95);
        }
        if let Some(p99) = stats.calculate_percentile(99.0) {
            info!("  P99:     {:?}", p99);
        }
        if let Some(max) = stats.max() {
            info!("  Max:     {:?}", max);
        }
    } else {
        info!("No latency measurements collected.");
    }

    info!("=== Worker-Level Errors ===");
    info!("Total worker errors: {}", worker_error_stats.total_worker_errors);
    info!("  IO timeouts:        {}", worker_error_stats.io_timeouts);
    info!("  HTTP 504 errors:    {}", worker_error_stats.http_504);
    info!("  HTTP 500 errors:    {}", worker_error_stats.http_500);
    info!("  Other HTTP errors:  {}", worker_error_stats.http_other);
    info!("  Join errors:        {}", worker_error_stats.join_errors);
    info!("  Other errors:       {}", worker_error_stats.other);

    info!("=== Drain Statistics ===");
    info!("Workers requiring drain: {}", stats.drain_starts);
    info!("  Completed drain:     {}", stats.drain_completed);
    info!("  Closed early:        {}", stats.drain_closed);
    info!("  Timed out:           {}", stats.drain_timeouts);
    info!("  Messages missing at shutdown: {}", stats.drain_missing_msgs);
    info!("Publish attempts: {}", stats.publish_attempts);
    info!("Publish successes: {}", stats.publish_success);

    Ok(())
}
