#!/usr/bin/env bash
#
# Build the drum corpus the quantize scenario tunes against.
#
# Nothing this script produces is committed. It writes to a cache
# directory outside the tree ($CORPUS_DIR, default
# ~/.cache/fts/drum-corpus) and prints where. The repository holds this
# script, the `drum-corpus` tool it drives, and small fixtures the tool
# generates — no audio.
#
# LICENCE, which is the constraint that shapes this whole thing:
#
#   DrumGizmo kits    CC BY 4.0. Commercial use fine. Anything derived
#                     from them that ships must carry the attribution
#                     "Drum samples provided by DrumGizmo.org".
#   E-GMD MIDI        CC BY 4.0.
#   ENST-Drums        CC BY-NC-ND 4.0, "no commercial use is possible".
#                     Internal evaluation ONLY: never vendored, never
#                     shipped, nothing derived from it in a release
#                     asset. This script will not fetch it without
#                     --i-accept-nc-nd.
#   drumgizmo CLI     GPL. A build-time tool that is *invoked*. It never
#                     enters this tree and nothing links against it.
#
# See README.md, and features/expression-editor/spec/research/drum-datasets.md.

set -euo pipefail

CORPUS_DIR="${CORPUS_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/fts/drum-corpus}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"

CROCELL_URL="https://drumgizmo.org/kits/CrocellKit/CrocellKit1_1.zip"
CROCELL_MD5="fa2be0f847bcd8ddef3830c1523690d3"
# Verified by reading the WAV headers out of the zip: 48 kHz,
# 32-bit IEEE float, 15 channels per file. Neither the wiki page nor
# any DrumGizmo page states this.
CROCELL_RATE=48000

DRS_URL="https://drumgizmo.org/kits/DRSKit/DRSKit2_1.zip"
DRS_MD5="8c4d4b61ad9d354b3b845edd5da9c133"
# 44.1 kHz, 32-bit IEEE float, 13 channels. Note this differs from
# Crocell: the two recommended kits are at *different* sample rates.
DRS_RATE=44100

EGMD_URL="https://storage.googleapis.com/magentadata/datasets/e-gmd/v1.0.0/e-gmd-v1.0.0-midi.zip"
ENST_URL="https://zenodo.org/api/records/7432188/files/ENST-drums-audio.tar.bz2/content"

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
die() { printf 'fetch-corpus: %s\n' "$*" >&2; exit 1; }

tool() {
  # The measuring half. Built once, then reused.
  cargo run --quiet --manifest-path "$REPO/Cargo.toml" \
    -p expression-editor-corpus --bin drum-corpus -- "$@"
}

drumgizmo_cmd() {
  # GPL, build-time only. Prefer one on PATH; fall back to nix so a
  # fresh checkout does not have to install anything globally.
  if command -v drumgizmo >/dev/null 2>&1; then
    echo drumgizmo
  elif command -v nix >/dev/null 2>&1; then
    echo "nix run nixpkgs#drumgizmo --"
  else
    die "no drumgizmo and no nix. Install drumgizmo (GPL, build-time only) to render."
  fi
}

fetch() {
  local url="$1" dest="$2" md5="${3:-}"
  if [[ -f "$dest" ]]; then
    echo "have $(basename "$dest")"
  else
    echo "fetching $(basename "$dest") …"
    curl -fL --retry 3 --progress-bar -o "$dest.part" "$url"
    mv "$dest.part" "$dest"
  fi
  if [[ -n "$md5" ]]; then
    local got
    got="$(md5sum "$dest" | cut -d' ' -f1)"
    [[ "$got" == "$md5" ]] || die "$dest: md5 $got, expected $md5"
    echo "  md5 ok"
  fi
}

unzip_once() {
  local zip="$1" dir="$2"
  if [[ -d "$dir" ]]; then
    echo "have $(basename "$dir")/"
  else
    echo "unpacking $(basename "$zip") …"
    mkdir -p "$dir.part"
    unzip -q "$zip" -d "$dir.part"
    mv "$dir.part" "$dir"
  fi
}

