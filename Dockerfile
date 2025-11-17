# Multi-stage build for wsproxy
# Stage 1: Build the application
FROM rust:1.83-bookworm as builder

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies first (for caching)
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src
COPY examples ./examples

# Build the actual application
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime image
FROM debian:bookworm-slim

# Install CA certificates and curl for health checks, create non-root user
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -m -u 1000 -s /bin/bash wsproxy

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/wsproxy /app/wsproxy

# Copy entrypoint script
COPY entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# Set ownership
RUN chown -R wsproxy:wsproxy /app

# Switch to non-root user
USER wsproxy

# Expose port
EXPOSE 4040

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:4040/health || exit 1

# Run the application
ENTRYPOINT ["/app/entrypoint.sh"]