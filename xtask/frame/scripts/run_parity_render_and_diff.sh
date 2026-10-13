#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/run_parity_render_and_diff.sh [--root crates/frame/render/tests/parity] [--all] [--core10] [--scenario <id>] [--threshold <percent>] [--require-pass] [--require-svg-pass]

ROOT="crates/frame/render/tests/parity"
SCENARIO_FILTER=""
RUN_ALL="false"
THRESHOLD="0.0"
REQUIRE_PASS="false"
REQUIRE_SVG_PASS="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT="${2:-}"
      shift 2
      ;;
    --scenario)
      SCENARIO_FILTER="${2:-}"
      shift 2
      ;;
    --all)
      RUN_ALL="true"
      shift 1
      ;;
    --core10)
      RUN_ALL="false"
      shift 1
      ;;
    --threshold)
      THRESHOLD="${2:-}"
      shift 2
      ;;
    --require-pass)
      REQUIRE_PASS="true"
      shift 1
      ;;
    --require-svg-pass)
      REQUIRE_SVG_PASS="true"
      shift 1
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

RENDER_ARGS=(--root "$ROOT")
SUMMARY_ARGS=(--root "$ROOT" --threshold "$THRESHOLD")
if [[ -n "$SCENARIO_FILTER" ]]; then
  RENDER_ARGS+=(--scenario "$SCENARIO_FILTER")
  SUMMARY_ARGS+=(--scenario "$SCENARIO_FILTER")
elif [[ "$RUN_ALL" == "true" ]]; then
  RENDER_ARGS+=(--all)
  SUMMARY_ARGS+=(--all)
else
  RENDER_ARGS+=(--core10)
  SUMMARY_ARGS+=(--core10)
fi
if [[ "$REQUIRE_PASS" == "true" ]]; then
  SUMMARY_ARGS+=(--require-pass)
fi

xtask/frame/scripts/render_parity_actuals.sh "${RENDER_ARGS[@]}"
xtask/frame/scripts/run_parity_scenarios.sh "${SUMMARY_ARGS[@]}"
if [[ "$REQUIRE_SVG_PASS" == "true" ]]; then
  xtask/frame/scripts/run_svg_parity_scenarios.sh "${RENDER_ARGS[@]}" --require-pass
else
  xtask/frame/scripts/run_svg_parity_scenarios.sh "${RENDER_ARGS[@]}"
fi
