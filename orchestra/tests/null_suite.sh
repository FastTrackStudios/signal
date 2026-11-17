#!/usr/bin/env bash
# Null-test one Cinematic Studio instrument against Kontakt, probe by probe:
#
#   features/rigs/orchestra/tests/null_suite.sh "Violas" [mode ...]
#   features/rigs/orchestra/tests/null_suite.sh "Solo Flute" [mode ...]
#
# For each mode (default: all of them) it writes the probe MIDI
# (gen_probe.py, transposed into the section's range), renders it twice
# through the section's factory Kontakt chain on voyager (two runs tell
# Kontakt's random draws from deterministic notes), renders ours
# (render_css_live --section) and scores the pair (null_test.py).
#
#   modes: probe legato leg2 marcato marc2 shorts pizz trem rebow attack ext
#
# The Kontakt chain is target/orchestra-ab/chains/fxchain_<tag>.txt — the
# track <FXCHAIN> of a REAPER project with the factory CSS <Section> loaded
# (see css_section.sh and the css-section-process notes). Kontakt renders
# run in a separate headless REAPER (-newinst), never in the user's session.
# Shorts and pizzicato draw a random detune, zone-2 legato (leg2) a random
# body variant: those modes fit Kontakt's draws first (fit_draws.py) and
# replay them, and are scored against the first Kontakt run only.
#
# Env: KONTAKT_HOST (default voyager), SKIP_KONTAKT=1 reuses existing renders,
# BIN another render_css_live.
set -euo pipefail

section="${1:?usage: null_suite.sh <Section> [mode ...]}"
shift
# section → library, chain tag and probe transposition (into the
# instrument's range): sections.tsv, one row per instrument patch folder
here="$(cd "$(dirname "$0")" && pwd)"
row=$(awk -F'\t' -v s="$section" '$1 == s' "$here/sections.tsv")
[ -n "$row" ] || { echo "unknown section: $section (add it to sections.tsv)" >&2; exit 1; }
IFS=$'\t' read -r _ lib tag tr <<< "$row"
export CS_LIBRARY="$lib"   # fit_draws.py passes it on to the renderer
modes=("$@")
# strings' own probes (pizz, trem, rebow) are string articulations; the
# winds and brass test their own (ext: flutter, trills, mutes, rips, …)
if [ ${#modes[@]} -eq 0 ]; then
  if [ "$lib" = strings ]; then
    modes=(probe legato leg2 marcato marc2 shorts pizz trem rebow attack)
  else
    modes=(probe legato leg2 marcato marc2 shorts attack ext)
  fi
fi

repo="$(cd "$here/../../../.." && pwd)"
host="${KONTAKT_HOST:-voyager}"
chain="$repo/target/orchestra-ab/chains/fxchain_$tag.txt"
work="$repo/target/orchestra-ab/sec/$tag"
bin="${BIN:-$repo/target/x86_64-unknown-linux-gnu/desktop-dev/examples/render_css_live}"
[ -f "$chain" ] || { echo "no Kontakt chain: $chain" >&2; exit 1; }
[ -x "$bin" ] || { echo "render_css_live not built: $bin" >&2; exit 1; }
# The Kontakt host is reached only when a render is missing (SKIP_KONTAKT
# with every render in place works with voyager away).
remote=""
cfg=""
remote_dir() {
  if [ -z "$remote" ]; then
    local home
    # shellcheck disable=SC2016  # $HOME is the remote one
    home="$(timeout 20 ssh "$host" 'echo $HOME')"
    [ -n "$home" ] || { echo "cannot reach $host" >&2; exit 1; }
    remote="$home/orchestra-ab/sec/$tag"
    # A render REAPER rewrites its ini on the way out, cut down to a stub: it
    # must never run on the user's reaper.ini. Each section renders on its own
    # copy of the user's settings (~/orchestra-ab/cfg/user-reaper.ini, with the
    # plugin caches beside it), passed with -cfgfile.
    cfg="$home/orchestra-ab/cfg/render-$tag.ini"
    timeout 20 ssh "$host" "mkdir -p '$remote' && cp ~/orchestra-ab/cfg/user-reaper.ini '$cfg'"
  fi
}

# python with numpy (null_test.py): the system one if it has it, else nix's,
# resolved once per run (a nix-shell per call cost seconds each, several per
# mode). NPY_PYTHON overrides; null_all.sh resolves it once for every job.
if [ -z "${NPY_PYTHON:-}" ]; then
  if python3 -c "import numpy" 2>/dev/null; then
    NPY_PYTHON=python3
  else
    NPY_PYTHON=$(timeout 600 nix-shell -p 'python3.withPackages (ps: [ ps.numpy ])' --run 'command -v python3')
  fi
fi
npy() {
  "$NPY_PYTHON" -I "$@"
}

flags_for() {
  case "$1" in
    probe) echo "" ;;
    legato) echo "--legato" ;;
    leg2) echo "--legato --zone2" ;;
    marcato) echo "--marcato" ;;
    marc2) echo "--marcato --overlay" ;;
    shorts) echo "--shorts" ;;
    pizz) echo "--shorts --pizz" ;;
    trem) echo "--tremolo" ;;
    rebow) echo "--tremolo --rebow" ;;
    attack) echo "--attack" ;;
    ext) if [ "$lib" = brass ]; then echo "--winds --brass"; else echo "--winds"; fi ;;
    *) echo "unknown mode: $1" >&2; return 1 ;;
  esac
}

