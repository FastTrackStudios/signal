#!/usr/bin/env bash
# Probe-and-diff harness.
#
# Usage: scripts/probe_diff.sh <probe_name>
#
# 1. Builds the named probe RPP→PTX via rpp_to_ptx_probe
# 2. Runs scripts/pt-convert.sh under trace_all_reads.js Frida hook
# 3. Diffs read offsets against /tmp/reads_baseline.log
# 4. Maps differing offsets through find_blocks_at
#
# Requires /tmp/reads_baseline.log to exist (run with `baseline` first).

set -e

PROBE="${1:?usage: $0 <probe_name>}"
ROOT=$(cd "$(dirname "$0")/.." && pwd)

cd "$ROOT"

# Build probe PTX
echo "[probe_diff] Building probe '$PROBE'..." >&2
cargo run --quiet -p daw-reaper --example rpp_to_ptx_probe -- "$PROBE" 2>&1 | tail -1 >&2
PROBE_PTX="/tmp/probe_${PROBE}.ptx"
[[ -f "$PROBE_PTX" ]] || { echo "missing $PROBE_PTX" >&2; exit 1; }

# Trace
LOG="/tmp/reads_${PROBE}.log"
echo "[probe_diff] Tracing convert via Frida..." >&2
timeout 60 ./scripts/pt-convert.sh \
  --hook ./scripts/frida/trace_all_reads.js \
  --log "$LOG" \
  "$PROBE_PTX" "/tmp/out_${PROBE}.rpp" 2>&1 | tail -1 >&2

# Diff
echo
echo "=== READ DIFF: baseline vs $PROBE ==="
DIFF_OUT=$(diff <(grep '"msg":"read"' /tmp/reads_baseline.log) <(grep '"msg":"read"' "$LOG"))
if [[ -z "$DIFF_OUT" ]]; then
  echo "(no diff)"
  exit 0
fi
echo "$DIFF_OUT" | head -40

# Extract differing offsets (from the > lines)
offsets=$(echo "$DIFF_OUT" | grep '^>' | python3 -c "
import sys, json
for line in sys.stdin:
    line = line[2:].strip()
    try:
        d = json.loads(line)
        print(d['off'])
    except: pass
" | sort -u | head -20)

if [[ -n "$offsets" ]]; then
  echo
  echo "=== BLOCK MAPPING for differing offsets ==="
  cargo run --quiet -p daw-reaper --example find_blocks_at -- "$PROBE_PTX" $offsets 2>/dev/null
fi
