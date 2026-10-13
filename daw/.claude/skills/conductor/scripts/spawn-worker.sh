#!/usr/bin/env bash
# Spawn one worker: worktree from origin/main + herdr pane + claude session.
# Usage: spawn-worker.sh <ticket> <branch-slug> [model]
#   e.g. spawn-worker.sh 260 cadence-engine opus
# Prints the pane id on the last line. The caller sends the worker prompt
# afterwards (pane run, then send-keys Enter — long prompts land as pasted
# text and need the explicit Enter).
set -euo pipefail
ticket="$1"; slug="$2"; model="${3:-opus}"
base="/run/media/Development/herdr-worktrees/FastTrackStudio"
wt="$base/worktree-files-$ticket"

git fetch origin main
git worktree add "$wt" -b "files/$ticket-$slug" origin/main

pane=$(herdr pane split --current --direction right --no-focus \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"]["pane"]["pane_id"])')
herdr pane rename "$pane" "files-$ticket" >/dev/null
# Full-width tab per worker: narrow split panes break herdr's agent-status
# detection (observed at ~5-column width), which the monitors depend on.
herdr pane move "$pane" --new-tab --label "files-$ticket" --no-focus >/dev/null
# The pane shell is Nushell: separate commands with ';', never '&&'.
herdr pane run "$pane" "cd $wt; direnv allow; claude --model $model"
herdr wait agent-status "$pane" --status idle --timeout 120000 >/dev/null
echo "$pane"
