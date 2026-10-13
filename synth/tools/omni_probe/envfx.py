#!/usr/bin/env python3
"""Effect envelope probe: a held saw (2 s) through an effect in the first
Common FX slot; prints the level every 10 ms (dB) for the first second and
the modulation period/depth. usage: envfx.py name="Type|P0=..,.." ...
env: OURS=1, NOTE, SLOT (EFFMODULE index, default 16)"""
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
EV_ARGS = sys.argv[1:]; sys.argv = sys.argv[:1]
_f = open(os.path.join(HERE, "fexp.py")).read()
exec(_f[:_f.index("def render(")])
_x = open(os.path.join(HERE, "fxprobe.py")).read()
exec(_x[_x.index("\ndef set_fx("):_x.index("\ndef render(xml, exe)")])
base = open(os.path.join(HERE, "init_part.prt_omn")).read()
SLOT = int(os.environ.get("SLOT", "16"))

def rend(xml, exe):
    open("/tmp/ev.prt_omn", "w").write(xml)
    subprocess.run([exe, "/tmp/ev.prt_omn", "/tmp/ev.wav", "--note", os.environ.get("NOTE", "48"),
                    "--hold", "2.0", "--tail", "0.1", "--bpm", os.environ.get("BPM", "120")], capture_output=True, stdin=subprocess.DEVNULL)
    d = open("/tmp/ev.wav", "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)

def env(x, w=480):
    return np.array([20*np.log10(np.sqrt(np.mean(x[k:k+w]**2)) + 1e-9) for k in range(0, len(x) - w, w)])

for spec in EV_ARGS:
    name, _, rest = spec.partition("=")
    typ, _, ps = rest.partition("|")
    params = {int(k[1:]): float(v) for k, v in (p.split("=") for p in filter(None, ps.split(",")))}
    xml = set_fx(base, typ, params, SLOT)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        e = env(rend(xml, exe)); d = env(rend(base, exe))
        g = e - d[20:190].mean()
        seg = g[20:190]
        sp = np.abs(np.fft.rfft(seg - seg.mean())); fr = np.fft.rfftfreq(len(seg), 0.01)
        k = 1 + int(np.argmax(sp[1:]))
        print(f"{name:12s} {tag} gain mean {seg.mean():5.1f} dB  swing {np.percentile(seg,95)-np.percentile(seg,5):5.1f} dB  period {1/fr[k]*1000:6.0f} ms", flush=True)
        print("   " + " ".join(f"{v:.0f}" for v in g[:100]), flush=True)
