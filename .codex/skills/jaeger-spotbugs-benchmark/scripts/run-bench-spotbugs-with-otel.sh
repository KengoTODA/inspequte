#!/usr/bin/env bash
set -euo pipefail

repeat="${1:-1}"
export OTEL_ENDPOINT="${OTEL_ENDPOINT:-http://localhost:4318/v1/traces}"

if [ ! -x "scripts/bench-spotbugs.sh" ]; then
  echo "scripts/bench-spotbugs.sh not found; run this command from repository root" >&2
  exit 1
fi

echo "running scripts/bench-spotbugs.sh repeat=${repeat}"
echo "OTEL_ENDPOINT=${OTEL_ENDPOINT}"

scripts/bench-spotbugs.sh "${repeat}"
