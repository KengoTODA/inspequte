#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

if [[ $# -ne 2 ]]; then
  echo "Usage: $0 <schema-path> <instance-path>" >&2
  exit 1
fi

cargo run \
  --quiet \
  --manifest-path "${repo_root}/Cargo.toml" \
  --example validate_rule_authoring_json \
  -- "$1" "$2"
