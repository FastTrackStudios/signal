#!/bin/bash
# usage: regress.sh <type-hex>...  (all when none) — renders ours, prints error table
HERE="$(cd "$(dirname "$0")" && pwd)"
PY="${PYTHON:-python3}"
# Run from the measurement dir (types.json / taper.json live there).
sel="$*"
args=(); while IFS=$'\t' read h n nm; do
  if [ -n "$sel" ] && [[ " $sel " != *" $h "* ]]; then continue; fi
  for r in 0 0.5 0.9; do args+=("${h}_r${r}=type1=h:$h;freq=f:0.3;freq1=f:0.5;res=f:$r"); done
  for v in 0.15 0.45 0.6 0.75; do args+=("${h}_v${v}=type1=h:$h;freq=f:$v;freq1=f:0.5;res=f:0"); done
done < "$HERE/types.txt"
QUIET=1 ONLYOURS=1 OURS=1 JSON=ours_sel.json NOTE=24 "$PY" "$HERE/fexp.py" "${args[@]}" < /dev/null > /dev/null 2>&1
"$PY" - "$HERE/types.txt" <<'PY'
import json, numpy as np
o={**json.load(open('types.json'))['curves'], **json.load(open('taper.json'))['curves']}
d=json.load(open('ours_sel.json')); f0=d['f0']; u=d['curves']
import sys
names={l.split('\t')[0]:l.split('\t')[2].strip() for l in open(sys.argv[1])}
pts=['r0','r0.5','r0.9','v0.15','v0.45','v0.6','v0.75']
for h in sorted({k.split('_')[0] for k in u}, key=list(names).index):
    row=[]
    for p in pts:
        k=f"{h}_{p}"; a=np.array(o[k+'/omni']); b=np.array(u[k+'/ours'])
        m=(a>-50)&((np.arange(len(a))+1)*f0<8000); row.append(float(np.sqrt(np.mean((a[m]-b[m])**2))))
    print(f"{names[h][:22]:22s} "+' '.join(f'{e:6.1f}' for e in row))
PY
