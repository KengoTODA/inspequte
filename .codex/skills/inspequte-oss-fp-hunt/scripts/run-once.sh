#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
export INSPEQUTE_REPO_ROOT="${repo_root}"
export INSPEQUTE_OTEL="${INSPEQUTE_OTEL:-http://localhost:4318/}"

if [ -z "${JAVA_HOME:-}" ]; then
  if command -v /usr/libexec/java_home >/dev/null 2>&1; then
    export JAVA_HOME="$(/usr/libexec/java_home -v 21)"
  fi
fi

if [ -z "${JAVA_HOME:-}" ] || [ ! -d "${JAVA_HOME}" ]; then
  echo "JAVA_HOME for Java 21 is required" >&2
  exit 1
fi

mkdir -p "${repo_root}/target/oss-fp/logs" "${repo_root}/target/oss-fp/triage" "${repo_root}/target/oss-fp/jaeger"

"${repo_root}/.codex/skills/inspequte-oss-fp-hunt/scripts/prepare-fixtures.sh"
"${repo_root}/.codex/skills/inspequte-oss-fp-hunt/scripts/patch-fixtures.sh"

(
  cd "${repo_root}"
  cargo build >/dev/null
)

export PATH="${repo_root}/target/debug:${PATH}"

"${repo_root}/.codex/skills/jaeger-spotbugs-benchmark/scripts/start-jaeger.sh" >/dev/null

decode_file_uri() {
  local uri="$1"
  uri="${uri#file://}"
  printf '%s' "${uri}"
}

declare -A rule_spec_map=()
while IFS=$'\t' read -r rule_id rule_dir; do
  if [ -n "${rule_id}" ] && [ -n "${rule_dir}" ]; then
    rule_spec_map["${rule_id}"]="${repo_root}/src/rules/${rule_dir}/spec.md"
  fi
done < <(
  rg -n 'id:' "${repo_root}"/src/rules/*/mod.rs | while IFS=: read -r path _line text; do
    rule_id="$(printf '%s' "${text}" | sed -n 's/.*id:[[:space:]]*"\([A-Z0-9_]*\)".*/\1/p')"
    if [ -n "${rule_id}" ]; then
      rule_dir="$(basename "$(dirname "${path}")")"
      printf '%s\t%s\n' "${rule_id}" "${rule_dir}"
    fi
  done
)

lookup_rule_spec() {
  local rule="$1"
  local spec_path="${rule_spec_map[${rule}]:-}"
  if [ -n "${spec_path}" ] && [ -f "${spec_path}" ]; then
    printf '%s' "${spec_path#${repo_root}/}"
    return 0
  fi
  printf 'N/A'
}

classify_finding() {
  local rule="$1"
  local file_uri="$2"
  local line="$3"
  local spec_ref="$4"

  if ! [[ "${line}" =~ ^[0-9]+$ ]]; then
    line=0
  fi

  if [[ "${file_uri}" == *"/.gradle/caches/modules-2/"* ]]; then
    printf 'FP\tclasspath dependency finding is out of scope (spec=%s)' "${spec_ref}"
    return 0
  fi

  if [ "${line}" -eq 0 ]; then
    printf 'FAILED\tline=0 cannot be mapped to actionable source (spec=%s)' "${spec_ref}"
    return 0
  fi

  if [ "${spec_ref}" = "N/A" ]; then
    printf 'FAILED\trule spec was not found; manual review required'
    return 0
  fi

  printf 'FAILED\tmanual rule validation required using spec=%s' "${spec_ref}"
}

