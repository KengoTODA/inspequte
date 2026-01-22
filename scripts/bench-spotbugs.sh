#!/bin/sh
set -euo pipefail

spotbugs_version="4.9.8"
smoke_root="target/smoketest"
spotbugs_dir="${smoke_root}/spotbugs-${spotbugs_version}"
lib_dir="${spotbugs_dir}/lib"

repeat="1"
if [ "$#" -ge 1 ]; then
  case "$1" in
    ''|*[!0-9]*)
      ;;
    *)
      repeat="$1"
      ;;
  esac
fi

if [ ! -d "${lib_dir}" ]; then
  mkdir -p "${smoke_root}"
  zip_file="${smoke_root}/spotbugs-${spotbugs_version}.zip"
  curl -L -o "${zip_file}" "https://github.com/spotbugs/spotbugs/releases/download/${spotbugs_version}/spotbugs-${spotbugs_version}.zip"
  unzip -q "${zip_file}" -d "${smoke_root}"
fi

if [ ! -d "${lib_dir}" ]; then
  echo "missing SpotBugs lib dir: ${lib_dir}" >&2
  exit 1
fi

log_dir="target/bench"
log_file="${log_dir}/spotbugs.log"
otel_dir="${log_dir}/otel"
mkdir -p "${log_dir}"
mkdir -p "${otel_dir}"
: > "${log_file}"

validate_env=""
if [ -n "${INSPEQUTE_VALIDATE_SARIF:-}" ]; then
  validate_env="INSPEQUTE_VALIDATE_SARIF=${INSPEQUTE_VALIDATE_SARIF}"
fi

cargo build >/dev/null

echo "bench: spotbugs_version=${spotbugs_version} repeat=${repeat}" | tee -a "${log_file}"
find "${lib_dir}" -type f -name "*.jar" | sort | while IFS= read -r jar_path; do
  i=1
  while [ "${i}" -le "${repeat}" ]; do
    tmp_log=$(mktemp)
    jar_name=$(basename "${jar_path}" | tr -c 'A-Za-z0-9._-' '_')
    otel_file="${otel_dir}/spotbugs-${jar_name}-run${i}.json"
    ${validate_env} ./target/debug/inspequte --input "${jar_path}" --timing --otel "${otel_file}" \
      1>/dev/null 2>"${tmp_log}"
    timing_line=$(tail -n 1 "${tmp_log}")
    rm -f "${tmp_log}"
    echo "run ${i}: ${jar_path} ${timing_line}" | tee -a "${log_file}"
    i=$((i + 1))
  done
done

echo "bench: log=${log_file}"
