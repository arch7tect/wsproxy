# wsproxy Deployment Guide

## Docker Deployment

### Quick Start with Docker Compose

```bash
# Start both wsproxy and Redis
docker-compose up -d

# View logs
docker-compose logs -f wsproxy

# Stop services
docker-compose down
```

### Building the Docker Image

```bash
# Build the image
docker build -t wsproxy:latest .

# Run with docker run
docker run -d \
  --name wsproxy \
  -p 8080:8080 \
  -e REDIS_URL=redis://host.docker.internal:6379 \
  -e RUST_LOG=info,wsproxy=debug \
  wsproxy:latest
```

### Environment Variables

All configuration is done via environment variables:

```bash
# Server
HOST=0.0.0.0
PORT=8080

# Redis
REDIS_URL=redis://localhost:6379

# WebSocket
WS_PING_INTERVAL_SECS=15
WS_PING_TIMEOUT_SECS=30

# Authentication
AUTH_TIMEOUT_SECS=5

# Shutdown
SHUTDOWN_GRACE_PERIOD_SECS=30

# Messages
MAX_MESSAGE_SIZE_BYTES=1048576

# Logging
RUST_LOG=info,wsproxy=debug
LOG_LEVEL=info
```

## Kubernetes Deployment

### Deployment Manifest

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: wsproxy
spec:
  replicas: 3
  selector:
    matchLabels:
      app: wsproxy
  template:
    metadata:
      labels:
        app: wsproxy
    spec:
      containers:
      - name: wsproxy
        image: wsproxy:latest
        ports:
        - containerPort: 8080
        env:
        - name: HOST
          value: "0.0.0.0"
        - name: PORT
          value: "8080"
        - name: REDIS_URL
          value: "redis://redis-service:6379"
        - name: RUST_LOG
          value: "info,wsproxy=info"
        livenessProbe:
          exec:
            command:
            - /app/wsproxy
            - --version
          initialDelaySeconds: 5
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 3
          periodSeconds: 10
        resources:
          requests:
            memory: "64Mi"
            cpu: "100m"
          limits:
            memory: "512Mi"
            cpu: "1000m"
---
apiVersion: v1
kind: Service
metadata:
  name: wsproxy
spec:
  selector:
    app: wsproxy
  ports:
  - protocol: TCP
    port: 8080
    targetPort: 8080
  type: LoadBalancer
  sessionAffinity: ClientIP  # Sticky sessions required
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 3600
```

## Production Checklist

### Security
- [ ] Use TLS/SSL termination at load balancer
- [ ] Enable firewall rules
- [ ] Set secure environment variables via secrets
- [ ] Run as non-root user (default in Docker image)

### Monitoring
- [ ] Set up health check endpoints
- [ ] Configure log aggregation
- [ ] Set up alerting for connection errors
- [ ] Monitor Redis connection status

### Scalability
- [ ] Configure sticky sessions on load balancer
- [ ] Set appropriate connection limits
- [ ] Monitor memory usage per connection
- [ ] Configure horizontal pod autoscaling (HPA)

### High Availability
- [ ] Run multiple replicas (minimum 3)
- [ ] Use Redis Sentinel or Cluster for HA
- [ ] Configure readiness/liveness probes
- [ ] Set up graceful shutdown handling

## Testing the Deployment

```bash
# Connect to health endpoint
curl http://localhost:8080/health

# Connect via WebSocket (requires auth token)
wscat -c "ws://localhost:8080/agent1/ws/test-session" \
  -H "Authorization: Bearer your-token-here"
```

## Troubleshooting

### Container won't start
```bash
# Check logs
docker logs wsproxy

# Verify Redis connectivity
docker exec -it wsproxy ping redis
```

### WebSocket connections failing
- Verify sticky sessions are enabled on load balancer
- Check auth tokens are set in Redis
- Verify REDIS_URL is correct

### High memory usage
- Check MAX_MESSAGE_SIZE_BYTES configuration
- Monitor active connection count
- Review message throughput

## Load Balancer Configuration

### nginx Example

```nginx
upstream wsproxy {
    ip_hash;  # Sticky sessions
    server wsproxy-1:8080;
    server wsproxy-2:8080;
    server wsproxy-3:8080;
}

server {
    listen 80;
    server_name wsproxy.example.com;

    location / {
        proxy_pass http://wsproxy;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;

        # WebSocket timeouts
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

### HAProxy Example

```
frontend wsproxy_frontend
    bind *:80
    default_backend wsproxy_backend

backend wsproxy_backend
    balance source  # Sticky sessions
    option httpchk GET /health
    server wsproxy1 10.0.1.1:8080 check
    server wsproxy2 10.0.1.2:8080 check
    server wsproxy3 10.0.1.3:8080 check
```

## Metrics and Monitoring

Future implementation will include:
- Prometheus metrics endpoint at `/metrics`
- Active connection count
- Message throughput
- Error rates
- Redis connection status