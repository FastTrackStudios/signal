#!/usr/bin/env bash
# CI entry point — runs the daw crate integration tests inside the
# fts-flake headless FHS environment.
#
# Usage:
#   nix run .#fts-test -- ./ci/test.sh
#   nix run .#fts-test-ci -- ./ci/test.sh            # minimal CI preset
#
# Or from the daw repo:
#   nix run path:../fts-flake#fts-test -- \
#     cargo test -p daw-reaper --test reaper_connection -- --ignored --nocapture
set -euo pipefail

echo "=== fts-flake CI test runner ==="
echo "REAPER: $FTS_REAPER_EXECUTABLE"
echo "Display: $DISPLAY"
echo ""

# Verify REAPER can start (quick smoke test)
echo "[ci] Smoke test: launching REAPER..."
"$FTS_REAPER_EXECUTABLE" -newinst -nosplash -ignoreerrors &
REAPER_PID=$!
sleep 3

if kill -0 "$REAPER_PID" 2>/dev/null; then
  echo "[ci] REAPER started successfully (PID $REAPER_PID)"
  kill "$REAPER_PID" 2>/dev/null || true
  wait "$REAPER_PID" 2>/dev/null || true
else
  echo "[ci] ERROR: REAPER failed to start"
  exit 1
fi

echo "[ci] Smoke test passed."
echo ""

# If a command was passed, run it (e.g. cargo test ...)
if [ $# -gt 0 ]; then
  echo "[ci] Running: $*"
  exec "$@"
fi

echo "[ci] No test command specified. Smoke test only."
