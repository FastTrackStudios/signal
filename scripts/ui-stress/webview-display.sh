#!/usr/bin/env bash
# A private X display for the WebView workstation, driven and screenshotted.
#
# The window is a native Wayland surface on the developer's own session,
# which no screenshot or input tool here can reach: `grim` is refused by
# the compositor and `xdotool` cannot see the surface. So the measurement
# gets a display of its own — the same shape as `daw::test::VirtualDisplay`
# (Xvfb + a window manager + xdotool + `import`), which is how the REAPER
# panels are already tested.
#
# WHAT THIS DISPLAY IS NOT: a GPU. Xvfb rasterizes in software, so
# absolute numbers here are not the numbers on the developer's screen.
# What it is good for is an A/B under a FIXED environment — both sides
# pay the same software-paint tax, so a difference between them is real.
# Say which of the two a number is before quoting it.
#
#   webview-display.sh run   [name]   # launch, scroll, report
#   webview-display.sh shot  [name]   # one screenshot of the live window
#   webview-display.sh stop           # tear the display down
set -euo pipefail

DISPLAY_NUM="${FTS_WEBVIEW_DISPLAY:-:99}"
GEOMETRY="${FTS_WEBVIEW_GEOMETRY:-1600x900x24}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${FTS_WEBVIEW_OUT:-$ROOT/target/webview-display}"
PIDS="$OUT/pids"

# Where the drum stack is, in the 1600x900 window: below the toolbar and
# the track switcher, right of the lane gutter. The wheel has to be over
# the pane it scrolls.
STACK_X="${FTS_WEBVIEW_STACK_X:-800}"
STACK_Y="${FTS_WEBVIEW_STACK_Y:-600}"

mkdir -p "$OUT" "$PIDS"

start_display() {
    if [[ -e "/tmp/.X${DISPLAY_NUM#:}-lock" ]]; then
        echo "display $DISPLAY_NUM is already up" >&2
        return
    fi
    Xvfb "$DISPLAY_NUM" -screen 0 "$GEOMETRY" -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
    echo $! > "$PIDS/xvfb"
    for _ in $(seq 50); do
        DISPLAY="$DISPLAY_NUM" xdotool getdisplaygeometry >/dev/null 2>&1 && break
        sleep 0.1
    done
    # A window manager, so the window is mapped, focused and sized. A bare
    # Xvfb leaves it unmanaged: keyboard input has nowhere to land and the
    # window may never be positioned at all.
    if command -v openbox >/dev/null 2>&1; then
        DISPLAY="$DISPLAY_NUM" openbox >"$OUT/wm.log" 2>&1 &
        echo $! > "$PIDS/wm"
        sleep 0.5
    else
        echo "note: no window manager on PATH; the window is unmanaged" >&2
    fi
}