run_fixture() {
  local name="$1"
  local dir="$2"
  local tasks="${3:-inspequteMain inspequteTest}"
  local log_file="${repo_root}/target/oss-fp/logs/${name}.log"
  local out_dir="${repo_root}/target/oss-fp/${name}"
  local triage_file="${repo_root}/target/oss-fp/triage/${name}.md"

  mkdir -p "${out_dir}"

  (
    cd "${dir}"
    ./gradlew --no-daemon clean ${tasks} >"${log_file}" 2>&1
  )

  rm -rf "${out_dir}/inspequte"
  cp -R "${dir}/build/inspequte" "${out_dir}/inspequte"

  {
    echo "# Triage: ${name}"
    echo
    echo "source: ${dir}"
    echo

    local total=0
    while IFS= read -r sarif; do
      local rel_sarif="${sarif#${dir}/}"
      echo "## ${rel_sarif}"
      local count
      count=$(jq '[.runs[]?.results[]?] | length' "${sarif}")
      echo "findings: ${count}"
      echo

      if [ "${count}" -eq 0 ]; then
        echo "No findings."
        echo
        continue
      fi

      local -A seen=()
      jq -r '
        .runs[]?.results[]? |
        [
          (.ruleId // ""),
          (.locations[0].physicalLocation.artifactLocation.uri // ""),
          ((.locations[0].physicalLocation.region.startLine // 0) | tostring),
          ((.message.text // "") | gsub("[\\n\\r\\t]+"; " "))
        ] | @tsv
      ' "${sarif}" | while IFS=$'\t' read -r rule file line message; do
        local key status reason classification spec_ref
        key="${rule}"$'\t'"${file}"$'\t'"${line}"$'\t'"${message}"
        spec_ref="$(lookup_rule_spec "${rule}")"
        if [[ -n "${seen[${key}]+x}" ]]; then
          status="FP"
          reason="duplicate finding (same rule, file, line, message; spec=${spec_ref})"
        else
          seen["${key}"]=1
          classification="$(classify_finding "${rule}" "${file}" "${line}" "${spec_ref}")"
          status="${classification%%$'\t'*}"
          reason="${classification#*$'\t'}"
        fi
        printf -- "- [ ] status=%s rule=%s file=%s line=%s message=%s reason=%s\n" \
          "${status}" "${rule}" "${file}" "${line}" "${message}" "${reason}"
      done
      echo
      total=$((total + count))
    done < <(find "${out_dir}/inspequte" -type f -name 'report.sarif' | sort)

    echo "total-findings: ${total}"
  } > "${triage_file}"
}

run_fixture "plasmo-config" "${repo_root}/target/oss-fp/workdir/plasmo-config" "inspequteMain inspequteTest"
run_fixture "okhttp-eventsource" "${repo_root}/target/oss-fp/workdir/okhttp-eventsource" "inspequteMain"

triage_files=(
  "${repo_root}/target/oss-fp/triage/plasmo-config.md"
  "${repo_root}/target/oss-fp/triage/okhttp-eventsource.md"
)

read -r triage_total triage_untriaged triage_tp triage_fp triage_failed dep_findings line0_findings kotlin_findings nullness_findings nullness_unique <<EOF
$(awk '
BEGIN {
  total = 0
  untriaged = 0
  tp = 0
  fp = 0
  failed = 0
  dep = 0
  line0 = 0
  kotlin = 0
  nullness = 0
}
/status=/ { total++ }
/status=UNTRIAGED/ { untriaged++ }
/status=TP/ { tp++ }
/status=FP/ { fp++ }
/status=FAILED/ { failed++ }
/\.gradle\/caches\/modules-2/ { dep++ }
/ line=0 / { line0++ }
/\/build\/classes\/kotlin\// { kotlin++ }
/rule=NULLNESS/ {
  nullness++
  entry = $0
  sub(/^.*- \[ \] /, "", entry)
  unique_nullness[entry] = 1
}
END {
  unique_count = 0
  for (item in unique_nullness) {
    unique_count++
  }
  printf "%d %d %d %d %d %d %d %d %d %d\n", total, untriaged, tp, fp, failed, dep, line0, kotlin, nullness, unique_count
}
' "${triage_files[@]}")
EOF

nullness_duplicate=$((nullness_findings - nullness_unique))
if [ "${nullness_duplicate}" -lt 0 ]; then
  nullness_duplicate=0
fi

assessment="preliminary"
if [ "${triage_total}" -eq 0 ]; then
  assessment="no-findings"
elif [ "${triage_untriaged}" -eq 0 ] && [ "${triage_failed}" -eq 0 ]; then
  assessment="triaged"
elif [ "${triage_untriaged}" -eq 0 ] && [ "${triage_failed}" -gt 0 ]; then
  assessment="needs-manual-review"
fi

fp_thoughts_json="$(
  jq -n \
    --arg assessment "${assessment}" \
    --argjson total "${triage_total}" \
    --argjson untriaged "${triage_untriaged}" \
    --argjson tp "${triage_tp}" \
    --argjson fp "${triage_fp}" \
    --argjson failed "${triage_failed}" \
    --argjson dep "${dep_findings}" \
    --argjson line0 "${line0_findings}" \
    --argjson kotlin "${kotlin_findings}" \
    --argjson nullness "${nullness_findings}" \
    --argjson null_unique "${nullness_unique}" \
    --argjson null_dup "${nullness_duplicate}" \
    '{
      assessment: $assessment,
      triage_status: {
        total_findings: $total,
        untriaged: $untriaged,
        tp: $tp,
        fp: $fp,
        failed: $failed
      },
      fp_signals: {
        dependency_cache_findings: $dep,
        line_zero_locations: $line0,
        kotlin_generated_findings: $kotlin,
        nullness_findings: $nullness,
        unique_nullness_findings: $null_unique,
        duplicated_nullness_findings: $null_dup
      },
      facts: [
        {
          id: "failed_findings",
          value: $failed
        },
        {
          id: "dependency_cache_findings",
          value: $dep
        },
        {
          id: "line_zero_locations",
          value: $line0
        },
        {
          id: "kotlin_generated_findings",
          value: $kotlin
        },
        {
          id: "nullness_findings",
          value: $nullness
        },
        {
          id: "unique_nullness_findings",
          value: $null_unique
        },
        {
          id: "duplicated_nullness_findings",
          value: $null_dup
        }
      ]
    }'
)"

trace_json="$(${repo_root}/.codex/skills/jaeger-spotbugs-benchmark/scripts/export-jaeger-trace.sh)"
summary_line="$(${repo_root}/.codex/skills/jaeger-spotbugs-benchmark/scripts/analyze-trace-json.sh "${trace_json}" | tr '\n' '\t')"
trace_id="$(basename "${trace_json}" | sed -E 's/^jaeger-trace-(.+)\.json$/\1/')"

{
  echo "# OSS FP Hunt Report"
  echo
  echo "fixtures:"
  cat "${repo_root}/target/oss-fp/fixture-shas.tsv"
  echo
  echo "trace-id: ${trace_id}"
  echo "trace-json: ${trace_json}"
  echo "trace-summary: ${summary_line}"
  echo
  echo "triage-files:"
  for triage in "${triage_files[@]}"; do
    echo "- ${triage#${repo_root}/}"
  done
  echo
  echo "fp-final-thoughts-json:"
  echo '```json'
  echo "${fp_thoughts_json}"
  echo '```'
} > "${repo_root}/target/oss-fp/report.md"

echo "run complete"
echo "trace-id=${trace_id}"
echo "report=${repo_root}/target/oss-fp/report.md"
