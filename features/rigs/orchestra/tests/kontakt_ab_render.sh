#!/usr/bin/env bash
# kontakt_ab_render.sh <name> <mid> <chain1> [<chain2> ...]  — one REAPER project on voyager (ethernet), a track per chain, stems back to ./<name>/
# SRATE=<hz>: render the project at that sample rate.
# KSP_SEED=<n>: the KSP Seed CLAP goes before Kontakt on every track and the render runs with KSP_SEED=<n>,
# so Kontakt's KSP random() draws are those of KONTRA with KONTRA_KSP_SEED=<n>.
set -euo pipefail
name=$1 mid=$2; shift 2
T=/run/media/Development/fts/studio-percussion/features/rigs/orchestra/tests
SSH=(ssh -o HostKeyAlias=voyager -o ConnectTimeout=10 cody@192.168.1.10)
REAPER=/Applications/REAPER.app/Contents/MacOS/REAPER
# $HOME expands on the host, not here
# shellcheck disable=SC2016
home=$(timeout 30 "${SSH[@]}" 'echo $HOME')
rdir="$home/orchestra-ab/batch/kstate_$name"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
i=0
for c in "$@"; do
  i=$((i+1)); python3 -I $T/make_ab_rpp.py "$c" "$mid" /dev/null "$tmp/t$i.rpp" >/dev/null
  python3 -I - "$tmp/t$i.rpp" "t$i" "$tmp/blk$i" "$tmp/head" <<'PY'
import sys
t=open(sys.argv[1]).read(); lines=t.split("\n"); a=lines.index("  <TRACK"); b=max(i for i,l in enumerate(lines) if l=="  >")
blk=lines[a:b+1]; blk[1]=f'    NAME "{sys.argv[2]}"'; blk.insert(2,"    SEL 1")
import os
if os.environ.get("KSP_SEED"):
    for i,l in enumerate(blk):
        if l.lstrip().startswith('<VST "VST3i: Kontakt'):
            ind=l[:len(l)-len(l.lstrip())]
            blk[i:i]=[ind+'<CLAP "CLAP: KSP Seed (FTS)" com.fts.ksp-seed ""', ind+'  CFG 4 760 0 ""', ind+'>']
            break
    else: sys.exit("no Kontakt line to seed")
open(sys.argv[3],"w").write("\n".join(blk)); open(sys.argv[4],"w").write(t.split("  <TRACK")[0])
PY
done
python3 -I - "$tmp" "$i" "$rdir" <<'PY'
import sys,re
tmp,n,rdir=sys.argv[1],int(sys.argv[2]),sys.argv[3]
h=open(f"{tmp}/head").read()
h=re.sub(r'^  RENDER_FILE .*$',f'  RENDER_FILE "{rdir}"',h,flags=re.M)
h=re.sub(r'^  RENDER_PATTERN .*$','  RENDER_PATTERN "$track"',h,flags=re.M)
h=re.sub(r'^  RENDER_STEMS .*$','  RENDER_STEMS 1',h,flags=re.M)
import os
sr=os.environ.get("SRATE")
if sr:   # project and render at another rate (SAMPLERATE <sr> 1 0 forces it)
    h=re.sub(r'^  RENDER_FMT 0 2 \d+$',f'  RENDER_FMT 0 2 {sr}',h,flags=re.M)
    h=re.sub(r'^  SAMPLERATE \d+ \d+ \d+$',f'  SAMPLERATE {sr} 1 0',h,flags=re.M)
open(f"{tmp}/p.rpp","w").write(h+"\n".join(open(f"{tmp}/blk{k}").read() for k in range(1,n+1))+"\n>\n")
PY
timeout 30 "${SSH[@]}" "rm -rf '$rdir' && mkdir -p '$rdir'"
timeout 300 scp -q -o HostKeyAlias=voyager "$tmp/p.rpp" "cody@192.168.1.10:$rdir/p.rpp"
cfg="$home/orchestra-ab/cfg/render-kstate.ini"
timeout 30 "${SSH[@]}" "cp ~/orchestra-ab/cfg/user-reaper.ini '$cfg'"
t0=$(date +%s)
timeout 900 "${SSH[@]}" "${KSP_SEED:+KSP_SEED=$KSP_SEED }$REAPER -newinst -nosplash -cfgfile '$cfg' -renderproject '$rdir/p.rpp' >/dev/null 2>&1 & pid=\$!; for k in \$(seq 280); do kill -0 \$pid 2>/dev/null || break; sleep 3; done; if kill -0 \$pid 2>/dev/null; then echo TIMEOUT-KILL; kill \$pid; fi" || true
echo "render took $(( $(date +%s)-t0 ))s"
mkdir -p "$name"
for k in $(seq 1 $i); do timeout 120 scp -q -o HostKeyAlias=voyager "cody@192.168.1.10:$rdir/t$k.wav" "$name/t$k.wav" || echo "no stem t$k"; done
ls -la "$name"
