#!/usr/bin/env python3
"""Measure Kontakt's per-group CC volume curves from a gen_probe.py --curves render.

    python3 curve_fit.py <kontakt.wav> <manifest.tsv> <css_root> [--json out.json]

Every unit plays MIDI 67 at velocity 40 (first-note zone: sample 0 or the
zone's own sample start, no script fades). Its steady body is fitted at lag 0
as a least-squares sum of the candidate group files, each read at its exact
Kontakt tune (group +9 c x zone tune) from its zone sample start:

  CURVE-CC1   Nonvib p/mf/ff  → the CC1 curve of each dynamic group
  CURVE-CC11  Nonvib ff       → the CC11 volume law
  CURVE-CC2   Nonvib ff + Vibsus ff → the CC2 crossfade

Gains are printed relative to UNITY_GAIN (Kontakt's level for a group whose
curves all read 1.0), so each number IS the curve value Kontakt applied.
"""
import csv, json, os, re, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load, window  # noqa: E402
from identify import resample, TAPS  # noqa: E402

UNITY_GAIN = 1.3414  # measured: nonvib ff, CC1=CC11=127, CC2=0 (probe #0)
GROUP_TUNE = 1.0052122
# MIDI 67, Mix mic: (file under <css_root>/Mix/1st Violins/Sustain, zone tune, vel-1 sample start)
VOICES = {
    "NV p": ("Nonvib/p_G3.wav", 0.997116, 0),
    "NV mf": ("Nonvib/mf_G3.wav", 1.0013872, 0),
    "NV ff": ("Nonvib/ff_G3.wav", 1.0018501, 0),
    "VS ff": ("Vibsus/ff_G3.wav", 1.0, 12284),
}
SETS = {
    "CURVE-CC1": ["NV p", "NV mf", "NV ff"],
    "CURVE-CC11": ["NV ff"],
    "CURVE-CC2": ["NV ff", "VS ff"],
}
FIT = (0.35, 1.15)  # seconds after note-on: after the CC lags settle, before note-off


def main():
    argv = sys.argv[1:]
    jpath = None
    if "--json" in argv:
        i = argv.index("--json")
        jpath = argv[i + 1]
        del argv[i : i + 2]
    kon_p, man_p, root = argv
    kon = load(kon_p)
    base = os.path.join(root, "Mix", "1st Violins", "Sustain")
    cache = {}
    for name, (f, zt, start) in VOICES.items():
        raw = load(os.path.join(base, f))[start : start + int(1.6 * SR)]
        r = GROUP_TUNE * zt
        cache[name] = resample(raw, r, int(len(raw) / r) - 2 * TAPS)
    a, b = int(FIT[0] * SR), int(FIT[1] * SR)
    out = []
    for row in csv.DictReader(open(man_p), delimiter="\t"):
        cat = row["category"]
        if cat not in SETS:
            continue
        s = int(round(float(row["t_start"]) * SR))
        seg = window(kon, s, s + b)
        names = SETS[cat]
        A = np.stack([np.concatenate([cache[n][a:b, 0], cache[n][a:b, 1]]) for n in names], 1)
        y = np.concatenate([seg[a:b, 0], seg[a:b, 1]])
        g, *_ = np.linalg.lstsq(A, y, rcond=None)
        res = db(np.sum((y - A @ g) ** 2) / max(np.sum(y * y), 1e-30))
        val = int(re.search(r"cc\d+=(\d+)", row["description"]).group(1))
        vals = {n: float(gi / UNITY_GAIN) for n, gi in zip(names, g)}
        out.append(dict(category=cat, cc=val, values=vals, residual_db=res))
        print(f"{cat:10} cc={val:3}  " + "  ".join(f"{n} {v:7.4f}" for n, v in vals.items()) + f"   res {res:6.1f} dB")
    if jpath:
        json.dump(out, open(jpath, "w"), indent=1)


if __name__ == "__main__":
    main()
