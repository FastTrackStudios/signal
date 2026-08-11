#!/usr/bin/env bash
# Emit one line per agent_status transition of a herdr pane; exits if the
# pane vanishes. Run under the Monitor tool (persistent) — plain Bash
# run_in_background monitors have been observed getting killed externally.
# Usage: monitor-transitions.sh <pane-id>
pane="$1"
prev=""
while true; do
  s=$(herdr pane get "$pane" 2>/dev/null | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"]["pane"]["agent_status"])' 2>/dev/null || echo gone)
  if [ "$s" != "$prev" ] && [ -n "$prev" ]; then
    echo "$pane status: $prev -> $s"
  fi
  [ "$s" = "gone" ] && break
  prev="$s"
  sleep 60
done
