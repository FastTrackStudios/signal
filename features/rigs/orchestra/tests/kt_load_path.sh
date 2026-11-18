#!/bin/bash
# kt_load_path.sh <nki or nkm path> <out.rpp>
#
# Load an NKI into the Kontakt instance of the REAPER project that is open (FX window floating at its
# default spot), save the project and keep a copy at <out.rpp>. Works for any library: no browser rows.
#
#   1. KONTAKT menu > Reset multi, OK            (an instrument must be loaded: the menu then has
#                                                  "Save as", which puts Reset multi at 326,393)
#   2. KONTAKT menu > Load… → native open panel → ⌘⇧G, then the path, Return, Return
#   3. wait KT_WAIT s, click the arrange area (⌘S only reaches REAPER from there), ⌘S, copy
#
# The path is typed through System Events, where macOS turns a double space into ". " (Damage has
# "PERC  Studio Kicks.nki"): so a hard link `_ktload.nki` is made next to the NKI (same folder, so its
# relative sample paths still resolve) and removed afterwards. The folder path must not hold a double
# space. Kontakt then names the instrument "_ktload"; that does not change what it plays.
#
# Coordinates are desktop POINTS; cua-driver maps desktop clicks through the latest capture, so every
# run starts with a capture at 1800 px (voyager's screen is 1800 pt wide). Overrides: KT_MENU, KT_RESET,
# KT_RESET_OK, KT_LOAD ("x y"). KT_RPP = the open project file. Escape is pressed only while the open panel
# exists (cancel_panel): with the FX window focused it closes the window and later keys go to
# REAPER's main window. Exit codes: 2 the panel did not open, 3 it did not take the path.
set -euo pipefail
export PATH=$PATH:~/.local/bin
nki=$1 out=$2
read -r MX MY <<<"${KT_MENU:-355 170}"
read -r RX RY <<<"${KT_RESET:-326 393}"
read -r OX OY <<<"${KT_RESET_OK:-958 482}"
read -r LX LY <<<"${KT_LOAD:-314 257}"
rpp=${KT_RPP:?set KT_RPP to the open project file}
D='"session":"kt","delivery_mode":"foreground","target":{"kind":"desktop","display_id":"primary"}'
click() { cua-driver call click "{$D,\"x\":$1,\"y\":$2}" >/dev/null; }
shot() { cua-driver call get_desktop_state "{\"session\":\"kt\",\"max_image_dimension\":1800,\"screenshot_out_file\":\"/tmp/kt_$1.png\"}" >/dev/null || true; }
osa() { osascript -e "tell application \"System Events\" to $1"; }
# the native open panel shows as a REAPER window named "Load"
panel_open() { osascript -e 'tell application "System Events" to get name of every window of process "REAPER"' | tr ',' '\n' | grep -qx ' *Load *'; }
wait_panel() { local want=$1 _; for _ in $(seq 20); do if panel_open; then [ "$want" = open ] && return 0; else [ "$want" = closed ] && return 0; fi; sleep 0.5; done; return 1; }
# Escape only while the panel holds focus (never with the FX window focused)
cancel_panel() { local _; for _ in 1 2 3; do panel_open || return 0; osa 'key code 53'; sleep 1.2; done; }
[ -f "$nki" ] || { echo "no such NKI: $nki" >&2; exit 1; }
dir=$(dirname "$nki")
case "$dir" in *"  "*) echo "folder has a double space, cannot type it: $dir" >&2; exit 1 ;; esac
# keep the extension: Kontakt loads a .nkm multi (every slot) as well as an .nki
ext="${nki##*.}"
link="$dir/_ktload.$ext"
ln -f "$nki" "$link"
trap 'rm -f "$link"' EXIT
shot start
# 1. reset the multi
click "$MX" "$MY"; sleep 1.2; click "$RX" "$RY"; sleep 1.5; click "$OX" "$OY"; sleep 2
# 2. load through the open panel
click "$MX" "$MY"; sleep 1.2; click "$LX" "$LY"
wait_panel open || { shot nopanel; echo "the Load panel did not open" >&2; exit 2; }
sleep 1; shot panel
osa 'keystroke "g" using {command down, shift down}'; sleep 1.2
# clear what the go-to field remembers (its sheet moves with the panel's content: no click)
osascript -e 'tell application "System Events"' -e 'repeat 300 times' -e 'key code 51' -e 'end repeat' -e 'end tell'
osa "keystroke \"$link\""; sleep 1.2
osa 'key code 36'; sleep 1.5; osa 'key code 36'
if ! wait_panel closed; then shot stuck; cancel_panel; echo "the panel did not take the path" >&2; exit 3; fi
# 3. wait for the samples, save, keep a copy
sleep "${KT_WAIT:-25}"; shot loaded
click 1000 1000; sleep 0.5
osa 'keystroke "s" using command down'; sleep 6
cp "$rpp" "$out"; ls -la "$out"
