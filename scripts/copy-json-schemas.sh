#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
src_dir="${repo_root}/src/assets"
dest_dir="${repo_root}/docs/schemas"

mkdir -p "${dest_dir}"
find "${dest_dir}" -type f -name '*.json' -delete

found=0
for schema in "${src_dir}"/*.schema.json; do
  if [[ ! -f "${schema}" ]]; then
    continue
  fi
  found=1
  base="$(basename "${schema}")"
  dest_name="${base%.schema.json}.json"
  # Publish a compact machine-oriented JSON form under docs/schemas.
  python3 -c '
import json
import pathlib
import sys

source = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])
with source.open("r", encoding="utf-8") as reader:
    data = json.load(reader)
with dest.open("w", encoding="utf-8") as writer:
    json.dump(data, writer, separators=(",", ":"), ensure_ascii=False)
    writer.write("\n")
' "${schema}" "${dest_dir}/${dest_name}"
done

if [[ "${found}" -eq 0 ]]; then
  echo "copy-json-schemas: no schema files found under ${src_dir}" >&2
fi

echo "Copied schema files to ${dest_dir}"
