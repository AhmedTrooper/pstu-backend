#!/usr/bin/env bash
set -e

echo "==> Stopping and erasing project docker containers, volumes, and networks..."
docker compose down -v --remove-orphans 2>/dev/null || true

PORTS=(3000 3001 5432 6379 4222 8222 9090 16686 4317 4318 8080 8000)

echo "==> Freeing conflicting ports: ${PORTS[*]}..."

# Stop any Docker container binding to these ports
for port in "${PORTS[@]}"; do
  CONTAINERS=$(docker ps -q --filter "publish=$port" 2>/dev/null || true)
  if [ -n "$CONTAINERS" ]; then
    echo "Stopping container(s) on port $port: $CONTAINERS"
    docker stop $CONTAINERS 2>/dev/null || true
  fi
done

# Kill any lingering host processes on local dev ports (8080, 3000, 3001)
for port in 8080 3000 3001; do
  if command -v fuser >/dev/null 2>&1; then
    fuser -k "${port}/tcp" 2>/dev/null || true
  fi
done

echo "==> All target ports (3000, 3001, 5432, 6379, 4222, 8080, 8000) are free."
