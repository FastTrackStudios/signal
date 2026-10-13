#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/capture_render_diagnostics.sh <fts_export_or_figma_json> <out_json>

INPUT="${1:-}"
OUT="${2:-}"

if [[ -z "$INPUT" || -z "$OUT" ]]; then
  echo "usage: $0 <fts_export_or_figma_json> <out_json>" >&2
  exit 2
fi
if [[ ! -f "$INPUT" ]]; then
  echo "missing input: $INPUT" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUT")"
cargo run -q -p frame-cli --bin render_diagnostics -- "$INPUT" > "$OUT"
echo "wrote $OUT"
