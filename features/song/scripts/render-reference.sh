#!/usr/bin/env bash
# render-reference.sh <song-dir> — render a single mixed-down `reference.ogg`
# for a song that has stems but no `original-track`/`reference` of its own, so
# the streaming player can play ONE file instead of every stem.
#
# Mixes all NON-guide stems (skips click/cue/guide/count), sums them
# (normalize=0 — the stems are a real mix), then loudness-normalises to a
# streaming target (-16 LUFS, -1.5 dBTP). Needs ffmpeg (e.g. `nix shell
# nixpkgs#ffmpeg-headless -c bash render-reference.sh <dir>`).
#
# After rendering, wire it into the arrangement:
#   cargo run -p song --example add_reference -- <song-dir> [...]
set -euo pipefail
dir="$1"
stems="$dir/stems"
[ -d "$stems" ] || { echo "no stems in $dir"; exit 1; }
mapfile -t music < <(ls "$stems"/*.ogg 2>/dev/null | grep -viE 'click|cue|guide|count')
n=${#music[@]}
[ "$n" -ge 1 ] || { echo "no music stems in $dir"; exit 1; }
args=(); for f in "${music[@]}"; do args+=(-i "$f"); done
echo "  mixing $n stems -> reference.ogg"
ffmpeg -y -loglevel error "${args[@]}" \
  -filter_complex "amix=inputs=$n:duration=longest:normalize=0,loudnorm=I=-16:TP=-1.5:LRA=11" \
  -c:a libopus -b:a 128k "$dir/reference.ogg"
