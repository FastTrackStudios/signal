#!/usr/bin/env python3
"""Free per-window gains of named voices in a Kontakt render.

    python3 voice_fit.py <render.wav> <t_ref_s> <from_ms> <to_ms> <win_ms> VOICE...

VOICE = path:start_frame:rate:onset_ms (onset relative to t_ref). Prints the
least-squares gain of every voice per window and the residual — the first
step in explaining a transition before modelling it.
"""
import os, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load  # noqa: E402
from identify import resample  # noqa: E402


def main():
    wav, tref, a, b, w = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4]), float(sys.argv[5])
    k = load(wav)
    n0 = int(round(tref * SR))
    voices = []
    for spec in sys.argv[6:]:
        path, st, rate, on = spec.rsplit(':', 3)
        x = load(path)
        on_f = int(round(float(on) * SR / 1000))
        y = resample(x[int(st):], float(rate), int((b + 50) * SR / 1000) - on_f + 64)
        voices.append((os.path.basename(path), on_f, y))
    W = int(w * SR / 1000)
    print("  ms    " + "  ".join(f"{v[0][:10]:>10}" for v in voices) + "   res")
    for s0 in range(int(a * SR / 1000), int(b * SR / 1000), W):
        seg = k[n0 + s0:n0 + s0 + W].ravel()
        cols = []
        for _, on, y in voices:
            i = s0 - on
            cols.append(y[i:i + W].ravel() if i >= 0 and i + W <= len(y) else np.zeros(2 * W))
        M = np.stack(cols, 1)
        ok = [j for j in range(M.shape[1]) if np.dot(M[:, j], M[:, j]) > 1e-12]
        g = np.zeros(M.shape[1])
        if ok:
            g[ok], *_ = np.linalg.lstsq(M[:, ok], seg, rcond=None)
        e = np.sum((seg - M @ g) ** 2) / (np.sum(seg * seg) + 1e-30)
        print(f"{s0 / SR * 1000:6.0f}  " + "  ".join(f"{v:10.3f}" for v in g) + f"  {db(e):6.1f}")


if __name__ == "__main__":
    main()
