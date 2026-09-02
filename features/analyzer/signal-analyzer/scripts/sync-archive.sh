#!/usr/bin/env bash
# Pull measurements off the measuring machine into the plugin-analysis archive.
#
# Captures are produced on voyager (which has the plugins) and archived on the
# workstation (which has the disk). The two layouts are identical — one
# directory per plugin — so this is a plain mirror.
#
# No --delete. The archive holds things voyager does not: manuals, extracted
# text, notes, the README. Deleting anything not present at the source would
# take those with it.
#
#   ./sync-archive.sh          # pull everything
#   ./sync-archive.sh --dry    # show what would move
set -euo pipefail

REMOTE="${MEASURE_HOST:-voyager}"
REMOTE_DIR="${MEASURE_DIR:-Development/signal-measure/captures}"
ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"

[ -d "$ARCHIVE" ] || { echo "archive not mounted at $ARCHIVE"; exit 1; }

DRY=""
[ "${1:-}" = "--dry" ] && DRY="--dry-run"

echo "── $REMOTE:$REMOTE_DIR  ->  $ARCHIVE"
rsync -a --stats $DRY "$REMOTE:$REMOTE_DIR/" "$ARCHIVE/" \
  | grep -E "files transferred|Total file size|Total transferred" || true

echo "── archive now:"
du -sh "$ARCHIVE"
find "$ARCHIVE" -maxdepth 1 -mindepth 1 -type d | wc -l | xargs printf "   %s units\n"
