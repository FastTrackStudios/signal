#!/usr/bin/env bash
# Paint the strip and montage it against the real MCP.
#
# The convergence loop for the Dioxus mixer: REAPER's own strip on the
# left, the hand-measured panel sheet in the middle, ours on the right,
# all at the same height. Run from the repo root.
#
#   features/daw-ui/daw-ui/tests/compare.sh
#
# Writes target/theme-shots/strip-compare.png and prints the measured
# band heights for each, which is the part that is actually checkable —
# "looks close" is not a result.
set -euo pipefail

OUT=target/theme-shots
REF=${REF:-$OUT/mcp-zoom.png}     # real REAPER MCP, 3x
HAND=${HAND:-$OUT/native-panels.png}
FONT=${FONT:-$(nix build --no-link --print-out-paths nixpkgs#dejavu_fonts)/share/fonts/truetype/DejaVuSans-Bold.ttf}
BANDS=features/daw-ui/daw-ui/tests/bands.py

nix develop /run/media/Development/FastTrackStudio -c \
  cargo test -q -p daw-ui --test strip_shot -- --nocapture

magick "$REF"  -crop 254x694+18+6   +repage "$OUT/.c-reaper.png"
magick "$HAND" -crop 178x470+8+803  +repage "$OUT/.c-hand.png"
cp "$OUT/strip-dioxus.png" "$OUT/.c-ours.png"

echo "── measured coloured bands ───────────────────────────"
# Columns clear of every control: a column through the pan knob reads the
# knob's dark art as a gap and splits one band into two.
python3 "$BANDS" "$OUT/.c-reaper.png"  8 0 694 3.0
python3 "$BANDS" "$OUT/.c-hand.png"    6 0 470 2.0
python3 "$BANDS" "$OUT/.c-ours.png"    3 0 228 1.0

# The aligned pixel diff. REAPER's crop starts inside the strip, so the two
# are registered on the one landmark they share — the top of the coloured
# band — and everything else is then measurable against it.
if [ -f "$OUT/.reaper1x.png" ] || magick "$REF" -resize 33.3333% +repage "$OUT/.reaper1x.png"; then
  MIXER_HEIGHT=235 nix develop /run/media/Development/FastTrackStudio -c \
    cargo test -q -p daw-ui --test strip_shot -- paint_the_strip >/dev/null 2>&1
  magick "$OUT/.reaper1x.png" -background "#262626" -splice 0x10 \
    -crop 85x231+0+0 +repage "$OUT/.ref.png"
  magick "$OUT/strip-dioxus.png" -crop 85x231+0+0 +repage "$OUT/.aligned.png"
  echo -n "── differing pixels: "
  magick compare -metric AE "$OUT/.ref.png" "$OUT/.aligned.png" \
    -compose src "$OUT/strip-diff.png" 2>&1 | tail -1
  echo "wrote $OUT/strip-diff.png"
fi

# The track panel, against a row of REAPER's own. Fuzzed, deliberately:
# at zero tolerance a one-unit rounding in the tint counts every pixel of
# every tinted field as a difference, and the diff comes back a solid
# block that says nothing.
if [ -f "$OUT/tcp-ref.png" ]; then
  magick "$OUT/tcp-ref.png" -crop 343x70+0+107 +repage "$OUT/.tcp-ref.png"
  echo -n "── track row, differing pixels: "
  magick compare -metric AE -fuzz 4% "$OUT/.tcp-ref.png" "$OUT/track-row.png" \
    -compose src "$OUT/track-row-diff.png" 2>&1 | tail -1
  echo "wrote $OUT/track-row-diff.png"
fi

cap() {
  magick "$1" -filter point -resize x600 -bordercolor "$2" -border 3 \
    -background "#141414" -gravity center -extent 240x612 "$OUT/.x.png"
  magick -size 240x36 -background "#141414" -fill "$2" -font "$FONT" \
    -pointsize 18 -gravity center label:"$3" "$OUT/.y.png"
  magick "$OUT/.x.png" "$OUT/.y.png" -background "#141414" -append "$4"
}

cap "$OUT/.c-reaper.png" "#9aa0a6" "REAPER MCP (real)"   "$OUT/.l-reaper.png"
cap "$OUT/.c-hand.png"   "#ffd400" "hand-measured sheet" "$OUT/.l-hand.png"
cap "$OUT/.c-ours.png"   "#3b9dff" "Dioxus / Tailwind"   "$OUT/.l-ours.png"

magick "$OUT/.l-reaper.png" "$OUT/.l-hand.png" "$OUT/.l-ours.png" +append \
  -bordercolor "#141414" -border 20 "$OUT/strip-compare.png"
rm -f "$OUT"/.[xylc]-*.png "$OUT"/.x.png "$OUT"/.y.png
echo "wrote $OUT/strip-compare.png"
