#!/usr/bin/env bash
# Generate a measurement plan from each unit's scan, then run it.
#
# This replaces the hand-written drive-control manifest entirely: the plan
# comes from what the scan measured, so a unit whose drive turns out to be
# Compress rather than Gain gets measured correctly without anyone editing a
# table. Resumable at both levels — a completed job is skipped, and an
# interrupted unit picks up at its next job.
#
#   ./capture-planned.sh              # every scanned unit
#   ./capture-planned.sh distressor   # units matching a pattern
set -uo pipefail

ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"
[ -d "$ARCHIVE" ] || ARCHIVE="captures"
SCRIPTS="$(cd "$(dirname "$0")" && pwd)"
FILTER="${1:-}"
START=$(date +%s)

mapfile -t SCANS < <(find "$ARCHIVE" -name scan.json -path "*/scan/*" | sort)
TOTAL=${#SCANS[@]}
INDEX=0

for scan in "${SCANS[@]}"; do
  unit_dir="$(dirname "$(dirname "$scan")")"
  name="$(basename "$unit_dir")"
  INDEX=$((INDEX + 1))
  if [ -n "$FILTER" ]; then
    case "$(echo "$name" | tr '[:upper:]' '[:lower:]')" in
      *"$(echo "$FILTER" | tr '[:upper:]' '[:lower:]')"*) ;;
      *) continue ;;
    esac
  fi
  grep -q '"complete": true' "$scan" 2>/dev/null || {
    echo "── [$INDEX/$TOTAL] $name — scan incomplete, skipping"; continue; }

  plan="$unit_dir/plan.json"
  python3 "$SCRIPTS/make-plan.py" "$scan" --out "$plan" >/dev/null || {
    echo "── [$INDEX/$TOTAL] $name — could not plan"; continue; }

  echo "── [$INDEX/$TOTAL] $name"
  python3 "$SCRIPTS/run-plan.py" "$plan" --out "$unit_dir/saturation" 2>&1 | sed 's/^/   /'

  NOW=$(date +%s); SPENT=$((NOW - START))
  if [ "$INDEX" -lt "$TOTAL" ]; then
    REMAIN=$(( SPENT * (TOTAL - INDEX) / INDEX ))
    printf -- "   fleet %d/%d · elapsed %dm%02ds · ETA %dm%02ds\n" \
      "$INDEX" "$TOTAL" $((SPENT/60)) $((SPENT%60)) $((REMAIN/60)) $((REMAIN%60))
  fi
done
ELAPSED=$(( $(date +%s) - START ))
printf -- "── all done in %dm%02ds\n" $((ELAPSED/60)) $((ELAPSED%60))
