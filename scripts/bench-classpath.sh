#!/bin/sh
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 <input> [repeat] [classpath...]" >&2
  exit 2
fi

input="$1"
shift

repeat="3"
if [ "$#" -gt 0 ]; then
  case "$1" in
    ''|*[!0-9]*)
      ;;
    *)
      repeat="$1"
      shift
      ;;
  esac
fi

classpath_args=""
for entry in "$@"; do
  classpath_args="${classpath_args} --classpath ${entry}"
done

log_dir="target/bench"
log_file="${log_dir}/classpath.log"
otel_dir="${log_dir}/otel"
mkdir -p "${log_dir}"
mkdir -p "${otel_dir}"

cargo build >/dev/null

echo "bench: input=${input} repeat=${repeat}" | tee -a "${log_file}"
i=1
while [ "${i}" -le "${repeat}" ]; do
  tmp_log=$(mktemp)
  input_name=$(basename "${input}" | tr -c 'A-Za-z0-9._-' '_')
  otel_file="${otel_dir}/classpath-${input_name}-run${i}.json"
  ./target/debug/inspequte --input "${input}" --timing --otel "${otel_file}" ${classpath_args} \
    1>/dev/null 2>"${tmp_log}"
  timing_line=$(tail -n 1 "${tmp_log}")
  rm -f "${tmp_log}"
  echo "run ${i}: ${timing_line}" | tee -a "${log_file}"
  i=$((i + 1))
done

echo "bench: log=${log_file}"
