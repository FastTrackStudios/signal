#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/scripts/run_visual_acceptance.sh <actual> <expected> [threshold_percent] [--out-json path] [--out-md path]
#
# Supports either:
# - file-to-file compare (PNG path + PNG path), or
# - directory compare (matching relative *.png paths)

ACTUAL_INPUT="${1:-}"
EXPECTED_INPUT="${2:-}"
THRESHOLD="${3:-2.0}"
OUT_JSON=""
OUT_MD=""

shift 3 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-json)
      OUT_JSON="${2:-}"
      shift 2
      ;;
    --out-md)
      OUT_MD="${2:-}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$ACTUAL_INPUT" || -z "$EXPECTED_INPUT" ]]; then
  echo "usage: $0 <actual> <expected> [threshold_percent] [--out-json path] [--out-md path]" >&2
  exit 2
fi

TMP_REPORT="$(mktemp)"
trap 'rm -f "$TMP_REPORT"' EXIT
printf '[]' > "$TMP_REPORT"

TOTAL=0
PASSED=0
FAILED=0
MISSING=0

run_compare() {
  local rel="$1"
  local actual="$2"
  local expected="$3"
  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$actual" ]]; then
    MISSING=$((MISSING + 1))
    jq --arg rel "$rel" '. += [{path:$rel, status:"missing_actual"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
    mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    return
  fi

  if output=$(cargo run -q -p frame-cli --features anyrender --bin visual_diff -- "$actual" "$expected" "$THRESHOLD" 2>&1); then
    PASSED=$((PASSED + 1))
    jq --argjson item "$output" '. += [$item + {status:"pass"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
    mv "$TMP_REPORT.tmp" "$TMP_REPORT"
  else
    FAILED=$((FAILED + 1))
    if echo "$output" | tail -n 1 | jq -e . >/dev/null 2>&1; then
      json_line="$(echo "$output" | tail -n 1)"
      jq --argjson item "$json_line" '. += [$item + {status:"fail"}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
      mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    else
      jq --arg rel "$rel" --arg err "$output" '. += [{path:$rel, status:"fail", error:$err}]' "$TMP_REPORT" > "$TMP_REPORT.tmp"
      mv "$TMP_REPORT.tmp" "$TMP_REPORT"
    fi
  fi
}

if [[ -f "$ACTUAL_INPUT" || -f "$EXPECTED_INPUT" ]]; then
  if [[ ! -f "$ACTUAL_INPUT" ]]; then
    echo "missing actual file: $ACTUAL_INPUT" >&2
    exit 2
  fi
  if [[ ! -f "$EXPECTED_INPUT" ]]; then
    echo "missing expected file: $EXPECTED_INPUT" >&2
    exit 2
  fi
  run_compare "$(basename "$EXPECTED_INPUT")" "$ACTUAL_INPUT" "$EXPECTED_INPUT"
else
  if [[ ! -d "$ACTUAL_INPUT" ]]; then
    echo "missing actual dir: $ACTUAL_INPUT" >&2
    exit 2
  fi
  if [[ ! -d "$EXPECTED_INPUT" ]]; then
    echo "missing expected dir: $EXPECTED_INPUT" >&2
    exit 2
  fi

  while IFS= read -r -d '' expected; do
    rel="${expected#${EXPECTED_INPUT}/}"
    actual="${ACTUAL_INPUT}/${rel}"
    run_compare "$rel" "$actual" "$expected"
  done < <(find "$EXPECTED_INPUT" -type f -name '*.png' -print0 | sort -z)
fi

SUMMARY=$(jq -n \
  --arg actualDir "$ACTUAL_INPUT" \
  --arg expectedDir "$EXPECTED_INPUT" \
  --argjson threshold "$THRESHOLD" \
  --argjson total "$TOTAL" \
  --argjson passed "$PASSED" \
  --argjson failed "$FAILED" \
  --argjson missing "$MISSING" \
  --argjson results "$(cat "$TMP_REPORT")" \
  '{actualDir:$actualDir, expectedDir:$expectedDir, thresholdPercent:$threshold, total:$total, passed:$passed, failed:$failed, missingActual:$missing, results:$results}')

echo "$SUMMARY"

if [[ -n "$OUT_JSON" ]]; then
  mkdir -p "$(dirname "$OUT_JSON")"
  printf '%s\n' "$SUMMARY" > "$OUT_JSON"
fi

if [[ -n "$OUT_MD" ]]; then
  mkdir -p "$(dirname "$OUT_MD")"
  {
    echo "# Frame Visual Acceptance Report"
    echo
    echo "- Actual: \`$ACTUAL_INPUT\`"
    echo "- Expected: \`$EXPECTED_INPUT\`"
    echo "- Threshold: \`$THRESHOLD\`%"
    echo "- Total: \`$TOTAL\`"
    echo "- Passed: \`$PASSED\`"
    echo "- Failed: \`$FAILED\`"
    echo "- Missing actual: \`$MISSING\`"
    echo
    echo "## Failures"
    jq -r '
      [.results[]
        | select(.status == "fail" or .status == "missing_actual")
        | "- `\(.path // (.actual + \" vs \" + .expected))`: \(.status)\(if .diffPercent then \", diff=\(.diffPercent)%\" else \"\" end)\(if .error then \", error=\(.error | gsub(\"\\n\"; \" \"))\" else \"\" end)"
      ] | if length == 0 then "- none" else .[] end
    ' <<<"$SUMMARY"
  } > "$OUT_MD"
fi

if [[ "$FAILED" -gt 0 || "$MISSING" -gt 0 ]]; then
  exit 1
fi
