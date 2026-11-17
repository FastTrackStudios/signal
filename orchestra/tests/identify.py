#!/usr/bin/env python3
"""Identify what Kontakt actually played for one note.

    python3 identify.py <kontakt.wav> <t_start_s> <win_s> <fit_from_s> <fit_to_s> <candidate.wav>... [--cents LO:HI]

Explains a window of a Kontakt render as a weighted sum of raw sample files
(e.g. the dynamic layers of one note on the active mic). Kontakt plays a zone
retuned by the zone/group/instrument tune, so every candidate is resampled by
a common pitch offset (cents) found by search: coarse grid over LO:HI, then
golden-section refinement. For each cents value the candidates are placed at
their cross-correlation lag and solved jointly by least squares over
[fit_from, fit_to] (seconds into the window) with one constant gain each.

Reports the best cents, each candidate's onset (ms after t_start, fractional
via the lag), gain in dB, and the joint residual. A residual near the noise
floor means the note is exactly those files, at that tune and those gains.
"""
import os, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load, window  # noqa: E402

TAPS = 32  # windowed-sinc half width


def resample(x, ratio, n_out):
    """Read x at positions k*ratio (ratio > 1 = pitched up), windowed sinc."""
    pos = np.arange(n_out) * ratio
    i0 = np.floor(pos).astype(np.int64)
    frac = pos - i0
    out = np.zeros((n_out, x.shape[1]))
    xp = np.concatenate([np.zeros((TAPS, x.shape[1])), x, np.zeros((TAPS + 2, x.shape[1]))])
    for k in range(-TAPS + 1, TAPS + 1):
        d = k - frac
        w = np.sinc(d) * (0.5 + 0.5 * np.cos(np.pi * d / TAPS))
        idx = np.clip(i0 + k + TAPS, 0, len(xp) - 1)
        out += xp[idx] * w[:, None]
    return out


def lag_of(seg, cand, max_lag):
    n = len(seg) + len(cand)
    nfft = 1 << (n - 1).bit_length()
    acc = np.zeros(nfft)
    for c in range(2):
        acc += np.fft.irfft(np.fft.rfft(seg[:, c], nfft) * np.conj(np.fft.rfft(cand[:, c], nfft)), nfft)
    return int(np.argmax(acc[: max_lag + 1]))


def fit(seg, raws, cents, a, b):
    ratio = 2.0 ** (cents / 1200.0)
    cols, lags = [], []
    for raw in raws:
        c = resample(raw, ratio, int(len(raw) / ratio) - 2 * TAPS)
        lag = lag_of(seg, c, len(seg) // 2)
        placed = np.zeros_like(seg)
        n = min(len(c), len(seg) - lag)
        placed[lag : lag + n] = c[:n]
        cols.append(placed)
        lags.append(lag)
    A = np.stack([np.concatenate([c[a:b, 0], c[a:b, 1]]) for c in cols], axis=1)
    y = np.concatenate([seg[a:b, 0], seg[a:b, 1]])
    g, *_ = np.linalg.lstsq(A, y, rcond=None)
    res = db(np.sum((y - A @ g) ** 2) / np.sum(y**2))
    return res, g, lags


def main():
    argv = sys.argv[1:]
    lo, hi = -20.0, 20.0
    if "--cents" in argv:
        i = argv.index("--cents")
        lo, hi = map(float, argv[i + 1].split(":"))
        del argv[i : i + 2]
    if len(argv) < 6:
        raise SystemExit(__doc__)
    kon = load(argv[0])
    t0, win, f0, f1 = map(float, argv[1:5])
    files = argv[5:]
    s = int(round(t0 * SR))
    seg = window(kon, s, s + int(round(win * SR)))
    a, b = int(f0 * SR), int(f1 * SR)
    # Only the part of each file that can reach the fit range matters.
    raws = [load(f)[: int((f1 + 0.1) * SR * 1.05)] for f in files]

    grid = np.arange(lo, hi + 1e-9, 1.0)
    scores = [fit(seg, raws, c, a, b)[0] for c in grid]
    best = float(grid[int(np.argmin(scores))])
    # golden-section on [best-1, best+1]
    gr = (np.sqrt(5) - 1) / 2
    x0, x3 = best - 1.0, best + 1.0
    x1, x2 = x3 - gr * (x3 - x0), x0 + gr * (x3 - x0)
    f1v, f2v = fit(seg, raws, x1, a, b)[0], fit(seg, raws, x2, a, b)[0]
    for _ in range(20):
        if f1v < f2v:
            x3, x2, f2v = x2, x1, f1v
            x1 = x3 - gr * (x3 - x0)
            f1v = fit(seg, raws, x1, a, b)[0]
        else:
            x0, x1, f1v = x1, x2, f2v
            x2 = x0 + gr * (x3 - x0)
            f2v = fit(seg, raws, x2, a, b)[0]
    cents = (x0 + x3) / 2
    res, g, lags = fit(seg, raws, cents, a, b)
    common = os.path.commonpath(files) if len(files) > 1 else os.path.dirname(files[0])
    print(f"tune {cents:+.3f} cents   (coarse grid best {best:+.0f}: {min(scores):.1f} dB)")
    for name, lag, gi in sorted(zip(files, lags, g), key=lambda t: -abs(t[2])):
        gdb = 20 * np.log10(abs(gi)) if gi != 0 else -999
        print(f"  {os.path.relpath(name, common):45} onset {lag / SR * 1000:8.2f} ms  gain {gdb:7.2f} dB{'  (neg)' if gi < 0 else ''}")
    print(f"joint residual over [{f0},{f1}]s: {res:.1f} dB")


if __name__ == "__main__":
    main()
