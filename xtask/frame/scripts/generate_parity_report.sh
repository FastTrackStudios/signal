#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/generate_parity_report.sh <fts_export_json> \
#     [--visual-json path] \
#     [--perf-json path] \
#     [--render-diag-json path] \
#     [--import-diag-json path] \
#     [--out-json path] \
#     [--out-md path]

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

EXPORT_JSON="${1:-}"
VISUAL_JSON=""
PERF_JSON=""
RENDER_DIAG_JSON=""
IMPORT_DIAG_JSON=""
OUT_JSON=""
OUT_MD=""

shift 1 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --visual-json)
      VISUAL_JSON="${2:-}"
      shift 2
      ;;
    --perf-json)
      PERF_JSON="${2:-}"
      shift 2
      ;;
    --render-diag-json)
      RENDER_DIAG_JSON="${2:-}"
      shift 2
      ;;
    --import-diag-json)
      IMPORT_DIAG_JSON="${2:-}"
      shift 2
      ;;
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

if [[ -z "$EXPORT_JSON" ]]; then
  echo "usage: $0 <fts_export_json> [--visual-json path] [--perf-json path] [--render-diag-json path] [--import-diag-json path] [--out-json path] [--out-md path]" >&2
  exit 2
fi
if [[ ! -f "$EXPORT_JSON" ]]; then
  echo "missing export json: $EXPORT_JSON" >&2
  exit 2
fi

FEATURE_JSON="$("$SCRIPT_DIR/feature_coverage_report.sh" "$EXPORT_JSON")"

if [[ -n "$VISUAL_JSON" && ! -f "$VISUAL_JSON" ]]; then
  echo "missing visual json: $VISUAL_JSON" >&2
  exit 2
fi
if [[ -n "$PERF_JSON" && ! -f "$PERF_JSON" ]]; then
  echo "missing perf json: $PERF_JSON" >&2
  exit 2
fi
if [[ -n "$RENDER_DIAG_JSON" && ! -f "$RENDER_DIAG_JSON" ]]; then
  echo "missing render diag json: $RENDER_DIAG_JSON" >&2
  exit 2
fi
if [[ -n "$IMPORT_DIAG_JSON" && ! -f "$IMPORT_DIAG_JSON" ]]; then
  echo "missing import diag json: $IMPORT_DIAG_JSON" >&2
  exit 2
fi

VISUAL_PAYLOAD="null"
if [[ -n "$VISUAL_JSON" ]]; then
  VISUAL_PAYLOAD="$(cat "$VISUAL_JSON")"
fi
PERF_PAYLOAD="null"
if [[ -n "$PERF_JSON" ]]; then
  PERF_PAYLOAD="$(cat "$PERF_JSON")"
fi
RENDER_DIAG_PAYLOAD="null"
if [[ -n "$RENDER_DIAG_JSON" ]]; then
  RENDER_DIAG_PAYLOAD="$(cat "$RENDER_DIAG_JSON")"
fi
IMPORT_DIAG_PAYLOAD="null"
if [[ -n "$IMPORT_DIAG_JSON" ]]; then
  IMPORT_DIAG_PAYLOAD="$(cat "$IMPORT_DIAG_JSON")"
fi

