#!/usr/bin/env bash
# Verify Keyscape soundsource packs one at a time as they arrive:
#   1. playability sweep on thebattleship (where the packs are built) —
#      a pack that fails is not transferred;
#   2. copy the pack (+ its soundsource list) to this Mac;
#   3. A/B every soundsource in it against Omnisphere (sscompare.py).
# Appends to $REPORT; skips packs already reported. usage:
#   keyscape_pipeline.sh [pack ...]    (default: the gig's first, then all)
set -u
HOST=cody@100.68.255.30
RDIR=/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Soundsources
RCHECK=/run/media/Development/omni-wip/signal/target/release/examples/soundsource_check
LDIR=/Volumes/dev-drive/AudioHaven/Signal/Libraries/Keys/Keyscape/Soundsources
HERE=$(cd "$(dirname "$0")" && pwd)
PY=${PY:-/private/tmp/claude-501/-Users-codywright/8eccb40f-5f6d-4a65-b161-c069fd3b189f/scratchpad/venv/bin/python}
REPORT=${REPORT:-$LDIR/verification.md}
mkdir -p "$LDIR"
if [ $# -gt 0 ]; then packs=("$@"); else
  # (the remote login shell is nushell: run the listing through bash)
  mapfile -t all < <(echo "ls '$RDIR'" | ssh -o HostName=100.68.255.30 thebattleship bash -s | grep '\.signalpack$' | sed 's/\.signalpack$//')
  packs=(Dolceola Clavichord)
  for p in "${all[@]}"; do [[ "$p" == Dolceola || "$p" == Clavichord ]] || packs+=("$p"); done
fi
for L in "${packs[@]}"; do
  grep -q "^## $L\$" "$REPORT" 2>/dev/null && { echo "skip $L (reported)"; continue; }
  echo "== $L"
  play=$(echo "PACKS='$L' '$RCHECK' '$RDIR'" | ssh -o HostName=100.68.255.30 thebattleship bash -s 2>&1)
  { echo "## $L"; echo; echo '```'; echo "$play" | grep -E '^(ok|WARN|FAIL)|ok, .* fail'; echo '```'; } >> "$REPORT"
  if echo "$play" | grep -q '^FAIL'; then
    echo "   playability FAIL — not transferred"; echo "**Not transferred: playability failures.**" >> "$REPORT"; echo >> "$REPORT"; continue
  fi
  rsync -as "$HOST:$RDIR/$L.signalpack" "$HOST:$RDIR/$L.soundsources.txt" "$LDIR/" || { echo "   transfer failed"; continue; }
  echo "   transferred; A/B vs Omnisphere"
  mapfile -t names < "$LDIR/$L.soundsources.txt"
  ab=$(cd "$HERE" && "$PY" sscompare.py "${names[@]}" < /dev/null 2>&1 | grep -E '^(ok|WARN|FAIL)')
  { echo; echo "A/B vs Omnisphere (pitch omni/ours, level ours−omni, spectrum rms dB):"; echo '```'; echo "$ab"; echo '```'; echo; } >> "$REPORT"
  echo "$ab" | awk '{print "   " $1}' | sort | uniq -c
done
