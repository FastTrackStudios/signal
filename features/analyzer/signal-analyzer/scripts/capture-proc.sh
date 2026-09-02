#!/usr/bin/env bash
# Capture the full Pro-C measurement matrix — everything needed to fit a
# compressor model against the real plugin.
#
# The presets say what the plugin does at the points its designers chose,
# which validates a conversion but cannot separate one control's effect from
# another's. These grids move one axis at a time, which is what a fit needs.
#
# Runs at roughly 1800x realtime, so the whole matrix is minutes rather than
# hours: ~2000 scenarios for Pro-C 2 and ~3100 for Pro-C 3.
#
#   ./capture-proc.sh 2 /Library/Audio/Plug-Ins/CLAP/"FabFilter Pro-C 2.clap"
#
# Output goes to the plugin-analysis archive when it is mounted, and to a
# local `captures/` otherwise — which is the normal case on a measuring
# machine that is not the one holding the archive. Copy the result into the
# archive under the same layout when the run is done.
#
set -euo pipefail

VERSION="${1:?usage: capture-proc.sh <2|3> <plugin path> [out dir]}"
PLUGIN="${2:?usage: capture-proc.sh <2|3> <plugin path> [out dir]}"

# One directory per plugin, named as the plugin is, so the archive stays
# readable as more plugins are measured over time.
NAME="FabFilter Pro-C $VERSION"
ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"
if [ -n "${3:-}" ]; then
  OUT="$3"
elif [ -d "$ARCHIVE" ]; then
  OUT="$ARCHIVE/$NAME/captures"
else
  OUT="captures/$NAME"
fi
mkdir -p "$OUT"
BIN=./target/release/examples/comp_capture
THREADS="${THREADS:-10}"

# Auto Gain defaults to ON in Pro-C, and it adds makeup that moves with
# threshold and ratio. Left enabled, every static-curve sweep below would
# measure compression *plus* automatic makeup — a curve that looks entirely
# reasonable and is not the one being modelled. Held off for the whole matrix
# so the gain read back is the compressor's own.
PIN="Auto Gain=0"

# Pro-C 2 has eight detector styles, Pro-C 3 fourteen. The style is the
# single most important axis: it selects the detector model outright, so a
# timing law fitted across styles is fitted to an average of several
# different compressors.
if [ "$VERSION" = "2" ]; then STYLE_MAX=7; else STYLE_MAX=13; fi
STYLES=$((STYLE_MAX + 1))

run() {
  local name="$1"; shift
  local spec="$1"; shift
  echo "── $name"
  $BIN --plugin "$PLUGIN" --sweep "$spec" --out "$OUT/$name" \
       --threads "$THREADS" --set "$PIN" "$@" > "/tmp/cap-${VERSION}-${name}.log" 2>&1 || {
         echo "   FAILED — see /tmp/cap-${VERSION}-${name}.log"; return 0; }
  echo "   $(ls "$OUT/$name"/*.bin 2>/dev/null | wc -l | tr -d ' ') scenarios"
}

# Timing: the attack/release plane at the default style.
run timing "Attack=0..1:14;Release=0..1:14"

# Timing per style — coarser on each axis, because the point is how the
# *shape* changes with style, not another 196 points of one style.
run style-timing "Style=0..${STYLE_MAX}:${STYLES};Attack=0..1:8;Release=0..1:8"

# The static curve. Sweeping threshold against a fixed input level traces
# the same curve a level sweep would, and threshold is a parameter, so it
# fits in the grid.
# Knee pinned to zero here: the default 18 dB knee spreads the threshold
# corner over a third of the sweep, which is exactly the feature being
# measured. Knee gets its own sweep below.
run static "Threshold=-60..0:16;Ratio=0..1:12" --set "Auto Gain=0;Knee=0"
run knee   "Knee=0..72:12;Ratio=0..1:12"
run style-static "Style=0..${STYLE_MAX}:${STYLES};Threshold=-60..0:8;Ratio=0..1:8" \
    --set "Auto Gain=0;Knee=0"

# The same static sweep 12 dB quieter. The curve should depend only on
# (input − threshold); if these two disagree the model needs a level term,
# and that is worth knowing before fitting one that cannot express it.
run static-quiet "Threshold=-60..0:16;Ratio=0..1:12" --gain-high -18 --gain-low -32 \
    --set "Auto Gain=0;Knee=0"

# Program dependence and the remaining envelope controls.
run autorelease "Auto Release=0,1;Attack=0..1:10;Release=0..1:10"
run hold        "Hold=0..1:10;Release=0..1:12"
run range       "Range=4.5..60:10;Threshold=-60..0:10"

# Pro-C 3 only: the saturation stage and auto-threshold, both of which the
# converter currently maps by judgement rather than measurement.
if [ "$VERSION" = "3" ]; then
  run character     "Character=0..3:4;Character Drive=-24..24:9;Threshold=-48..0:6"
  run autothreshold "Auto Threshold=0,1;Threshold=-60..0:8;Ratio=0..1:8"
fi

echo "── done: $(du -sh "$OUT" | cut -f1) in $OUT"
echo "   presets are a separate run: comp_capture --presets <library dir> --out \"$OUT/presets\""
