#!/usr/bin/env bash
set -euo pipefail

container_name="jaeger"
jaeger_ui_url="${JAEGER_BASE_URL:-http://localhost:16686}"
otlp_http_url="${OTEL_ENDPOINT:-http://localhost:4318/}"

if docker ps --format '{{.Names}}' | grep -Fxq "${container_name}"; then
  echo "jaeger container is already running"
elif docker ps -a --format '{{.Names}}' | grep -Fxq "${container_name}"; then
  docker start "${container_name}" >/dev/null
  echo "started existing jaeger container"
else
  docker run -d --name jaeger -p 16686:16686 -p 4317:4317 -p 4318:4318 jaegertracing/all-in-one:latest >/dev/null
  echo "created and started jaeger container"
fi

for attempt in $(seq 1 30); do
  if curl -fsS "${jaeger_ui_url}/api/services" >/dev/null 2>&1; then
    break
  fi
  if [ "${attempt}" -eq 30 ]; then
    echo "jaeger ui did not become ready at ${jaeger_ui_url}" >&2
    exit 1
  fi
  sleep 2
done

for attempt in $(seq 1 30); do
  if curl -sS -o /dev/null --max-time 2 "${otlp_http_url}"; then
    break
  fi
  if [ "${attempt}" -eq 30 ]; then
    echo "jaeger otlp http endpoint did not become reachable at ${otlp_http_url}" >&2
    exit 1
  fi
  sleep 2
done

echo "jaeger ui: ${jaeger_ui_url}"
echo "otlp grpc: localhost:4317"
echo "otlp http: ${otlp_http_url}"
