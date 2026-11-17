#!/usr/bin/env python3
"""Check Kontakt's playback of a probe unit against the instrument's own zone table.

    python3 predict_fit.py <kontakt.wav> <manifest.tsv> <zones.tsv> <css_root> <mic> <unit_idx>... [--fit A:B]

For each unit (key from the manifest; articulation family and layer from the
description, e.g. "nonvib ff only key67") the zone covering the key is read
from zones.tsv (vel 1 = the first-note body zone), its NCW sample name is
mapped to the extracted WAV (<css_root>/<mic>/1st Violins/Sustain/<Art>/<layer>_<note>.wav,
NI naming C3=60), and the PREDICTED tune is

    group +9.0 c  +  1200*log2(zone tune)  +  100 * (key - root)

The unit is then fitted twice: at the predicted tune (gain + lag only) and
with the tune free (+-3 c). Agreement within ~0.05 c and a residual near the
-60..-70 dB interpolation floor means the zone table + rule fully explain it.
"""
import csv, os, re, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load, window  # noqa: E402
from identify import resample, lag_of, TAPS  # noqa: E402

GROUP_CENTS = 1200 * np.log2(1.0052122)


def fit_at(seg, raw, cents, a, b):
    r = 2 ** (cents / 1200)
    c = resample(raw, r, int(len(raw) / r) - 2 * TAPS)
    lag = lag_of(seg, c, int(0.05 * SR))
    placed = np.zeros_like(seg)
    n = min(len(c), len(seg) - lag)
    placed[lag : lag + n] = c[:n]
    x = np.concatenate([placed[a:b, 0], placed[a:b, 1]])
    y = np.concatenate([seg[a:b, 0], seg[a:b, 1]])
    g = float(np.dot(x, y) / max(np.dot(x, x), 1e-30))
    return db(np.sum((y - g * x) ** 2) / np.sum(y * y)), g, lag


def main():
    argv = sys.argv[1:]
    fa, fb = 0.05, 3.0
    if "--fit" in argv:
        i = argv.index("--fit")
        fa, fb = map(float, argv[i + 1].split(":"))
        del argv[i : i + 2]
    kon_p, man_p, zones_p, root, mic = argv[:5]
    units = [int(u) for u in argv[5:]]
    kon = load(kon_p)
    rows = {int(r["idx"]): r for r in csv.DictReader(open(man_p), delimiter="\t")}
    zones = list(csv.DictReader(open(zones_p), delimiter="\t"))
    for u in units:
        r = rows[u]
        key = int(r["pitch"])
        desc = r["description"]
        art = "Nonvib" if "nonvib" in desc else "Vibsus"
        layer = re.search(r"\b(ppp|p|mf|ff)\b", desc).group(1)
        pat = f"1v_{art}_{mic}_{layer}_"
        z = [z for z in zones if int(z["key_lo"]) <= key <= int(z["key_hi"]) and z["vel_lo"] == "1"
             and os.path.basename(z["sample"]).startswith(pat)]
        if len(z) != 1:
            print(f"#{u} {desc}: {len(z)} zones match {pat} key {key}")
            continue
        z = z[0]
        note = os.path.basename(z["sample"])[len(pat):-4]
        wav = os.path.join(root, mic, "1st Violins", "Sustain", art, f"{layer}_{note}.wav")
        pred = GROUP_CENTS + 1200 * np.log2(float(z["tune"])) + 100 * (key - int(z["root"]))
        s = int(round(float(r["t_start"]) * SR))
        seg = window(kon, s, s + int((fb + 0.2) * SR))
        raw = load(wav)[: int((fb + 0.5) * SR * 1.2)]
        a, b = int(fa * SR), int(fb * SR)
        res_p, g_p, lag_p = fit_at(seg, raw, pred, a, b)
        best = min(((fit_at(seg, raw, c, a, b)[0], c) for c in np.arange(pred - 3, pred + 3.01, 0.25)))
        lo, hi = best[1] - 0.25, best[1] + 0.25
        for _ in range(12):
            m1, m2 = lo + (hi - lo) / 3, hi - (hi - lo) / 3
            if fit_at(seg, raw, m1, a, b)[0] < fit_at(seg, raw, m2, a, b)[0]:
                hi = m2
            else:
                lo = m1
        c_free = (lo + hi) / 2
        res_f, g_f, lag_f = fit_at(seg, raw, c_free, a, b)
        print(f"#{u:<2} {desc:42} {art}/{layer}_{note} zone tune {float(z['tune']):.7f} zvol {z['zvol']}")
        print(f"     predicted {pred:+8.3f} c → {res_p:6.1f} dB  gain {20*np.log10(abs(g_p)):+6.2f} dB  onset {lag_p/SR*1000:6.2f} ms")
        print(f"     free      {c_free:+8.3f} c → {res_f:6.1f} dB  gain {20*np.log10(abs(g_f)):+6.2f} dB  onset {lag_f/SR*1000:6.2f} ms")


if __name__ == "__main__":
    main()
