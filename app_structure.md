# Application Structure

## Directory Structure

```
wsproxy/
├── Cargo.toml
├── Cargo.lock
├── .gitignore
├── README.md
├── CLAUDE.md
├── architecture.md
├── project_plan.md
├── app_structure.md
│
├── src/
│   ├── main.rs              # Entry point, server setup, signal handling
│   │
│   ├── config.rs            # Configuration management (env vars)
│   │
│   ├── handlers/            # HTTP/WebSocket request handlers
│   │   ├── mod.rs
│   │   ├── websocket.rs     # WebSocket upgrade handler
│   │   └── health.rs        # Health check endpoints
│   │
│   ├── ws/                  # WebSocket connection management
│   │   ├── mod.rs
│   │   ├── session.rs       # WebSocket session actor
│   │   └── messages.rs      # Actor messages definitions
│   │
│   ├── redis/               # Redis integration
│   │   ├── mod.rs
│   │   ├── client.rs        # Redis connection manager
│   │   └── pubsub.rs        # Pub/Sub subscription handler
│   │
│   ├── state.rs             # Application shared state
│   │
│   └── shutdown.rs          # Graceful shutdown coordination
│
├── tests/                   # Integration tests
│   └── integration_test.rs
│
└── examples/                # Example clients/publishers
    ├── ws_client.rs
    └── redis_publisher.rs
```

## Module Responsibilities

### `main.rs`
**Purpose**: Application entry point and server lifecycle management.

**Responsibilities**:
- Initialize tracing/logging
- Load configuration
- Setup application state
- Create Actix Web HTTP server
- Setup signal handlers (SIGTERM/SIGINT)
- Coordinate graceful shutdown
- Start server and await shutdown

**Key Functions**:
```rust
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging
    // Load config
    // Setup Redis client
    // Setup cancellation token
    // Create HTTP server with routes
    // Setup signal handlers
    // Run server
    // Await graceful shutdown
}
```

---

### `config.rs`
**Purpose**: Configuration management from environment variables.

**Responsibilities**:
- Define `Config` struct with all settings
- Load and validate configuration from environment
- Provide defaults
- Validate configuration on startup

**Configuration Fields**:
```rust
pub struct Config {
    // Server
    pub host: String,
    pub port: u16,

    // Redis
    pub redis_url: String,

    // WebSocket
    pub ws_ping_interval: Duration,
    pub ws_ping_timeout: Duration,

    // Shutdown
    pub shutdown_grace_period: Duration,

    // Logging
    pub log_level: String,
}
```

---

### `handlers/websocket.rs`
**Purpose**: HTTP handler for WebSocket upgrade requests.

**Responsibilities**:
- Parse `agent_id` and `session_id` from URL path
- Validate request (basic validation, no auth in Phase 1)
- Upgrade HTTP connection to WebSocket
- Create WebSocket session actor
- Return HTTP 101 Switching Protocols

**Handler Signature**:
```rust
pub async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<(String, String)>, // (agent_id, session_id)
    app_state: web::Data<AppState>,
) -> Result<HttpResponse, Error>
```

---

### `handlers/health.rs`
**Purpose**: Health check endpoints for monitoring.

**Responsibilities**:
- `/health` - Basic liveness check
- `/ready` - Readiness check (optional: verify Redis connection)

**Handlers**:
```rust
pub async fn health_handler() -> HttpResponse
pub async fn ready_handler(app_state: web::Data<AppState>) -> HttpResponse
```

---

### `ws/session.rs`
**Purpose**: WebSocket session actor managing single client connection.

**Responsibilities**:
- Maintain WebSocket connection
- Subscribe to Redis Pub/Sub channel `session:{session_id}:down`
- Receive messages from Redis and forward to WebSocket client
- Handle WebSocket close/error events
- Cleanup on disconnection (unsubscribe from Redis)
- Integrate with shutdown token

**Actor Structure**:
```rust
pub struct WsSession {
    id: Uuid,                           // Unique session ID
    session_id: String,                 // From URL path
    agent_id: String,                   // From URL path
    redis_client: RedisClient,          // Redis connection
    redis_subscription: Option<...>,    // Active subscription
    shutdown_token: CancellationToken,  // For graceful shutdown
    hb: Instant,                        // Last heartbeat
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Start heartbeat
        // Subscribe to Redis channel
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        // Unsubscribe from Redis
        // Cleanup
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // Handle WebSocket messages from client
        // In Phase 1: just handle Ping/Pong/Close
    }
}

impl Handler<RedisMessage> for WsSession {
    // Handle messages from Redis Pub/Sub
    // Forward to WebSocket client
}
```

---

### `ws/messages.rs`
**Purpose**: Actor message type definitions.

**Responsibilities**:
- Define message types for actor communication
- Message from Redis to WebSocket session

**Message Types**:
```rust
#[derive(Message)]
#[rtype(result = "()")]
pub struct RedisMessage {
    pub payload: String,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Shutdown;
```

---

### `redis/client.rs`
**Purpose**: Redis connection management.

**Responsibilities**:
- Create and manage Redis connection
- Provide connection pool
- Handle connection errors
- Support reconnection (basic in Phase 1, advanced in Phase 4)

**Interface**:
```rust
#[derive(Clone)]
pub struct RedisClient {
    client: redis::Client,
}

impl RedisClient {
    pub fn new(redis_url: &str) -> Result<Self, RedisError>

    pub async fn get_connection(&self) -> Result<redis::aio::Connection, RedisError>

    pub async fn subscribe(&self, channel: &str) -> Result<redis::aio::PubSub, RedisError>
}
```

---

### `redis/pubsub.rs`
**Purpose**: Redis Pub/Sub subscription handler.