stop() {
    for f in "$PIDS"/*; do
        [[ -e "$f" ]] || continue
        kill "$(cat "$f")" 2>/dev/null || true
        rm -f "$f"
    done
    rm -f "/tmp/.X${DISPLAY_NUM#:}-lock"
}

project() {
    # The reused staging, not a fresh 5.6 GB copy.
    cargo run --quiet -p expression-editor-standalone --example practice -- \
        --cached "${FTS_WEBVIEW_SONG:-set-in-stone}"
}

window_id() {
    DISPLAY="$DISPLAY_NUM" xdotool search --name "FastTrackStudio" 2>/dev/null | head -1
}

launch() {
    local name="$1" probe="$2" proj
    proj="$(project)"
    echo "project: $proj" >&2
    (
        export DISPLAY="$DISPLAY_NUM"
        # WRY is GTK: without this it would look for the developer's own
        # Wayland session and ignore the display we just made.
        export GDK_BACKEND=x11
        # HiDPI, when asked for. A scaled display is not a cosmetic
        # difference here: the engine rasterizes the stack's ~1900 SVG
        # elements at device pixels, so a 2x screen is four times the
        # paint for the same layout — and the developer's session is
        # scaled where this harness's Xvfb is not.
        [[ -n "${FTS_WEBVIEW_SCALE:-}" ]] && export GDK_SCALE="$FTS_WEBVIEW_SCALE"
        export FTS_DIOXUS_PROBE="$probe"
        export RUST_LOG="${RUST_LOG:-warn,expression_editor_ui::frame_meter=info}"
        exec "$ROOT/target/debug/examples/webview" \
            "$proj" --drums --size ${FTS_WEBVIEW_SIZE:-1600x900}
    ) >"$OUT/$name.log" 2>&1 &
    echo $! > "$PIDS/app-$name"

    for _ in $(seq 600); do
        [[ -n "$(window_id)" ]] && break
        sleep 0.2
    done
    local id
    id="$(window_id)"
    [[ -n "$id" ]] || { echo "no window appeared; see $OUT/$name.log" >&2; return 1; }
    echo "window: $id" >&2
    DISPLAY="$DISPLAY_NUM" xdotool windowactivate --sync "$id" 2>/dev/null || true
}

# Scroll the drum stack, then let the frame meter's window close.
#
# One wheel event per 16 ms, which is about what a real wheel or trackpad
# delivers and is deliberately NOT as fast as xdotool can send: a burst
# faster than frames measures the queue, not the view.
scroll() {
    local notches="${1:-180}" button="${2:-5}"
    export DISPLAY="$DISPLAY_NUM"
    # XTEST, not XSendEvent. `xdotool click --window <id>` sends a
    # synthetic event with `send_event` set, and GTK/WebKit drop those —
    # the run looks like it scrolled and the picture never moves, which
    # is the exact failure the screenshot pair below exists to catch.
    # Without `--window` xdotool drives the real pointer instead, so the
    # events are indistinguishable from a wheel.
    xdotool mousemove "$STACK_X" "$STACK_Y"
    sleep 0.3
    for _ in $(seq "$notches"); do
        xdotool click "$button"
        sleep 0.016
    done
    # The meter reports over a 500 ms window; give it one clear of the
    # gesture so the last reading is not half idle.
    sleep 0.7
}

# The `ui.fps` readings in a log, as `fps worst_ms` pairs.
#
# ANSI first, always: `tracing`'s fmt layer colours the field NAME, so
# the bytes on disk are `ui.fps<esc>[0m<esc>[2m=<esc>[0m62.1` and a
# pattern written against what the terminal shows matches nothing. That
# silence reads as "the gesture did nothing" rather than "the filter is
# wrong", which is a good hour if you let it be.
fps_of() {
    sed -E 's/\x1b\[[0-9;]*[a-zA-Z]//g' "$OUT/$1.log" \
      | sed -nE 's/.*ui\.fps=([0-9.]+) +ui\.worst_frame_ms=([0-9.]+).*/\1 \2/p'
}

# What a gesture cost, and only that gesture.
#
# The meter reports continuously, so a log read whole is mostly the
# window sitting still. This marks the log, drives the scroll, and reads
# back only what was measured while the pointer was working — then prints
# the shape of it, because the mean of a drag is the least interesting
# number in it. The floor is what the hand feels.
# Whether the drum stack has anything in it yet.
#
# The window opens on nothing and fills in behind itself — the project,
# the daw facade, the audio engine, then eighteen mics decoded to find
# the hits. Until that last step lands the stack is a flat dark
# rectangle, and a gesture measured against it measures nothing at all.
# Standard deviation over the pane says which it is without needing to
# know what the lanes look like: empty is uniform, loaded is not.
stack_ready() {
    local id; id="$(window_id)"
    [[ -n "$id" ]] || return 1
    local sigma
    sigma=$(DISPLAY="$DISPLAY_NUM" import -window "$id" -crop ${FTS_WEBVIEW_STACK_CROP:-900x380+60+470} +repage \
              -format '%[fx:standard_deviation]' info: 2>/dev/null || echo 0)
    # Measured on this pane: an empty stack reads 0.035, a loaded one
    # 0.181. The threshold sits between them with room on both sides.
    python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) > 0.09 else 1)" "$sigma"
}

# How long the window took to become usable, which is a headline number
# in its own right: the kit decode is the slowest thing in the process,
# so anything taxing every thread shows up here first and unambiguously.
time_to_stack() {
    local limit="${1:-300}" started elapsed
    started=$(date +%s)
    while (( $(date +%s) - started < limit )); do
        if stack_ready; then
            elapsed=$(( $(date +%s) - started ))
            echo "$elapsed"
            return 0
        fi
        sleep 2
    done
    echo "timeout"
    return 1
}

