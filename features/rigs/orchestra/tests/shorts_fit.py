#!/usr/bin/env python3
"""Explain a CSS short burst from gen_probe.py --shorts, note by note.

    TUNESEARCH=1 python3 shorts_fit.py <manifest.tsv> <kontakt.wav> <trace.txt> <css_root> <draws> <units>

Each note is fitted (file, start, gain, and with TUNESEARCH the random
detune in cents) against the render minus the notes before it, and set
next to what our engine played (render_css_live --trace). <draws> is the
comma list of random-tune draws in burst order (an alternating burst has
two: its first 67 and its first 68), the same list render_css_live
--tunes replays. Kontakt draws a uniform -5..5 c only when a key was not
struck in the last 500 ms; the first note of each burst fits within one
cent at -50 dB or better.
"""
import sys, csv, glob, os, re
sys.path.insert(0, 'features/rigs/orchestra/tests')
from null_test import SR, load, db
from identify import resample
import numpy as np
man, kon, trace, root, tunes_s, units_s = sys.argv[1:7]
k = load(kon)
draws = [int(v) for v in tunes_s.split(',')]
units = [int(u) for u in units_s.split(',')]
ours = {}
for line in open(trace):
    m = re.match(r'\[v\d+\]\s+([\d.]+)s\s+Short\s+(.+?\.wav)\s+note\s+(\d+).*gain\s+([\d.]+)', line)
    if m:
        ours[round(float(m.group(1)), 3)] = (os.path.basename(m.group(2)), float(m.group(4)))
tsv = list(csv.DictReader(open(os.path.join(root, '_ksp', 'CSS 1st Violins', 'zones.tsv')), delimiter='\t'))
ztune = {}
for z in tsv:
    n = os.path.basename(z['sample'])
    if '_Mix_' in n and int(z['key_lo']) <= 67 <= int(z['key_hi']):
        ztune.setdefault(n, float(z['tune']))
cache = {}
def files(typ):
    fs = sorted(glob.glob(os.path.join(root, 'Mix', '1st Violins', '*', typ, '*_G3_*.wav')))
    for f in fs:
        if f not in cache: cache[f] = load(f)
    return fs
GROUP = 1.0052122
di = 0
for r in csv.DictReader(open(man), delimiter='\t'):
    u = int(r['idx']); alt = 'alt' in r['description']
    nd = 2 if alt else 1
    my = draws[di:di + nd]; di += nd
    if u not in units: continue
    typ = r['category']; gap = 0.2 if alt else 0.3
    keys = [67, 68] * 3 if alt else [67] * 6
    t0 = float(r['t_start']); n00 = int(round(t0 * SR))
    span = int((gap * 6 + 2.0) * SR)
    resid = k[n00:n00 + span].copy()
    print(f"unit {u} {r['description']}  draws {my}")
    for i, key in enumerate(keys):
        tune = my[0] if i == 0 else (my[1] if (alt and i == 1) else 0)
        off = int(round(i * gap * SR)); L = int(0.1 * SR)
        best = None
        tunes = range(-5, 6) if os.environ.get('TUNESEARCH') else [tune]
        for f, tune in ((f, tn) for f in files(typ) for tn in tunes):
            zn = '1v_' + os.path.basename(os.path.dirname(f)) + '_Mix_' + os.path.basename(f)[:-4] + '.ncw'
            rate = GROUP * ztune.get(zn, 1.0) * (2 ** (1 / 12) if key == 68 else 1) * 2 ** (tune / 1200)
            for st in (288, 0):
                y = resample(cache[f][st:], rate, L)
                seg = resid[off:off + L]
                g = np.sum(seg * y) / (np.sum(y * y) + 1e-30)
                e = np.sum((seg - g * y) ** 2) / (np.sum(seg * seg) + 1e-30)
                if best is None or e < best[0]:
                    best = (e, f, st, g, rate, tune)
        e, f, st, g, rate, tune = best
        full = resample(cache[f][st:], rate, min(span - off, int(len(cache[f]) / rate) - 64))
        resid[off:off + len(full)] -= g * full
        o = ours.get(round(t0 + i * gap, 3), ('-', 0))
        nm = os.path.basename(f)
        print(f"  {i} key{key} tune{tune:+d}  kontakt {nm:12s} st {st:3d} gain {g:6.3f} ({db(g*g):5.1f} dB) res {db(e):6.1f}   ours {o[0]:12s} {o[1]/1.3414:6.3f} ({db((o[1]/1.3414)**2):5.1f} dB){'' if o[0]==nm else '  <-- differs'}")
