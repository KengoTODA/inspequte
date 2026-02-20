#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rules_root="$repo_root/src/rules"
docs_root="$repo_root/docs/rules"

fail() {
  echo "generate-rule-docs: $*" >&2
  exit 1
}

[[ -d "$rules_root" ]] || fail "rules directory does not exist: $rules_root"

if ! mkdir -p "$docs_root"; then
  fail "failed to create output directory: $docs_root"
fi

mapfile -t rule_dirs < <(find "$rules_root" -mindepth 1 -maxdepth 1 -type d | LC_ALL=C sort)
[[ ${#rule_dirs[@]} -gt 0 ]] || fail "no rule directories found under $rules_root"

declare -A seen_rule_ids=()
declare -a rule_ids=()

for rule_dir in "${rule_dirs[@]}"; do
  rule_id="$(basename "$rule_dir")"
  rule_key="$(echo "$rule_id" | tr '[:upper:]' '[:lower:]')"
  spec_path="$rule_dir/spec.md"

  # Skip empty directories (e.g. no-go rules whose files were removed)
  if [[ -z "$(ls -A "$rule_dir")" ]]; then
    continue
  fi

  if [[ -n "${seen_rule_ids[$rule_key]:-}" ]]; then
    fail "duplicate rule ID detected: $rule_id"
  fi
  seen_rule_ids["$rule_key"]="$rule_id"

  [[ -f "$spec_path" ]] || fail "missing spec.md: $spec_path"

  rule_ids+=("$rule_id")
done

find "$docs_root" -mindepth 1 -maxdepth 1 -type f -name "*.md" ! -name "index.md" -delete

index_tmp="$(mktemp "$docs_root/.index.md.XXXXXX")"
{
  echo "# Rules"
  echo
  echo "This index is generated from \`src/rules/*/spec.md\` by \`scripts/generate-rule-docs.sh\`."
  echo
} >"$index_tmp"

for rule_id in "${rule_ids[@]}"; do
  spec_path="$rules_root/$rule_id/spec.md"
  output_path="$docs_root/$rule_id.md"

  cp "$spec_path" "$output_path"

  title="$(awk '/^#/{line=$0; sub(/^#+[[:space:]]*/, "", line); print line; exit}' "$spec_path")"
  if [[ -n "$title" ]]; then
    printf -- "- [%s](./%s.md) - %s\n" "$rule_id" "$rule_id" "$title" >>"$index_tmp"
  else
    printf -- "- [%s](./%s.md)\n" "$rule_id" "$rule_id" >>"$index_tmp"
  fi
done

chmod 0644 "$index_tmp"
mv "$index_tmp" "$docs_root/index.md"

echo "Generated docs in $docs_root (rules: ${#rule_ids[@]})."
