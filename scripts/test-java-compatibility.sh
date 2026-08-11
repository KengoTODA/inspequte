#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 2 || ! "$1" =~ ^[0-9]+$ || ! "$2" =~ ^[0-9]+$ ]]; then
  echo "Usage: $0 <java-release> <expected-class-major-version>" >&2
  exit 2
fi

java_release="$1"
expected_major="$2"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
fixture="${repo_root}/tests/fixtures/java-compatibility/ClassA.java"
work_dir="${repo_root}/target/java-compatibility/java-${java_release}"
classes_dir="${work_dir}/classes"
reports_dir="${work_dir}/reports"
jar_path="${work_dir}/class-a.jar"
inspequte_bin="${INSPEQUTE_BIN:-${repo_root}/target/release/inspequte}"
javac_bin="${JAVA_HOME:?JAVA_HOME must point to the requested JDK}/bin/javac"
java_bin="${JAVA_HOME}/bin/java"
jar_bin="${JAVA_HOME}/bin/jar"

for required in "${fixture}" "${inspequte_bin}" "${javac_bin}" "${java_bin}" "${jar_bin}"; do
  if [[ ! -e "${required}" ]]; then
    echo "Required compatibility-lane input is missing: ${required}" >&2
    exit 1
  fi
done

javac_version="$("${javac_bin}" -version 2>&1)"
if [[ "${javac_version}" != "javac ${java_release}"* ]]; then
  echo "Expected javac ${java_release}, got: ${javac_version}" >&2
  exit 1
fi

rm -rf "${work_dir}"
mkdir -p "${classes_dir}" "${reports_dir}"
{
  "${java_bin}" -XshowSettings:properties -version
  "${javac_bin}" -version
} >"${work_dir}/jdk-version.txt" 2>&1

"${javac_bin}" --release "${java_release}" -g -parameters -d "${classes_dir}" "${fixture}"

class_count=0
while IFS= read -r class_file; do
  class_count=$((class_count + 1))
  actual_major="$(od -An -t u1 -j 6 -N 2 "${class_file}" | awk 'NF >= 2 { print ($1 * 256) + $2; exit }')"
  if [[ "${actual_major}" != "${expected_major}" ]]; then
    echo "Expected class-file major ${expected_major}, got ${actual_major}: ${class_file}" >&2
    exit 1
  fi
done < <(find "${classes_dir}" -type f -name '*.class' | sort)

if [[ "${class_count}" -lt 2 ]]; then
  echo "Expected the Java compatibility fixture to produce multiple class files." >&2
  exit 1
fi

"${jar_bin}" --create --file "${jar_path}" -C "${classes_dir}" .

normalize_report() {
  local source="$1"
  local destination="$2"
  jq '(.runs[].invocations[]?.properties) |= with_entries(select(.key | endswith("_ms") | not))' \
    "${source}" >"${destination}"
}

scan_twice() {
  local label="$1"
  local input_path="$2"
  local report_dir="${reports_dir}/${label}"
  local report_path="${report_dir}/report.sarif"
  mkdir -p "${report_dir}"

  for attempt in 1 2; do
    local stderr_path="${report_dir}/attempt-${attempt}.stderr"
    if ! INSPEQUTE_VALIDATE_SARIF=1 "${inspequte_bin}" \
      --input "${input_path}" \
      --output "${report_path}" 2>"${stderr_path}"; then
      if grep -q "failed to parse class file" "${stderr_path}"; then
        echo "Java ${java_release} class-file parser incompatibility (${label})." >&2
      else
        echo "Java ${java_release} analysis failed after parsing (${label})." >&2
      fi
      sed -n '1,160p' "${stderr_path}" >&2
      return 1
    fi

    jq -e --arg schema "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json" \
      '.version == "2.1.0" and ."$schema" == $schema and (.runs | length) > 0 and (.runs[0].artifacts | length) > 0' \
      "${report_path}" >/dev/null
    normalize_report "${report_path}" "${report_dir}/attempt-${attempt}.normalized.sarif"
  done

  if ! cmp -s "${report_dir}/attempt-1.normalized.sarif" "${report_dir}/attempt-2.normalized.sarif"; then
    diff -u "${report_dir}/attempt-1.normalized.sarif" \
      "${report_dir}/attempt-2.normalized.sarif" >&2 || true
    echo "Java ${java_release} ${label} SARIF is not deterministic after timing normalization." >&2
    return 1
  fi
}

scan_twice "classes" "${classes_dir}"
scan_twice "jar" "${jar_path}"

echo "Java ${java_release} compatibility passed for class-file major ${expected_major}."
