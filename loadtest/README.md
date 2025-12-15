# wsproxy Load Testing Infrastructure

Comprehensive load testing setup for wsproxy with FastAPI server simulation, HAProxy load balancing, and Python asyncio clients.

## Table of Contents
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Setup](#setup)
- [Running Tests](#running-tests)
- [Configuration](#configuration)
- [Results & Metrics](#results--metrics)
- [Scaling](#scaling)
- [Testing Scenarios](#testing-scenarios)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Implementation Details](#implementation-details)

## Quick Start

### Prerequisites
- Docker and Docker Compose
- Python 3.12
- uv (for dependency management)

### 1. Setup Environment
```bash
# Create virtual environment and install dependencies
uv venv
uv pip install -e .
```

### 2. Start Infrastructure
```bash
cd loadtest/docker
docker-compose -f docker-compose.loadtest.yml up -d
```

### 3. Run Smoke Test
```bash
FASTAPI_URL=http://localhost:8080 \
WSPROXY_URL=ws://localhost:8081 \
CLIENTS=5 \
QUERIES_PER_CLIENT=2 \
JWT_SECRET=<same-secret-from-.env.loadtest> \
.venv/bin/python -m loadtest.client.load_client
```

### 4. View Results
- Console output shows summary
- Detailed results in `loadtest_results/`
- HAProxy stats: http://localhost:8404/stats

### 5. Stop Infrastructure
```bash
cd loadtest/docker
docker-compose -f docker-compose.loadtest.yml down
```

## Architecture

### Component Diagram
```
Load Clients (Python asyncio)
         ↓
    HAProxy :8080 (HTTP)
         ↓
    FastAPI × 2 (Granian: 4 workers each)
         ↓
    Redis Pub/Sub
         ↓
    wsproxy × 3
         ↑
    HAProxy :8081 (WebSocket)
         ↑
Load Test Clients (WebSocket)
```

### Message Flow
1. Client → `POST /session/create` → FastAPI
2. FastAPI creates session, generates JWT token with claims `{session_id, iat}`
3. Client → WebSocket connect via `/{agent_id}/ws/{session_id}` → HAProxy → wsproxy
4. wsproxy validates JWT signature (stateless, CPU-only validation)
5. Client → `POST /session/{id}/run` → FastAPI
6. FastAPI publishes tokens to Redis `session:{id}:down`
7. wsproxy receives from Redis, forwards to WebSocket client
8. Client receives streaming tokens
9. Client → `POST /session/{id}/close`

### Components

#### FastAPI Server (Granian)
Simulates LLM backend with token streaming via Redis
- `POST /session/create` - Create session, returns session_id and JWT auth_token
- `POST /session/{id}/run` - Submit query, streams tokens via Redis
- `POST /session/{id}/close` - Close session
- `GET /health` - Health check

#### Python Load Test Client
Async client that exercises full workflow
- Creates session via HTTP API
- Connects WebSocket through wsproxy
- Runs N queries per session
- Receives streamed tokens
- Collects metrics (latency, throughput, success rate)

#### Infrastructure
- **Redis**: Message bus for Pub/Sub (optimized for 10k+ connections)
- **HAProxy**: Load balancer with session affinity
  - FastAPI backend: Round-robin (stateless HTTP)
  - wsproxy backend: Hash on session_id (sticky sessions)
- **wsproxy**: 3 instances handling WebSocket connections
- **FastAPI**: 2 instances with 4 workers each

## Setup

### Install Dependencies
```bash
# Create virtual environment
uv venv

# Install project in development mode
uv pip install -e .

# Activate virtual environment (optional)
source .venv/bin/activate
```

### Start Infrastructure
```bash
cd loadtest/docker
docker-compose -f docker-compose.loadtest.yml up -d
```

### Check Status
```bash
# View all containers
docker-compose -f loadtest/docker/docker-compose.loadtest.yml ps

# Health checks
curl http://localhost:8080/health  # FastAPI
curl http://localhost:8081/health  # wsproxy

# HAProxy stats
open http://localhost:8404/stats
```

## Running Tests

### Using Python Directly

**Smoke Test** (5 clients, 2 queries):
```bash
FASTAPI_URL=http://localhost:8080 \
WSPROXY_URL=ws://localhost:8081 \
CLIENTS=5 \
QUERIES_PER_CLIENT=2 \
JWT_SECRET=<same-secret-from-.env.loadtest> \
.venv/bin/python -m loadtest.client.load_client
```

**Standard Test** (20 clients, 3 queries):
```bash
FASTAPI_URL=http://localhost:8080 \
WSPROXY_URL=ws://localhost:8081 \
CLIENTS=20 \
QUERIES_PER_CLIENT=3 \
JWT_SECRET=<same-secret-from-.env.loadtest> \
.venv/bin/python -m loadtest.client.load_client
```

**Load Test** (100 clients, 10 queries):
```bash
FASTAPI_URL=http://localhost:8080 \
WSPROXY_URL=ws://localhost:8081 \
CLIENTS=100 \
QUERIES_PER_CLIENT=10 \
JWT_SECRET=<same-secret-from-.env.loadtest> \
.venv/bin/python -m loadtest.client.load_client
```

### Using Orchestration Script

Basic test with defaults:
```bash
./loadtest/scripts/run_loadtest.sh
```

Custom configuration:
```bash
./loadtest/scripts/run_loadtest.sh \
  --clients 1000 \
  --queries 10 \
  --wsproxy-instances 5 \
  --fastapi-instances 3 \
  --fastapi-workers 4
```

Start infrastructure only:
```bash
./loadtest/scripts/run_loadtest.sh --up-only
```

View logs:
```bash
./loadtest/scripts/run_loadtest.sh --logs
```

Stop infrastructure:
```bash
./loadtest/scripts/run_loadtest.sh --down
```

### Run FastAPI Server Locally (without Docker)
```bash
granian --interface asgi --host 0.0.0.0 --port 8000 --workers 4 loadtest.server.main:app
```

## Configuration

### Environment Variables

Edit `loadtest/docker/.env.loadtest`:

**Redis:**
- `REDIS_URL` - Redis connection URL (default: redis://redis:6379)

**JWT Authentication (shared by FastAPI and wsproxy):**
- `JWT_SECRET` - HMAC secret for signing/validating JWT tokens (required, >=32 bytes)
- `JWT_ALGORITHM` - JWT signing algorithm (default: HS256)

**FastAPI Server:**
- `FASTAPI_HOST` - Server host (default: 0.0.0.0)
- `FASTAPI_PORT` - Server port (default: 8000)
- `FASTAPI_WORKERS` - Granian workers (default: 4)
- `SESSION_TTL_SECONDS` - Session metadata TTL in Redis (default: 10800 / 3 hours)
- `TOKEN_DELAY_MS` - Delay between tokens (default: 50ms)
- `LLM_RESPONSE_TEXT` - Response text to stream

**wsproxy:**
- `HOST` - Server host (default: 0.0.0.0)
- `PORT` - Server port (default: 4040)
- `PUBSUB_POOL_SIZE` - Redis connection pool (default: 512)
- `WS_PING_INTERVAL_SECS` - WebSocket ping interval (default: 15)
- `WS_PING_TIMEOUT_SECS` - WebSocket ping timeout (default: 30)

**Load Test Client:**
- `WSPROXY_URL` - WebSocket URL (default: ws://haproxy:8081)
- `FASTAPI_URL` - HTTP API URL (default: http://haproxy:8080)
- `CLIENTS` - Number of concurrent clients (default: 100)
- `QUERIES_PER_CLIENT` - Queries per client (default: 10)
- `OUTPUT_DIR` - Results output directory (default: /app/loadtest_results)

**WebSocket Reconnection:**
- `WS_MAX_RECONNECT_ATTEMPTS` - Maximum reconnection attempts (default: 5)
- `WS_INITIAL_BACKOFF_SECONDS` - Initial backoff delay (default: 1.0)
- `WS_MAX_BACKOFF_SECONDS` - Maximum backoff delay (default: 30.0)

## Results & Metrics

### Automatic Reports

After each test run, results are saved to `loadtest_results/`:
- `loadtest_YYYYMMDD_HHMMSS.csv` - Detailed per-query data
- `loadtest_YYYYMMDD_HHMMSS_summary.json` - Aggregated metrics

### Analyze Results

```bash
.venv/bin/python loadtest/scripts/analyze_results.py loadtest_results/loadtest_*.csv
```

Output includes:
- Session and query counts
- Latency percentiles (p10, p25, p50, p75, p90, p95, p99)
- Latency histogram
- Error breakdown

### Metrics Collected

**Per-Session:**
- Session create time
- WebSocket connect time
- Query latencies (array)
- Token counts (per query)
- Total tokens received
- Reconnection attempts
- Successful reconnections
- Errors (if any)

**Aggregated:**
- Total sessions / Successful / Failed
- Success rate (%)
- Total queries / Total tokens
- Queries per second (QPS)
- Tokens per second (TPS)
- Latency percentiles: p50, p95, p99, min, max, avg
- Average create/connect times
- Total reconnection attempts
- Total successful reconnections
- Sessions affected by reconnections

**Metrics Guide:**
- **Success Rate**: Percentage of sessions that completed without errors
- **QPS**: Queries per second (throughput)
- **TPS**: Tokens per second (streaming throughput)
- **p50/p95/p99**: Latency percentiles (median, 95th, 99th percentile)
- **Create/Connect Time**: Session establishment latency

## Scaling

### Horizontal Scaling

**Add wsproxy instances:**
1. Edit `docker-compose.loadtest.yml` to add wsproxy-4, wsproxy-5, etc.
2. Update `haproxy.cfg` to include new backends
3. Restart HAProxy: `docker-compose restart haproxy`

**Add FastAPI instances:**
1. Edit `docker-compose.loadtest.yml` to add fastapi-3, fastapi-4, etc.
2. Update `haproxy.cfg` to include new backends
3. Restart HAProxy: `docker-compose restart haproxy`

### Vertical Scaling

**Increase FastAPI workers:**
```bash
# Set in .env.loadtest
FASTAPI_WORKERS=8
```

**Increase Redis Pub/Sub pool:**
```bash
# Set in .env.loadtest
PUBSUB_POOL_SIZE=1024
```

### Recommended Scaling for 10k+ Connections
- 5-10 wsproxy instances
- 3-4 FastAPI instances
- 8-16 workers per FastAPI
- PUBSUB_POOL_SIZE=1024
- Run load clients on separate machines

## Testing Scenarios

### Smoke Test
Quick validation (10 clients, 1 query each):
```bash
./loadtest/scripts/run_loadtest.sh --clients 10 --queries 1
```

### Load Test
Moderate load (1000 clients, 10 queries each):
```bash
./loadtest/scripts/run_loadtest.sh --clients 1000 --queries 10
```

### Stress Test
High load (5000 clients, 20 queries each):
```bash
./loadtest/scripts/run_loadtest.sh \
  --clients 5000 \
  --queries 20 \
  --wsproxy-instances 5 \
  --fastapi-instances 3
```

### Soak Test
Long-running test (100 clients, 100 queries each):
```bash
./loadtest/scripts/run_loadtest.sh --clients 100 --queries 100
```

## Development

### Project Structure

```
loadtest/
├── __init__.py
├── config.py              # Configuration management
├── server/                # FastAPI server components
│   ├── __init__.py
│   ├── main.py           # Application entry point
│   ├── models.py         # Pydantic models
│   ├── session_manager.py # Session lifecycle
│   ├── llm_simulator.py  # Token streaming simulator
│   └── redis_client.py   # Redis Pub/Sub wrapper
├── client/               # Load test client
│   ├── __init__.py
│   ├── session_client.py # Single session handler
│   ├── load_client.py    # Multi-client orchestrator
│   ├── metrics.py        # Metrics collection
│   └── reporter.py       # Results reporting
├── docker/               # Docker infrastructure
│   ├── Dockerfile.fastapi
│   ├── haproxy.cfg
│   ├── docker-compose.loadtest.yml
│   └── .env.loadtest
└── scripts/              # Automation scripts
    ├── run_loadtest.sh
    └── analyze_results.py
```

### Adding New Metrics

1. Add fields to `SessionMetrics` in `loadtest/client/metrics.py`
2. Collect metrics in `SessionClient` in `loadtest/client/session_client.py`
3. Update aggregation in `AggregatedMetrics.from_session_metrics()`
4. Update reporter in `loadtest/client/reporter.py`

### Modifying LLM Simulation

Edit `loadtest/server/llm_simulator.py`:
- Change token generation logic
- Adjust streaming delay
- Add randomization for realistic behavior

## Troubleshooting

### Port Conflicts
```bash
# Check what's using a port
lsof -i :8080
lsof -i :8081
lsof -i :8404
lsof -i :6379
```

### Services Not Starting
```bash
# Check logs
docker logs loadtest-fastapi-1
docker logs loadtest-wsproxy-1
docker logs loadtest-haproxy
docker logs loadtest-redis

# Or all at once
cd loadtest/docker
docker-compose -f docker-compose.loadtest.yml logs
```

### Connection Refused Errors
```bash
# Ensure all services are healthy
docker-compose -f loadtest/docker/docker-compose.loadtest.yml ps

# Wait for health checks to pass (may take 10-20 seconds)
```

### Code Changes Not Reflected
```bash
# Rebuild specific service
docker-compose -f loadtest/docker/docker-compose.loadtest.yml build fastapi-1
docker-compose -f loadtest/docker/docker-compose.loadtest.yml up -d fastapi-1
```

### WebSocket Connection Failures
- Verify wsproxy URL uses correct format: `ws://localhost:8081/{agent_id}/ws/{session_id}`
- Check JWT_SECRET matches between FastAPI and wsproxy in `.env.loadtest`
- Verify HAProxy backend health: http://localhost:8404/stats
- Check wsproxy logs for JWT validation errors

### High Error Rates
1. Check Redis capacity (increase maxclients in redis_100percent.conf)
2. Increase wsproxy instances
3. Reduce concurrent clients
4. Check HAProxy stats for backend health

### Low Throughput
1. Increase FastAPI workers
2. Reduce token delay (TOKEN_DELAY_MS)
3. Add more wsproxy instances
4. Scale FastAPI instances

## Implementation Details

### Technologies Used
- **Python 3.12**: Latest stable Python
- **uv**: Fast Python package installer
- **Granian**: Rust-based ASGI server for better performance
- **FastAPI**: Modern web framework
- **asyncio**: All I/O operations are async
- **Redis Pub/Sub**: Existing wsproxy architecture
- **HAProxy**: Industry-standard load balancer
- **Docker Compose**: Easy orchestration

### Key Design Decisions

1. **Granian over uvicorn**: Better performance for high load
2. **Asyncio-based client**: Maximum concurrency
3. **Redis Pub/Sub**: Realistic testing of wsproxy architecture
4. **Metrics-first**: Comprehensive metrics from the start
5. **Docker Compose**: Easy scaling and orchestration
6. **Session-based load balancing**: HAProxy hashes on session_id from URL path
   - Ensures same session always routes to same wsproxy instance
   - Enables efficient Redis Pub/Sub (single subscription per session)
   - Works when all clients have same source IP (common in load testing)
   - Reconnections for same session go to same instance

### WebSocket URL Format

wsproxy requires URL format: `/{agent_id}/ws/{session_id}`
- `agent_id`: Any string identifier (e.g., "load-test-agent")
- `session_id`: UUID returned from session creation

### Authentication

Sessions use JWT-based Bearer token authentication:
1. FastAPI generates JWT token with claims: `{session_id, iat}` signed with HS256
2. Client includes in WebSocket header: `Authorization: Bearer {jwt_token}`
3. wsproxy validates JWT signature and session_id claim (stateless, no Redis lookup)

**JWT Configuration:**
- Algorithm: HS256 (symmetric HMAC-SHA256)
- Claims: `session_id` (must match URL path), `iat` (issued at timestamp)
- No expiry: Tokens valid indefinitely (no `exp` claim)
- Same `JWT_SECRET` must be configured on all FastAPI and wsproxy instances

**Performance:**
- JWT validation is CPU-only (no Redis RTT)
- 10-20x faster than Redis-based token validation (~0.1-1ms vs ~5-20ms)

**Security:**
- Generate secret: `python3 -c "import secrets; print(secrets.token_urlsafe(48))"`
- Secret must be at least 32 bytes
- Same secret required on all FastAPI and wsproxy instances
- In production, use secret management service (Vault, AWS Secrets Manager, etc.)
- Tokens have no expiry - rely on secret rotation to invalidate

### WebSocket Reconnection

The load test client implements automatic reconnection with exponential backoff to handle network failures and temporary service interruptions:

**Features:**
- Automatic reconnection on WebSocket failures (connection errors, timeouts, network issues)
- Exponential backoff strategy to avoid overwhelming the server
- Configurable retry attempts and backoff parameters
- Comprehensive metrics tracking (attempts, successes, affected sessions)

**How It Works:**
1. When a WebSocket connection fails, the client catches the exception
2. Client waits for initial backoff period (default: 1 second)
3. If reconnection fails, backoff doubles up to maximum (default: 30 seconds)
4. Process repeats up to maximum attempts (default: 5)
5. All reconnection attempts are tracked in metrics

**Configuration:**
- `WS_MAX_RECONNECT_ATTEMPTS`: Maximum number of reconnection attempts (default: 5)
- `WS_INITIAL_BACKOFF_SECONDS`: Initial delay before first retry (default: 1.0)
- `WS_MAX_BACKOFF_SECONDS`: Maximum backoff delay cap (default: 30.0)

**Metrics:**
- Per-session: reconnect_attempts, successful_reconnects
- Aggregated: total_reconnect_attempts, total_successful_reconnects, sessions_with_reconnects

This feature ensures the load test can continue even with intermittent network issues, providing realistic testing of production scenarios where network failures may occur.

### Bug Fixes During Implementation

#### Redis time() Coroutine
**Issue:** `TypeError: 'coroutine' object is not subscriptable`
**Fix:** Changed `self.redis.time()` to `time.time()`

#### WebSocket Library API
**Issue:** `extra_headers` parameter not recognized
**Fix:** Updated to `additional_headers` (websockets 15.x)

#### WebSocket URL Path
**Issue:** HTTP 404 when connecting
**Fix:** Added agent_id to URL path

#### HAProxy Service Discovery
**Issue:** Couldn't resolve service names
**Fix:** Updated to use full container names (loadtest-*)

## Performance Expectations

### Small Scale (2 FastAPI, 3 wsproxy, default config)
- 20 concurrent clients, 3 queries each
- Success Rate: 100%
- QPS: ~20
- TPS: ~310
- Session creation: 22ms
- WebSocket connect: 50ms
- Query p50: ~960ms
- No reconnections needed

### Medium Scale (2 FastAPI, 3 wsproxy, default config)
- 1000 concurrent clients, 10 queries each
- Success rate: >99%
- Expected p99 latency: <2s

### High Scale - Stress Test Results

**Configuration:**
- FastAPI: 2 instances × 4 workers (8 total)
- wsproxy: 3 instances
- Redis: Pub/Sub pool size 1024
- HAProxy: maxconn 100k, timeouts 10s/60s/60s

**Test: 5000 Concurrent Clients × 3 Queries**

#### Before Optimization (Default Limits)
| Metric | Value |
|--------|-------|
| Success Rate | 98.78% (4939/5000) |
| Test Duration | 83.10s |
| QPS | 178.31 |
| TPS | 215.21 |
| Query p50 | 16.18s |
| Query p99 | 27.48s |
| Reconnect Attempts | 4618 (87% of sessions) |

**Bottlenecks:**
- PUBSUB_POOL_SIZE: 512 (insufficient for 5000 sessions)
- HAProxy maxconn: 50,000
- HAProxy timeouts: 5s/30s/30s (too short for bursts)

#### After Optimization
| Metric | Value | Improvement |
|--------|-------|-------------|
| Success Rate | 99.46% (4973/5000) | +0.68% |
| Failed Sessions | 27 | -56% failures |
| Test Duration | 70.65s | -15% faster |
| QPS | 211.18 | +18.4% |
| TPS | 336.94 | +56.5% |
| Query p50 | 15.81s | -0.37s faster |
| Query p99 | 26.82s | -0.66s faster |
| Query avg | 15.03s | -1.08s faster |
| Reconnect Attempts | 4032 (78% of sessions) | -12.7% fewer |
| Reconnect Success | 96.7% | +3.3% |

**Optimizations Applied:**
```bash
# .env.loadtest
PUBSUB_POOL_SIZE=1024  # Was: 512

# haproxy.cfg
maxconn 100000         # Was: 50000
timeout connect 10s    # Was: 5s
timeout client 60s     # Was: 30s
timeout server 60s     # Was: 30s
server fastapi1 ... maxconn 10000
server fastapi2 ... maxconn 10000
server wsproxy1 ... maxconn 5000
server wsproxy2 ... maxconn 5000
server wsproxy3 ... maxconn 5000
```

**Key Findings:**
- JWT authentication scaled successfully to 5000 concurrent clients
- Zero JWT validation errors (all failures were capacity-related)
- Optimizations delivered 56% reduction in failures
- 56% improvement in token streaming throughput
- Reconnection logic handled bursts effectively (96.7% success rate)

### Scaling Recommendations

**For 5000+ Concurrent Clients (99.9%+ success rate):**
1. Increase FastAPI workers to 8-12 per instance
2. Add more FastAPI instances (2 → 3-4)
3. Implement connection rate limiting to smooth bursts
4. Monitor system resources (CPU, memory, network)
5. Consider dedicated load testing infrastructure

**For 10,000+ Concurrent Clients:**
1. Scale to 5-7 wsproxy instances
2. Scale to 4-5 FastAPI instances with 8-12 workers each
3. Increase PUBSUB_POOL_SIZE to 2048
4. Use multiple load testing machines to distribute client load
5. Optimize Redis with maxclients and maxmemory configuration

## Dependencies

```toml
[project]
requires-python = ">=3.12"
dependencies = [
    "websockets>=12.0",
    "redis>=5.0.0",
    "aiohttp>=3.9.0",
    "fastapi>=0.109.0",
    "granian>=1.2.0",
    "pydantic>=2.5.0",
    "pydantic-settings>=2.1.0",
    "uvloop>=0.19.0",
    "orjson>=3.9.0",
    "pyjwt>=2.8.0",
]
```

## Further Reading

- [wsproxy Architecture](../architecture.md)
- [Project Plan](../project_plan.md)
- [CLAUDE.md](../CLAUDE.md) - Project guidelines
