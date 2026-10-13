#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CRATE_DIR="$ROOT_DIR/modules/dawfile/dawfile-reaper"
OUT_DIR="${OUT_DIR:-$CRATE_DIR/docs/perf-runs}"
if [[ "$OUT_DIR" != /* ]]; then
  OUT_DIR="$ROOT_DIR/$OUT_DIR"
fi
STAMP="$(date +%Y%m%d-%H%M%S)"

TEMPO_FIXTURE="${TEMPO_FIXTURE:-$CRATE_DIR/tests/fixtures/tempo-map-advanced.RPP}"
GOODNESS_FIXTURE="${GOODNESS_FIXTURE:-$CRATE_DIR/tests/fixtures/local/Goodness of God.RPP}"
WARMUP="${WARMUP:-1}"
REPEAT="${REPEAT:-3}"
TYPED_MODE="${TYPED_MODE:-full}"

mkdir -p "$OUT_DIR"

RAW_OUT="$OUT_DIR/perf-matrix-${STAMP}.txt"
SUMMARY_OUT="$OUT_DIR/perf-matrix-${STAMP}.md"
LATEST_MD="$OUT_DIR/latest.md"
LATEST_TXT="$OUT_DIR/latest.txt"

pushd "$ROOT_DIR" >/dev/null

echo "Running dawfile-reaper perf matrix..."
echo "  tempo fixture:    $TEMPO_FIXTURE"
echo "  goodness fixture: $GOODNESS_FIXTURE"
echo "  warmup/repeat:    $WARMUP/$REPEAT"
echo "  typed mode:       $TYPED_MODE"
echo

cargo run -p dawfile-reaper --release --bin rpp_perf -- \
  --fixture "$TEMPO_FIXTURE" \
  --fixture "$GOODNESS_FIXTURE" \
  --warmup "$WARMUP" \
  --repeat "$REPEAT" \
  --typed-mode "$TYPED_MODE" | tee "$RAW_OUT"

{
  echo "# Performance Matrix ($STAMP)"
  echo
  echo "- command:"
  echo '  ```bash'
  echo "  cargo run -p dawfile-reaper --release --bin rpp_perf -- --fixture \"$TEMPO_FIXTURE\" --fixture \"$GOODNESS_FIXTURE\" --warmup \"$WARMUP\" --repeat \"$REPEAT\" --typed-mode \"$TYPED_MODE\""
  echo '  ```'
  echo "- raw output: \`$RAW_OUT\`"
  echo
  echo "| Fixture | Size MB | Parse Avg s | Typed Avg s | Throughput MB/s | Peak RSS MB | Parse Alloc Calls | Parse Alloc MB | Typed Alloc Calls | Typed Alloc MB |"
  echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
  awk '
    /^fixture:/ { fixture=$2; for (i=3; i<=NF; i++) fixture=fixture " " $i; next }
    /size_mb:/ { size=$2; next }
    /parse_avg_s:/ { parse=$2; next }
    /typed_avg_s:/ { typed=$2; next }
    /parse_throughput_mb_s:/ { tput=$2; next }
    /peak_rss_mb:/ { rss=$2; next }
    /parse_alloc_calls_avg:/ { pac=$2; next }
    /parse_alloc_mb_avg:/ { pamb=$2; next }
    /typed_alloc_calls_avg:/ { tac=$2; next }
    /typed_alloc_mb_avg:/ {
      tamb=$2;
      if (fixture != "") {
        printf("| `%s` | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", fixture, size, parse, typed, tput, rss, pac, pamb, tac, tamb);
      }
      fixture=""; size=""; parse=""; typed=""; tput=""; rss=""; pac=""; pamb=""; tac=""; tamb="";
      next
    }
  ' "$RAW_OUT"
} >"$SUMMARY_OUT"

cp "$SUMMARY_OUT" "$LATEST_MD"
cp "$RAW_OUT" "$LATEST_TXT"

echo
echo "Wrote:"
echo "  $RAW_OUT"
echo "  $SUMMARY_OUT"
echo "  $LATEST_MD"
echo "  $LATEST_TXT"

popd >/dev/null
