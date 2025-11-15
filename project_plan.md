# Project Plan: wsproxy

## Overview

Development plan for implementing WebSocket proxy service that decouples WebSocket connections from Python FastAPI workers using Redis Pub/Sub.

## Current Status

**Current Phase**: Phase 2 (Authentication & Security) - COMPLETED

**Completed Phases**:
- Phase 1: MVP - Core Functionality - COMPLETED
- Phase 2: Authentication & Security - COMPLETED

**Next Phase**: Phase 3 (Bidirectional Communication)

**Recent Achievements**:
- WebSocket server with Redis Pub/Sub integration
- Bearer token authentication via Redis
- Graceful shutdown with proper cleanup
- Health check endpoints
- Structured logging with tracing
- Heartbeat mechanism

## Development Phases

### Phase 1: MVP - Core Functionality

**Goal**: Basic working proxy with minimal features for initial testing.

**Tasks**:
1. **Project setup**
   - Initialize Rust project with dependencies (tokio, axum, redis, tungstenite)
   - Setup basic logging (tracing/tracing-subscriber)
   - Create configuration structure (environment variables)

2. **Basic WebSocket server**
   - HTTP server with WebSocket upgrade handler
   - Accept connections on `/{agent_id}/ws/{session_id}`
   - 
   - Parse agent_id and session_id from URL path
   - Complete handshake and maintain connection

3. **Redis Pub/Sub integration**
   - Connect to Redis
   - Subscribe to channel `session:{session_id}:down`
   - Receive messages from Redis channel
   - Forward messages to WebSocket client

4. **Basic error handling**
   - Log errors
   - Close connections on errors
   - Return appropriate HTTP status codes

5. **Graceful shutdown**
   - Implement CancellationToken pattern for coordinated shutdown
   - Handle SIGTERM/SIGINT signals
   - Stop accepting new WebSocket connections on shutdown signal
   - Wait for existing connections to finish (with timeout)
   - Clean up Redis subscriptions
   - Exit gracefully

**Deliverable**: Working proxy that accepts WebSocket connections and forwards Redis Pub/Sub messages to clients. Supports graceful shutdown.

**Testing**: Manual testing with simple client and Redis publisher.

---

### Phase 2: Authentication & Security

**Goal**: Implement Bearer token authentication via Redis.

**Tasks**:
1. **Bearer token validation**
   - Extract `Authorization: Bearer {token}` header
   - Read token from Redis: `GET session:{session_id}:auth`
   - Compare tokens
   - Return 401/403 on auth failure
   - Delete token after successful validation

2. **Handshake timing**
   - Delay WebSocket handshake completion until after:
     - Token validation
     - Redis subscription
   - Return errors before completing handshake if validation fails

3. **Timeout handling**
   - Add timeout for Redis operations during handshake (5s)
   - Return 504 Gateway Timeout if subscription takes too long

**Deliverable**: Secure proxy that requires valid bearer token for connections.

**Testing**: Test auth failures, token expiration, missing tokens.

---

### Phase 3: Bidirectional Communication

**Goal**: Support client-to-agent messaging via upstream channel.

**Tasks**:
1. **Upstream channel**
   - Subscribe to `session:{session_id}:up` channel
   - Receive messages from WebSocket client
   - Validate JSON format
   - Publish to Redis upstream channel

2. **Message validation**
   - Validate JSON structure
   - Enforce max message size
   - Handle invalid messages gracefully

