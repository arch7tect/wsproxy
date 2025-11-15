# wsproxy

WebSocket proxy service for decoupling WebSocket connections from Python FastAPI workers using Redis Pub/Sub.

## Features (Phase 1 MVP)

- WebSocket server accepting connections on `/{agent_id}/ws/{session_id}`
- Redis Pub/Sub integration for message forwarding
- Graceful shutdown with CancellationToken
- Heartbeat/ping-pong for connection liveness
- Structured logging with tracing
- Health check endpoints (`/health`, `/ready`)
- Active connection tracking

## Quick Start

### Prerequisites

- Rust 1.70+
- Redis server running on `127.0.0.1:6379`

### Build and Run

```bash
# Build the project
cargo build

# Run the server
cargo run

# Run with custom configuration
HOST=0.0.0.0 PORT=8080 REDIS_URL=redis://127.0.0.1:6379 cargo run
```

### Configuration

Configuration via environment variables:

```bash
# Server
HOST=0.0.0.0
PORT=8080

# Redis
REDIS_URL=redis://127.0.0.1:6379

# WebSocket
WS_PING_INTERVAL_SECS=30
WS_PING_TIMEOUT_SECS=10

# Shutdown
SHUTDOWN_GRACE_PERIOD_SECS=30

# Logging
RUST_LOG=info,wsproxy=debug
```

Copy `.env.example` to `.env` and modify as needed.

## Usage

### WebSocket Connection

Connect to the WebSocket endpoint:

```
ws://localhost:8080/{agent_id}/ws/{session_id}
```

Example with websocat:
```bash
websocat ws://localhost:8080/agent1/ws/test-session-123
```

### Publishing Messages via Redis

Publish messages to the Redis channel to forward them to the WebSocket client:

```bash
# Using redis-cli
redis-cli PUBLISH "session:test-session-123:down" '{"type": "data", "payload": "Hello from Redis!"}'
```

Or use the example publisher:

```bash
cargo run --example redis_publisher test-session-123
```

### Example WebSocket Client

```bash
cargo run --example ws_bidir_client agent1 test-session-123 my-token
```

For more examples and testing scenarios, see [examples/README.md](examples/README.md).

## API Endpoints

### WebSocket

- `GET /{agent_id}/ws/{session_id}` - WebSocket upgrade endpoint

### Health Checks

- `GET /health` - Returns `{"status": "healthy"}`
- `GET /ready` - Returns `{"status": "ready"}`

## Architecture

See [architecture.md](architecture.md) for detailed architecture documentation.

## Development

### Running Tests

```bash
cargo test
```

### Code Style

```bash
# Format code
cargo fmt

# Run linter
cargo clippy
```

## License

MIT License - see [LICENSE](LICENSE) file for details.