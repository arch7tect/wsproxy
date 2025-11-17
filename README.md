# wsproxy

WebSocket proxy enabling backends to replace WebSocket connections with Redis Pub/Sub messaging.

## Features

- WebSocket server with session-based connections
- Redis Pub/Sub integration for message forwarding
- Token-based authentication
- Bidirectional communication between clients and backends
- Stateless reconnection with token expiration after grace period

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

# Authentication
AUTH_TIMEOUT_SECS=5
AUTH_GRACE_PERIOD_SECS=10800

# WebSocket
WS_PING_INTERVAL_SECS=30
WS_PING_TIMEOUT_SECS=10

# Shutdown
SHUTDOWN_GRACE_PERIOD_SECS=30

# Logging
RUST_LOG=info,wsproxy=debug
```

Copy `.env.example` to `.env` and modify as needed.

## Authentication

All WebSocket connections require Bearer token authentication. The auth flow:

1. Backend sets token with initial timeout: `SETEX session:{session_id}:auth {timeout_seconds} {token}`
2. Client connects with `Authorization: Bearer {token}` header
3. Wsproxy validates token from Redis
4. Token gets TTL reset to grace period to enable stateless reconnection
5. Connection is established
6. Token expires after grace period unless refreshed by reconnection

If token is missing, invalid, or expired, wsproxy returns HTTP 401 or 403.

## Testing

### Integration Tests

Run the full integration test suite using Python with asyncio:

```bash
# Install dependencies with uv
uv sync

# Run all tests
uv run test_wsproxy.py
```

The test suite covers:
- WebSocket connection with authentication
- Authentication failure scenarios
- Redis → WebSocket message forwarding
- WebSocket → Redis message forwarding
- Bidirectional communication

All tests require wsproxy and Redis to be running.

### Manual Testing

See [examples/README.md](examples/README.md) for manual testing scenarios with example clients.

## Usage

See [examples/README.md](examples/README.md) for usage examples and testing scenarios.

## License

MIT License - see [LICENSE](LICENSE) file for details.