#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/scripts/analyze_svg_canonical_precision.sh [--root features/render/tests/parity] [--scenario <id>] [--all]

ROOT="features/render/tests/parity"
SCENARIO_FILTER=""
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
    --scenario)
      SCENARIO_FILTER="${2:-}"
      shift 2
      ;;
    --all)
      RUN_ALL="true"
      shift 1
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

SCENARIOS_DIR="$ROOT/scenarios"
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

echo "scenario,precision,equal,actual_paths,expected_paths,first_diff_offset"
for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  actual_svg="$scenario_dir/actual.svg"
  expected_svg="$scenario_dir/expected.svg"
  if [[ ! -f "$actual_svg" || ! -f "$expected_svg" ]]; then
    continue
  fi
  for precision in 2 3 4 5 6; do
    set +e
    out="$(cargo run -q -p frame-cli --bin svg_canonical_diff -- "$actual_svg" "$expected_svg" --precision "$precision" --json 2>/dev/null)"
    status=$?
    set -e
    if [[ -z "$out" ]]; then
      echo "$scenario_id,$precision,error,0,0,"
      continue
    fi
    equal="$(jq -r '.equal' <<<"$out")"
    ap="$(jq -r '.actual_paths' <<<"$out")"
    ep="$(jq -r '.expected_paths' <<<"$out")"
    diff="$(jq -r '.first_diff_offset // ""' <<<"$out")"
    if [[ "$status" -ne 0 && "$status" -ne 2 ]]; then
      equal="error"
    fi
    echo "$scenario_id,$precision,$equal,$ap,$ep,$diff"
  done
done
