#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"

"${script_dir}/validate-verify-input.sh"
"${script_dir}/validate-rule-authoring-json.sh" \
  "${repo_root}/schemas/rule-authoring/verification-result.schema.json" \
  "${repo_root}/verify-input/verify-result.json"
exec python3 "${script_dir}/rule_authoring_verification.py" finalize "$@"
