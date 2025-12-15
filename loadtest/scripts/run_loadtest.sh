#!/usr/bin/env bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DOCKER_DIR="$PROJECT_ROOT/loadtest/docker"
ENV_FILE="$DOCKER_DIR/.env.loadtest"

CLIENTS=${CLIENTS:-100}
QUERIES_PER_CLIENT=${QUERIES_PER_CLIENT:-10}
WSPROXY_INSTANCES=${WSPROXY_INSTANCES:-3}
FASTAPI_INSTANCES=${FASTAPI_INSTANCES:-2}
FASTAPI_WORKERS=${FASTAPI_WORKERS:-4}

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Run load test against wsproxy infrastructure

OPTIONS:
    --clients NUM               Number of concurrent clients (default: 100)
    --queries NUM               Queries per client (default: 10)
    --wsproxy-instances NUM     Number of wsproxy instances (default: 3)
    --fastapi-instances NUM     Number of FastAPI instances (default: 2)
    --fastapi-workers NUM       Workers per FastAPI instance (default: 4)
    --up-only                   Only start infrastructure, don't run test
    --down                      Stop and remove all containers
    --logs                      Show logs from all services
    -h, --help                  Show this help message

EXAMPLES:
    $0 --clients 1000 --queries 10
    $0 --wsproxy-instances 5 --fastapi-instances 3
    $0 --up-only
    $0 --down
EOF
}

UP_ONLY=false
DOWN=false
LOGS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --clients)
            CLIENTS="$2"
            shift 2
            ;;
        --queries)
            QUERIES_PER_CLIENT="$2"
            shift 2
            ;;
        --wsproxy-instances)
            WSPROXY_INSTANCES="$2"
            shift 2
            ;;
        --fastapi-instances)
            FASTAPI_INSTANCES="$2"
            shift 2
            ;;
        --fastapi-workers)
            FASTAPI_WORKERS="$2"
            shift 2
            ;;
        --up-only)
            UP_ONLY=true
            shift
            ;;
        --down)
            DOWN=true
            shift
            ;;
        --logs)
            LOGS=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

if [ "$DOWN" = true ]; then
    echo "Stopping load test infrastructure..."
    cd "$DOCKER_DIR"
    docker-compose -f docker-compose.loadtest.yml down -v
    echo "Infrastructure stopped"
    exit 0
fi

if [ "$LOGS" = true ]; then
    cd "$DOCKER_DIR"
    docker-compose -f docker-compose.loadtest.yml logs -f
    exit 0
fi

export CLIENTS QUERIES_PER_CLIENT FASTAPI_WORKERS

echo "Starting load test infrastructure..."
echo "  wsproxy instances: $WSPROXY_INSTANCES"
echo "  FastAPI instances: $FASTAPI_INSTANCES"
echo "  FastAPI workers per instance: $FASTAPI_WORKERS"
echo ""

cd "$DOCKER_DIR"

docker-compose -f docker-compose.loadtest.yml up -d redis haproxy

for i in $(seq 1 $WSPROXY_INSTANCES); do
    docker-compose -f docker-compose.loadtest.yml up -d wsproxy-$i
done

for i in $(seq 1 $FASTAPI_INSTANCES); do
    docker-compose -f docker-compose.loadtest.yml up -d fastapi-$i
done

echo "Waiting for services to be healthy..."
sleep 10

echo ""
echo "Infrastructure is ready!"
echo "  HAProxy stats: http://localhost:8404/stats"
echo "  FastAPI: http://localhost:8080"
echo "  WebSocket: ws://localhost:8081"
echo ""

if [ "$UP_ONLY" = true ]; then
    echo "Infrastructure started. Use --down to stop."
    exit 0
fi

echo "Running load test..."
echo "  Clients: $CLIENTS"
echo "  Queries per client: $QUERIES_PER_CLIENT"
echo ""

docker run --rm \
    --network loadtest_loadtest \
    --env-file "$ENV_FILE" \
    -e FASTAPI_URL=http://haproxy:8080 \
    -e WSPROXY_URL=ws://haproxy:8081 \
    -e CLIENTS=$CLIENTS \
    -e QUERIES_PER_CLIENT=$QUERIES_PER_CLIENT \
    -e OUTPUT_DIR=/results \
    -v "$PROJECT_ROOT/loadtest_results:/results" \
    $(docker-compose -f docker-compose.loadtest.yml images -q fastapi-1) \
    python -m loadtest.client.load_client

echo ""
echo "Load test complete!"
echo "Results saved to: $PROJECT_ROOT/loadtest_results/"
echo ""
echo "To analyze results:"
echo "  python loadtest/scripts/analyze_results.py loadtest_results/<csv_file>"
echo ""
echo "To stop infrastructure:"
echo "  $0 --down"
