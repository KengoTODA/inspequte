#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

trace_id="${1:-}"
service_name="${SERVICE_NAME:-inspequte}"
jaeger_base_url="${JAEGER_BASE_URL:-http://localhost:16686}"
out_dir="target/bench"
mkdir -p "${out_dir}"

if [ -z "${trace_id}" ]; then
  query_url="${jaeger_base_url}/api/traces?service=${service_name}&limit=20&lookback=1h"
  trace_id="$(curl -fsSL "${query_url}" | jq -r '.data[0].traceID // .data[0].traceId // empty')"
fi

if [ -z "${trace_id}" ]; then
  echo "could not resolve trace id from jaeger" >&2
  exit 1
fi

out_file="${out_dir}/jaeger-trace-${trace_id}.json"
curl -fsSL "${jaeger_base_url}/api/traces/${trace_id}" -o "${out_file}"

echo "${out_file}"
