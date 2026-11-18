#!/usr/bin/env python3
"""Reverb probe: a click through an effect (fxprobe's setup), analysed as an
impulse response: RT60 overall and in low (<400 Hz) / mid / high (>4 kHz)
bands by Schroeder integration (fit −5…−25 dB), the onset (predelay, ms
after the click), wet level (dB re the dry click) and stereo width.

usage: rvprobe.py name="Type|P0=..,.." ...   env: OURS=1, EDITS, SLOT
"""
import os, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
RV_ARGS = sys.argv[1:]; sys.argv = sys.argv[:1]
src = open(os.path.join(HERE, "fxprobe.py")).read()
exec(src[:src.index("for spec in _args:")])
os.environ["TAIL"] = os.environ.get("TAIL", "10.0")

def band(x, lo, hi):
    X = np.fft.rfft(x); f = np.fft.rfftfreq(len(x), 1/SR)
    X[(f < lo) | (f >= hi)] = 0
    return np.fft.irfft(X, len(x))

def rt60(x):
    e = x**2
    edc = np.cumsum(e[::-1])[::-1]; edc = 10*np.log10(edc/edc[0] + 1e-30)
    i5, i25 = np.argmax(edc < -5), np.argmax(edc < -25)
    if i25 <= i5: return None
    t = np.arange(i5, i25) / SR
    slope = np.polyfit(t, edc[i5:i25], 1)[0]
    return round(-60/slope, 2) if slope < 0 else None

def rv(x, ref):
    m = x.mean(1)
    tail = m[int(0.035*SR):]  # after the 30 ms click
    env = np.abs(tail)
    on = np.argmax(env > env.max()*0.01)
    wet = 10*np.log10(np.sum(tail**2)/ref + 1e-30)
    side = 10*np.log10(np.mean((x[:,0]-x[:,1])**2)/max(np.mean((x[:,0]+x[:,1])**2),1e-20)+1e-12)
    if os.environ.get("OCT"):
        return {f"{int(c)}": rt60(band(tail, c/1.414, c*1.414)) for c in (125, 250, 500, 1000, 2000, 4000, 8000)}
    return dict(rt=rt60(tail), rt_lo=rt60(band(tail, 20, 400)), rt_mid=rt60(band(tail, 400, 4000)),
                rt_hi=rt60(band(tail, 4000, 20000)), onset_ms=round(on/SR*1000+35, 1),
                wet_db=round(float(wet), 1), side_db=round(float(side), 1))

REFE = {}
for spec in RV_ARGS:
    name, _, rest = spec.partition("=")
    typ, _, ps = rest.partition("|")
    params = {int(k[1:]): float(v) for k, v in (p.split("=") for p in filter(None, ps.split(",")))}
    xml = set_click(base)
    if typ:
        xml = set_fx(xml, typ, params, int(os.environ.get("SLOT", "16")))
    for e in filter(None, os.environ.get("EDITS", "").split(";")):
        xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag == "omni" and os.environ.get("ONLYOURS"): continue
        if tag not in REFE:
            d = render(set_click(base), exe).mean(1); REFE[tag] = float(np.sum(d[:int(0.035*SR)]**2))
        print(f"{name:12s} {tag} {rv(render(xml, exe), REFE[tag])}", flush=True)
