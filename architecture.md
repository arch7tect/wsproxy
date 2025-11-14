# Architecture

## Current Architecture (As-Is)

### Components

- **AI Agents**: Tens of FastAPI servers (Python), each running with 1 worker
- **Clients**: Thousands of web browsers

### Connection Flow

1. Client connects to an AI agent via HTTP
2. Client creates a session and receives a `session_id` (UUID)
3. Client establishes a WebSocket connection using the session_id
4. Client initiates generation via HTTP request
5. Client receives streaming data through the WebSocket connection

### Current Issues

- Single worker per FastAPI server limits concurrency
- Direct client-to-agent connections don't scale well with thousands of clients
- No load balancing or connection pooling between clients and agents
- Cannot scale Python workers independently of WebSocket connections

## Target Architecture (To-Be)

### Proposed Solution

Introduce a dedicated WebSocket proxy (`wsproxy`) to decouple WebSocket connections from Python workers, using Redis Pub/Sub for communication.

### Components

- **Clients**: Web browsers (unchanged behavior from client perspective)
- **WebSocket Proxy** (`wsproxy`): Dedicated Rust service handling WebSocket connections
- **AI Agents**: FastAPI servers (Python) with multiple workers
- **Redis**: Pub/Sub message broker for streaming data

### Connection Flow

1. Client connects to AI agent via HTTP to create session and get `session_id` (UUID)
2. Client establishes WebSocket connection to **wsproxy** using `session_id`
3. Client initiates generation via HTTP request to AI agent
4. AI agent worker publishes streaming data to Redis channel (keyed by `session_id`)
5. `wsproxy` subscribes to Redis channel and forwards data to client via WebSocket

### Benefits

- **Scalability**: Python workers can scale independently of WebSocket connections
- **Performance**: Rust-based proxy handles thousands of concurrent WebSocket connections efficiently
- **Minimal client changes**: Client connects to different WebSocket endpoint, but flow remains the same
- **Load distribution**: Multiple Python workers can handle requests while maintaining persistent WebSocket connections

### Implementation Notes

- Redis Pub/Sub channels keyed by `session_id`
- `wsproxy` maintains mapping of `session_id` to WebSocket connection
- FastAPI workers are stateless and can scale horizontally
- WebSocket connections remain persistent in `wsproxy` regardless of which worker handles the request
- **AI agent servers can start and stop arbitrarily** - the architecture must handle dynamic scaling of FastAPI servers

## Requirements

### Performance Requirements

#### wsproxy
- **Concurrent connections**: Must handle 10,000+ simultaneous WebSocket connections
- **Latency**: Message delivery latency < 50ms (p99)
- **Throughput**: Support streaming data at 1-10 MB/s per connection
- **Memory efficiency**: ~1-2 KB per idle connection
- **CPU utilization**: Maintain < 70% CPU under normal load with room for traffic spikes
- **Connection handling**: Support 1000+ new connections per second during peak times

#### Redis
- **Message throughput**: Handle 100,000+ messages per second across all channels
- **Pub/Sub latency**: Message propagation < 10ms
- **Channel capacity**: Support 10,000+ active Pub/Sub channels simultaneously
- **Memory**: Sufficient for message buffering (estimated 1-2 GB for active sessions)

### Reliability Requirements

#### wsproxy
- **Connection lifecycle**:
  - Detect and clean up dead connections (ping/pong with 30s timeout)
  - Graceful shutdown: close connections properly, finish processing in-flight messages
  - Handle Redis reconnection without dropping WebSocket connections
  - Session timeout: if no messages received for configured period, close WebSocket with timeout error code
  - Support reconnection scenarios where client re-establishes WebSocket with same session_id

- **Error handling**:
  - Redis unavailable: buffer messages or reject new connections with clear error
  - Message delivery failure: log and close affected WebSocket connection
  - Malformed messages: log and skip without crashing

- **Resource limits**:
  - Maximum connections per instance (configurable, e.g., 50,000)
  - Maximum message size (configurable, e.g., 10 MB)
  - Connection rate limiting to prevent DDoS

- **Monitoring**:
  - Metrics: active connections, messages/sec, errors, latency percentiles
  - Health check endpoint for load balancer
  - Structured logging with trace IDs for debugging

#### Redis
- **High availability**:
  - Redis Sentinel for automatic failover (or Redis Cluster for higher scale)
  - Minimum 3-node setup (1 master, 2 replicas)

