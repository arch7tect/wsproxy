# wsproxy

WebSocket proxy enabling backends to replace WebSocket connections with Redis Pub/Sub messaging.

## Features

- WebSocket server with session-based connections
- Redis Pub/Sub integration for message forwarding
- JWT-based stateless authentication (HS256)
- Bidirectional communication between clients and backends
- Stateless reconnection with JWT token reuse

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

# JWT Authentication
JWT_SECRET=<generate-with-python3 -c "import secrets; print(secrets.token_urlsafe(48))">
JWT_ALGORITHM=HS256

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

All WebSocket connections require JWT-based Bearer token authentication using HS256 (HMAC-SHA256).

### JWT Authentication Flow

1. Backend generates JWT token with claims: `{session_id, iat}`
2. Client connects with `Authorization: Bearer {jwt_token}` header
3. wsproxy validates JWT signature using shared secret (stateless, no Redis lookup)
4. wsproxy verifies `session_id` claim matches URL path parameter
5. Connection is established

### JWT Token Details

- **Algorithm**: HS256 (symmetric HMAC-SHA256)
- **Required Claims**:
  - `session_id`: Must match the session_id in the WebSocket URL path
  - `iat`: Issued at timestamp (Unix epoch)
- **No Expiry**: Tokens do not include `exp` claim and are valid indefinitely
- **Secret Management**: Same `JWT_SECRET` must be configured on all backend services and wsproxy instances

### Authentication Errors

- Missing token: HTTP 401 Unauthorized
- Invalid signature or malformed token: HTTP 403 Forbidden
- Session ID mismatch (claim ≠ URL path): HTTP 403 Forbidden

### Performance

JWT validation is CPU-only (no Redis RTT), providing 10-20x faster authentication compared to Redis-based token validation:
- JWT: ~0.1-1ms
- Redis-based (legacy): ~5-20ms

**Validated at Scale:**
- Successfully tested with 5000 concurrent clients
- 99.46% success rate with JWT authentication
- Zero JWT validation errors under load
- See [Load Testing Results](loadtest/README.md#performance-expectations) for detailed benchmarks

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