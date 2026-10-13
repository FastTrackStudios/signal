#!/usr/bin/env bash
# Frontier query: open ready-for-agent tickets in [MIN,MAX] with open-blocker
# counts and assignee counts. Takeable = blocked_by=0 and assignees=0.
# Usage: frontier.sh <min-issue> <max-issue> [repo]
set -euo pipefail
min="$1"; max="$2"; repo="${3:-FastTrackStudios/FastTrackStudio}"
for n in $(gh issue list -R "$repo" --label ready-for-agent --state open --limit 100 --json number --jq '.[].number'); do
  if [ "$n" -ge "$min" ] && [ "$n" -le "$max" ]; then
    gh api "repos/$repo/issues/$n" \
      --jq '"\(.number)\tblocked_by=\(.issue_dependencies_summary.blocked_by)\tassignees=\(.assignees | length)\t\(.title)"' \
      || echo "$n	QUERY-FAILED (retry this one)"
  fi
done