**Responsibilities**:
- Subscribe to Redis channel
- Listen for messages
- Forward messages to WebSocket session actor
- Handle subscription lifecycle

**Implementation**:
```rust
pub async fn subscribe_to_channel(
    channel: String,
    session_addr: Addr<WsSession>,
    mut pubsub: redis::aio::PubSub,
    shutdown_token: CancellationToken,
) {
    // Subscribe to channel
    // Loop: receive messages or wait for shutdown
    // Forward messages to session actor
    // Cleanup on shutdown
}
```

---

### `state.rs`
**Purpose**: Application-wide shared state.

**Responsibilities**:
- Store shared resources (Redis client, config, metrics)
- Provide thread-safe access via `web::Data<AppState>`
- Track active connections for graceful shutdown

**Structure**:
```rust
pub struct AppState {
    pub config: Config,
    pub redis_client: RedisClient,
    pub shutdown_token: CancellationToken,
    pub active_connections: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new(config: Config, redis_client: RedisClient, shutdown_token: CancellationToken) -> Self
}
```

---

### `shutdown.rs`
**Purpose**: Graceful shutdown coordination.

**Responsibilities**:
- Provide CancellationToken for coordinated shutdown
- Setup signal handlers (SIGTERM/SIGINT)
- Coordinate shutdown sequence
- Wait for active connections to finish

**Interface**:
```rust
pub fn setup_shutdown_handler() -> CancellationToken {
    // Create cancellation token
    // Spawn task to listen for SIGTERM/SIGINT
    // Cancel token on signal
    // Return token
}

pub async fn wait_for_shutdown(
    shutdown_token: CancellationToken,
    active_connections: Arc<AtomicUsize>,
    grace_period: Duration,
) {
    // Wait for cancellation
    // Wait for active connections to drain or timeout
    // Log shutdown progress
}
```

---

## Data Flow

### WebSocket Connection Establishment
```
1. Client → HTTP GET /{agent_id}/ws/{session_id}
2. handlers/websocket.rs → Parse path params
3. handlers/websocket.rs → Upgrade to WebSocket
4. ws/session.rs → Create WsSession actor
5. ws/session.rs → Subscribe to Redis channel session:{session_id}:down
6. WebSocket connection established
```

### Message Flow (Redis → Client)
```
1. External Publisher → Redis PUBLISH session:{session_id}:down "message"
2. redis/pubsub.rs → Receive message from subscription
3. redis/pubsub.rs → Send RedisMessage to WsSession actor
4. ws/session.rs → Forward message to WebSocket
5. Client ← Receive message via WebSocket
```

### Graceful Shutdown
```
1. OS → SIGTERM signal
2. shutdown.rs → Cancel shutdown_token
3. main.rs → Stop accepting new connections
4. ws/session.rs → All sessions detect cancellation
5. ws/session.rs → Close WebSocket connections gracefully
6. ws/session.rs → Unsubscribe from Redis channels
7. main.rs → Wait for active_connections == 0 or timeout
8. main.rs → Exit process
```

---

## Phase 1 Implementation Order

1. **Basic scaffolding** (Day 1)
   - `main.rs` with minimal HTTP server
   - `config.rs` with basic configuration
   - `state.rs` with AppState
   - Setup logging

2. **WebSocket handler** (Day 1-2)
   - `handlers/websocket.rs` basic upgrade
   - `ws/session.rs` actor skeleton
   - `ws/messages.rs` message types
   - Test: can establish WebSocket connection

3. **Redis integration** (Day 2-3)
   - `redis/client.rs` connection management
   - `redis/pubsub.rs` subscription handler
   - `ws/session.rs` subscribe on start
   - Test: can forward Redis messages to WebSocket

4. **Graceful shutdown** (Day 3-4)
   - `shutdown.rs` signal handling
   - Integrate CancellationToken into all components
   - Track active connections
   - Test: graceful shutdown works

5. **Health endpoints** (Day 4)
   - `handlers/health.rs` basic health checks
   - Test: health endpoints respond

6. **Integration testing** (Day 5)
   - End-to-end tests
   - Manual testing with example client
   - Bug fixes and polish

---

## Testing Strategy

### Unit Tests
- `config.rs` - configuration parsing and validation
- `redis/client.rs` - connection handling
- Message type serialization

### Integration Tests
- WebSocket connection establishment
- Message forwarding from Redis to WebSocket
- Graceful shutdown
- Health endpoints

### Manual Testing
- Use `examples/ws_client.rs` to connect
- Use `examples/redis_publisher.rs` to publish messages
- Use `redis-cli` to publish messages
- Test with multiple concurrent connections
- Test shutdown scenarios

---

## Key Dependencies (Cargo.toml)

```toml
[dependencies]
# Actix Web framework
actix-web = "4"
actix-web-actors = "4"
actix = "0.13"

# Redis
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tracing-actix-web = "0.7"

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
tokio-util = "0.7"

# Graceful shutdown
tokio-graceful-shutdown = "0.14"  # Or use tokio_util::sync::CancellationToken

# Error handling
anyhow = "1"
thiserror = "1"
```

---

## Configuration via Environment Variables

```bash
# Server
HOST=0.0.0.0
PORT=8080

# Redis
REDIS_URL=redis://127.0.0.1:6379

# WebSocket
WS_PING_INTERVAL=30s
WS_PING_TIMEOUT=10s

# Shutdown
SHUTDOWN_GRACE_PERIOD=30s

# Logging
RUST_LOG=info
LOG_FORMAT=json  # json or pretty
```

---

## Next Steps

1. Create basic project structure with `cargo init`
2. Add dependencies to `Cargo.toml`
3. Implement modules in the order specified above
4. Write tests as you go
5. Create example clients for manual testing
6. Document any deviations from this plan