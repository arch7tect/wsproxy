# Examples

This directory contains example programs for testing and demonstrating wsproxy functionality.

## Overview

The examples demonstrate the WebSocket proxy message flow:

```
WebSocket Client → wsproxy → Redis (upstream channel) → Backend Agent
                              ↓
WebSocket Client ← wsproxy ← Redis (downstream channel) ← Backend Agent
```

Redis channels:
- **Downstream**: `session:{session_id}:down` - messages from backend to client
- **Upstream**: `session:{session_id}:up` - messages from client to backend
- **Auth**: `session:{session_id}:auth` - authentication token storage

## Examples

### auth_token_setter

Sets authentication token in Redis for WebSocket connections.

**Usage:**
```bash
cargo run --example auth_token_setter <session_id> <token>
```

**Example:**
```bash
cargo run --example auth_token_setter test-session-123 my-secret-token
```

This stores the token in Redis key `session:test-session-123:auth`. The token is required for WebSocket authentication and gets a TTL set after successful connection to enable stateless reconnection.

### redis_publisher

Publishes messages to the downstream Redis channel, simulating a backend agent sending data to clients.

**Usage:**
```bash
cargo run --example redis_publisher [session_id]
```

**Example:**
```bash
cargo run --example redis_publisher test-session-123
```

Publishes JSON messages every 2 seconds to `session:test-session-123:down`. Press Ctrl+C to stop.

### redis_subscriber

Subscribes to the upstream Redis channel, simulating a backend agent receiving messages from clients.

**Usage:**
```bash
cargo run --example redis_subscriber <session_id>
```

**Example:**
```bash
cargo run --example redis_subscriber test-session-123
```

Listens for messages on `session:test-session-123:up` and prints them.

### ws_bidir_client

Bidirectional WebSocket client that sends and receives messages.

**Usage:**
```bash
cargo run --example ws_bidir_client <agent_id> <session_id> <auth_token>
```

**Example:**
```bash
cargo run --example ws_bidir_client agent1 test-session-123 my-secret-token
```

Connects to wsproxy, sends ping messages every 3 seconds to the upstream channel, and prints received downstream messages.

### load_test

Load testing tool to measure message latency at different connection scales.

**Usage:**
```bash
cargo run --example load_test <num_workers> <warmup_secs> <test_duration_secs> <message_interval_secs> <ws_host> <ws_port>
```

**Example:**
```bash
cargo run --example load_test 50 3 10 1 127.0.0.1 4040
```

This runs a load test with:
- 50 concurrent WebSocket connections
- 3 seconds warmup period
- 10 seconds measurement period
- 1 second message interval
- wsproxy at 127.0.0.1:4040

Each worker:
1. Creates an auth token in Redis
2. Establishes a WebSocket connection
3. Publishes messages via Redis pub/sub at specified interval
4. Receives messages through WebSocket
5. Measures end-to-end latency

After the test completes, statistics are printed:
- Total measurements collected
- Latency: Min, Average, P50, P95, P99, Max
- Error count

**Example output:**
```
=== Load Test Results ===
Total measurements: 550
Errors: 0

Latency Statistics:
  Min:     678.501µs
  Average: 2.63634ms
  P50:     2.400249ms
  P95:     5.094876ms
  P99:     5.822917ms
  Max:     6.390793ms
```

**Performance notes:**
- Latencies increase with connection count
- Typical results on local machine:
  - 2 workers: ~1.5ms average
  - 50 workers: ~2.6ms average
  - 100 workers: ~2.9ms average

## Testing Bidirectional Communication

Test full bidirectional message flow.

1. Start wsproxy server:
   ```bash
   cargo run
   ```

2. Set auth token:
   ```bash
   cargo run --example auth_token_setter test-session-123 my-token
   ```

3. In terminal 1, subscribe to upstream:
   ```bash
   cargo run --example redis_subscriber test-session-123
   ```

4. In terminal 2, connect bidirectional client:
   ```bash
   cargo run --example ws_bidir_client agent1 test-session-123 my-token
   ```

5. In terminal 3, publish downstream messages:
   ```bash
   cargo run --example redis_publisher test-session-123
   ```

You should see:
- Terminal 1: Upstream ping messages from client
- Terminal 2: Downstream messages from publisher
- Terminal 3: Published message confirmations

## Environment Variables

Examples use default configuration but respect environment variables:

- `REDIS_URL` - Redis connection URL (default: `redis://127.0.0.1:6379`)

## Message Format

Messages are JSON strings. Examples use:

Downstream (backend to client):
```json
{"type": "data", "payload": "Message #1"}
```

Upstream (client to backend):
```json
{"type": "ping", "counter": 1, "timestamp": "2025-01-15T12:00:00Z"}
```

The proxy validates JSON format but does not enforce schema. Invalid JSON is rejected with an error message.

## Notes

- Ensure Redis is running before starting examples
- Ensure wsproxy server is running before connecting clients
- Auth tokens can be reused for reconnection within the grace period
- Press Ctrl+C to stop any running example