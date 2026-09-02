#!/usr/bin/env bash
# Scan every unit's controls to find out what each one actually does.
#
# This replaces hand-picking a drive control per plugin, which was wrong on
# three of sixteen units in the first fleet run and wrong silently — the
# resulting captures looked perfect. The scan sweeps every front-panel
# control, reports which are selectors (and their states) and which move the
# distortion, and ranks them. The measurement plan is then built from data
# rather than from someone's guess.
#
# NOTE: the manifest below is a single-quoted string. No apostrophes.
set -uo pipefail

BIN=./target/release/examples/param_scan
VST=/Library/Audio/Plug-Ins/VST3
ARCHIVE="${PLUGIN_ANALYSIS_ROOT:-/run/media/AudioHaven/Plugin Analysis}"
[ -d "$ARCHIVE" ] || ARCHIVE="captures"
FILTER="${1:-}"
# FORCE=1 re-scans units that are already complete.
FORCE="${FORCE:-}"
FLEET_START=$(date +%s)

# UADx exposes ~2091 parameters and the first eight are the front panel.
# SSL exposes 12, all real, so it needs no restriction.
# plugin file | display name | selector args
FLEET='
FabFilter Pro-Q 4|FabFilter Pro-Q 4|--ids 550-562
FabFilter Pro-C 3|FabFilter Pro-C 3|--first 20
Soundtoys/Decapitator|Soundtoys Decapitator|--first 8
Soundtoys/Devil-Loc_Deluxe|Soundtoys Devil-Loc Deluxe|--first 6
Spicerack|Process Audio Spicerack|
uaudio_ua_1176ln_rev_e|UADx 1176LN Rev E|--first 8
uaudio_ua_1176_rev_a|UADx 1176 Rev A|--first 8
uaudio_ua_1176ae|UADx 1176AE|--first 8
uaudio_teletronix_la-2a_gray|UADx LA-2A Gray|--first 8
uaudio_teletronix_la-2a_silver|UADx LA-2A Silver|--first 8
uaudio_teletronix_la-2|UADx LA-2|--first 8
uaudio_la3a|UADx LA-3A|--first 8
uaudio_fairchild_660|UADx Fairchild 660|--first 8
uaudio_dbx_160|UADx dbx 160|--first 8
uaudio_distressor|UADx Distressor|--first 8
uaudio_api_2500|UADx API 2500|--first 8
uaudio_175_b|UADx UA 175-B|--first 8
uaudio_176|UADx UA 176|--first 8
SSL Native Bus Compressor 2|SSL Native Bus Compressor 2|
uaudio_pultec_eqp-1a|UADx Pultec EQP-1A|--first 12
uaudio_pultec_meq-5|UADx Pultec MEQ-5|--first 12
uaudio_pultec_hlf-3c|UADx Pultec HLF-3C|--first 12
uaudio_manley_massive_passive|UADx Manley Massive Passive|--first 40
uaudio_manley_massive_passive_m|UADx Manley Massive Passive MST|--first 40
uaudio_neve_1073|UADx Neve 1073|--first 12
uaudio_hitsville_eq|UADx Hitsville EQ|--first 12
uaudio_hitsville_eq_mastering|UADx Hitsville EQ Mastering|--first 24
uaudio_avalon_vt-737sp|UADx Avalon VT-737sp|--first 16
uaudio_api_vision_channel_strip|UADx API Vision Channel Strip|--first 16
uaudio_century_channel_strip|UADx Century Channel Strip|--first 16
uaudio_manley_voxbox|UADx Manley VOXBOX|--first 16
uaudio_manley_preamp|UADx Manley Preamp|--first 8
uaudio_ampex_atr-102_tape|UADx Ampex ATR-102|--first 12
uaudio_studer_a800|UADx Studer A800|--first 12
uaudio_oxide_tape|UADx Oxide Tape|--first 8
uaudio_fairchild_670|UADx Fairchild 670|--first 8
uaudio_capitol_compressor|UADx Capitol Mastering Compressor|--first 12
'

TOTAL=$(echo "$FLEET" | grep -c '|')
INDEX=0
echo "$FLEET" | while IFS='|' read -r file name sel; do
  [ -z "$file" ] && continue
  INDEX=$((INDEX + 1))
  [ -n "$FILTER" ] && case "$file" in *"$FILTER"*) ;; *) continue ;; esac
  [ -e "$VST/$file.vst3" ] || { echo "SKIP  $name — not installed"; continue; }
  out="$ARCHIVE/$name/scan"
  mkdir -p "$out"

  # Resume at the fleet level too: a unit whose scan is marked complete is
  # skipped outright. Partial scans are handed to param_scan, which resumes
  # them control by control from its own checkpoint.
  if [ -z "$FORCE" ] && grep -q '"complete": true' "$out/scan.json" 2>/dev/null; then
    echo "── [$INDEX/$TOTAL] $name — already complete, skipping"
    continue
  fi
  echo "── [$INDEX/$TOTAL] $name"
  # tee, not plain redirection: a scan takes minutes per unit and printing
  # only the ranking at the end makes a working run indistinguishable from a
  # hung one. The plugin's own logging is dropped so the progress lines stay
  # readable.
  # shellcheck disable=SC2086
  # FORCE has to reach param_scan too. Without --no-resume it happily
  # resumes from its own per-control checkpoint and returns the cached
  # result instantly — a "forced" re-scan of 37 units that re-measured
  # nothing and reported success.
  RESUME=""
  [ -n "$FORCE" ] && RESUME="--no-resume"
  # shellcheck disable=SC2086
  $BIN --plugin "$VST/$file.vst3" $sel $RESUME --out "$out/scan.json" 2>&1 \
      | tee "$out/scan.log" \
      | grep --line-buffered -E "^  \[|^scanned" | sed 's/^/  /'
  if [ -f "$out/scan.json" ]; then
    grep -A 6 "drive candidates" "$out/scan.log" | tail -5 | sed 's/^/   /'
  else
    echo "   FAILED — see $out/scan.log"
  fi
  # Fleet-level ETA from units actually measured this run.
  NOW=$(date +%s)
  SPENT=$(( NOW - FLEET_START ))
  if [ "$INDEX" -gt 0 ] && [ "$INDEX" -lt "$TOTAL" ]; then
    REMAIN=$(( SPENT * (TOTAL - INDEX) / INDEX ))
    printf -- "   fleet %d/%d · elapsed %dm%02ds · ETA %dm%02ds\n" \
      "$INDEX" "$TOTAL" $((SPENT / 60)) $((SPENT % 60)) $((REMAIN / 60)) $((REMAIN % 60))
  fi
done
ELAPSED=$(( $(date +%s) - FLEET_START ))
printf -- "── done in %dm%02ds\n" $((ELAPSED / 60)) $((ELAPSED % 60))
