#!/usr/bin/env bash
set -euo pipefail

workflow="${1:?Usage: verify-release-artifacts.sh WORKFLOW SOURCE_SHA SIGNER_SHA FILE...}"
source_sha="${2:?Expected source SHA is required}"
signer_sha="${3:?Expected reusable workflow SHA is required}"
shift 3
repository="${GITHUB_REPOSITORY:?Expected repository is required}"

case "${workflow}" in
  build-cli-release.yml|build-gradle-release.yml) ;;
  *) echo 'Unknown release builder workflow' >&2; exit 1 ;;
esac
if [[ ! "${source_sha}" =~ ^[0-9a-f]{40}$ || ! "${signer_sha}" =~ ^[0-9a-f]{40}$ || "$#" -eq 0 ]]; then
  echo 'Expected commit SHAs and at least one artifact are required' >&2
  exit 1
fi

for artifact in "$@"; do
  if [[ ! -f "${artifact}" || -L "${artifact}" ]]; then
    echo "Missing or unsafe release artifact: ${artifact}" >&2
    exit 1
  fi
  gh attestation verify "${artifact}" \
    --repo "${repository}" \
    --signer-workflow "${repository}/.github/workflows/${workflow}" \
    --source-digest "${source_sha}" \
    --signer-digest "${signer_sha}" \
    --deny-self-hosted-runners
done
