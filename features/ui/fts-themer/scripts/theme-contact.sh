#!/usr/bin/env bash
# Photograph every FTS surface with the current theme, side by side.
#
# The point is judging colours ACROSS surfaces at once: a palette that reads
# well in the expression editor can go muddy in REAPER's mixer, and looking
# at one surface at a time hides exactly that.
#
# Run it inside the REAPER test shell so a window manager exists — a bare
# Xvfb leaves windows unmanaged, and the REAPER capture then stops
# representing what a user would see:
#
#   nix develop .#reaper-test -c just reaper theme-contact
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
OUT="${1:-$ROOT/target/theme-contact}"
SHOTS="${FTS_SHOTS_DIR:-$ROOT/target/gui-shots/expression-editor}"
mkdir -p "$OUT"

echo "── expression editor ──"
# Scenes come from the same demo module the runnable example mounts, so
# these pictures can't drift from what the app actually launches.
FTS_SHOTS_DIR="$SHOTS" cargo test -q -p expression-editor-ui --test screenshots 2>&1 | tail -1

echo "── REAPER ──"
cargo run -q -p fts-themer --bin fts-themer -- \
    --theme "$ROOT/features/reaper/fts-theme" \
    shot --out "$OUT/reaper.png" --settle "${SETTLE:-16}"

if ! command -v magick >/dev/null; then
    echo
    echo "No imagemagick — skipping the contact sheet."
    echo "  REAPER:        $OUT/reaper.png"
    echo "  editor scenes: $SHOTS/"
    exit 0
fi

echo "── contact sheet ──"
# Rows are normalised to a common WIDTH, not height. Normalising on height
# is the obvious thing and the wrong one: the REAPER shot is a single
# window and the editor row is three scenes, so equal heights leave the
# REAPER row a third of the width and half the sheet empty.
SHEET_W="${SHEET_W:-1800}"

row() {
    local out="$1"; shift
    local present=()
    for f in "$@"; do [ -f "$f" ] && present+=("$f"); done
    [ ${#present[@]} -eq 0 ] && return 1
    magick "${present[@]}" -background '#000000' +append \
        -resize "${SHEET_W}x" "$out"
}

if row "$OUT/.editor-row.png" \
        "$SHOTS/01-phrase.png" "$SHOTS/03-microtonal.png" "$SHOTS/05-all-lanes.png" \
   && row "$OUT/.reaper-row.png" "$OUT/reaper.png"; then
    magick "$OUT/.reaper-row.png" "$OUT/.editor-row.png" \
        -background '#000000' -gravity center -append "$OUT/contact.png"
    rm -f "$OUT/.editor-row.png" "$OUT/.reaper-row.png"
    echo
    echo "Wrote $OUT/contact.png"
else
    echo "Nothing to composite — check the two steps above."
fi

echo "  REAPER:        $OUT/reaper.png"
echo "  editor scenes: $SHOTS/"
