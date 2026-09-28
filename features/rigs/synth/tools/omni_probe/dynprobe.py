#!/usr/bin/env python3
"""Dynamics probe: a held note through an effect at input levels 0, −6,
−12, −18, −24 dB (the layer's `OSC atrm`, a linear pre-FX gain), printing
the effect's gain (output − input, dB) at each and the 2nd/3rd harmonic
(dB re the fundamental) at full input. usage: dynprobe.py name="Type|P..." """
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
DY_ARGS = sys.argv[1:]; sys.argv = sys.argv[:1]
_f = open(os.path.join(HERE, "fexp.py")).read()
exec(_f[:_f.index("def render(")])
_x = open(os.path.join(HERE, "fxprobe.py")).read()
exec(_x[_x.index("\ndef set_fx("):_x.index("\ndef render(xml, exe)")])
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
base = open(os.path.join(HERE, "init_part.prt_omn")).read()
LEVELS = [int(v) for v in os.environ.get("LEVELS", "0,-6,-12,-18,-24").split(",")]

def rend(xml, exe):
    open("/tmp/dy.prt_omn", "w").write(xml)
    subprocess.run([exe, "/tmp/dy.prt_omn", "/tmp/dy.wav", "--note", "48", "--hold", "1.5", "--tail", "0.1"],
                   capture_output=True, stdin=subprocess.DEVNULL)
    d = open("/tmp/dy.wav", "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    x = np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)[int(0.8*48000):int(1.4*48000)]
    return x

def level(x): return 20*np.log10(np.sqrt(np.mean(x**2)) + 1e-12)

def harmonics(x):
    F = 130.81; w = np.hanning(len(x)); sp = np.abs(np.fft.rfft(x*w, 1 << 18)); df = 48000/(1 << 18)
    h = [sp[int(F*k*0.98/df):int(F*k*1.02/df)].max() for k in (1, 2, 3)]
    return [round(20*np.log10(h[k]/h[0]), 1) for k in (1, 2)]

def with_level(xml, db):
    return apply(xml, f"OSC.atrm=f:{10**(db/20):.6f}")

REF = {}
for spec in DY_ARGS:
    name, _, rest = spec.partition("=")
    typ, _, ps = rest.partition("|")
    params = {int(k[1:]): float(v) for k, v in (p.split("=") for p in filter(None, ps.split(",")))}
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag == "omni" and os.environ.get("ONLYOURS"): continue
        if tag not in REF:
            REF[tag] = {db: level(rend(with_level(base, db), exe)) for db in LEVELS}
            REF[tag + "h"] = harmonics(rend(base, exe))
        xml = set_fx(base, typ, params)
        gains = []
        for db in LEVELS:
            x = rend(with_level(xml, db), exe)
            gains.append(round(level(x) - REF[tag][db], 1))
            if db == 0: hh = harmonics(x)
        print(f"{name:12s} {tag} gain@in(0,-6,-12,-18,-24) {gains}  h2/h3 {hh} (dry {REF[tag+'h']})", flush=True)
