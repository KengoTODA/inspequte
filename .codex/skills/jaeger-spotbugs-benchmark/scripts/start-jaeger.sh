#!/usr/bin/env bash
set -euo pipefail

container_name="jaeger"

if docker ps --format '{{.Names}}' | grep -Fxq "${container_name}"; then
  echo "jaeger container is already running"
elif docker ps -a --format '{{.Names}}' | grep -Fxq "${container_name}"; then
  docker start "${container_name}" >/dev/null
  echo "started existing jaeger container"
else
  docker run -d --name jaeger -p 16686:16686 -p 4317:4317 -p 4318:4318 jaegertracing/all-in-one:latest >/dev/null
  echo "created and started jaeger container"
fi

echo "jaeger ui: http://localhost:16686"
echo "otlp grpc: localhost:4317"
echo "otlp http: http://localhost:4318/v1/traces"
