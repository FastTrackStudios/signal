#!/bin/bash
# Launch the staged host on a staged plugin. Everything it touches is on the
# internal disk, so macOS never asks about the removable volume.
#
#   scripts/run-host.sh EQ
set -euo pipefail
APP="${FTS_HOST_APP:-$HOME/Applications/FTSPluginHost.app}"
STAGE="${FTS_HOST_STAGE:-$HOME/Library/Application Support/FTSPluginHost/plugins}"
LOG="${FTS_HOST_LOG:-$HOME/Library/Logs/fts-clap-host.log}"
name="${1:-EQ}"
bundle="$STAGE/FTS $name.clap"
[ -d "$bundle" ] || { echo "not staged: $bundle — run scripts/install-host-app.sh $name" >&2; exit 1; }
mkdir -p "$(dirname "$LOG")"
: > "$LOG"
exec open -a "$APP" --stdout "$LOG" --stderr "$LOG" --args "$bundle"
