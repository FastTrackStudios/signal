#!/usr/bin/env bash
# perc_null.sh <library> <tag> — one percussion instrument, KONTRA vs seeded Kontakt, end to end.
#
# <library>: Damage | STRIKEFORCE | "Symphony Series Percussion"; <tag> a row of its
# _notes/load_list.tsv (tag, NKI path under Instruments/), whose chain kontakt_load_batch.py saved
# as target/orchestra-ab/chains/fxchain_<tag>.txt.
# Steps, into $WORK/<tag> (default /tmp/kontra): perc_null_probe.py (keys found by KONTRA), the
# Kontakt reference on voyager (kontakt_ab_render.sh with KSP_SEED, one at a time under a lock),
# KONTRA's render (KONTRA_KSP_SEED, block 512), null_test.py. Last line: "<tag>: <null summary>".
# A step whose output exists is skipped (FORCE=1 redoes the KONTRA render and null).
# KONTRA=<kontra-inspect>, SEED=<n> (777), NPY_PYTHON=<python with numpy>.
set -euo pipefail
lib=$1 tag=$2
T=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$T/../../../.." && pwd)
KB=${KONTRA:-/run/media/Development/KONTRA-perc/target/release/kontra-inspect}
SEED=${SEED:-777}
# Kontakt's instrument had run long before the render: KONTRA settles 2 s before frame 0 too
export KONTRA_INSPECT_PREROLL=${KONTRA_INSPECT_PREROLL:-96000}
NPY=${NPY_PYTHON:-$(cat /tmp/kstate-tools/npy_python)}
case $lib in
  "Symphony Series Percussion") inst="/run/media/AudioHaven/Kontakt/Orchestral/Percussion/Symphony Series Percussion Library/Instruments" ;;
  *) inst="/run/media/AudioHaven/Kontakt/Orchestral/Percussion/$lib/Instruments" ;;
esac
data="/run/media/AudioHaven/Sampled/Percussion/$lib"
# instruments in load_list.tsv; multis (.nkm, every slot summed) in multi_list.tsv
rel=$(cat "$data/_notes/load_list.tsv" "$data/_notes/multi_list.tsv" 2>/dev/null | awk -F'\t' -v t="$tag" '$1==t {print $2}')
[ -n "$rel" ] || { echo "$tag: not in $data/_notes/{load,multi}_list.tsv" >&2; exit 2; }
case $rel in
  *.nkm) nki="$(dirname "$inst")/$rel" ksp=- ;;
  *) nki="$inst/$rel" ksp="$data/_ksp/${rel%.nki}" ;;
esac
chain="$ROOT/target/orchestra-ab/chains/fxchain_$tag.txt"
[ -f "$chain" ] || { echo "$tag: no chain $chain" >&2; exit 2; }
w="${WORK:-/tmp/kontra}/$tag"
mkdir -p "$w"
cd "$w"
[ -f manifest.tsv ] || timeout 3600 "$NPY" -I "$T/perc_null_probe.py" "$nki" "$ksp" . --kontra "$KB" >probe.log 2>&1 \
  || { echo "$tag: probe failed ($(tail -1 probe.log))"; exit 1; }
if [ ! -s "ref$SEED/t1.wav" ]; then
  # One REAPER on voyager at a time.
  ( flock 9; KSP_SEED=$SEED timeout 3000 "$T/kontakt_ab_render.sh" "ref$SEED" probe.mid "$chain" >ref.log 2>&1 ) 9>/tmp/kontra/.voyager.lock \
    || { echo "$tag: reference render failed ($(tail -1 ref.log))"; exit 1; }
fi
if [ ! -s k24.wav ] || [ -n "${FORCE:-}" ]; then
  KONTRA_INSPECT_BLOCK=512 KONTRA_KSP_SEED=$SEED KONTRA_INSPECT_WAV="$w/k.wav" \
    timeout 5400 "$KB" play "$nki" --scenario scn.json --json out.json >kontra.log 2>&1 || true
  [ -s k.wav ] || { echo "$tag: KONTRA render failed ($(tail -1 kontra.log))"; exit 1; }
  "$NPY" -I "$T/analysis/wav_to_pcm24.py" k.wav k24.wav >/dev/null && rm -f k.wav
fi
line=$("$NPY" -I "$T/null_test.py" manifest.tsv k24.wav "ref$SEED/t1.wav" --json null.json | tail -1)
echo "$tag: $line"
# CLEAN=1: drop KONTRA's render and its JSON (both made again in minutes); keep Kontakt's
# reference only for an instrument that does not null on every unit.
if [ -n "${CLEAN:-}" ]; then
  rm -f k24.wav out.json
  # keep the reference while there is something to look at: a miss, or a median above -95 dB
  if [[ $line =~ median\ +(-?[0-9.]+)\ dB.*NULLED\ \(\<=\ -60\ dB\)\ ([0-9]+)/([0-9]+) ]] \
    && [ "${BASH_REMATCH[2]}" = "${BASH_REMATCH[3]}" ] \
    && awk -v m="${BASH_REMATCH[1]}" 'BEGIN { exit !(m <= -95) }'; then
    rm -rf "ref$SEED"
  fi
fi
