#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/render_parity_actuals.sh [--root crates/frame/render/tests/parity] [--all] [--scenario <id>] [--core10]

ROOT="crates/frame/render/tests/parity"
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
    --core10)
      RUN_ALL="false"
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

for scenario_dir in "${SCENARIO_DIRS[@]}"; do
  scenario_id="$(basename "$scenario_dir")"
  export_json="$scenario_dir/latest-bridge-export.json"
  if [[ ! -f "$export_json" ]]; then
    export_json="$scenario_dir/export/latest-bridge-export.json"
  fi
  actual_png="$scenario_dir/actual.png"
  actual_svg="$scenario_dir/actual.svg"

  if [[ ! -f "$export_json" ]]; then
    echo "[$scenario_id] skip: missing $export_json"
    continue
  fi

  echo "[$scenario_id] rendering actual.png + actual.svg"
  width_int="$(jq -r '(.nodes[0].absoluteBoundingBox.width // 0) | ceil' "$export_json" 2>/dev/null || echo 0)"
  height_int="$(jq -r '(.nodes[0].absoluteBoundingBox.height // 0) | ceil' "$export_json" 2>/dev/null || echo 0)"

  render_args=(
    "$export_json"
    --out "$actual_png"
  )

  if [[ "$width_int" -gt 0 && "$height_int" -gt 0 ]]; then
    render_args+=(--width "$width_int" --height "$height_int")
  fi

  cargo run -q -p frame-cli --features anyrender --bin render_png -- "${render_args[@]}"

  svg_render_args=(
    "$export_json"
    --out "$actual_svg"
  )
  if [[ "$width_int" -gt 0 && "$height_int" -gt 0 ]]; then
    svg_render_args+=(--width "$width_int" --height "$height_int")
  fi
  cargo run -q -p frame-cli --features svg_export --bin render_svg -- "${svg_render_args[@]}"
done

echo "done rendering parity actual PNGs/SVGs"
