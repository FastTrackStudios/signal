#!/usr/bin/env bash
set -euo pipefail

# Usage:
#   xtask/frame/scripts/capture_load_profile.sh <fts_export_or_figma_json> <out_json> [iterations]

INPUT="${1:-}"
OUT="${2:-}"
ITERATIONS="${3:-3}"

if [[ -z "$INPUT" || -z "$OUT" ]]; then
  echo "usage: $0 <fts_export_or_figma_json> <out_json> [iterations]" >&2
  exit 2
fi
if [[ ! -f "$INPUT" ]]; then
  echo "missing input: $INPUT" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUT")"
cargo run -q -p frame-cli --features anyrender --bin load_profile -- "$INPUT" --iterations "$ITERATIONS" > "$OUT"
echo "wrote $OUT"
