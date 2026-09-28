#!/usr/bin/env python3
"""Timbre Shift probe: sweep a sample layer's MULTISAMPLE `timbre` on the
plugin (the layer cleaned as in samplecal.py), REPS renders each to average
round-robins; prints level (dB, ± spread), spectral centroid and the 1/3-oct
spectrum relative to timbre 0.5.
usage: timbreprobe.py <patch> <layer>   env: NOTE=60 VEL=100 REPS=4 TS=0,0.25,..."""
import os, re, struct, subprocess, sys, tempfile
import numpy as np
H = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
src, layer = sys.argv[1], int(sys.argv[2])
NOTE, VEL, REPS = os.environ.get("NOTE", "60"), os.environ.get("VEL", "100"), int(os.environ.get("REPS", "4"))
TS = [float(t) for t in os.environ.get("TS", "0,0.2,0.35,0.45,0.5,0.55,0.65,0.8,1").split(",")]
base = open(src, encoding="latin1").read()
n = [0]
def only(m):
    k = n[0]; n[0] += 1
    return m.group(0) if k == layer else re.sub(r'onOff="[^"]*"', 'onOff="0"', m.group(0))
base = re.sub(r'<AENVPARAMS [^>]*>', only, base)
base = re.sub(r'<FILTER ([^>]*)>', lambda m: "<FILTER " + re.sub(r'\bact="[^"]*"', 'act="0"', m.group(1), count=1) + ">", base)
base = re.sub(r'\bhrmOn="[^"]*"', 'hrmOn="0"', base)
base = re.sub(r'\bsource(\d+)="[^"]*"', r'source\1="off"', base)
base = re.sub(r'(<EFFMODULE\b[^>]*?\bActive=")[^"]*(")', r'\g<1>0\g<2>', base)
E = 50 * 2 ** (np.arange(0, 28) / 3)

def with_timbre(t):
    k = [0]
    def g(m):
        i = k[0]; k[0] += 1
        return re.sub(r'\btimbre="[^"]*"', f'timbre="{struct.pack(">f", t).hex()}"', m.group(0)) if i == layer else m.group(0)
    return re.sub(r'<MULTISAMPLE [^>]*>', g, base)

def render(xml):
    fd, p = tempfile.mkstemp(suffix=".prt_omn"); os.close(fd)
    w = p[:-8] + ".wav"; open(p, "w", encoding="latin1").write(xml)
    subprocess.run([H, p, w, "--note", NOTE, "--vel", VEL, "--hold", "1.0", "--tail", "0.05"],
                   capture_output=True, stdin=subprocess.DEVNULL, timeout=300)
    d = open(w, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    os.remove(p); os.remove(w)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1).astype(np.float64)[480:int(0.8 * 48000)]

def analyse(x):
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2; fr = np.fft.rfftfreq(len(x), 1 / 48000)
    b = np.array([sp[(fr >= a) & (fr < c)].sum() for a, c in zip(E[:-1], E[1:])])
    return 10 * np.log10(np.mean(x ** 2) + 1e-20), (sp * fr).sum() / sp.sum(), 10 * np.log10(b + 1e-20)

ref = None
for t in sorted(TS, key=lambda v: abs(v - 0.5)):
    rs = [analyse(render(with_timbre(t))) for _ in range(REPS)]
    lv = np.array([r[0] for r in rs]); cen = np.mean([r[1] for r in rs]); bands = np.mean([r[2] for r in rs], 0)
    if ref is None: ref = (lv.mean(), bands)
    rel = bands - ref[1]
    print(f"t {t:4.2f}  level {lv.mean() - ref[0]:+6.1f} ±{lv.std():.1f}  centroid {cen:6.0f} Hz  bands " +
          " ".join(f"{v:+4.0f}" for v in rel[3:24:2]), flush=True)
print("band Hz " + " ".join(f"{int(e)}" for e in E[3:24:2]))
