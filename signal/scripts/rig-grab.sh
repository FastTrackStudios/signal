#!/usr/bin/env bash
# Screenshot the running rig window — and nothing else on the screen.
#
# The headless renderer (`just guitar-shot`) cannot show this: its
# `VelloImageRenderer` hands out no wgpu device, so every painted panel falls
# back to vectors there and the WGSL never runs. The only place the shaders
# actually execute is the live window, so the only honest picture of them is a
# capture of that window.
#
# Deliberately NOT a full-screen grab. KWin is asked to make the rig active and
# then to capture the active window, so the frame contains the rig and nothing
# the user happens to have open beside it.
set -euo pipefail

OUT="${1:-rig-live.png}"
EXPECT_W="${2:-}"
# Which window to raise. The rig by default; the gallery is the other thing
# worth photographing and it is a different binary.
MATCH="${3:-signal-desktop}"

if ! pgrep -x "$MATCH" >/dev/null; then
    echo "rig-grab: no $MATCH is running — start one with 'just guitar --design' (or 'just viz-gallery')" >&2
    exit 1
fi

SCRIPT="$(mktemp --suffix=.js)"
trap 'rm -f "$SCRIPT"' EXIT
cat > "$SCRIPT" <<JS
// Raise the target, so "the active window" means the target.
var list = workspace.windowList();
for (var i = 0; i < list.length; i++) {
    var w = list[i];
    var id = (w.resourceClass || "") + " " + (w.resourceName || "");
    if (id.indexOf("$MATCH") !== -1) {
        workspace.activeWindow = w;
        break;
    }
}
JS

NAME="fts-rig-grab-$$"
ID="$(qdbus org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript "$SCRIPT" "$NAME")"
qdbus "org.kde.KWin" "/Scripting/Script$ID" org.kde.kwin.Script.run >/dev/null
qdbus org.kde.KWin /Scripting org.kde.kwin.Scripting.unloadScript "$NAME" >/dev/null
# The compositor needs a beat to finish raising before the grab.
sleep 0.4

rm -f "$OUT"
spectacle -a -b -n -o "$OUT" >/dev/null 2>&1

if [ ! -s "$OUT" ]; then
    echo "rig-grab: nothing was captured" >&2
    exit 1
fi

SIZE="$(magick identify -format '%wx%h' "$OUT")"
echo "rig-grab: $OUT $SIZE"

# A capture of the wrong window is worse than no capture: it is a picture of
# whatever else was on screen. Refuse it rather than hand it on.
if [ -n "$EXPECT_W" ]; then
    GOT_W="${SIZE%x*}"
    if [ "$GOT_W" != "$EXPECT_W" ]; then
        echo "rig-grab: expected a ${EXPECT_W}px-wide window, got ${GOT_W} — the rig was not the active window" >&2
        rm -f "$OUT"
        exit 1
    fi
fi
