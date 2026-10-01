#!/usr/bin/env bash
# Render every page of the rig's phone layout as an iPhone 16 Pro held
# sideways shows it: the app's viewport (874 × 381 points — the iOS shell
# keeps the home indicator's 21 out of it) on the 874 × 402 screen, with
# the screen's rounded corners, the Dynamic Island on the right and the
# home indicator drawn over — plus a contact sheet of them all.
#
#   features/rigs/guitar/ui/scripts/phone-shots.sh [OUT_DIR] [SCALE] [PAGE…]
#
# Every page at once: an app each (design mode — no audio, writes nothing)
# on a private X display of its own, so it takes no focus and leaves a
# running app alone. SCALE 1 renders at points, 3 at the phone's pixels.
# PAGE… renders only those (`drives amps`). Build first:
# `cargo build -p signal-desktop`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../../../.." && pwd)"
OUT="${1:-$ROOT/target/phone-shots}"
SCALE="${2:-1}"
shift $(( $# > 2 ? 2 : $# ))
ALL=(input pedals special pre-mod-trem pre-delay-verb compressor drives amps eq gate-post-comp mod-motion delays reverbs)
if [ $# -gt 0 ]; then PAGES=("$@"); else PAGES=("${ALL[@]}"); fi
W=$((874 * SCALE)) H=$((402 * SCALE)) VH=$((381 * SCALE))
BASE="${PHONE_SHOTS_DISPLAY:-120}"
mkdir -p "$OUT"
rm -f "$OUT"/[0-9][0-9]-*.png
XVFB_BIN="$(nix shell nixpkgs#xorg-server -c sh -c 'command -v Xvfb' 2>/dev/null)"

PIDS=()
trap 'kill "${PIDS[@]}" 2>/dev/null || true' EXIT
cd "$ROOT"
for i in "${!PAGES[@]}"; do
    "$XVFB_BIN" ":$((BASE + i))" -screen 0 "${W}x$((H + 40))x24" -nolisten tcp >/dev/null 2>&1 &
    PIDS+=($!)
done
sleep 1
for i in "${!PAGES[@]}"; do
    env -u WAYLAND_DISPLAY DISPLAY=":$((BASE + i))" \
        SIGNAL_RIG_DESIGN=1 FTS_FORM_FACTOR=phone FTS_PHONE_PAGE="${PAGES[$i]}" \
        FTS_WINDOW_SIZE="874x381" WINIT_X11_SCALE_FACTOR="$SCALE" \
        ./target/debug/signal-desktop >"$OUT/${PAGES[$i]}.log" 2>&1 &
    PIDS+=($!)
done

s() { echo $(( $1 * SCALE )); }
# The device's screen: corners (55 pt), the island (37 × 126 pt, 11 pt in
# from the right edge — the phone turned with its top to the right) and
# the home indicator (134 × 5, 8 pt up).
shoot() {
    local i=$1 page=$2 d=":$((BASE + $1))" win=""
    # Up when its window is (a minute at most), then time to draw.
    for _ in $(seq 120); do
        win=$(DISPLAY="$d" xdotool search --name . 2>/dev/null | tail -1 || true)
        [ -n "$win" ] && break
        sleep 0.5
    done
    [ -z "$win" ] && { echo "$page: no window — see $OUT/$page.log" >&2; return 1; }
    sleep "${PHONE_SHOTS_SETTLE:-8}"
    local raw="$OUT/raw-$page.png"
    DISPLAY="$d" magick import -window "$win" "$raw"
    magick -size "${W}x${H}" xc:"#0f1012" \( "$raw" -crop "${W}x${VH}+0+0" +repage \) -geometry +0+0 -composite \
        \( -size "${W}x${H}" xc:black -fill white -draw "roundrectangle 0,0 $((W - 1)),$((H - 1)) $(s 55),$(s 55)" \) \
        -alpha off -compose copy_opacity -composite -compose over \
        -fill black -draw "roundrectangle $((W - $(s 48))),$(( (H - $(s 126)) / 2 )) $((W - $(s 11))),$(( (H + $(s 126)) / 2 )) $(s 18),$(s 18)" \
        -fill "#e5e7ebcc" -draw "roundrectangle $(( (W - $(s 134)) / 2 )),$((H - $(s 13))) $(( (W + $(s 134)) / 2 )),$((H - $(s 8))) $(s 3),$(s 3)" \
        -background "#2b2b2e" -flatten \
        "$OUT/$(printf %02d "$((i + 1))")-$page.png"
    rm -f "$raw"
}
SHOTS=()
for i in "${!PAGES[@]}"; do
    shoot "$i" "${PAGES[$i]}" &
    SHOTS+=($!)
done
for p in "${SHOTS[@]}"; do wait "$p" || true; done
magick montage "$OUT"/[0-9][0-9]-*.png -tile 3x -geometry +12+12 -background "#1b1b1d" "$OUT/sheet.png"
echo "$OUT"
