#!/bin/bash
# seed_renders.sh <sec dir> <mode> <name:seed>... — render seeded copies of
# <mode>_r1.rpp: the KSP Seed CLAP inserted before Kontakt, KSP_SEED in the
# render REAPER's environment, each in a headless REAPER of its own.
set -u
dir=$1
mode=$2
shift 2
cfg="$HOME/orchestra-ab/cfg/seedtest.ini"
cp "$HOME/orchestra-ab/cfg/user-reaper.ini" "$cfg"
cd "$dir" || exit 1
for v in "$@"; do
  n=${v%%:*}
  s=${v#*:}
  out="$dir/${mode}_seed_$n.wav"
  python3 - "${mode}_r1.rpp" "${mode}_seed_$n.rpp" "$out" <<'PY'
import sys
src, dst, out = sys.argv[1:4]
lines, done = [], False
for l in open(src):
    if l.strip().startswith("RENDER_FILE"):
        l = '  RENDER_FILE "%s"\n' % out
    if not done and l.lstrip().startswith('<VST "VST3i: Kontakt 8'):
        ind = l[: len(l) - len(l.lstrip())]
        lines += [ind + '<CLAP "CLAP: KSP Seed (FTS)" com.fts.ksp-seed ""\n',
                  ind + '  CFG 4 760 0 ""\n', ind + '>\n']
        done = True
    lines.append(l)
open(dst, "w").write("".join(lines))
print("seed block inserted" if done else "NO KONTAKT LINE FOUND")
PY
  rm -f "$out" "$dir/${mode}_seed_$n.log"
  KSP_SEED=$s KSP_SEED_LOG="$dir/${mode}_seed_$n.log" /Applications/REAPER.app/Contents/MacOS/REAPER -newinst -nosplash \
    -cfgfile "$cfg" -renderproject "$dir/${mode}_seed_$n.rpp" >/dev/null 2>&1
  for _ in $(seq 150); do
    pgrep -f "${mode}_seed_${n}[.]rpp" >/dev/null || break
    sleep 2
  done
  ls -la "$out"
done
md5 -r "$dir/${mode}"_seed_*.wav "$dir/${mode}"_kontakt_r1.wav "$dir/${mode}"_kontakt_r2.wav
