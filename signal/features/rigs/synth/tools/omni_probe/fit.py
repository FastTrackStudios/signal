#!/usr/bin/env python3
"""Fit each Omnisphere filter type's measured response (types.json) to a
realizable model: topology (svf cascade | ladder), mode, poles, cutoff scale
relative to the knob table, Q per resonance setting, passband gain."""
import json, os, sys, itertools
import numpy as np
SR = 48000
d = json.load(open(sys.argv[1] if len(sys.argv) > 1 else "types.json"))
F0 = d["f0"]; curves = d["curves"]
KNOB_HZ = float(sys.argv[2]) if len(sys.argv) > 2 else 1535.0  # 0.22 type f3 at this v

def warp(f, fc):
    return np.tan(np.pi * np.minimum(f, SR * 0.49) / SR) / np.tan(np.pi * min(fc, SR * 0.45) / SR)

def svf_cascade(f, fc, mode, poles, q):
    x = warp(f, fc); s = 1j * x; H = np.ones_like(s)
    n2, odd = divmod(poles, 2)
    for i in range(n2):
        qq = q if i == 0 else 0.7071
        den = s * s + s / qq + 1
        H = H * {"lp": 1 / den, "hp": s * s / den, "bp": (s / qq) / den, "notch": (s * s + 1) / den}[mode]
    if odd:
        H = H * ({"lp": 1 / (1 + s), "hp": s / (1 + s)}.get(mode, 1 / (1 + s)))
    return H

def ladder(f, fc, mode, poles, k, comp=0.0):
    x = warp(f, fc); s = 1j * x
    G = (1 / (1 + s)) if mode == "lp" else (s / (1 + s))
    Gn = G ** poles
    return Gn * (1 + k * comp) / (1 + k * Gn)

f = (np.arange(len(next(iter(curves.values())))) + 1) * F0
def err(meas, model_db, mask):
    return float(np.sqrt(np.mean((meas[mask] - model_db[mask]) ** 2)))

types = sorted({k.split("_r")[0] for k in curves})
res_keys = ["0", "0.5", "0.9"]
out = {}
for t in types:
    ms = {r: np.array(curves[f"{t}_r{r}/omni"]) for r in res_keys if f"{t}_r{r}/omni" in curves}
    if "0" not in ms: continue
    m0 = ms["0"]
    mask = (m0 > -50) & (f < 8000)
    best = None
    for topo, mode, poles in itertools.chain(
        itertools.product(["svf"], ["lp", "hp", "bp", "notch"], [1, 2, 3, 4, 6, 8]),
        itertools.product(["ladder"], ["lp", "hp"], [1, 2, 3, 4, 6, 8]),
    ):
        if mode in ("bp", "notch") and poles % 2: continue
        for scale in np.exp(np.linspace(np.log(1/16), np.log(16), 97)):
            fc = KNOB_HZ * scale
            if fc > 21000: continue
            for g in [0.0]:
                if topo == "svf":
                    for q in [0.5, 0.6, 0.7071, 0.85, 1.0]:
                        H = 20*np.log10(np.abs(svf_cascade(f, fc, mode, poles, q)) + 1e-12)
                        gain = float(np.median((m0 - H)[mask][:6]))
                        e = err(m0, H + gain, mask)
                        if best is None or e < best[0]: best = (e, topo, mode, poles, scale, q, gain)
                else:
                    H = 20*np.log10(np.abs(ladder(f, fc, mode, poles, 0.0)) + 1e-12)
                    gain = float(np.median((m0 - H)[mask][:6]))
                    e = err(m0, H + gain, mask)
                    if best is None or e < best[0]: best = (e, topo, mode, poles, scale, 0.0, gain)
    e, topo, mode, poles, scale, q0, gain = best
    fc = KNOB_HZ * scale
    # Resonance: with everything else fixed, fit q (svf) or k (ladder) per setting,
    # letting the cutoff shift too (resonance moves some models' peak).
    resfit = {}
    for r, m in ms.items():
        if r == "0": continue
        mk = (m > -50) & (f < 8000); bb = None
        shifts = [1.0] if os.environ.get("NOSHIFT") else np.exp(np.linspace(np.log(0.5), np.log(2), 25))
        for sh in shifts:
            if topo == "svf":
                for p in np.exp(np.linspace(np.log(0.5), np.log(40), 60)):
                    H = 20*np.log10(np.abs(svf_cascade(f, fc*sh, mode, poles, p))+1e-12) + gain
                    ee = err(m, H, mk)
                    if bb is None or ee < bb[0]: bb = (ee, float(p), float(sh), 0.0)
            else:
                for p in np.linspace(0, 4.4, 45):
                    for c in (0.0, 0.25, 0.5, 0.75, 1.0):
                        H = 20*np.log10(np.abs(ladder(f, fc*sh, mode, poles, p, c))+1e-12) + gain
                        ee = err(m, H, mk)
                        if bb is None or ee < bb[0]: bb = (ee, float(p), float(sh), c)
        resfit[r] = dict(err=round(bb[0],2), p=round(bb[1],3), shift=round(bb[2],3), comp=bb[3])
    out[t] = dict(err=round(e,2), topo=topo, mode=mode, poles=poles, scale=round(float(scale),4), q0=q0, gain=round(gain,2), res=resfit)
    print(t, out[t], flush=True)
json.dump(out, open("fit.json", "w"), indent=1)