- **Data persistence**:
  - NOT required for Pub/Sub (transient streaming data)
  - Consider disabling AOF/RDB if Redis is only used for Pub/Sub to improve performance

- **Connection pool**:
  - `wsproxy` maintains persistent connection pool to Redis
  - Automatic reconnection with exponential backoff

- **Channel cleanup**:
  - Channels are automatically cleaned up by Redis when no subscribers
  - `wsproxy` unsubscribes from Redis channel when WebSocket disconnects

### Operational Requirements

- **Deployment**: `wsproxy` should be deployable as multiple instances behind a load balancer
- **Configuration**: All limits and timeouts should be configurable via environment variables or config file
- **Logging**: Support structured logging (JSON format) with configurable log levels
- **Graceful updates**: Support rolling updates without dropping active connections
- **Observability**: Export metrics in Prometheus format

## Critical Design Requirements

### 1. Race Condition Prevention (Pub/Sub Message Loss)

**Problem**: Redis Pub/Sub doesn't store message history. Messages published before subscription are lost.

**Solution**: Complete WebSocket handshake only after successful Redis subscription.

**Connection Flow**:
1. Client initiates WebSocket connection to `wsproxy` with `session_id` (sends HTTP Upgrade request with `Authorization: Bearer {token}` header)
2. `wsproxy` validates Bearer token (see Authentication section)
3. `wsproxy` subscribes to Redis channel `session:{session_id}:down` (and `:up` if bidirectional enabled)
4. **After successful subscription**, `wsproxy` completes WebSocket handshake (sends HTTP 101 Switching Protocols)
5. Client receives WebSocket connection established event and immediately makes HTTP request to agent to start generation
6. Agent publishes messages to Redis channel `session:{session_id}:down`
7. `wsproxy` receives from Redis and forwards to client

**Requirements**:
- `wsproxy` must NOT complete WebSocket handshake until Redis subscription succeeds
- If Redis subscription fails, reject handshake with HTTP 503 Service Unavailable
- Redis channel naming: `session:{session_id}:down` for downstream (agent→client)
- Client waits for WebSocket connection to open (standard WebSocket API), no protocol changes needed
- Handshake timeout: if Redis subscription takes > configured timeout (default: 5s), fail with HTTP 504 Gateway Timeout

**Benefits**:
- No client code changes required (standard WebSocket behavior)
- Guaranteed subscription before any messages can be published
- Clear error handling if Redis unavailable

**Limitation**: This only prevents race condition during initial connection. Reconnections will lose messages published during disconnection.

### 2. Backpressure Handling

**Problem**: Fast agent publishing to Redis + slow client consuming from WebSocket = memory exhaustion in `wsproxy`.

**Solution**: Per-connection send buffer with limits and configurable backpressure strategy.

**Requirements**:
- Per-connection send buffer with configurable maximum size (default: 10 MB)
- When buffer exceeds threshold (e.g., 80% full):
  - Log warning with session_id
  - Increment backpressure metric counter
  - **Primary strategy**: Continue buffering up to max size
  - **When max size reached**: Close WebSocket with code 1008 (policy violation) and message "client too slow"
- Buffer size configurable via environment variable `MAX_BUFFER_SIZE_BYTES`
- Monitor buffer utilization per connection
- Metrics: `wsproxy_buffer_utilization_bytes{session_id}`, `wsproxy_backpressure_events_total`

**Trade-off**: Cannot signal backpressure to agent (Pub/Sub is fire-and-forget). Agent will continue publishing regardless of client speed.

### 3. Load Balancer and Session Affinity

**Problem**: Multiple `wsproxy` instances require session affinity for consistent connection handling.

**Solution**: Sticky sessions on load balancer based on `session_id`.

**Requirements**:
- **Load balancer configuration**:
  - Sticky sessions based on URL path parameter or cookie
  - Example WebSocket URL: `wss://wsproxy.example.com/{agent_id}/ws/{session_id}`
  - Hash routing by `session_id` to ensure same client always reaches same `wsproxy` instance
  - Session affinity timeout should match WebSocket idle timeout

- **Health check endpoint**:
  - `GET /health` returns 200 OK if Redis is reachable, 503 otherwise
  - `GET /ready` returns 200 OK if ready to accept connections
  - Health checks must not break session affinity (use separate backend pool or IP hash exception)