# What a scroll cost, in the terms the hand judges it by.
#
# Three numbers, because the obvious one lies here:
#
#  travelled  — pixels the picture actually changed over the gesture. A
#               run that moved nothing measured an idle window however
#               good its frame rate looked, and the drum camera clamps at
#               both ends, so this goes down AND back up: from either
#               limit one of the two directions still has somewhere to
#               go. `GUIDE.md` is emphatic about reading this first.
#  backlog    — pixels that kept changing for two seconds AFTER the last
#               notch. This is the real symptom of the pane being slower
#               than the wheel: input queues, and the view goes on
#               travelling to somewhere the hand left long ago. Zero is
#               the goal; anything large means the gesture is running
#               open-loop.
#  fps        — the page's own rAF rate. Reported LAST and trusted least:
#               it measures the engine's presentation loop, which stays
#               happy while the Rust side is too backed up to send it any
#               mutations. A high number here next to a large backlog
#               means the UI is unresponsive, not fast.
measure() {
    local name="${1:-run}" notches="${2:-150}"
    local shots="$OUT/shots"; mkdir -p "$shots"
    local id since started
    id="$(window_id)"
    export DISPLAY="$DISPLAY_NUM"

    since=$(date -u +%Y-%m-%dT%H:%M:%S)
    import -window "$id" "$shots/$name-0.png"
    started=$(date +%s.%N)
    # Vertical wheel by default (buttons 5/4); horizontal (7/6) with
    # FTS_WEBVIEW_AXIS=h. They are not the same measurement: a vertical
    # scroll moves the lane stack, while a horizontal one moves the TIME
    # camera — which changes what is in view and so regenerates every
    # lane's waveform, overlay and hit from scratch.
    local fwd=5 back=4
    if [[ "${FTS_WEBVIEW_AXIS:-v}" == "h" ]]; then fwd=7; back=6; fi
    scroll "$((notches / 2))" "$fwd"
    # The midpoint is where travel is measured. Down-and-back ends where
    # it started, so a start-to-end diff of a symmetric gesture is zero
    # whether the pane kept up perfectly or never moved at all.
    import -window "$id" "$shots/$name-mid.png"
    scroll "$((notches / 2))" "$back"
    local elapsed
    elapsed=$(echo "$(date +%s.%N) - $started" | bc)
    import -window "$id" "$shots/$name-1.png"
    sleep 2
    import -window "$id" "$shots/$name-2.png"

    local travelled backlog
    # `compare` exits nonzero whenever the images differ, which under
    # `set -e` is every run that worked.
    # Both legs, and the larger of them. The camera clamps at each end,
    # so whichever direction the window was already sitting against
    # moves nothing — reporting only one leg makes a clamped edge look
    # exactly like a frozen pane.
    local down up
    down=$( { compare -metric AE "$shots/$name-0.png" "$shots/$name-mid.png" null: 2>&1 || true; } | cut -d' ' -f1)
    up=$( { compare -metric AE "$shots/$name-mid.png" "$shots/$name-1.png" null: 2>&1 || true; } | cut -d' ' -f1)
    travelled=$(python3 -c "import sys; print(int(max(float(sys.argv[1]), float(sys.argv[2]))))" "$down" "$up")
    backlog=$( { compare -metric AE "$shots/$name-1.png" "$shots/$name-2.png" null: 2>&1 || true; } | cut -d' ' -f1)
    printf '%-12s %d notches in %.1fs   travelled %s px   backlog %s px\n' \
        "$name" "$notches" "$elapsed" "$travelled" "$backlog"
    python3 "$ROOT/scripts/ui-stress/webview-fps.py" "$name" "$since" \
        "$OUT/$name.log" "$ROOT/target/dioxus-mcp/events*.jsonl" || true
}

case "${1:-run}" in
  run)
    name="${2:-run}"
    start_display
    launch "$name" "${FTS_DIOXUS_PROBE:-0}"
    # The kit decodes on a thread behind the window; scrolling an empty
    # stack measures nothing. Wait for lanes to exist — and report how
    # long that took, because it is the least ambiguous number here.
    ready=$(time_to_stack "${FTS_WEBVIEW_WARMUP:-300}")
    echo "$name: stack ready in ${ready}s"
    if [[ "$ready" == "timeout" ]]; then
        echo "$name: never loaded; not measuring an empty pane" >&2
        exit 1
    fi
    measure "$name" "${FTS_WEBVIEW_NOTCHES:-150}"
    ;;
  shot)
    DISPLAY="$DISPLAY_NUM" import -window "$(window_id)" "$OUT/${2:-shot}.png"
    echo "$OUT/${2:-shot}.png"
    ;;
  scroll)  scroll "${2:-180}" "${3:-5}" ;;
  measure) measure "${2:-run}" "${3:-150}" ;;
  fps)    fps_of "${2:-run}" ;;
  stop)   stop ;;
  *) echo "usage: webview-display.sh {run|measure|shot|scroll|fps|stop} [name]" >&2; exit 2 ;;
esac
