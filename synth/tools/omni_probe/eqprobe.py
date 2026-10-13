#!/usr/bin/env python3
"""EQ / tone probe: a held low saw (note 24) through an effect in the first
Common FX slot, |H| per harmonic = with ÷ without (FFT peak-picking), printed
at 1/3-octave points (dB). usage: eqprobe.py name="Type|P0=..,.." ...
env: OURS=1, ONLYOURS=1, NOTE"""
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
EQ_ARGS = sys.argv[1:]; sys.argv = sys.argv[:1]
_f = open(os.path.join(HERE, "fexp.py")).read()
exec(_f[:_f.index("def render(")])
exec(_f[_f.index("def harm(x):"):_f.index("def summary")])
_x = open(os.path.join(HERE, "fxprobe.py")).read()
exec(_x[_x.index("\ndef set_fx("):_x.index("\ndef render(xml, exe)")])
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
base = open(os.path.join(HERE, "init_part.prt_omn")).read()
PTS = [31, 63, 125, 250, 500, 1000, 2000, 4000, 8000, 12000, 16000]

def rend(xml, exe):
    open("/tmp/eq.prt_omn", "w").write(xml)
    subprocess.run([exe, "/tmp/eq.prt_omn", "/tmp/eq.wav", "--note", str(NOTE), "--hold", "1.2",
                    "--tail", "0.1"], capture_output=True, stdin=subprocess.DEVNULL)
    d = open("/tmp/eq.wav", "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)

REF = {}
for spec in EQ_ARGS:
    name, _, rest = spec.partition("=")
    typ, _, ps = rest.partition("|")
    params = {int(k[1:]): float(v) for k, v in (p.split("=") for p in filter(None, ps.split(",")))}
    xml = set_fx(base, typ, params)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag == "omni" and os.environ.get("ONLYOURS"): continue
        if tag not in REF:
            REF[tag] = harm(rend(base, exe))
        f, r = REF[tag]; _, h = harm(rend(xml, exe))
        H = 20*np.log10(np.maximum(h, 1e-12) / np.maximum(r, 1e-12))
        ok = 20*np.log10(np.maximum(r, 1e-12) / r[0]) > -70
        vals = []
        for p in PTS:
            i = int(np.argmin(np.abs(f - p)))
            vals.append(f"{H[i]:6.1f}" if ok[i] else "   ---")
        print(f"{name:14s} {tag} " + " ".join(vals), flush=True)
