#!/usr/bin/env bash
# Drive the PT Reaper Converter remotely on voyager (macOS) for RE work.
#
# Usage:
#   scripts/pt-convert.sh <input> <output>
#   scripts/pt-convert.sh --hook <local-script.js> --log <local-log> <input> <output>
#
# Input/output paths are LOCAL paths on Linux. The script:
#   1. Uploads input to voyager:/tmp/pt-re/input/
#   2. Uploads the Frida hook script (if --hook) to voyager
#   3. Runs ~/pt-re/run.sh on voyager
#   4. Downloads the output (.rpp/.ptx) and Frida log back to local paths
#
# Voyager state lives at /tmp/pt-re/ (cache) and is reused across runs.

set -euo pipefail

VOYAGER="${VOYAGER:-100.123.80.40}"
REMOTE_DIR="/tmp/pt-re"

HOOK=""
LOG=""
while [[ "${1:-}" == --* ]]; do
  case "$1" in
    --hook) HOOK="$2"; shift 2 ;;
    --log)  LOG="$2";  shift 2 ;;
    *) echo "unknown flag: $1"; exit 1 ;;
  esac
done

INPUT="$1"; OUTPUT="$2"
[[ -f "$INPUT" ]] || { echo "missing input: $INPUT"; exit 1; }

# Ensure remote dirs
ssh -o BatchMode=yes "$VOYAGER" "mkdir -p $REMOTE_DIR/input $REMOTE_DIR/output" >/dev/null

# Upload input
remote_input="$REMOTE_DIR/input/$(basename "$INPUT")"
scp -q "$INPUT" "$VOYAGER:$remote_input"

# Determine remote output
out_ext="${OUTPUT##*.}"
remote_output="$REMOTE_DIR/output/$(basename "$OUTPUT")"

# Optional Frida hook
hook_args=""
if [[ -n "$HOOK" ]]; then
  [[ -f "$HOOK" ]] || { echo "missing hook script: $HOOK"; exit 1; }
  remote_hook="$REMOTE_DIR/hook.js"
  scp -q "$HOOK" "$VOYAGER:$remote_hook"
  remote_log="$REMOTE_DIR/last.log"
  hook_args="--hook $remote_hook --log $remote_log"
fi

# Run converter
ssh -o BatchMode=yes "$VOYAGER" \
  "~/pt-re/run.sh $hook_args '$remote_input' '$remote_output'"

# Pull output back
mkdir -p "$(dirname "$OUTPUT")"
scp -q "$VOYAGER:$remote_output" "$OUTPUT"

# Pull frida log if requested
if [[ -n "$LOG" ]]; then
  mkdir -p "$(dirname "$LOG")"
  scp -q "$VOYAGER:$REMOTE_DIR/last.log" "$LOG"
fi

echo "wrote $OUTPUT ($(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT") bytes)"
if [[ -n "$LOG" ]]; then
  echo "frida log: $LOG ($(wc -l < "$LOG") lines)"
fi
exit 0
