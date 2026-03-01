#!/usr/bin/env bash
set -euo pipefail

# renovate: datasource=maven depName=com.google.guava:guava versioning=loose
DATASET_VERSION_GUAVA="33.5.0-jre"
# renovate: datasource=maven depName=org.sonarsource.sonarqube:sonar-application
DATASET_VERSION_SONARQUBE="26.2.0.119303"

# renovate: datasource=github-releases depName=spotbugs/spotbugs
TOOL_VERSION_SPOTBUGS="4.9.8"
# renovate: datasource=github-releases depName=pmd/pmd
TOOL_VERSION_PMD="7.14.0"
# renovate: datasource=github-releases depName=typetools/checker-framework
TOOL_VERSION_CHECKER_FRAMEWORK="3.52.0"
# renovate: datasource=maven depName=com.uber.nullaway:nullaway
TOOL_VERSION_NULLAWAY="0.13.1"
# renovate: datasource=maven depName=com.google.errorprone:error_prone_core
TOOL_VERSION_ERROR_PRONE="2.47.0"

# renovate: datasource=maven depName=org.jspecify:jspecify
GUAVA_VERSION_JSPECIFY="1.0.0"
# renovate: datasource=maven depName=com.google.errorprone:error_prone_annotations
GUAVA_VERSION_ERROR_PRONE_ANNOTATIONS="2.47.0"
# renovate: datasource=maven depName=com.google.j2objc:j2objc-annotations
GUAVA_VERSION_J2OBJC="3.1"

dataset="all"
min_runs="5"
warmup="1"
output_dir="docs/benchmarks"
dry_run="false"

usage() {
  cat <<'EOF'
Usage: scripts/bench-nullness-compare.sh [options]

Options:
  --dataset <guava|sonarqube|all>  Target dataset (default: all)
  --min-runs <N>                   hyperfine min runs (default: 5)
  --warmup <N>                     hyperfine warmup count (default: 1)
  --output-dir <PATH>              Output directory for benchmark JSON (default: docs/benchmarks)
  --dry-run                        Print planned benchmark commands only
  --help                           Show this help
EOF
}

log() {
  printf '[bench-nullness] %s\n' "$*"
}

fail() {
  printf '[bench-nullness] %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    fail "required command not found: ${cmd}"
  fi
}

download_with_retry() {
  local url="$1"
  local dest="$2"
  local attempts=3
  local i=1

  if [[ -f "${dest}" ]]; then
    return 0
  fi

  mkdir -p "$(dirname "${dest}")"
  while [[ "${i}" -le "${attempts}" ]]; do
    if curl -fsSL --retry 3 --retry-all-errors -o "${dest}" "${url}"; then
      return 0
    fi
    log "download failed (${i}/${attempts}): ${url}"
    i=$((i + 1))
    sleep 2
  done

  fail "failed to download after ${attempts} attempts: ${url}"
}

ensure_unzip() {
  local zip_path="$1"
  local extract_root="$2"
  local marker_dir="$3"

  if [[ -d "${marker_dir}" ]]; then
    return 0
  fi

  mkdir -p "${extract_root}"
  unzip -q -o "${zip_path}" -d "${extract_root}"
}

validate_hyperfine_result() {
  local json_path="$1"
  local label="$2"
  jq -e '.results | length > 0' "${json_path}" >/dev/null \
    || fail "${label} benchmark did not produce any results: ${json_path}"
  jq -e 'all(.results[]; all(.exit_codes[]; . == 0))' "${json_path}" >/dev/null \
    || fail "${label} benchmark contains non-zero exit codes: ${json_path}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dataset)
      dataset="${2:-}"
      shift 2
      ;;
    --min-runs)
      min_runs="${2:-}"
      shift 2
      ;;
    --warmup)
      warmup="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    --dry-run)
      dry_run="true"
      shift
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

case "${dataset}" in
  guava|sonarqube|all) ;;
  *)
    fail "--dataset must be guava, sonarqube, or all: ${dataset}"
    ;;
esac

case "${min_runs}" in
  ''|*[!0-9]*)
    fail "--min-runs must be numeric: ${min_runs}"
    ;;
