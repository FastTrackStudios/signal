#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/feature_coverage_report.sh <fts_export_json> [--out-json path]
#
# Produces aggregate feature counts used by parity tracking.

INPUT="${1:-}"
OUT_JSON=""

shift 1 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-json)
      OUT_JSON="${2:-}"
      shift 2
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$INPUT" ]]; then
  echo "usage: $0 <fts_export_json> [--out-json path]" >&2
  exit 2
fi
if [[ ! -f "$INPUT" ]]; then
  echo "missing input: $INPUT" >&2
  exit 2
fi

SUMMARY="$(jq '
  def nodes:
    if (.nodes | type) == "array" then
      [ .nodes[] | .. | objects | select(has("id") and has("type")) ]
    else
      ((.document // .figma.document // empty)
      | if . == empty then [] else [.. | objects | select(has("id") and has("type"))] end)
    end;

  def visible_effects($n):
    (($n.effects // [])
      | map(select((.visible // true) == true) | .type));

  def visible_fills($n):
    (($n.fills // [])
      | map(select((.visible // true) == true) | .type));

  def is_non_normal_blend($n):
    ((($n.blendMode // "NORMAL") as $b | ($b != "NORMAL" and $b != "PASS_THROUGH")));

  (nodes) as $nodes
  | {
      schema: (.schema // "unknown"),
      nodeCount: ($nodes | length),
      nodeTypeCounts: ($nodes | map(.type) | group_by(.) | map({key: .[0], value: length}) | from_entries),
      features: {
        hasMask: ($nodes | map(select(.isMask == true)) | length),
        alphaMask: ($nodes | map(select(.isMask == true and ((.maskType // "") == "ALPHA"))) | length),
        luminanceMask: ($nodes | map(select(.isMask == true and ((.maskType // "") == "LUMINANCE"))) | length),
        hasClip: ($nodes | map(select(.clipsContent == true)) | length),
        nonNormalBlend: ($nodes | map(select(is_non_normal_blend(.))) | length),
        gradientFill: ($nodes | map(select((visible_fills(.) | any(. == "GRADIENT_LINEAR" or . == "GRADIENT_RADIAL" or . == "GRADIENT_ANGULAR" or . == "GRADIENT_DIAMOND")))) | length),
        imageFill: ($nodes | map(select((visible_fills(.) | any(. == "IMAGE")))) | length),
        dropShadow: ($nodes | map(select((visible_effects(.) | any(. == "DROP_SHADOW")))) | length),
        innerShadow: ($nodes | map(select((visible_effects(.) | any(. == "INNER_SHADOW")))) | length),
        layerBlur: ($nodes | map(select((visible_effects(.) | any(. == "LAYER_BLUR")))) | length),
        backgroundBlur: ($nodes | map(select((visible_effects(.) | any(. == "BACKGROUND_BLUR")))) | length),
        svgExported: ($nodes | map(select(((.exports // {}) | has("svgBase64")))) | length)
      }
    }
' "$INPUT")"

echo "$SUMMARY"

if [[ -n "$OUT_JSON" ]]; then
  mkdir -p "$(dirname "$OUT_JSON")"
  printf '%s\n' "$SUMMARY" > "$OUT_JSON"
fi
