#!/usr/bin/env bash
set -euo pipefail

directory="${1:?Usage: verify-cli-release.sh DIRECTORY TAG SHA}"
tag="${2:?Release tag is required}"
sha="${3:?Expected source SHA is required}"
if [[ ! "${tag}" =~ ^(inspequte-)?v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo 'Invalid CLI release tag' >&2
  exit 1
fi

artifacts=()
for target in amd64-unknown-linux-gnu arm64-unknown-linux-gnu arm64-apple-darwin amd64-apple-darwin amd64-pc-windows-msvc; do
  extension=tar.gz
  if [[ "${target}" == *-windows-* ]]; then extension=zip; fi
  artifacts+=("${directory}/inspequte-${tag}-${target}.${extension}")
done

# Check the full inventory before making any verification or publication calls.
for artifact in "${artifacts[@]}"; do
  if [[ ! -f "${artifact}" ]]; then
    echo "Missing expected CLI archive: ${artifact}" >&2
    exit 1
  fi
done
shopt -s nullglob
found=("${directory}"/*.tar.gz "${directory}"/*.zip)
if [[ "${#found[@]}" -ne "${#artifacts[@]}" ]]; then
  echo 'Unexpected CLI release archive inventory' >&2
  exit 1
fi

bash "$(dirname "$0")/verify-release-artifacts.sh" build-cli-release.yml "${sha}" "${sha}" "${artifacts[@]}"