# Find a kit's two XML files. Every DrumGizmo kit ships a drumkit
# description and a midimap, and their names differ per kit, so they are
# discovered rather than assumed.
kit_xml() { find "$1" -maxdepth 3 -iname '*.xml' ! -iname '*midimap*' | head -1; }
midimap_xml() { find "$1" -maxdepth 3 -iname '*midimap*.xml' | head -1; }

cmd_kits() {
  say "DrumGizmo kits — CC BY 4.0, attribution required"
  mkdir -p "$CORPUS_DIR/zips"
  fetch "$CROCELL_URL" "$CORPUS_DIR/zips/CrocellKit1_1.zip" "$CROCELL_MD5"
  fetch "$DRS_URL" "$CORPUS_DIR/zips/DRSKit2_1.zip" "$DRS_MD5"
  unzip_once "$CORPUS_DIR/zips/CrocellKit1_1.zip" "$CORPUS_DIR/kits/CrocellKit"
  unzip_once "$CORPUS_DIR/zips/DRSKit2_1.zip" "$CORPUS_DIR/kits/DRSKit"

  say "Reading the WAV headers"
  # The deliverable #158 could not answer: no DrumGizmo page states a
  # sample rate or a bit depth, so they are read off the files. A kit
  # that comes back with more than one row is mixed-rate and the render
  # below would resample half of it.
  tool probe "$CORPUS_DIR/kits/CrocellKit"
  tool probe "$CORPUS_DIR/kits/DRSKit"

  cat <<'EOF'

Drum samples provided by DrumGizmo.org (CC BY 4.0).
Carry that line with anything derived from this material that ships.
EOF
}

cmd_egmd() {
  say "E-GMD MIDI — CC BY 4.0"
  # The MIDI only. E-GMD's audio is 96 GB of Roland module output off
  # mesh pads, with no acoustic transient physics worth tuning against;
  # the MIDI is the reusable asset and it is 103 MB.
  mkdir -p "$CORPUS_DIR/zips"
  fetch "$EGMD_URL" "$CORPUS_DIR/zips/e-gmd-midi.zip"
  unzip_once "$CORPUS_DIR/zips/e-gmd-midi.zip" "$CORPUS_DIR/e-gmd"
}

cmd_sweep() {
  say "Flam sweep — ours, and the part no public dataset has"
  # Eleven annotated flams exist in the entire public corpus, all in
  # MDB-Drums, whose QC collapses labels inside a 50 ms window so even
  # those cannot say where the second strike was. Authoring the grid
  # removes the problem instead of working around it.
  mkdir -p "$CORPUS_DIR/sweep"
  tool sweep-midi --out "$CORPUS_DIR/sweep/flam-sweep.mid"
  tool sweep-truth --out "$CORPUS_DIR/sweep/flam-sweep.csv"
  tool sweep-wav --out "$CORPUS_DIR/sweep/flam-sweep-synth.wav"
  echo "sweep in $CORPUS_DIR/sweep"
}

cmd_render() {
  local kit="${1:-CrocellKit}" rate
  case "$kit" in
    CrocellKit) rate=$CROCELL_RATE ;;
    DRSKit) rate=$DRS_RATE ;;
    *) die "unknown kit $kit (CrocellKit or DRSKit)" ;;
  esac

  local dir="$CORPUS_DIR/kits/$kit"
  [[ -d "$dir" ]] || die "$dir not found — run '$0 kits' first"
  [[ -f "$CORPUS_DIR/sweep/flam-sweep.mid" ]] || cmd_sweep

  local kitxml midimap out
  kitxml="$(kit_xml "$dir")"
  midimap="$(midimap_xml "$dir")"
  [[ -n "$kitxml" ]] || die "no kit XML under $dir"
  [[ -n "$midimap" ]] || die "no midimap XML under $dir"
  out="$CORPUS_DIR/render/$kit"
  mkdir -p "$out"

  say "Rendering the sweep through $kit at $rate Hz"
  echo "  kit     $kitxml"
  echo "  midimap $midimap"
  # -r disables resampling, and the rate is set to the kit's own so
  #    there is nothing to resample; the two kits differ.
  # -p makes sample selection deterministic. Without it DrumGizmo picks
  #    a different round-robin every run and the ground truth still
  #    holds but the measurement does not repeat.
  # No -t and no -x: the timing and velocity humanizers would move the
  #    notes away from the times we authored, which is the one thing
  #    this corpus cannot afford.
  # shellcheck disable=SC2046 # deliberate word splitting on the nix form
  $(drumgizmo_cmd) \
    -i midifile -I "file=$CORPUS_DIR/sweep/flam-sweep.mid,midimap=$midimap" \
    -o wavfile -O "file=$out/flam,srate=$rate" \
    -r \
    -p close=1.0,diverse=0.0,random=0.0 \
    "$kitxml"

  say "Headers of what came back"
  tool probe "$out"

  say "Flam recall, per mic"
  # Per channel, never summed: mixing the array together manufactures
  # bleed, and measuring each mic separately is the whole point of a kit
  # captured on the array at once.
  local n=0
  for f in "$out"/flam*.wav; do
    echo
    echo "--- $(basename "$f")"
    tool recall --wav "$f" --truth "$CORPUS_DIR/sweep/flam-sweep.csv" \
      --csv "$out/$(basename "$f" .wav)-recall.csv" || true
    n=$((n + 1))
  done
  [[ $n -gt 0 ]] || die "drumgizmo produced no wavs in $out"
}

