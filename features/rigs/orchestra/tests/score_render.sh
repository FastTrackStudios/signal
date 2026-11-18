#!/usr/bin/env bash
# Render a MusicXML score through our CSS engine (and, for an A/B, through
# the factory Kontakt patches on voyager):
#
#   features/rigs/orchestra/tests/score_render.sh <score.mxl> <outdir> <name> \
#       P1="1st Violins":-0.55 P2="2nd Violins":-0.2 P3=Violas:0.2 P4=Cellos:0.5
#
# Each argument maps a score part id (score_to_css.py prints them) to a CSS
# section and a pan (-1 left .. 1 right). Writes, in <outdir>:
#   midi/<part>.mid             the part as CSS MIDI (score_to_css.py)
#   ours/<part>.wav, "<name> (Signal CSS).wav"
#   kontakt/<part>.wav, "<name> (Kontakt CSS).wav"   (with KONTAKT=1)
# and prints each part's render time.
#
# Mixes play the Mix mic only (the spec's default, as the factory patch).
# Env: SWING=0.6 swings eighths (default straight), KONTAKT=1 also renders
# Kontakt (needs target/orchestra-ab/chains/fxchain_<tag>.txt, see
# null_suite.sh), KONTAKT_HOST (default voyager), BIN another render_css_live.
# Scores are copyrighted: keep them and the renders out of the repo.
set -euo pipefail

score="${1:?usage: score_render.sh <score> <outdir> <name> <part>=<Section>:<pan> ...}"
out="${2:?outdir}"
name="${3:?name}"
shift 3
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../../../.." && pwd)"
bin="${BIN:-$repo/target/x86_64-unknown-linux-gnu/desktop-dev/examples/render_css_live}"
host="${KONTAKT_HOST:-voyager}"
[ -x "$bin" ] || { echo "render_css_live not built: $bin" >&2; exit 1; }
mkdir -p "$out/midi" "$out/ours" "$out/kontakt"

npy() {
  if python3 -c "import numpy" 2>/dev/null; then
    python3 -I "$@"
  else
    timeout 3000 nix-shell -p 'python3.withPackages (ps: [ ps.numpy ])' --run "python3 -I $(printf '%q ' "$@")" 2>&1 | grep -v "search path"
  fi
}

tag_for() {
  case "$1" in
    "1st Violins") echo 1v ;; "2nd Violins") echo 2v ;; Violas) echo va ;;
    Cellos) echo vc ;; Basses) echo cb ;;
    *) echo "no Kontakt chain for: $1" >&2; return 1 ;;
  esac
}

swing=()
[ -n "${SWING:-}" ] && swing=("--swing=$SWING")
python3 -I "$here/score_to_css.py" "$score" "$out/midi" "${swing[@]}"

if [ -n "${KONTAKT:-}" ]; then
  # shellcheck disable=SC2016  # $HOME is the remote one
  remote="$(timeout 20 ssh "$host" 'echo $HOME')/orchestra-ab/score"
  timeout 20 ssh "$host" "mkdir -p '$remote'"
fi

ours=() kon=()
for a in "$@"; do
  part="${a%%=*}"; rest="${a#*=}"; section="${rest%:*}"; pan="${rest##*:}"
  s0=$(date +%s.%N)
  timeout 1800 "$bin" "$out/midi/$part.mid" "$out/ours/$part.wav" --section "$section" >/dev/null 2>&1
  echo "$part $section: ours $(echo "$(date +%s.%N) - $s0" | bc | cut -c1-5) s"
  ours+=("$out/ours/$part.wav:$pan")
  if [ -n "${KONTAKT:-}" ]; then
    tag="$(tag_for "$section")"
    python3 -I "$here/make_ab_rpp.py" "$repo/target/orchestra-ab/chains/fxchain_$tag.txt" \
      "$out/midi/$part.mid" "$remote/$part.wav" "$out/kontakt/$part.rpp" >/dev/null
    timeout 120 scp -q "$out/kontakt/$part.rpp" "$host:$remote/"
    s0=$(date +%s.%N)
    # A headless REAPER of its own: renders, then quits.
    timeout 1800 ssh "$host" "/Applications/REAPER.app/Contents/MacOS/REAPER -newinst -nosplash \
      -renderproject '$remote/$part.rpp' >/dev/null 2>&1" || true
    echo "$part $section: kontakt $(echo "$(date +%s.%N) - $s0" | bc | cut -c1-5) s"
    timeout 300 scp -q "$host:$remote/$part.wav" "$out/kontakt/"
    kon+=("$out/kontakt/$part.wav:$pan")
  fi
done

npy "$here/mix_parts.py" "$out/$name (Signal CSS).wav" "${ours[@]}"
[ ${#kon[@]} -eq 0 ] || npy "$here/mix_parts.py" "$out/$name (Kontakt CSS).wav" "${kon[@]}"
