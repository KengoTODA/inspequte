#!/usr/bin/env bash
set -euo pipefail

component="${1:?Usage: validate-release-source.sh cli|gradle TAG SHA}"
tag="${2:?Release tag is required}"
sha="${3:?Release SHA is required}"

case "${component}" in
  cli) pattern='^(inspequte-)?v[0-9]+\.[0-9]+\.[0-9]+$' ;;
  gradle) pattern='^gradle-plugin-v[0-9]+\.[0-9]+\.[0-9]+$' ;;
  *) echo 'Unknown release component' >&2; exit 1 ;;
esac

if [[ ! "${tag}" =~ ${pattern} || ! "${sha}" =~ ^[0-9a-f]{40}$ ]]; then
  echo 'Invalid release tag or commit SHA' >&2
  exit 1
fi
if [[ "${sha}" != "${GITHUB_SHA:?Triggering commit SHA is required}" || "$(git rev-parse HEAD)" != "${sha}" ]]; then
  echo 'Release commit must match both the triggering commit and checkout' >&2
  exit 1
fi

git fetch --no-tags origin "refs/tags/${tag}"
if [[ "$(git rev-parse 'FETCH_HEAD^{commit}')" != "${sha}" ]]; then
  echo 'Release tag does not point to the triggering commit' >&2
  exit 1
fi