REPORT="$(jq -n \
  --arg generatedAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg exportPath "$EXPORT_JSON" \
  --argjson feature "$FEATURE_JSON" \
  --argjson visual "$VISUAL_PAYLOAD" \
  --argjson perf "$PERF_PAYLOAD" \
  --argjson renderDiag "$RENDER_DIAG_PAYLOAD" \
  --argjson importDiag "$IMPORT_DIAG_PAYLOAD" \
  '
  {
    generatedAt: $generatedAt,
    exportPath: $exportPath,
    featureCoverage: $feature,
    visualAcceptance: $visual,
    performance: $perf,
    renderDiagnostics: $renderDiag,
    importDiagnostics: $importDiag
  }
  | .gates = {
      visualPass: (
        if .visualAcceptance == null then null
        else ((.visualAcceptance.failed // 0) == 0 and (.visualAcceptance.missingActual // 0) == 0)
        end
      ),
      hasLuminanceMasks: ((.featureCoverage.features.luminanceMask // 0) > 0),
      hasLayerBlur: ((.featureCoverage.features.layerBlur // 0) > 0),
      hasBackgroundBlur: ((.featureCoverage.features.backgroundBlur // 0) > 0),
      renderHasLayerBlurApprox: ((.renderDiagnostics.diagnostics.layerBlurApproxNodes // 0) > 0),
      renderHasBackgroundBlurApprox: ((.renderDiagnostics.diagnostics.backgroundBlurApproxNodes // 0) > 0),
      renderHasLuminanceMaskApprox: ((.renderDiagnostics.diagnostics.luminanceMaskNodes // 0) > 0),
      importHasUnsupportedKeys: ((.importDiagnostics.unsupportedNodeKeyCount // 0) > 0)
    }
  | .gates.parityPass = (
      (.gates.visualPass // true)
      and ((.gates.renderHasLayerBlurApprox // false) | not)
      and ((.gates.renderHasBackgroundBlurApprox // false) | not)
      and ((.gates.renderHasLuminanceMaskApprox // false) | not)
      and ((.gates.importHasUnsupportedKeys // false) | not)
    )
  | .gates.blockers = [
      (if (.gates.visualPass // true) then empty else "visual_diff_failed" end),
      (if (.gates.renderHasLayerBlurApprox // false) then "layer_blur_approximation" else empty end),
      (if (.gates.renderHasBackgroundBlurApprox // false) then "background_blur_approximation" else empty end),
      (if (.gates.renderHasLuminanceMaskApprox // false) then "luminance_mask_approximation" else empty end),
      (if (.gates.importHasUnsupportedKeys // false) then "import_unsupported_keys" else empty end)
    ]
  ')"

echo "$REPORT"

if [[ -n "$OUT_JSON" ]]; then
  mkdir -p "$(dirname "$OUT_JSON")"
  printf '%s\n' "$REPORT" > "$OUT_JSON"
fi

if [[ -n "$OUT_MD" ]]; then
  mkdir -p "$(dirname "$OUT_MD")"
  {
    echo "# Figma Parity Report"
    echo
    echo "- Generated: \`$(jq -r '.generatedAt' <<<"$REPORT")\`"
    echo "- Export: \`$(jq -r '.exportPath' <<<"$REPORT")\`"
    echo "- Nodes: \`$(jq -r '.featureCoverage.nodeCount' <<<"$REPORT")\`"
    echo
    echo "## Feature Coverage"
    jq -r '
      .featureCoverage.features
      | to_entries
      | map("- `\(.key)`: `\(.value)`")
      | .[]
    ' <<<"$REPORT"
    echo
    echo "## Gates"
    jq -r '
      .gates
      | to_entries
      | map("- `\(.key)`: `\(.value)`")
      | .[]
    ' <<<"$REPORT"
    echo "- blockers: \`$(jq -r '(.gates.blockers // []) | join(",")' <<<"$REPORT")\`"
    if jq -e '.visualAcceptance != null' <<<"$REPORT" >/dev/null; then
      echo
      echo "## Visual Acceptance"
      echo "- Total: \`$(jq -r '.visualAcceptance.total // 0' <<<"$REPORT")\`"
      echo "- Passed: \`$(jq -r '.visualAcceptance.passed // 0' <<<"$REPORT")\`"
      echo "- Failed: \`$(jq -r '.visualAcceptance.failed // 0' <<<"$REPORT")\`"
      echo "- Missing actual: \`$(jq -r '.visualAcceptance.missingActual // 0' <<<"$REPORT")\`"
    fi
    if jq -e '.renderDiagnostics != null' <<<"$REPORT" >/dev/null; then
      echo
      echo "## Render Diagnostics"
      echo "- Summary: \`$(jq -r '.renderDiagnostics.summary // "n/a"' <<<"$REPORT")\`"
    fi
    if jq -e '.importDiagnostics != null' <<<"$REPORT" >/dev/null; then
      echo
      echo "## Import Diagnostics"
      echo "- Unsupported key count: \`$(jq -r '.importDiagnostics.unsupportedNodeKeyCount // 0' <<<"$REPORT")\`"
      echo "- Top unsupported keys:"
      jq -r '
        (.importDiagnostics.topUnsupportedKeys // [])
        | if length == 0 then "- none" else .[] | "- `\(.key)`: `\(.count)`" end
      ' <<<"$REPORT"
    fi
  } > "$OUT_MD"
fi
