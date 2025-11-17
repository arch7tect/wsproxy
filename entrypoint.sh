#!/bin/sh
set -e
echo "=== Entrypoint script started ==="
echo "Checking binary..."
ls -l /app/wsproxy
echo "Checking environment..."
env | grep -E "REDIS|HOST|PORT|LOG" || echo "No relevant env vars found"
echo "Running wsproxy binary..."
exec /app/wsproxy