3. **Control message handling**
   - Implement ping/pong locally (don't forward to Redis)
   - Handle control messages vs data messages

**Deliverable**: Proxy supports bidirectional WebSocket communication.

**Testing**: Test upstream/downstream messaging, ping/pong, large messages.

---

### Phase 4: Reliability & Resource Management

**Goal**: Handle failures gracefully and manage resources efficiently.

**Tasks**:
1. **Connection lifecycle**
   - WebSocket ping/pong with 30s timeout
   - Detect and clean up dead connections
   - Session idle timeout
   - Cleanup on disconnect: unsubscribe, remove from tracking map

2. **Backpressure handling**
   - Per-connection send buffer (configurable size, default 10 MB)
   - Monitor buffer utilization
   - Close connection when buffer full (code 1008)
   - Log backpressure events

3. **Redis reconnection**
   - Handle Redis connection loss
   - Exponential backoff for reconnection
   - Close affected WebSocket connections or buffer messages
   - Log reconnection events

4. **Resource limits**
   - Maximum connections per instance (configurable)
   - Maximum message size (configurable)
   - Connection rate limiting

**Deliverable**: Robust proxy that handles failures and manages resources.

**Testing**:
- Simulate Redis failures
- Test connection limits
- Test graceful shutdown
- Load testing with slow clients

---

### Phase 5: Observability & Monitoring

**Goal**: Full visibility into system health and performance.

**Tasks**:
1. **Prometheus metrics**
   - Implement metrics exporter
   - `/metrics` endpoint
   - Key metrics:
     - `wsproxy_active_connections`
     - `wsproxy_connections_total`
     - `wsproxy_messages_received_total`
     - `wsproxy_messages_sent_total`
     - `wsproxy_message_latency_seconds`
     - `wsproxy_errors_total`
     - `wsproxy_buffer_utilization_bytes`
     - `wsproxy_backpressure_events_total`
     - `wsproxy_redis_pubsub_channels_active`

2. **Health endpoints**
   - `GET /health` - liveness probe (Redis reachable)
   - `GET /ready` - readiness probe (ready for connections)

3. **Structured logging**
   - JSON log format
   - Fields: timestamp, level, session_id, message, error
   - Configurable log level via `LOG_LEVEL` env var
   - Log key events:
     - Connection open/close
     - Auth success/failure
     - Redis sub/unsub
     - Errors and exceptions
     - Backpressure events

4. **Distributed tracing** (optional)
   - Add trace IDs for request correlation
   - OpenTelemetry integration

**Deliverable**: Fully observable proxy with metrics, health checks, and structured logging.

**Testing**:
- Verify metrics accuracy
- Test health endpoints
- Verify log output format

---

### Phase 6: Performance Optimization & Load Testing

**Goal**: Ensure proxy meets performance requirements.

**Tasks**:
1. **Performance testing**
   - Test with 10,000+ concurrent connections
   - Measure latency (target: p99 < 50ms)
   - Measure throughput per connection
   - Memory profiling (target: 1-2 KB per idle connection)
   - CPU profiling under load

2. **Optimization**
   - Optimize hot paths
   - Reduce allocations
   - Use zero-copy where possible
   - Tune buffer sizes
   - Optimize Redis connection pool

3. **Stress testing**
   - Test connection handling (1000+ new connections/sec)
   - Test message throughput (100,000+ msg/sec)
   - Test with slow clients
   - Test Redis failure scenarios

**Deliverable**: Optimized proxy meeting all performance requirements.

**Testing**: Load tests, benchmarks, profiling.

---

### Phase 7: Deployment & Documentation

**Goal**: Production-ready deployment.

**Tasks**:
1. **Docker image**
   - Create optimized Dockerfile (multi-stage build)
   - Minimal base image (distroless or alpine)
   - Health check in Dockerfile
   - Published to container registry

2. **Configuration management**
   - Document all environment variables
   - Provide example configs
   - Validation on startup

3. **Deployment documentation**
   - Kubernetes manifests (Deployment, Service, HPA)
   - Load balancer configuration (nginx, HAProxy, AWS ALB)
   - Sticky session setup
   - Redis Sentinel/Cluster setup
   - Monitoring setup (Prometheus, Grafana dashboards)

4. **API documentation**
   - WebSocket connection protocol
   - Authentication flow
   - Message format specification
   - Error codes and handling
   - Client integration guide

5. **Operational runbooks**
   - Deployment procedures
   - Rollback procedures
   - Common issues and troubleshooting
   - Scaling guidelines
   - Monitoring and alerting

**Deliverable**: Production-ready deployment with complete documentation.

---

## Dependencies & Prerequisites

### Development Dependencies
- Rust toolchain (edition 2024)
- Redis server for testing
- WebSocket client for testing

### Runtime Dependencies
- Redis with Pub/Sub support
- Load balancer with sticky session support (for multi-instance deployment)

### Key Rust Crates
- `actix-web` - HTTP/WebSocket server framework
- `actix-web-actors` - WebSocket actor support
- `actix` - actor framework
- `redis` - Redis client with Pub/Sub (or `fred` for async Redis)
- `serde` / `serde_json` - JSON serialization
- `tracing` - structured logging framework
- `tracing-subscriber` - logging subscriber implementation
- `tracing-actix-web` - Actix Web tracing integration
- `prometheus` - metrics (or `actix-web-prometheus`)
- `uuid` - UUID generation and parsing

---

## Risk Management

### Technical Risks

1. **Redis Pub/Sub message loss**
   - Mitigation: Strict handshake protocol, document limitations
   - Status: Addressed in architecture

2. **Performance at scale**
   - Mitigation: Load testing, profiling, optimization
   - Status: Phase 6

3. **Memory leaks**
   - Mitigation: Proper cleanup, periodic garbage collection
   - Status: Phase 4

4. **Redis connection stability**
   - Mitigation: Reconnection logic, monitoring
   - Status: Phase 4

### Operational Risks

1. **Sticky session requirement**
   - Mitigation: Documentation, example configs
   - Status: Phase 7

2. **Complex deployment**
   - Mitigation: Complete documentation, automation
   - Status: Phase 7

---

## Success Criteria

### Phase 1 (MVP)
- [x] Can accept WebSocket connections
- [x] Can forward Redis messages to clients
- [x] Basic error handling works
- [x] Graceful shutdown with CancellationToken works

### Phase 2 (Auth)
- [x] Bearer token authentication working
- [x] Auth failures return correct error codes
- [x] Handshake timing prevents race conditions

### Phase 3 (Bidirectional)
- [ ] Client can send messages to agent
- [ ] Messages properly routed through Redis
- [ ] Control messages handled correctly

### Phase 4 (Reliability)
- [ ] Dead connections cleaned up automatically
- [ ] Backpressure prevents memory exhaustion
- [ ] Redis reconnection works

### Phase 5 (Observability)
- [ ] All metrics exposed
- [ ] Health endpoints work
- [ ] Logs structured and useful

### Phase 6 (Performance)
- [ ] Handles 10,000+ concurrent connections
- [ ] p99 latency < 50ms
- [ ] Memory < 2 KB per idle connection
- [ ] Passes all load tests

### Phase 7 (Production)
- [ ] Docker image built and published
- [ ] All documentation complete
- [ ] Deployed to staging environment
- [ ] Production deployment successful

---

## Timeline Estimate

- **Phase 1**: 3-5 days
- **Phase 2**: 2-3 days
- **Phase 3**: 2-3 days
- **Phase 4**: 5-7 days
- **Phase 5**: 3-4 days
- **Phase 6**: 5-7 days
- **Phase 7**: 3-5 days

**Total**: 23-34 days (approximately 5-7 weeks)

Note: Timeline assumes full-time dedicated development. Adjust based on actual resource availability.

---

## Next Steps

1. Review and approve project plan
2. Set up development environment
3. Begin Phase 1: MVP implementation
4. Schedule regular check-ins to review progress