- **Graceful shutdown**:
  - On SIGTERM: stop accepting new WebSocket connections
  - Mark `/ready` endpoint as unhealthy (return 503)
  - Wait for existing connections to complete or timeout (configurable grace period, default: 30s)
  - Close remaining connections gracefully
  - Exit process

**Deployment notes**:
- Document sticky session configuration for nginx, HAProxy, AWS ALB
- Provide example configs in deployment documentation

### 4. Bidirectional Communication

**Problem**: Client needs to send messages to agent (e.g., cancel generation, user feedback).

**Solution**: Separate Redis Pub/Sub channels for upstream and downstream communication.

**Requirements**:
- **Downstream channel** (agent→client): `session:{session_id}:down`
  - Agent publishes to this channel
  - `wsproxy` subscribes to this channel
  - `wsproxy` forwards messages to client WebSocket

- **Upstream channel** (client→agent): `session:{session_id}:up`
  - Client sends WebSocket messages to `wsproxy`
  - `wsproxy` publishes to this channel
  - Agent subscribes to this channel (if bidirectional communication needed)

- `wsproxy` subscribes to both channels on connection establishment
- Control messages (ping/pong) handled locally by `wsproxy`, not published to Redis

**Optional**: If agents don't need client messages, upstream channel can be disabled via configuration.

### 5. Authentication and Authorization

**Problem**: Any client knowing `session_id` can connect and receive stream data.

**Solution**: Bearer token authentication with Redis-based validation. Each agent has its own bearer token, client already knows the token for the agent it's connecting to.

**Authentication Flow**:

1. **Session creation** (agent responsibility):
   - Client sends HTTP request to agent to create session (with `Authorization: Bearer {token}` header)
   - Agent validates bearer token
   - Agent generates `session_id` (UUID)
   - Agent stores token in Redis: `SET session:{session_id}:auth {bearer_token} EX {ttl}`
     - Key: `session:{session_id}:auth`
     - Value: the bearer token string
     - TTL: configurable (default: 300 seconds / 5 minutes)
   - Agent returns `session_id` to client

2. **WebSocket connection** (client):
   - Client connects to: `wss://wsproxy/{agent_id}/ws/{session_id}`
   - Client includes header: `Authorization: Bearer {token}`
   - Same bearer token that was used for session creation

