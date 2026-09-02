#!/usr/bin/env bash
# Gain-reduction captures for the compressor fleet.
#
# The saturation fleet measures what these units *add*. This measures what
# they do to the gain — the attack corner, the settled depth, the release
# shape, and how all three move with frequency. It is the half a control-plane
# mapping is fitted against: an 1176's Attack knob is only mapped correctly if
# the measured corner lands where the mapping says it should.
#
# Panels have nothing in common, so every sweep is by control *name* and any
# control a unit does not have is written `-` and skipped. An LA-2A has no
# attack, release or ratio — it has Peak Reduction and a time constant baked
# into the optical cell — and asking for one would measure nothing.
#
# NOTE: the manifest below is a single-quoted string. No apostrophes.
#
#   ./capture-comp.sh            # every unit
#   ./capture-comp.sh 1176       # units matching a pattern
set -uo pipefail

BIN=./target/release/examples/comp_capture

# Single-threaded, deliberately.
#
# UADx plugins cannot be rendered as concurrent instances. Sweeping the
# 1176's Input across four values gave a clean monotonic curve on one thread
# (-11.36, -5.90, -2.26, -0.87 dB) and railed three of the four at -48 dB on
# ten. Nothing in the output says so: a railed capture is a well-formed file
# full of plausible-looking numbers, and the first run of this fleet produced
# 15 units of them.
#
# FabFilter tolerates the concurrency, which is why the Pro-C matrix was fine
# and this went unnoticed. The cost is real — the fleet goes from 7 minutes to
# roughly an hour — and it is the difference between data and noise.
THREADS="${THREADS:-1}"
VST=/Library/Audio/Plug-Ins/VST3
ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"
[ -d "$ARCHIVE" ] || ARCHIVE="captures"
FILTER="${1:-}"
START=$(date +%s)

# vst3 file | display name | drive | attack | release | ratio
FLEET='
uaudio_ua_1176ln_rev_e|UADx 1176LN Rev E|Input|Attack|Release|Ratio
uaudio_ua_1176_rev_a|UADx 1176 Rev A|Input|Attack|Release|Ratio
uaudio_ua_1176ae|UADx 1176AE|Input|Attack|Release|Ratio
uaudio_teletronix_la-2a_gray|UADx LA-2A Gray|Peak Reduct|-|-|-
uaudio_teletronix_la-2a_silver|UADx LA-2A Silver|Peak Reduct|-|-|-
uaudio_teletronix_la-2|UADx LA-2|Peak Reduct|-|-|-
uaudio_la3a|UADx LA-3A|Peak Reduction|-|-|-
uaudio_fairchild_660|UADx Fairchild 660|Input|-|-|Time Const
uaudio_dbx_160|UADx dbx 160|Thresh|-|-|Compress
uaudio_distressor|UADx Distressor|Input|Attack|Release|Ratio
uaudio_api_2500|UADx API 2500|Threshold|Attack|Release|Ratio
uaudio_capitol_compressor|UADx Capitol Mastering Compressor|L Input|L Attack|L Release|L Ratio
uaudio_175_b|UADx UA 175-B|Input|Attack|Release|-
uaudio_176|UADx UA 176|Input|Attack|Release|Ratio
SSL Native Bus Compressor 2|SSL Native Bus Compressor 2|Threshold|Attack|Release|Ratio
'

TOTAL=$(echo "$FLEET" | grep -c '|')
INDEX=0
echo "$FLEET" | while IFS='|' read -r file name drive atk rel ratio; do
  [ -z "$file" ] && continue
  INDEX=$((INDEX + 1))
  if [ -n "$FILTER" ]; then
    case "$(echo "$name" | tr '[:upper:]' '[:lower:]')" in
      *"$(echo "$FILTER" | tr '[:upper:]' '[:lower:]')"*) ;;
      *) continue ;;
    esac
  fi
  [ -e "$VST/$file.vst3" ] || { echo "── [$INDEX/$TOTAL] $name — not installed"; continue; }

  OUT="$ARCHIVE/$name/captures"
  mkdir -p "$OUT"
  echo "── [$INDEX/$TOTAL] $name"

  run() {
    local job="$1"; shift
    [ -f "$OUT/$job/metadata.json" ] && { echo "   $job — already captured"; return; }
    $BIN --plugin "$VST/$file.vst3" --out "$OUT/$job" --threads "$THREADS" "$@" \
        > "$OUT/${job}.log" 2>&1 \
      && echo "   $job: $(ls "$OUT/$job"/*.bin 2>/dev/null | wc -l | tr -d ' ') scenarios" \
      || echo "   $job FAILED — see $OUT/${job}.log"
  }

  # The static curve. Most of these have no threshold control: the input knob
  # sets how hard the detector is hit, so sweeping it IS the static sweep.
  run static --sweep "$drive=0..1:16"

  # Time constants, at a drive deep enough to be compressing.
  if [ "$atk" != "-" ] && [ "$rel" != "-" ]; then
    run timing --sweep "$atk=0..1:8;$rel=0..1:8" --set "$drive=0.7"
  elif [ "$rel" != "-" ]; then
    run timing --sweep "$rel=0..1:12" --set "$drive=0.7"
  fi

  # Ratio (or whatever selector stands in for it) against drive, because on a
  # unit whose input knob also sets the threshold the two interact.
  if [ "$ratio" != "-" ]; then
    run ratio --sweep "$ratio=0..1:11;$drive=0..1:6"
  fi

  NOW=$(date +%s); SPENT=$((NOW - START))
  [ "$INDEX" -lt "$TOTAL" ] && printf -- "   fleet %d/%d · elapsed %dm%02ds\n" \
    "$INDEX" "$TOTAL" $((SPENT/60)) $((SPENT%60))
done
ELAPSED=$(( $(date +%s) - START ))
printf -- "── all done in %dm%02ds\n" $((ELAPSED/60)) $((ELAPSED%60))
