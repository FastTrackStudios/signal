#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/run_parity_pipeline.sh <export_json> \
#     --out-dir <dir> \
#     [--actual-png-dir <dir> --expected-png-dir <dir> --threshold <percent>] \
#     [--require-pass]

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

EXPORT_JSON="${1:-}"
OUT_DIR=""
ACTUAL_PNG_DIR=""
EXPECTED_PNG_DIR=""
THRESHOLD="2.0"
REQUIRE_PASS="false"

shift 1 || true
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir)
      OUT_DIR="${2:-}"
      shift 2
      ;;
    --actual-png-dir)
      ACTUAL_PNG_DIR="${2:-}"
      shift 2
      ;;
    --expected-png-dir)
      EXPECTED_PNG_DIR="${2:-}"
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

if [[ -z "$EXPORT_JSON" || -z "$OUT_DIR" ]]; then
  echo "usage: $0 <export_json> --out-dir <dir> [--actual-png-dir <dir> --expected-png-dir <dir> --threshold <percent>]" >&2
  exit 2
fi
if [[ ! -f "$EXPORT_JSON" ]]; then
  echo "missing export json: $EXPORT_JSON" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"

PARITY_JSON="$OUT_DIR/report.json"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
RENDER_DIAG_JSON="$TMP_DIR/render-diagnostics.json"
IMPORT_DIAG_JSON="$TMP_DIR/import-diagnostics.json"
PERF_JSON="$TMP_DIR/load-profile.json"
VISUAL_JSON="$TMP_DIR/visual-acceptance.json"

"$SCRIPT_DIR/capture_render_diagnostics.sh" "$EXPORT_JSON" "$RENDER_DIAG_JSON"
"$SCRIPT_DIR/capture_import_diagnostics.sh" "$EXPORT_JSON" "$IMPORT_DIAG_JSON"
"$SCRIPT_DIR/capture_load_profile.sh" "$EXPORT_JSON" "$PERF_JSON" 3

VISUAL_ARGS=()
if [[ -n "$ACTUAL_PNG_DIR" || -n "$EXPECTED_PNG_DIR" ]]; then
  if [[ -z "$ACTUAL_PNG_DIR" || -z "$EXPECTED_PNG_DIR" ]]; then
    echo "both --actual-png-dir and --expected-png-dir are required together" >&2
    exit 2
  fi
  if "$SCRIPT_DIR/run_visual_acceptance.sh" "$ACTUAL_PNG_DIR" "$EXPECTED_PNG_DIR" "$THRESHOLD" --out-json "$VISUAL_JSON"; then
    :
  else
    # Keep pipeline running so parity report still gets generated.
    echo "visual acceptance reported failures; continuing to parity report generation"
  fi
  VISUAL_ARGS=(--visual-json "$VISUAL_JSON")
fi

"$SCRIPT_DIR/generate_parity_report.sh" \
  "$EXPORT_JSON" \
  --perf-json "$PERF_JSON" \
  --render-diag-json "$RENDER_DIAG_JSON" \
  --import-diag-json "$IMPORT_DIAG_JSON" \
  "${VISUAL_ARGS[@]}" \
  --out-json "$PARITY_JSON"

echo "wrote:"
echo "  $PARITY_JSON"

if [[ "$REQUIRE_PASS" == "true" ]]; then
  if jq -e '.gates.parityPass == true' "$PARITY_JSON" >/dev/null; then
    echo "parityPass=true"
  else
    echo "parityPass=false (failing due to --require-pass)" >&2
    exit 1
  fi
fi
