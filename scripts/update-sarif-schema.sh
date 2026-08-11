#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
schema_path="${repo_root}/src/assets/sarif-2.1.0.json"
schema_url="https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json"
expected_sha256="c3b4bb2d6093897483348925aaa73af03b3e3f4bd4ca38cef26dcb4212a2682e"

usage() {
  echo "Usage: $0 --check|--refresh|--update" >&2
}

checksum() {
  shasum -a 256 "$1" | awk '{print $1}'
}

verify_checksum() {
  local candidate="$1"
  local actual
  actual="$(checksum "${candidate}")"
  if [[ "${actual}" != "${expected_sha256}" ]]; then
    echo "SARIF schema checksum mismatch: expected ${expected_sha256}, got ${actual}" >&2
    return 1
  fi
}

if [[ "$#" -ne 1 ]]; then
  usage
  exit 2
fi

case "$1" in
  --check)
    verify_checksum "${schema_path}"
    echo "Verified official OASIS SARIF schema: ${expected_sha256}"
    ;;
  --refresh|--update)
    tmp_dir="$(mktemp -d)"
    trap 'rm -rf "${tmp_dir}"' EXIT
    downloaded="${tmp_dir}/sarif-schema-2.1.0.json"
    curl -fsSL "${schema_url}" -o "${downloaded}"
    verify_checksum "${downloaded}"

    if [[ "$1" == "--refresh" ]]; then
      if ! cmp -s "${downloaded}" "${schema_path}"; then
        echo "Vendored SARIF schema differs from the official artifact." >&2
        exit 1
      fi
      echo "Verified vendored schema against ${schema_url}"
    else
      cp "${downloaded}" "${schema_path}"
      echo "Updated ${schema_path} from ${schema_url}"
    fi
    ;;
  *)
    usage
    exit 2
    ;;
esac
