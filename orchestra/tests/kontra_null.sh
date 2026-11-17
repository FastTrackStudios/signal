#!/usr/bin/env bash
# Null KONTRA against Kontakt, probe by probe: the library's own script and
# NKI rendered by KONTRA (kontra-inspect play), scored against the same
# Kontakt reference renders null_suite.sh uses.
#
#   features/rigs/orchestra/tests/kontra_null.sh <tag> <instrument.nki> [mode ...]
#
# <tag> is the section's directory under target/orchestra-ab/sec (ss_v1, …),
# holding <mode>/<mode>.mid, <mode>_manifest.tsv and <mode>_kontakt_r1.wav.
# Writes <mode>/<mode>_kontra.wav, <mode>/kontra.json (trace) and
# <mode>/kontra_null.json, and prints each mode's summary line.
#
# KONTRA_BIN (default: the local/inspect-vars build) and NPY_PYTHON override
# the tools. A difference is a KONTRA bug to fix, not a parameter to fit.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
bin=${KONTRA_BIN:-/run/media/Development/KONTRA-inspect-vars/target/release/kontra-inspect}
npy=${NPY_PYTHON:-/nix/store/2q51krmlq844ggq5ikimky7507nk6s4j-python3-3.14.8-env/bin/python3}
tag=$1
nki=$2
shift 2
modes=("$@")
sec="$repo/target/orchestra-ab/sec/$tag"
if [ ${#modes[@]} -eq 0 ]; then
  mapfile -t modes < <(cd "$sec" && command ls -d ./*/ | sed "s|^\./||; s|/$||")
fi
for mode in "${modes[@]}"; do
  d="$sec/$mode"
  [ -f "$d/$mode.mid" ] && [ -f "$d/${mode}_kontakt_r1.wav" ] || continue
  python3 -I "$here/analysis/mid2kontra.py" "$d/$mode.mid" > "$d/kontra_scn.json"
  KONTRA_INSPECT_WAV="$d/${mode}_kontra_f32.wav" timeout 3000 "$bin" play "$nki" \
    --scenario "$d/kontra_scn.json" --json "$d/kontra.json" > /dev/null 2>&1
  "$npy" -I "$here/analysis/wav_f32_to_24.py" "$d/${mode}_kontra_f32.wav" "$d/${mode}_kontra.wav"
  rm -f "$d/${mode}_kontra_f32.wav"
  printf '%-8s %-8s ' "$tag" "$mode"
  "$npy" "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_kontra.wav" \
    "$d/${mode}_kontakt_r1.wav" --json "$d/kontra_null.json" | tail -1
done