3. **Token validation** (`wsproxy` responsibility):
   - Extract bearer token from `Authorization` header
   - Read expected token from Redis: `GET session:{session_id}:auth`
   - Compare tokens:
     - If match: proceed with Redis subscription and complete handshake
     - If mismatch: reject with HTTP 403 Forbidden
     - If Redis key not found: reject with HTTP 401 Unauthorized (session expired or doesn't exist)
   - Delete auth token from Redis after successful validation: `DEL session:{session_id}:auth`
     - Prevents token reuse for new connections
     - Single-use authentication token per WebSocket connection

**Requirements**:
- **Redis key naming**: `session:{session_id}:auth` for storing bearer tokens
- **Token TTL**: Configurable via agent (recommended: 5 minutes)
- **Authorization header format**: `Authorization: Bearer {token}`
- **Token cleanup**: `wsproxy` deletes auth token after validation to prevent reuse
- **Validation timeout**: Redis GET operation timeout (default: 1s)

**Error responses**:
- 400 Bad Request: Missing `Authorization` header or invalid format
- 401 Unauthorized: Token not found in Redis (session doesn't exist or expired)
- 403 Forbidden: Token found but doesn't match provided token
- 503 Service Unavailable: Redis is unreachable during validation

**Reconnection handling**:
- For reconnection support, agent must store a new auth token in Redis before client reconnects
- Agent can expose endpoint for refreshing session auth: `POST /sessions/{session_id}/refresh-auth`
- This creates a new temporary auth token for reconnection

### 6. Session Lifecycle and Cleanup

**Problem**: Completed sessions must be cleaned up to prevent resource leaks.

**Solution**: Explicit cleanup on connection close and periodic garbage collection.

**Requirements**:
- **On WebSocket close** (client disconnect or error):
  - Unsubscribe from Redis channels `session:{session_id}:down` and `session:{session_id}:up`
  - Remove session from internal tracking map
  - Log connection close with session_id and reason

- **Control message for stream end**:
  - Agent publishes `{"type": "control", "command": "stream_end", "reason": "completed|error|timeout"}`
  - `wsproxy` forwards to client
  - `wsproxy` **does not** auto-close WebSocket (client decides when to close)
  - Timeout: if WebSocket idle for configurable period after "stream_end" (default: 60s), close with code 1000

- **Redis channel lifecycle**:
  - Redis automatically removes channels when no subscribers
  - No explicit cleanup needed in Redis

- **Periodic cleanup task**:
  - Every N minutes (configurable, default: 5 minutes)
  - Check for zombie sessions (WebSocket closed but entry still in map)
  - Remove stale entries

**Metrics**: Track active sessions, cleanup events, zombie sessions detected

### 7. Message Format and Protocol

**Problem**: Message format between agent, wsproxy, and client must be standardized.

**Solution**: JSON-based message envelope with type discrimination.

**Message Format**:
```json
{
  "type": "control|data",
  "timestamp": "2025-01-14T12:34:56Z",
  "payload": {}
}
```

**Control Messages**:
- `{"type": "control", "command": "stream_end", "reason": "completed|error|timeout"}` - generation finished
- `{"type": "control", "command": "ping"}` / `{"type": "control", "command": "pong"}` - keepalive
- `{"type": "control", "command": "error", "code": "...", "message": "..."}` - error occurred

**Data Messages**:
- `{"type": "data", "payload": {...}}` - actual streaming data from agent

**Requirements**:
- All WebSocket messages must be valid JSON text messages
- `wsproxy` validates JSON structure on receive
- `wsproxy` forwards messages as-is without inspecting payload content
- Invalid JSON: log error and close WebSocket with code 1003 (unsupported data)
- Maximum message size: configurable (default: 10 MB), reject larger messages
- Binary WebSocket messages: not supported, close with code 1003

**Redis Pub/Sub messages**: Same JSON format as WebSocket messages

### 8. Reconnection Support (Limited)

**Problem**: Network interruptions cause message loss with Pub/Sub.

**Solution**: Limited reconnection support with documented data loss.

**Requirements**:
- Client can reconnect to same `session_id` by establishing new WebSocket connection
- `wsproxy` treats reconnection as new connection:
  - Re-subscribes to Redis channels before completing handshake
  - Forwards messages published **after** reconnection
- **Messages published during disconnection are LOST** (Pub/Sub limitation)
- Document this limitation in API specification
- Client should implement application-level idempotency and recovery if needed

**Optional enhancement**: Agent could maintain session state and allow client to request "replay" via HTTP endpoint, then resume streaming. This is outside `wsproxy` scope.

### 9. Monitoring and Observability

**Problem**: Need visibility into system health and message flow.

**Solution**: Prometheus metrics, structured logging, and health endpoints.

**Metrics** (Prometheus format at `/metrics`):
- `wsproxy_active_connections` - current WebSocket connections (gauge)
- `wsproxy_connections_total{status="success|auth_failed|error"}` - connection attempts (counter)
- `wsproxy_messages_received_total{source="redis"}` - messages from Redis (counter)
- `wsproxy_messages_sent_total{dest="websocket"}` - messages to clients (counter)
- `wsproxy_message_latency_seconds{quantile="0.5|0.95|0.99"}` - Redis→WebSocket latency (histogram)
- `wsproxy_errors_total{type="redis_error|websocket_error|json_error"}` - errors by type (counter)
- `wsproxy_buffer_utilization_bytes` - send buffer usage per connection (histogram)
- `wsproxy_backpressure_events_total` - buffer overflow events (counter)
- `wsproxy_redis_pubsub_channels_active` - active Redis subscriptions (gauge)

**Logging**:
- Structured JSON logs with fields:
  - `timestamp`: ISO8601
  - `level`: ERROR|WARN|INFO|DEBUG
  - `session_id`: for correlation
  - `message`: human-readable description
  - `error`: error details if applicable
- Log key events:
  - Connection open/close with session_id
  - Authentication success/failure
  - Redis subscription/unsubscription
  - Errors and exceptions
  - Backpressure events

**Health Endpoints**:
- `GET /health`: Returns 200 if Redis is reachable, 503 otherwise (for liveness probe)
- `GET /ready`: Returns 200 if ready to accept connections, 503 during shutdown (for readiness probe)
- `GET /metrics`: Prometheus metrics

**Requirements**:
- Log level configurable via `LOG_LEVEL` environment variable
- Metrics endpoint must not require authentication
- Health checks must be lightweight (< 10ms response time)