esac

case "${warmup}" in
  ''|*[!0-9]*)
    fail "--warmup must be numeric: ${warmup}"
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ "${output_dir}" != /* ]]; then
  output_dir="${repo_root}/${output_dir}"
fi

if [[ "${dry_run}" == "true" ]]; then
  mkdir -p "${output_dir}"
  if [[ "${dataset}" == "guava" || "${dataset}" == "all" ]]; then
    cat <<EOF
hyperfine --warmup ${warmup} --min-runs ${min_runs} --export-json ${output_dir}/guava.json \
  --command-name inspequte ./target/bench/work/cmd/guava/inspequte.sh \
  --command-name spotbugs ./target/bench/work/cmd/guava/spotbugs.sh \
  --command-name pmd ./target/bench/work/cmd/guava/pmd.sh \
  --command-name checker-framework ./target/bench/work/cmd/guava/checker.sh \
  --command-name nullaway ./target/bench/work/cmd/guava/nullaway.sh
EOF
  fi
  if [[ "${dataset}" == "sonarqube" || "${dataset}" == "all" ]]; then
    cat <<EOF
hyperfine --warmup ${warmup} --min-runs ${min_runs} --export-json ${output_dir}/sonarqube.json \
  --command-name inspequte ./target/bench/work/cmd/sonarqube/inspequte.sh \
  --command-name spotbugs ./target/bench/work/cmd/sonarqube/spotbugs.sh \
  --command-name pmd ./target/bench/work/cmd/sonarqube/pmd.sh
EOF
  fi
  exit 0
fi

require_cmd curl
require_cmd jq
require_cmd unzip
require_cmd hyperfine
require_cmd cargo
require_cmd java
require_cmd javac
if [[ "${dataset}" == "guava" || "${dataset}" == "all" ]]; then
  require_cmd mvn
fi

cache_dir="${repo_root}/target/bench/cache"
work_dir="${repo_root}/target/bench/work"
tools_dir="${work_dir}/tools"
config_dir="${work_dir}/config"
cmd_dir="${work_dir}/cmd"
result_dir="${work_dir}/result"
mkdir -p "${cache_dir}" "${tools_dir}" "${config_dir}" "${cmd_dir}" "${result_dir}" "${output_dir}"

spotbugs_zip="${cache_dir}/spotbugs-${TOOL_VERSION_SPOTBUGS}.zip"
spotbugs_url="https://github.com/spotbugs/spotbugs/releases/download/${TOOL_VERSION_SPOTBUGS}/spotbugs-${TOOL_VERSION_SPOTBUGS}.zip"
download_with_retry "${spotbugs_url}" "${spotbugs_zip}"
ensure_unzip "${spotbugs_zip}" "${tools_dir}" "${tools_dir}/spotbugs-${TOOL_VERSION_SPOTBUGS}"
spotbugs_home="${tools_dir}/spotbugs-${TOOL_VERSION_SPOTBUGS}"

pmd_zip="${cache_dir}/pmd-dist-${TOOL_VERSION_PMD}-bin.zip"
pmd_url="https://github.com/pmd/pmd/releases/download/pmd_releases/${TOOL_VERSION_PMD}/pmd-dist-${TOOL_VERSION_PMD}-bin.zip"
download_with_retry "${pmd_url}" "${pmd_zip}"
ensure_unzip "${pmd_zip}" "${tools_dir}" "${tools_dir}/pmd-bin-${TOOL_VERSION_PMD}"
pmd_home="${tools_dir}/pmd-bin-${TOOL_VERSION_PMD}"

checker_zip="${cache_dir}/checker-framework-${TOOL_VERSION_CHECKER_FRAMEWORK}.zip"
checker_url="https://github.com/typetools/checker-framework/releases/download/checker-framework-${TOOL_VERSION_CHECKER_FRAMEWORK}/checker-framework-${TOOL_VERSION_CHECKER_FRAMEWORK}.zip"
download_with_retry "${checker_url}" "${checker_zip}"
ensure_unzip "${checker_zip}" "${tools_dir}" "${tools_dir}/checker-framework-${TOOL_VERSION_CHECKER_FRAMEWORK}"
checker_home="${tools_dir}/checker-framework-${TOOL_VERSION_CHECKER_FRAMEWORK}"

chmod +x "${checker_home}/checker/bin/javac"
chmod +x "${pmd_home}/bin/pmd"

spotbugs_filter="${config_dir}/spotbugs-nullness-include.xml"
cat > "${spotbugs_filter}" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<FindBugsFilter>
  <Match>
    <Bug code="NP" />
  </Match>
</FindBugsFilter>
EOF

pmd_ruleset="${config_dir}/pmd-nullness-ruleset.xml"
cat > "${pmd_ruleset}" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<ruleset name="Nullness subset"
         xmlns="http://pmd.sourceforge.net/ruleset/2.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://pmd.sourceforge.net/ruleset/2.0.0 https://pmd.github.io/schema/ruleset_2_0_0.xsd">
  <description>Nullness-focused subset for cross-tool performance comparison.</description>
  <rule ref="category/java/errorprone.xml/BrokenNullCheck"/>
  <rule ref="category/java/errorprone.xml/MisplacedNullCheck"/>
  <rule ref="category/java/errorprone.xml/NullAssignment"/>
</ruleset>
EOF

log "building inspequte release binary"
cargo build --release --locked >/dev/null

run_guava="false"
run_sonarqube="false"
if [[ "${dataset}" == "guava" || "${dataset}" == "all" ]]; then
  run_guava="true"
fi
if [[ "${dataset}" == "sonarqube" || "${dataset}" == "all" ]]; then
  run_sonarqube="true"
fi

if [[ "${run_guava}" == "true" ]]; then
  log "preparing guava dataset"
  guava_dir="${work_dir}/dataset-guava"
  mkdir -p "${guava_dir}"

  guava_jar="${cache_dir}/guava-${DATASET_VERSION_GUAVA}.jar"
  guava_jar_url="https://repo1.maven.org/maven2/com/google/guava/guava/${DATASET_VERSION_GUAVA}/guava-${DATASET_VERSION_GUAVA}.jar"
  download_with_retry "${guava_jar_url}" "${guava_jar}"

  guava_sources_jar="${cache_dir}/guava-${DATASET_VERSION_GUAVA}-sources.jar"
  guava_sources_url="https://repo1.maven.org/maven2/com/google/guava/guava/${DATASET_VERSION_GUAVA}/guava-${DATASET_VERSION_GUAVA}-sources.jar"
  download_with_retry "${guava_sources_url}" "${guava_sources_jar}"

  guava_sources_root="${guava_dir}/sources"
  if [[ ! -d "${guava_sources_root}" ]]; then
    mkdir -p "${guava_sources_root}"
    unzip -q "${guava_sources_jar}" -d "${guava_sources_root}"
  fi

  jspecify_jar="${cache_dir}/jspecify-${GUAVA_VERSION_JSPECIFY}.jar"
  download_with_retry \
    "https://repo1.maven.org/maven2/org/jspecify/jspecify/${GUAVA_VERSION_JSPECIFY}/jspecify-${GUAVA_VERSION_JSPECIFY}.jar" \
    "${jspecify_jar}"

  error_prone_annotations_jar="${cache_dir}/error_prone_annotations-${GUAVA_VERSION_ERROR_PRONE_ANNOTATIONS}.jar"
  download_with_retry \
    "https://repo1.maven.org/maven2/com/google/errorprone/error_prone_annotations/${GUAVA_VERSION_ERROR_PRONE_ANNOTATIONS}/error_prone_annotations-${GUAVA_VERSION_ERROR_PRONE_ANNOTATIONS}.jar" \
    "${error_prone_annotations_jar}"

  j2objc_annotations_jar="${cache_dir}/j2objc-annotations-${GUAVA_VERSION_J2OBJC}.jar"
  download_with_retry \
    "https://repo1.maven.org/maven2/com/google/j2objc/j2objc-annotations/${GUAVA_VERSION_J2OBJC}/j2objc-annotations-${GUAVA_VERSION_J2OBJC}.jar" \
    "${j2objc_annotations_jar}"

  guava_input_list="${guava_dir}/inputs.txt"
  printf '%s\n' "${guava_jar}" > "${guava_input_list}"

  guava_harness_dir="${guava_dir}/harness"
  mkdir -p "${guava_harness_dir}/src/main/java/bench/nullness"
  guava_harness_src="${guava_harness_dir}/src/main/java/bench/nullness/ClassA.java"
  cat > "${guava_harness_src}" <<'EOF'
package bench.nullness;

import com.google.common.base.Strings;
import org.jspecify.annotations.NullMarked;
import org.jspecify.annotations.Nullable;

@NullMarked
class ClassA {
  int methodX(@Nullable String varOne) {
    if (Strings.isNullOrEmpty(varOne)) {
      return 0;
    }
    return varOne.length();
  }
}
EOF

  guava_nullaway_pom="${guava_harness_dir}/pom.xml"
  cat > "${guava_nullaway_pom}" <<EOF
<project xmlns="http://maven.apache.org/POM/4.0.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>bench</groupId>
  <artifactId>nullaway-harness</artifactId>
  <version>1.0.0</version>

  <dependencies>
    <dependency>
      <groupId>com.google.guava</groupId>
      <artifactId>guava</artifactId>
      <version>${DATASET_VERSION_GUAVA}</version>
    </dependency>
    <dependency>
      <groupId>org.jspecify</groupId>
      <artifactId>jspecify</artifactId>
      <version>${GUAVA_VERSION_JSPECIFY}</version>
    </dependency>
  </dependencies>

  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-compiler-plugin</artifactId>
        <version>3.13.0</version>
        <configuration>
          <release>17</release>
          <compilerId>javac-with-errorprone</compilerId>
          <forceLegacyJavacApi>true</forceLegacyJavacApi>
          <showWarnings>true</showWarnings>
          <compilerArgs>
            <arg>-XepDisableAllChecks</arg>
            <arg>-Xep:NullAway:WARN</arg>
            <arg>-XepOpt:NullAway:OnlyNullMarked=true</arg>
            <arg>-XepOpt:NullAway:AnnotatedPackages=bench.nullness</arg>
          </compilerArgs>
        </configuration>
        <dependencies>
          <dependency>
            <groupId>org.codehaus.plexus</groupId>
            <artifactId>plexus-compiler-javac-errorprone</artifactId>
            <version>2.15.0</version>
          </dependency>
          <dependency>
            <groupId>com.google.errorprone</groupId>
            <artifactId>error_prone_core</artifactId>
            <version>${TOOL_VERSION_ERROR_PRONE}</version>
          </dependency>
          <dependency>
            <groupId>com.uber.nullaway</groupId>
            <artifactId>nullaway</artifactId>
            <version>${TOOL_VERSION_NULLAWAY}</version>
          </dependency>
        </dependencies>
      </plugin>
    </plugins>
  </build>
</project>
EOF

  guava_cmd_dir="${cmd_dir}/guava"
  mkdir -p "${guava_cmd_dir}"
  guava_result_dir="${result_dir}/guava"
  mkdir -p "${guava_result_dir}"

  cat > "${guava_cmd_dir}/inspequte.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
"${repo_root}/target/release/inspequte" --input @"${guava_input_list}" --output "${guava_result_dir}/inspequte.sarif" --rules NULLNESS >/dev/null
EOF

  cat > "${guava_cmd_dir}/spotbugs.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
mapfile -t jars < "${guava_input_list}"
java -jar "${spotbugs_home}/lib/spotbugs.jar" \
  -textui -quiet -effort:min -low \
  -include "${spotbugs_filter}" \
  -xml:withMessages -output "${guava_result_dir}/spotbugs.xml" \
  "\${jars[@]}" >/dev/null
EOF

  cat > "${guava_cmd_dir}/pmd.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
"${pmd_home}/bin/pmd" check \
  --no-cache \
  --no-fail-on-violation \
  --no-fail-on-error \
  -d "${guava_sources_root}" \
  -R "${pmd_ruleset}" \
  -f text >/dev/null
EOF

  guava_compile_cp="${guava_jar}:${jspecify_jar}:${error_prone_annotations_jar}:${j2objc_annotations_jar}"
  cat > "${guava_cmd_dir}/checker.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
rm -rf "${guava_result_dir}/checker-classes"
mkdir -p "${guava_result_dir}/checker-classes"
"${checker_home}/checker/bin/javac" \
  -classpath "${guava_compile_cp}" \
  -processor org.checkerframework.checker.nullness.NullnessChecker \
  -Awarns \
  -d "${guava_result_dir}/checker-classes" \
  "${guava_harness_src}" >/dev/null
EOF

  cat > "${guava_cmd_dir}/nullaway.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
rm -rf "${guava_harness_dir}/target"
mvn -q -f "${guava_nullaway_pom}" \
  -Dmaven.repo.local="${cache_dir}/m2" \
  -Dmaven.compiler.failOnError=false \
  -Dstyle.color=never \
  -DskipTests \
  clean compile >/dev/null
EOF

  chmod +x "${guava_cmd_dir}"/*.sh

  guava_json="${output_dir}/guava.json"
  log "running guava benchmark"
  hyperfine \
    --warmup "${warmup}" \
    --min-runs "${min_runs}" \
    --export-json "${guava_json}" \
    --command-name "inspequte" "${guava_cmd_dir}/inspequte.sh" \
    --command-name "spotbugs" "${guava_cmd_dir}/spotbugs.sh" \
    --command-name "pmd" "${guava_cmd_dir}/pmd.sh" \
    --command-name "checker-framework" "${guava_cmd_dir}/checker.sh" \
    --command-name "nullaway" "${guava_cmd_dir}/nullaway.sh"
  validate_hyperfine_result "${guava_json}" "guava"
fi

if [[ "${run_sonarqube}" == "true" ]]; then
  log "preparing sonarqube dataset"
  sonarqube_dir="${work_dir}/dataset-sonarqube"
  mkdir -p "${sonarqube_dir}"

  sonar_bin_zip="${cache_dir}/sonar-application-${DATASET_VERSION_SONARQUBE}.zip"
  sonar_bin_url="https://repo1.maven.org/maven2/org/sonarsource/sonarqube/sonar-application/${DATASET_VERSION_SONARQUBE}/sonar-application-${DATASET_VERSION_SONARQUBE}.zip"
  download_with_retry "${sonar_bin_url}" "${sonar_bin_zip}"
  sonar_bin_root="${sonarqube_dir}/binary"
  if [[ ! -d "${sonar_bin_root}" ]]; then
    mkdir -p "${sonar_bin_root}"
    unzip -q "${sonar_bin_zip}" -d "${sonar_bin_root}"
  fi

  sonar_src_zip="${cache_dir}/sonarqube-${DATASET_VERSION_SONARQUBE}-sources.zip"
  sonar_src_url="https://github.com/SonarSource/sonarqube/archive/refs/tags/${DATASET_VERSION_SONARQUBE}.zip"
  download_with_retry "${sonar_src_url}" "${sonar_src_zip}"
  sonar_src_root="${sonarqube_dir}/source"
  if [[ ! -d "${sonar_src_root}" ]]; then
    mkdir -p "${sonar_src_root}"
    unzip -q "${sonar_src_zip}" -d "${sonar_src_root}"
  fi

  sonar_src_extract_dir="$(find "${sonar_src_root}" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  [[ -n "${sonar_src_extract_dir}" ]] || fail "failed to locate extracted sonarqube source directory"

  sonar_bin_extract_dir="$(find "${sonar_bin_root}" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  [[ -n "${sonar_bin_extract_dir}" ]] || fail "failed to locate extracted sonar application directory"

  sonar_input_list="${sonarqube_dir}/inputs.txt"
  {
    find "${sonar_bin_extract_dir}/lib" -maxdepth 1 -type f -name "sonar-application-*.jar"
    find "${sonar_bin_extract_dir}/lib" -maxdepth 1 -type f -name "sonar-shutdowner-*.jar"
  } | LC_ALL=C sort -u > "${sonar_input_list}"
  [[ -s "${sonar_input_list}" ]] || fail "sonarqube input list is empty: ${sonar_input_list}"

  sonarqube_cmd_dir="${cmd_dir}/sonarqube"
  mkdir -p "${sonarqube_cmd_dir}"
  sonarqube_result_dir="${result_dir}/sonarqube"
  mkdir -p "${sonarqube_result_dir}"

  cat > "${sonarqube_cmd_dir}/inspequte.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
"${repo_root}/target/release/inspequte" --input @"${sonar_input_list}" --output "${sonarqube_result_dir}/inspequte.sarif" --rules NULLNESS --allow-duplicate-classes >/dev/null
EOF

  cat > "${sonarqube_cmd_dir}/spotbugs.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
mapfile -t jars < "${sonar_input_list}"
java -jar "${spotbugs_home}/lib/spotbugs.jar" \
  -textui -quiet -effort:min -low \
  -include "${spotbugs_filter}" \
  -xml:withMessages -output "${sonarqube_result_dir}/spotbugs.xml" \
  "\${jars[@]}" >/dev/null
EOF

  cat > "${sonarqube_cmd_dir}/pmd.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
"${pmd_home}/bin/pmd" check \
  --no-cache \
  --no-fail-on-violation \
  --no-fail-on-error \
  -d "${sonar_src_extract_dir}" \
  -R "${pmd_ruleset}" \
  -f text >/dev/null
EOF

  chmod +x "${sonarqube_cmd_dir}"/*.sh

  sonarqube_json="${output_dir}/sonarqube.json"
  log "running sonarqube benchmark"
  hyperfine \
    --warmup "${warmup}" \
    --min-runs "${min_runs}" \
    --export-json "${sonarqube_json}" \
    --command-name "inspequte" "${sonarqube_cmd_dir}/inspequte.sh" \
    --command-name "spotbugs" "${sonarqube_cmd_dir}/spotbugs.sh" \
    --command-name "pmd" "${sonarqube_cmd_dir}/pmd.sh"
  validate_hyperfine_result "${sonarqube_json}" "sonarqube"
fi

log "writing benchmark metadata"
java_version_line="$(java -version 2>&1 | head -n 1)"
cpu_model="$(uname -m)"
if command -v lscpu >/dev/null 2>&1; then
  cpu_model="$(lscpu | awk -F: '/Model name/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
fi

jq -n \
  --arg generated_at_utc "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
  --arg dataset "${dataset}" \
  --arg os "$(uname -s)" \
  --arg kernel "$(uname -r)" \
  --arg cpu "${cpu_model}" \
  --arg java "${java_version_line}" \
  --argjson warmup "${warmup}" \
  --argjson min_runs "${min_runs}" \
  --arg guava_version "${DATASET_VERSION_GUAVA}" \
  --arg sonarqube_version "${DATASET_VERSION_SONARQUBE}" \
  --arg spotbugs_version "${TOOL_VERSION_SPOTBUGS}" \
  --arg pmd_version "${TOOL_VERSION_PMD}" \
  --arg checker_version "${TOOL_VERSION_CHECKER_FRAMEWORK}" \
  --arg nullaway_version "${TOOL_VERSION_NULLAWAY}" \
  --arg error_prone_version "${TOOL_VERSION_ERROR_PRONE}" \
  '{
    generated_at_utc: $generated_at_utc,
    dataset: $dataset,
    benchmark: {
      warmup: $warmup,
      min_runs: $min_runs
    },
    environment: {
      os: $os,
      kernel: $kernel,
      cpu: $cpu,
      java: $java
    },
    datasets: {
      guava: {
        version: $guava_version,
        binary_source: "maven-central",
        source_source: "maven-central"
      },
      sonarqube: {
        version: $sonarqube_version,
        binary_source: "maven-central",
        source_source: "github-tags"
      }
    },
    tools: {
      spotbugs: $spotbugs_version,
      pmd: $pmd_version,
      checker_framework: $checker_version,
      nullaway: $nullaway_version,
      error_prone: $error_prone_version
    }
  }' > "${output_dir}/meta.json"

log "benchmark json generated in ${output_dir}"
