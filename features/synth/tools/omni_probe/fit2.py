#!/usr/bin/env python3
"""Joint fit: topology/mode/poles chosen on res 0 + 0.5 + 0.9 together;
then per-knob-setting corner scales from taper.json."""
import json, os, sys, itertools
import numpy as np
SR = 48000
_src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'fit.py')).read()
exec(_src[_src.index("def warp"):_src.index("f = (np.arange")])
d = json.load(open("types.json")); F0 = d["f0"]; curves = d["curves"]
tp = json.load(open("taper.json"))["curves"] if os.path.exists("taper.json") else {}
REF = {0.15: 382.0, 0.3: 1535.0, 0.45: 3513.0, 0.6: 6532.0, 0.75: 11401.0}
f = (np.arange(len(next(iter(curves.values())))) + 1) * F0
band = f < 8000
def db(H): return 20*np.log10(np.abs(H)+1e-12)
def rms(a, b, m): return float(np.sqrt(np.mean((a[m]-b[m])**2))) if m.any() else 99.0
SC = np.exp(np.linspace(np.log(1/16), np.log(16), 97))
def kcrit(n): return 8.0 if n <= 2 else 1/np.cos(np.pi/n)**n

def model(topo, mode, poles, fc, p, comp=0.0):
    return svf_cascade(f, fc, mode, poles, p) if topo == "svf" else ladder(f, fc, mode, poles, p, comp)

def fit_scale(m, topo, mode, poles, p, comp, gain, ref_hz):
    mk = (m > -50) & band; best = (99, 1.0)
    for sc in SC:
        fc = ref_hz*sc
        if fc > 21000: continue
        e = rms(m, db(model(topo, mode, poles, fc, p, comp)) + gain, mk)
        if e < best[0]: best = (e, float(sc))
    return best

out = {}
types = sorted({k.split("_r")[0] for k in curves})
for t in types:
    ms = {r: np.array(curves[f"{t}_r{r}/omni"]) for r in ["0", "0.5", "0.9"]}
    m0 = ms["0"]; mk0 = (m0 > -50) & band
    cands = []
    for topo, mode, poles in itertools.chain(
        itertools.product(["svf"], ["lp", "hp", "bp", "notch"], [1, 2, 3, 4, 6, 8]),
        itertools.product(["ladder"], ["lp", "hp"], [1, 2, 3, 4, 6, 8])):
        if mode in ("bp", "notch") and poles % 2: continue
        for q0 in ([0.5, 0.6, 0.7071, 0.85, 1.0] if topo == "svf" else [0.0]):
            for sc in SC:
                fc = 1535*sc
                if fc > 21000: continue
                H = db(model(topo, mode, poles, fc, q0))
                g = float(np.median((m0-H)[mk0][:6]))
                cands.append((rms(m0, H+g, mk0), topo, mode, poles, q0, float(sc), g))
    cands.sort()
    # Keep the best few distinct structures within 1 dB of the best at res 0.
    seen, short = set(), []
    for c in cands:
        key = c[1:4]
        if key in seen: continue
        if c[0] > cands[0][0] + 1.0: break
        seen.add(key); short.append(c)
        if len(short) >= 6: break
    best = None
    for e0, topo, mode, poles, q0, sc, g in short:
        fc = 1535*sc; res = {}; tot = e0
        for r in ["0.5", "0.9"]:
            m = ms[r]; mk = (m > -50) & band; bb = None
            shifts = np.exp(np.linspace(np.log(0.5), np.log(2), 13))
            if topo == "svf":
                grid = [(p, 0.0) for p in np.exp(np.linspace(np.log(0.5), np.log(40), 40))]
            else:
                grid = [(p, c) for p in np.linspace(0, 1.2*kcrit(poles) if poles > 2 else 8, 40) for c in (-0.25, 0.0, 0.25, 0.5, 0.75, 1.0)]
            for sh in shifts:
                for p, c in grid:
                    ee = rms(m, db(model(topo, mode, poles, fc*sh, p, c)) + g, mk)
                    if bb is None or ee < bb[0]: bb = (ee, float(p), float(sh), c)
            res[r] = dict(err=round(bb[0], 2), p=round(bb[1], 3), shift=round(bb[2], 3), comp=bb[3])
            tot += bb[0]
        if best is None or tot < best[0]:
            best = (tot, dict(err=round(e0, 2), topo=topo, mode=mode, poles=poles, scale=round(sc, 4), q0=q0, gain=round(g, 2), res=res))
    v = best[1]
    # Per-setting corner scale (shape fixed).
    tap = {"0.3": v["scale"]}
    for vv, ref_hz in REF.items():
        key = f"{t}_v{vv}/omni"
        if key in tp:
            e, sc = fit_scale(np.array(tp[key]), v["topo"], v["mode"], v["poles"], v["q0"], 0.0, v["gain"], ref_hz)
            tap[str(vv)] = (round(sc, 4), round(e, 2))
    v["taper"] = tap
    out[t] = v
    print(t, v, flush=True)
json.dump(out, open("fit2.json", "w"), indent=1)