# A WAV whose data is all there (its header's data size; a render or copy
# cut short keeps the full size in the header).
wav_complete() {
  [ -f "$1" ] && python3 -I -c '
import os, struct, sys
b = open(sys.argv[1], "rb").read(4096)
j = b.find(b"data")
n = struct.unpack("<I", b[j + 4 : j + 8])[0] if j > 0 else 0
sys.exit(0 if n > 0 and os.path.getsize(sys.argv[1]) - (j + 8) >= n else 1)' "$1"
}

for mode in "${modes[@]}"; do
  d="$work/$mode"
  mkdir -p "$d"
  read -r -a fl <<< "$(flags_for "$mode")"
  python3 -I "$here/gen_probe.py" "$d/$mode.mid" "$d/${mode}_manifest.tsv" "${fl[@]}" "--transpose=$tr" >/dev/null

  if [ -z "${SKIP_KONTAKT:-}" ] || ! wav_complete "$d/${mode}_kontakt_r1.wav" \
      || ! wav_complete "$d/${mode}_kontakt_r2.wav"; then
    remote_dir
    for r in r1 r2; do
      out="$d/${mode}_kontakt_$r.wav"
      [ -n "${SKIP_KONTAKT:-}" ] && wav_complete "$out" && continue
      python3 -I "$here/make_ab_rpp.py" "$chain" "$d/$mode.mid" \
        "$remote/${mode}_kontakt_$r.wav" "$d/${mode}_$r.rpp" >/dev/null
      # a finished render already on voyager (a copy that was cut short)
      if [ -n "${SKIP_KONTAKT:-}" ]; then
        timeout 600 scp -q "$host:$remote/${mode}_kontakt_$r.wav" "$d/" 2>/dev/null
        wav_complete "$out" && continue
      fi
      for attempt in 1 2; do
        timeout 120 scp -q "$d/${mode}_$r.rpp" "$host:$remote/"
        # A headless REAPER of its own: renders, then quits.
        # REAPER may return before the render is done: wait for it to exit
        timeout 900 ssh "$host" "/Applications/REAPER.app/Contents/MacOS/REAPER -newinst -nosplash \
          -cfgfile '$cfg' -renderproject '$remote/${mode}_$r.rpp' >/dev/null 2>&1
          for i in \$(seq 280); do
            pgrep -f '^/Applications/REAPER.app/Contents/MacOS/REAPER .*/${mode}_${r}[.]rpp' >/dev/null || break
            sleep 3
          done" || true
        # the WAV header holds the full length: a copy or render cut short
        # (a slow link, a REAPER that quit early) shows as missing data
        timeout 600 scp -q "$host:$remote/${mode}_kontakt_$r.wav" "$d/"
        wav_complete "$out" && break
        echo "!! $section $mode $r: incomplete Kontakt render (attempt $attempt)" >&2
      done
    done
  fi

  echo "== $section $mode"
  # woodwinds / brass: the marcato overlay is a random-RR Repetitions short,
  # fitted like the shorts
  fit_mode="$mode"
  if [ "$lib" != strings ] && [ "$mode" = marc2 ]; then
    fit_mode=shorts
  fi
  # brass marcato: a body struck with another key down detunes at random
  # (held_random_tune_cents) — fitted like a short's detune
  if [ "$lib" = brass ] && [ "$mode" = marcato ]; then
    fit_mode=shorts
  fi
  # brass ext: Mutereps / Rips draw random RRs and a Taka phrase its hits
  # (up to 9 candidates: the second hit's list) — fitted like the shorts
  if [ "$lib" = brass ] && [ "$mode" = ext ]; then
    fit_mode=shorts
    export CS_RR_MAX=9
  fi
  # Solo Strings marcato: each connection draws its body's extra entry
  if [ "$lib" = solo-strings ] && [ "$mode" = marcato ]; then
    fit_mode=offsets
  fi
  # Solo Strings shorts draw among up to 9 RRs (Spiccato)
  [ "$lib" = solo-strings ] && export CS_RR_MAX=9
  # Solo Strings draws a loud first note's body variant at random: fit it
  # like leg2's in the modes with loud first notes
  if [ "$lib" = solo-strings ] && { [ "$mode" = probe ] || [ "$mode" = legato ]; }; then
    fit_mode=leg2
  fi
  # woodwinds marcato draws its pitch envelope per strike (fit_pitch_env.py)
  if [ "$lib" = woodwinds ] && [ "$mode" = marcato ]; then
    fit_mode=penv
  fi
  case "$fit_mode" in
    offsets)
      npy "$here/fit_draws.py" --offsets "$bin" "$section" "$d/$mode.mid" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" --json "$d/null.json" | tail -1
      ;;
    penv)
      npy "$here/fit_pitch_env.py" "$bin" "$section" "$d/$mode.mid" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_penv.txt"
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" --json "$d/null.json" | tail -1
      ;;
    shorts|pizz|leg2|attack)
      kind=--tunes
      # woodwinds / brass draw their shorts' round robins at random: fit
      # the RR each note drew
      if [ "$lib" != strings ] && { [ "$fit_mode" = shorts ] || [ "$mode" = pizz ]; }; then
        kind=--rrs
      fi
      [ "$lib" = brass ] && [ "$mode" = marcato ] && kind=--tunes
      [ "$fit_mode" = leg2 ] && kind=--variants
      [ "$mode" = attack ] && kind=--attacks
      # woodwinds marc2: its marcato strikes draw a pitch envelope too —
      # held at 0 while the shorts' draws are fitted, fitted after them
      penv0=""
      if [ "$lib" = woodwinds ] && [ "$mode" = marc2 ]; then
        penv0="--pitch-envs $(printf '0:333,%.0s' $(seq 400) | sed 's/,$//')"
      fi
      if { [ "$lib" = woodwinds ] || { [ "$lib" = brass ] && { [ "$mode" = marc2 ] || [ "$mode" = ext ]; }; }; } && [ "$kind" = --rrs ]; then
        # woodwinds shorts also detune a note the key has not struck within
        # the gate (KSP $14yum / $5i0r1): fit the RRs with the detune held at
        # 0, then the detune with those RRs held
        zeros=$(printf '0,%.0s' $(seq 400)); zeros=${zeros%,}
        FIT_EXTRA="--tunes $zeros $penv0" npy "$here/fit_draws.py" --rrs "$bin" "$section" "$d/$mode.mid" \
          "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_rr.txt"
        FIT_EXTRA="--rrs $(head -1 "$d/draws_rr.txt") $penv0" npy "$here/fit_draws.py" --tunes "$bin" "$section" \
          "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
        # once more each way: the RRs against the fitted detune, then the
        # detune against those RRs (2 Flutes detunes every note)
        FIT_EXTRA="--tunes $(head -1 "$d/draws.txt") $penv0" npy "$here/fit_draws.py" --rrs "$bin" "$section" \
          "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_rr.txt"
        FIT_EXTRA="--rrs $(head -1 "$d/draws_rr.txt") $penv0" npy "$here/fit_draws.py" --tunes "$bin" "$section" \
          "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
        # then each note still off searched over (RR, detune) together
        FIT_EXTRA="$penv0" npy "$here/fit_joint.py" "$bin" "$section" "$d/$mode.mid" \
          "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" "$d/draws_rr.txt" "$d/draws.txt"
      elif [ "$lib" = solo-strings ] && [ "$mode" = attack ]; then
        # Solo Strings' loud first note draws its attack voice AND its body
        # variant (random(2,4)): fit each with the other held, twice
        vz=$(printf '2,%.0s' $(seq 400)); vz=${vz%,}
        FIT_EXTRA="--variants $vz" npy "$here/fit_draws.py" --attacks "$bin" "$section" "$d/$mode.mid" \
          "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
        for _ in 1 2; do
          FIT_EXTRA="--attacks $(head -1 "$d/draws.txt")" npy "$here/fit_draws.py" --variants "$bin" "$section" \
            "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_var.txt"
          FIT_EXTRA="--variants $(head -1 "$d/draws_var.txt")" npy "$here/fit_draws.py" --attacks "$bin" "$section" \
            "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
        done
      elif [ "$lib" = solo-strings ] && [ "$mode" = marc2 ]; then
        # Solo Strings marc2: the overlay shorts draw their RRs and each
        # marcato connection its body's extra entry: fit each with the
        # other held (the offsets first at 0), twice
        zeros=$(printf '0,%.0s' $(seq 400)); zeros=${zeros%,}
        echo "$zeros" > "$d/draws.txt"
        for _ in 1 2; do
          FIT_EXTRA="--offsets $(head -1 "$d/draws.txt")" npy "$here/fit_draws.py" --rrs "$bin" "$section" \
            "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_rr.txt"
          FIT_EXTRA="--rrs $(head -1 "$d/draws_rr.txt")" npy "$here/fit_draws.py" --offsets "$bin" "$section" \
            "$d/$mode.mid" "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
        done
      else
        npy "$here/fit_draws.py" "$kind" "$bin" "$section" "$d/$mode.mid" \
          "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws.txt"
      fi
      if [ -n "$penv0" ]; then
        FIT_EXTRA="--rrs $(head -1 "$d/draws_rr.txt") --tunes $(head -1 "$d/draws.txt")" \
          npy "$here/fit_pitch_env.py" "$bin" "$section" "$d/$mode.mid" \
          "$d/${mode}_kontakt_r1.wav" "$d/${mode}_ours.wav" > "$d/draws_penv.txt"
      fi
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" --json "$d/null.json" | tail -1
      ;;
    *)
      "$bin" "$d/$mode.mid" "$d/${mode}_ours.wav" --section "$section" --library "$lib" >/dev/null 2>&1
      npy "$here/null_test.py" "$d/${mode}_manifest.tsv" "$d/${mode}_ours.wav" \
        "$d/${mode}_kontakt_r1.wav" "$d/${mode}_kontakt_r2.wav" --json "$d/null.json" | tail -2
      ;;
  esac
done
