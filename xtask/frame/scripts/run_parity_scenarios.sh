#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/run_parity_scenarios.sh \
#     [--root crates/frame/render/tests/parity] \
#     [--core10] \
#     [--all] \
#     [--scenario <scenario-id>] \
#     [--threshold <percent>] \
#     [--require-pass]

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="crates/frame/render/tests/parity"
SCENARIO_FILTER=""
THRESHOLD="0.0"
REQUIRE_PASS="false"
RUN_ALL="false"

CORE10_SCENARIOS=(
  "primitives.fractional-geometry"
  "paint.gradient-matrix"
  "paint.blend-mode-stack"
  "effects.layer-background-blur"
  "strokes.caps-joins-dashes"
  "masks.vector-text-image-mask"
  "images.fill-mode-matrix"
  "text.mixed-style-runs"
  "layout.auto-layout-hug-fill"
  "regression.kitchen-sink"
)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT="${2:-}"
      shift 2
      ;;
    --core10)
      RUN_ALL="false"
      shift 1
      ;;
    --all)
      RUN_ALL="true"
      shift 1
      ;;
    --scenario)
      SCENARIO_FILTER="${2:-}"
      shift 2
      ;;
    --threshold)
      THRESHOLD="${2:-}"
      shift 2
      ;;
    --require-pass)
      REQUIRE_PASS="true"
      shift 1
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

SCENARIOS_DIR="$ROOT/scenarios"
REPORTS_DIR="$ROOT/reports"
mkdir -p "$REPORTS_DIR"

if [[ ! -d "$SCENARIOS_DIR" ]]; then
  echo "missing scenarios dir: $SCENARIOS_DIR" >&2
  exit 2
fi

mapfile -t SCENARIO_DIRS < <(find "$SCENARIOS_DIR" -mindepth 1 -maxdepth 1 -type d | LC_ALL=C sort)

if [[ -n "$SCENARIO_FILTER" ]]; then
  SCENARIO_DIRS=("$SCENARIOS_DIR/$SCENARIO_FILTER")
elif [[ "$RUN_ALL" != "true" ]]; then
  SCENARIO_DIRS=()
  for scenario_id in "${CORE10_SCENARIOS[@]}"; do
    SCENARIO_DIRS+=("$SCENARIOS_DIR/$scenario_id")
  done
fi

if [[ ${#SCENARIO_DIRS[@]} -eq 0 ]]; then
  echo "no scenarios found in $SCENARIOS_DIR" >&2
  exit 2
fi

SUMMARY_TMP="$(mktemp)"
trap 'rm -f "$SUMMARY_TMP"' EXIT
printf '[]' > "$SUMMARY_TMP"

TOTAL=0
PASSED=0
FAILED=0
SKIPPED=0

for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  export_json="$scenario_dir/latest-bridge-export.json"
  if [[ ! -f "$export_json" ]]; then
    export_json="$scenario_dir/export/latest-bridge-export.json"
  fi
  actual_png="$scenario_dir/actual.png"
  expected_png="$scenario_dir/expected.png"
  if [[ ! -f "$actual_png" && -f "$scenario_dir/actual/frame.png" ]]; then
    actual_png="$scenario_dir/actual/frame.png"
  fi
  if [[ ! -f "$expected_png" && -f "$scenario_dir/expected/frame.png" ]]; then
    expected_png="$scenario_dir/expected/frame.png"
  fi
  out_dir="$scenario_dir"

  TOTAL=$((TOTAL + 1))

  if [[ ! -f "$export_json" ]]; then
    echo "[$scenario_id] skip: missing $export_json"
    SKIPPED=$((SKIPPED + 1))
    jq --arg id "$scenario_id" --arg reason "missing_export_json" \
      '. += [{scenarioId:$id, status:"skipped", reason:$reason}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
    mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"
    continue
  fi

  mkdir -p "$out_dir"
  echo "[$scenario_id] running parity pipeline"

  pipeline_args=("$export_json" "--out-dir" "$out_dir" "--threshold" "$THRESHOLD")
  if [[ -f "$actual_png" && -f "$expected_png" ]]; then
    pipeline_args+=("--actual-png-dir" "$actual_png" "--expected-png-dir" "$expected_png")
  else
    echo "[$scenario_id] note: missing actual.png/expected.png pair, running non-visual diagnostics only"
  fi

  if "$SCRIPT_DIR/run_parity_pipeline.sh" "${pipeline_args[@]}"; then
    if jq -e '.gates.parityPass == true' "$out_dir/report.json" >/dev/null 2>&1; then
      PASSED=$((PASSED + 1))
      status="pass"
    else
      FAILED=$((FAILED + 1))
      status="fail"
    fi
  else
    FAILED=$((FAILED + 1))
    status="fail"
  fi

  jq --arg id "$scenario_id" \
    --arg status "$status" \
    --arg report "$out_dir/report.json" \
    '. += [{scenarioId:$id, status:$status, report:$report}]' "$SUMMARY_TMP" > "$SUMMARY_TMP.tmp"
  mv "$SUMMARY_TMP.tmp" "$SUMMARY_TMP"
done

SUMMARY_JSON="$REPORTS_DIR/summary.json"
SUMMARY_MD="$REPORTS_DIR/summary.md"

jq -n \
  --arg generatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg root "$ROOT" \
  --arg mode "$(if [[ -n "$SCENARIO_FILTER" ]]; then echo "single"; elif [[ "$RUN_ALL" == "true" ]]; then echo "all"; else echo "core10"; fi)" \
  --arg threshold "$THRESHOLD" \
  --argjson total "$TOTAL" \
  --argjson passed "$PASSED" \
  --argjson failed "$FAILED" \
  --argjson skipped "$SKIPPED" \
  --argjson scenarios "$(cat "$SUMMARY_TMP")" \
  '{generatedAt:$generatedAt, root:$root, mode:$mode, thresholdPercent:($threshold|tonumber), total:$total, passed:$passed, failed:$failed, skipped:$skipped, scenarios:$scenarios}' > "$SUMMARY_JSON"

{
  echo "# Parity Scenario Summary"
  echo
  echo "- Generated: \`$(jq -r '.generatedAt' "$SUMMARY_JSON")\`"
  echo "- Root: \`$(jq -r '.root' "$SUMMARY_JSON")\`"
  echo "- Mode: \`$(jq -r '.mode' "$SUMMARY_JSON")\`"
  echo "- Threshold: \`$(jq -r '.thresholdPercent' "$SUMMARY_JSON")\`%"
  echo "- Total: \`$(jq -r '.total' "$SUMMARY_JSON")\`"
  echo "- Passed: \`$(jq -r '.passed' "$SUMMARY_JSON")\`"
  echo "- Failed: \`$(jq -r '.failed' "$SUMMARY_JSON")\`"
  echo "- Skipped: \`$(jq -r '.skipped' "$SUMMARY_JSON")\`"
  echo
  echo "## Scenarios"
  jq -r '.scenarios[] | "- `\(.scenarioId)`: `\(.status)`"' "$SUMMARY_JSON"
} > "$SUMMARY_MD"

echo "wrote:"
echo "  $SUMMARY_JSON"
echo "  $SUMMARY_MD"

if [[ "$REQUIRE_PASS" == "true" ]]; then
  if jq -e '.failed == 0 and .skipped == 0' "$SUMMARY_JSON" >/dev/null; then
    echo "all scenarios passed"
  else
    echo "one or more scenarios failed/skipped (failing due to --require-pass)" >&2
    exit 1
  fi
fi