cmd_enst() {
  [[ "${1:-}" == "--i-accept-nc-nd" ]] || cat <<'EOF' >&2
ENST-Drums is CC BY-NC-ND 4.0. Its own site states "No commercial use is
possible." It is an internal evaluation corpus only:

  - never vendored into this repository
  - never shipped
  - nothing derived from it in a release asset

If you understand that and want it locally anyway, re-run with:

  ./fetch-corpus.sh enst --i-accept-nc-nd

EOF
  [[ "${1:-}" == "--i-accept-nc-nd" ]] || exit 1

  say "ENST-Drums — CC BY-NC-ND, internal evaluation only"
  local dir="$CORPUS_DIR/internal-eval-only"
  mkdir -p "$dir"
  cat > "$dir/LICENCE-NOTICE.txt" <<'EOF'
CC BY-NC-ND 4.0. No commercial use. No distribution of derivatives.
Internal evaluation only. Do not copy anything from this directory into
the FastTrackStudio tree, a release asset, or a shipped model.
EOF
  fetch "$ENST_URL" "$dir/enst-audio.tar.bz2"
  if [[ ! -d "$dir/ENST-drums-audio" ]]; then
    echo "unpacking …"
    tar -xjf "$dir/enst-audio.tar.bz2" -C "$dir"
  fi

  say "Is a flam even visible in these annotations?"
  # ENST has no flam label. The inference — from the annotation method,
  # not from any author claim — is that a flam must show up as two `sd`
  # onsets tens of ms apart, because annotation is per onset per track
  # and ghost notes were deliberately annotated. #158 said verify that
  # before relying on it. This is the verification.
  local ann
  ann="$(find "$dir" -type d -iname 'annotation*' | head -1)"
  [[ -n "$ann" ]] || die "no annotation directory found under $dir"
  tool enst "$ann" --label sd --bpm 120 --subdivision 4
}

usage() {
  cat <<EOF
usage: $0 <command>

  kits              fetch + unpack CrocellKit and DRSKit, and read their
                    WAV headers (CC BY 4.0)
  egmd              fetch the E-GMD MIDI corpus (CC BY 4.0)
  sweep             generate the flam sweep: MIDI, ground truth, and a
                    synthetic render
  render [kit]      render the sweep through a kit and measure recall
                    per mic (default CrocellKit)
  enst              ENST-Drums, gated behind --i-accept-nc-nd
  all               kits + egmd + sweep + render

  CORPUS_DIR=$CORPUS_DIR
EOF
}

case "${1:-}" in
  kits) cmd_kits ;;
  egmd) cmd_egmd ;;
  sweep) cmd_sweep ;;
  render) shift; cmd_render "${1:-CrocellKit}" ;;
  enst) shift; cmd_enst "${1:-}" ;;
  all) cmd_kits; cmd_egmd; cmd_sweep; cmd_render CrocellKit ;;
  *) usage; exit 1 ;;
esac
