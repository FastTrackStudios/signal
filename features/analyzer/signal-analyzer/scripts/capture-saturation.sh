#!/usr/bin/env bash
# Saturation captures for the vintage-compressor fleet.
#
# For each unit: sweep its own drive control across settings chosen for even
# coverage in THD (not in knob position), and at each one measure the
# harmonic series and the static transfer curve. What comes out is the
# saturation as a family of curves indexed by drive, with gain divided out —
# the thing a 2-D waveshaper is built from.
#
# The drive control is per-unit and is not always called Input; the manifest
# below records which knob actually drives the nonlinearity on each.
#
#   ./capture-saturation.sh                 # the whole fleet
#   ./capture-saturation.sh 1176            # just units matching a pattern
#
set -uo pipefail

BIN=./target/release/examples/saturation_capture
VST=/Library/Audio/Plug-Ins/VST3
ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"
[ -d "$ARCHIVE" ] || ARCHIVE="captures"

FREQS="${FREQS:-100,1000,5000}"
LEVELS="${LEVELS:--30,-24,-18,-12,-6,0}"
DRIVE_STEPS="${DRIVE_STEPS:-8}"
FILTER="${1:-}"

# The Fairchild 670 is this same circuit in stereo and is deliberately absent:
# its controls are per-channel, so driving only L gave a THD span of 2.2x
# against the 660 mono unit at 14x — a measurement artefact, not a difference
# between the units. Measure the 660.
#
# NOTE: the manifest below is a single-quoted string. No apostrophes.
#
# plugin file | display name | drive control
#
# The drive control is whatever feeds the nonlinearity, which is not always
# the knob called Gain: on the LA-3A, dbx 160 and SSL the Gain/Makeup control
# sits *after* the gain element and is a clean trim, so sweeping it produced
# a flat THD span and told us nothing. Stereo units (Fairchild 670, Capitol)
# name their controls per channel.
FLEET='
uaudio_ua_1176ln_rev_e|UADx 1176LN Rev E|Input
uaudio_ua_1176_rev_a|UADx 1176 Rev A|Input
uaudio_ua_1176ae|UADx 1176AE|Input
uaudio_teletronix_la-2a_gray|UADx LA-2A Gray|Gain
uaudio_teletronix_la-2a_silver|UADx LA-2A Silver|Gain
uaudio_teletronix_la-2|UADx LA-2|Gain
uaudio_la3a|UADx LA-3A|Peak Reduction
uaudio_fairchild_660|UADx Fairchild 660|Input
uaudio_dbx_160|UADx dbx 160|Thresh
uaudio_distressor|UADx Distressor|Input
uaudio_api_2500|UADx API 2500|Threshold
uaudio_capitol_compressor|UADx Capitol Mastering Compressor|L Input
uaudio_175_b|UADx UA 175-B|Gain
uaudio_176|UADx UA 176|Input
SSL Native Bus Compressor 2|SSL Native Bus Compressor 2|Threshold
'

echo "$FLEET" | while IFS='|' read -r file name drive; do
  [ -z "$file" ] && continue
  [ -n "$FILTER" ] && case "$file" in *"$FILTER"*) ;; *) continue ;; esac
  # -e, not -f: a VST3 bundle on macOS is a directory.
  [ -e "$VST/$file.vst3" ] || { echo "SKIP  $name — not installed"; continue; }

  out="$ARCHIVE/$name/saturation"
  mkdir -p "$out"
  echo "── $name (drive: $drive)"
  $BIN --plugin "$VST/$file.vst3" --out "$out" \
       --drive-param "$drive" --drive-steps "$DRIVE_STEPS" \
       --freqs "$FREQS" --levels "$LEVELS" --no-sweep \
       > "$out/capture.log" 2>&1
  if [ -f "$out/saturation.json" ]; then
    grep -E "^   chose|WARNING" "$out/capture.log" | sed 's/^/   /'
  else
    echo "   FAILED — see $out/capture.log"
  fi
done
echo "── done"
