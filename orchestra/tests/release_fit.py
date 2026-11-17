#!/usr/bin/env python3
"""Measure Kontakt's note-off: the body's release envelope and the release
sample's gain envelope, separately, from one render.

    python3 release_fit.py <kontakt.wav> <note_on_s> <note_off_s> <body.wav> <body_rate> <rel.wav> <rel_rate> [rel_delay_ms]

After note-off Kontakt's output is (body file, still advancing at its rate) x
body_env(t) + (release file, started rel_delay_ms after note-off) x rel_env(t).
Both files are read exactly where Kontakt reads them (windowed sinc), and in
every 10 ms window the two gains are solved jointly by least squares. Prints
both envelopes (linear, relative to the body's held level) and the fit
residual per window — the residual says whether the two-voice model is the
whole story.
"""
import os, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load  # noqa: E402
from identify import TAPS  # noqa: E402


def read(x, pos):
    i0 = np.floor(pos).astype(np.int64)
    frac = pos - i0
    out = np.zeros((len(pos), 2))
    for k in range(-TAPS + 1, TAPS + 1):
        d = k - frac
        w = np.sinc(d) * (0.5 + 0.5 * np.cos(np.pi * d / TAPS))
        idx = np.clip(i0 + k, 0, len(x) - 1)
        valid = ((i0 + k) >= 0) & ((i0 + k) < len(x))
        out += x[idx] * (w * valid)[:, None]
    return out


def main():
    a = sys.argv[1:]
    if len(a) < 7:
        raise SystemExit(__doc__)
    kon = load(a[0])
    t_on, t_off = float(a[1]), float(a[2])
    body, br = load(a[3]), float(a[4])
    rel, rr = load(a[5]), float(a[6])
    delay = float(a[7]) if len(a) > 7 else 2.0
    on, off = int(round(t_on * SR)), int(round(t_off * SR))
    # held level of the body just before note-off (reference = 1.0)
    n = np.arange(off - int(0.3 * SR), off)
    y = read(body, (n - on) * br)
    held = float(np.dot(y.ravel(), kon[n].ravel()) / np.dot(y.ravel(), y.ravel()))
    rel_start = off + delay * SR / 1000.0
    print(f"held body gain {held:.4f} (×file).  t = ms after note-off")
    print("   t     body_env   rel_gain(×file)   residual")
    W = int(0.01 * SR)
    for w0 in range(off - 2 * W, off + int(1.3 * SR), W):
        n = np.arange(w0, w0 + W)
        B = read(body, (n - on) * br)
        Rpos = (n - rel_start) * rr
        R = read(rel, Rpos) if Rpos[-1] > 0 else np.zeros_like(B)
        A = np.stack([B.ravel(), R.ravel()], 1)
        t = kon[n].ravel()
        if np.sum(t * t) < 1e-14:
            continue
        cols = [i for i in range(2) if np.sum(A[:, i] ** 2) > 1e-14]
        g = np.zeros(2)
        sol, *_ = np.linalg.lstsq(A[:, cols], t, rcond=None)
        g[cols] = sol
        res = db(np.sum((t - A @ g) ** 2) / np.sum(t * t))
        print(f"{(w0 - off) / SR * 1000:6.0f}   {g[0] / held:8.4f}   {g[1]:10.4f}        {res:6.1f}")


if __name__ == "__main__":
    main()
