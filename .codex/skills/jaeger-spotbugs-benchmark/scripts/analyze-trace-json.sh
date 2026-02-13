#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <trace-json-path>" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

input_json="$1"
if [ ! -f "${input_json}" ]; then
  echo "trace json not found: ${input_json}" >&2
  exit 1
fi

summary_json="${input_json%.json}.summary.json"

jq '
  def to_str:
    if type == "string" then .
    elif type == "number" then tostring
    elif type == "boolean" then tostring
    elif . == null then ""
    else tostring
    end;

  def attr_value($v):
    if ($v | type) == "object" then
      ($v.stringValue // $v.intValue // $v.doubleValue // $v.boolValue // "") | to_str
    else
      $v | to_str
    end;

  def attrs_to_map($attrs):
    reduce ($attrs // [])[] as $a ({}; .[$a.key] = attr_value($a.value));

  def tags_to_map($tags):
    reduce ($tags // [])[] as $t ({}; .[$t.key] = (($t.value // "") | to_str));

  def normalize_rows:
    if (has("data") and (.data | type) == "array") then
      [ .data[]?.spans[]? | {
          name: (.operationName // .name // ""),
          duration_ms: (((.duration // 0) | tonumber) / 1000),
          attrs: tags_to_map(.tags)
        }
      ]
    else
      [ .resourceSpans[]?.scopeSpans[]?.spans[]? | {
          name: (.name // ""),
          duration_ms: ((((.endTimeUnixNano | tonumber) - (.startTimeUnixNano | tonumber))) / 1000000),
          attrs: attrs_to_map(.attributes)
        }
      ]
    end
    | map(
        . + {
          rule: (
            .attrs["inspequte.rule_id"]
            // (if (.name | startswith("rule:")) then (.name | sub("^rule:"; "")) else "" end)
          ),
          jar: (
            (
              if ((.attrs["inspequte.jar_path"] // "") != "") then
                (.attrs["inspequte.jar_path"] | split("/") | last)
              else
                empty
              end
            )
            // (
              if ((.attrs["inspequte.artifact_uri"] // "") | test("\\.jar!")) then
                ((.attrs["inspequte.artifact_uri"] | capture(".*\\/(?<jar>[^/!]+\\.jar)!").jar) // "")
              else
                ""
              end
            )
          ),
          class: (
            .attrs["inspequte.jar_entry"]
            // .attrs["inspequte.class"]
            // (
              if ((.attrs["inspequte.artifact_uri"] // "") | test("!/.+\\.class")) then
                ((.attrs["inspequte.artifact_uri"] | capture("!/(?<cls>.+)\\.class").cls) // "")
              else
                ""
              end
            )
          )
        }
      );

  def aggregate($rows; $field):
    ($rows | map(select(.[$field] != ""))) as $subset
    | if ($subset | length) == 0 then
        {id: "", total_duration_ms: 0, max_span_ms: 0, span_count: 0}
      else
        ($subset
          | sort_by(.[$field])
          | group_by(.[$field])
          | map({
              id: .[0][$field],
              total_duration_ms: (map(.duration_ms) | add),
              max_span_ms: (map(.duration_ms) | max),
              span_count: length
            })
          | max_by(.total_duration_ms))
      end;

  def round3: ((. * 1000) | round) / 1000;

  (normalize_rows) as $rows
  | {
      span_count: ($rows | length),
      top_span: (
        if ($rows | length) == 0 then
          {name: "", duration_ms: 0, rule: "", jar: "", class: ""}
        else
          ($rows | max_by(.duration_ms))
        end
      ),
      slowest_rule: aggregate($rows; "rule"),
      slowest_jar: aggregate($rows; "jar"),
      slowest_class: aggregate($rows; "class")
    }
  | .top_span.duration_ms |= round3
  | .slowest_rule.total_duration_ms |= round3
  | .slowest_rule.max_span_ms |= round3
  | .slowest_jar.total_duration_ms |= round3
  | .slowest_jar.max_span_ms |= round3
  | .slowest_class.total_duration_ms |= round3
  | .slowest_class.max_span_ms |= round3
' "${input_json}" > "${summary_json}"

jq -r --arg trace_json "${input_json}" --arg summary_json "${summary_json}" '
  "trace_json=\($trace_json)",
  "summary_json=\($summary_json)",
  "top_span=\(.top_span.name)\tduration_ms=\(.top_span.duration_ms)",
  "slowest_rule=\(.slowest_rule.id)\ttotal_ms=\(.slowest_rule.total_duration_ms)\tspans=\(.slowest_rule.span_count)",
  "slowest_jar=\(.slowest_jar.id)\ttotal_ms=\(.slowest_jar.total_duration_ms)\tspans=\(.slowest_jar.span_count)",
  "slowest_class=\(.slowest_class.id)\ttotal_ms=\(.slowest_class.total_duration_ms)\tspans=\(.slowest_class.span_count)"
' "${summary_json}"
