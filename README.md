# wsproxy

WebSocket proxy enabling backends to replace WebSocket connections with Redis Pub/Sub messaging.

## Features

- WebSocket server with session-based connections
- Redis Pub/Sub integration for message forwarding
- Token-based authentication
- Bidirectional communication between clients and backends
- Stateless reconnection support with auth token grace periods

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

See [examples/README.md](examples/README.md) for usage examples and testing scenarios.

## License

MIT License - see [LICENSE](LICENSE) file for details.