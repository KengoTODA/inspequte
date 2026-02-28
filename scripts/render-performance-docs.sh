#!/usr/bin/env bash
set -euo pipefail

bench_dir="docs/benchmarks"
output_file="docs/performance.md"

usage() {
  cat <<'EOF'
Usage: scripts/render-performance-docs.sh [options]

Options:
  --bench-dir <PATH>  Benchmark JSON directory (default: docs/benchmarks)
  --output <PATH>     Output markdown file (default: docs/performance.md)
  --help              Show this help
EOF
}

fail() {
  printf '[render-performance-docs] %s\n' "$*" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bench-dir)
      bench_dir="${2:-}"
      shift 2
      ;;
    --output)
      output_file="${2:-}"
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${bench_dir}" != /* ]]; then
  bench_dir="${repo_root}/${bench_dir}"
fi
if [[ "${output_file}" != /* ]]; then
  output_file="${repo_root}/${output_file}"
fi

mkdir -p "$(dirname "${output_file}")"

meta_json="${bench_dir}/meta.json"
guava_json="${bench_dir}/guava.json"
sonarqube_json="${bench_dir}/sonarqube.json"

json_get_or_default() {
  local file="$1"
  local query="$2"
  local default="$3"
  if [[ -f "${file}" ]]; then
    jq -r "${query} // empty" "${file}" 2>/dev/null || true
  fi | awk -v d="${default}" 'NF {print; found=1} END {if (!found) print d}'
}

format_seconds() {
  local value="$1"
  awk -v v="${value}" 'BEGIN { printf "%.3f s", v }'
}

render_results_table() {
  local json_file="$1"
  local json_label
  json_label="$(basename "${json_file}")"
  if [[ ! -f "${json_file}" ]]; then
    echo "_No benchmark data file found: ${json_label}_"
    return
  fi

  if ! jq -e '.results | length > 0' "${json_file}" >/dev/null 2>&1; then
    echo "_No benchmark results in ${json_label}_"
    return
  fi

  echo "| Tool | Median | Mean | Min | Max |"
  echo "| --- | ---: | ---: | ---: | ---: |"
  while IFS=$'\t' read -r command median mean min max; do
    echo "| ${command} | $(format_seconds "${median}") | $(format_seconds "${mean}") | $(format_seconds "${min}") | $(format_seconds "${max}") |"
  done < <(jq -r '.results | sort_by(.median)[] | [.command, .median, .mean, .min, .max] | @tsv' "${json_file}")
}

generated_at="$(json_get_or_default "${meta_json}" '.generated_at_utc' 'unknown')"
env_os="$(json_get_or_default "${meta_json}" '.environment.os' 'unknown')"
env_kernel="$(json_get_or_default "${meta_json}" '.environment.kernel' 'unknown')"
env_cpu="$(json_get_or_default "${meta_json}" '.environment.cpu' 'unknown')"
env_java="$(json_get_or_default "${meta_json}" '.environment.java' 'unknown')"
benchmark_min_runs="$(json_get_or_default "${meta_json}" '.benchmark.min_runs' 'unknown')"
benchmark_warmup="$(json_get_or_default "${meta_json}" '.benchmark.warmup' 'unknown')"

guava_version="$(json_get_or_default "${meta_json}" '.datasets.guava.version' '33.5.0-jre')"
sonarqube_version="$(json_get_or_default "${meta_json}" '.datasets.sonarqube.version' '25.6.0.109173')"

spotbugs_version="$(json_get_or_default "${meta_json}" '.tools.spotbugs' '4.9.8')"
pmd_version="$(json_get_or_default "${meta_json}" '.tools.pmd' '7.14.0')"
checker_version="$(json_get_or_default "${meta_json}" '.tools.checker_framework' '3.52.0')"
nullaway_version="$(json_get_or_default "${meta_json}" '.tools.nullaway' '0.12.12')"
errorprone_version="$(json_get_or_default "${meta_json}" '.tools.error_prone' '2.30.0')"

{
  echo "# Performance"
  echo
  echo "## Purpose"
  echo '- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.'
  echo "- Keep scope to common nullness semantics rather than total rule count."
  echo
  echo "## Methodology"
  echo '- Benchmark metric: median runtime (`hyperfine` JSON `median`).'
  printf -- "- Parameters: \`--warmup %s\`, \`--min-runs %s\`.\n" "${benchmark_warmup}" "${benchmark_min_runs}"
  printf -- "- Generated at (UTC): \`%s\`.\n" "${generated_at}"
  echo
  echo "## Datasets"
  printf -- "- Library: Guava \`%s\`.\n" "${guava_version}"
  echo "  - Binary input: Maven Central JAR."
  echo "  - Source input: Maven Central source JAR."
  printf -- "- Application: SonarQube \`%s\`.\n" "${sonarqube_version}"
  echo '  - Binary input: Maven Central `sonar-application` ZIP.'
  echo "  - Source input: GitHub tag source archive."
  echo
  echo "## Tooling and Versions"
  echo "| Tool | Version | Nullness scope used in this page |"
  echo "| --- | --- | --- |"
  echo '| inspequte | workspace build | `NULLNESS` rule only |'
  printf -- "| SpotBugs | %s | \`NP_*\` via include filter (\`Bug code=NP\`) |\n" "${spotbugs_version}"
  printf -- "| PMD | %s | null-related subset in \`category/java/errorprone.xml\` (cache=off) |\n" "${pmd_version}"
  printf -- "| Checker Framework | %s | \`NullnessChecker\` |\n" "${checker_version}"
  printf -- "| NullAway | %s | Error Prone plugin (\`error_prone_core %s\`) |\n" "${nullaway_version}" "${errorprone_version}"
  echo
  echo "Environment:"
  printf -- "- OS: \`%s\`\n" "${env_os}"
  printf -- "- Kernel: \`%s\`\n" "${env_kernel}"
  printf -- "- CPU: \`%s\`\n" "${env_cpu}"
  printf -- "- Java: \`%s\`\n" "${env_java}"
  echo
  echo "## Results: Guava"
  render_results_table "${guava_json}"
  echo
  echo "## Results: SonarQube"
  render_results_table "${sonarqube_json}"
  echo
  echo "## Caveats and Fairness"
  echo "- Rule sets are aligned to nullness intent, not full one-to-one semantic equivalence."
  echo "- Source-oriented tools and bytecode-oriented tools have different execution models."
  echo "- For operational comparison, an order-of-magnitude speed difference is acceptable in this report."
  echo "- This page compares performance only, not detection quality or precision/recall."
  echo
  echo "## Repro Command"
  echo '```bash'
  echo 'bash scripts/bench-nullness-compare.sh --dataset all --min-runs 5 --warmup 1'
  echo 'bash scripts/render-performance-docs.sh'
  echo '```'
} > "${output_file}"

printf '[render-performance-docs] wrote %s\n' "${output_file}"
