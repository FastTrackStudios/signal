#!/usr/bin/env python3
"""Fit Kontakt's random pitch-envelope draws (CSW marcato) for replay.

    python3 fit_pitch_env.py <render_css_live> <Section> <probe.mid> <kontakt.wav> <out.wav> [--hold-ms 333] [--decay LO:HI]

A `pitch_env_random` articulation draws, per strike, a depth `c` (cents)
and a decay length `D` (ms) for a pitch envelope that holds `c` for the
hold, then falls linearly to 0 over `D` (KSP 9220-9240). Kontakt's draws
cannot be reproduced; they are read off its render instead of searched by
rendering: with our render made at depth 0, Kontakt's sample position runs
ahead of ours by

    L(t) = SR · ln2/1200 · c · ∫₀ᵗ env(τ) dτ        (frames)

so the alignment between the two (tracked in short steps, unwrapped)
traces a ramp whose height is `c` and whose bend is `D`. Each strike — a
spawn of the articulation's voices in our `--trace` — gets the `(c, D)`
whose ramp fits its alignment best (D on a grid, c by least squares).
The alignment read is only good to a few cents, and a cent off leaves the
strike's residual a few dB worse (the depth's integral is a phase offset
that stays): each strike's depth is then searched by rendering, around the
read value (REFINE_CENTS, and 0), for the least residual against Kontakt up
to the next strike, every other draw held (REFINE_ROUNDS passes; one
windowed batch per pass). Prints `c:D,c:D,…` (render_css_live --pitch-envs)
and writes our render with those draws to <out.wav>.

Env: FIT_EXTRA other forced draws for every render; CS_LIBRARY the library.
"""
import os, sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fit_draws import render, render_batch, energy  # noqa: E402
from null_test import SR, load  # noqa: E402

MAX_DRAWS = 400
STEP = 0.02  # s between alignment reads
WIN = 0.02  # s per read
SEARCH = 8.0  # frames each read may move from the last
FIT_SPAN = 1.3  # s after the strike (hold + longest decay + margin)
A = SR * np.log(2) / 1200  # frames per cent-second
REFINE_CENTS = (-3.0, -2.0, -1.5, -1.0, -0.5, 0.5, 1.0, 1.5, 2.0, 3.0)
REFINE_ROUNDS = 2
REFINE_CHUNK = 32


def track(k, o, t0, t1):
    """Lag (frames, ours later = +) of `o` against `k` from t0 to t1, unwrapped:
    each read searches near the last one (quarter-frame steps)."""
    out, prev = [], 0.0
    for t in np.arange(t0, t1 - WIN, STEP):
        s = int(t * SR)
        e = s + int(WIN * SR)
        kk = k[s:e, 0]
        if np.sum(kk * kk) < 1e-10:
            break
        best = None
        for lag in np.arange(prev - SEARCH, prev + SEARCH + 1e-9, 0.25):
            i = int(np.floor(lag))
            f = lag - i
            if s + i < 0 or e + i + 1 > len(o):
                continue
            oo = (1 - f) * o[s + i : e + i, 0] + f * o[s + i + 1 : e + i + 1, 0]
            r = np.sum((kk - oo) ** 2)
            if best is None or r < best[1]:
                best = (lag, r)
        if best is None:
            break
        prev = best[0]
        out.append((t - t0, prev))
    return np.array(out)


def ramp(t, hold, decay):
    """∫ env: 1 over the hold, a linear fall to 0 over `decay`, then 0 (s)."""
    h, d = hold, decay
    return np.where(
        t < h, t, np.where(t < h + d, h + (t - h) - (t - h) ** 2 / (2 * d), h + d / 2)
    )


def fit(curve, hold, decays):
    """Best (cents, decay_ms) for one strike's alignment curve."""
    if len(curve) < 5:
        return 0.0, decays[0] * 1000
    t, lag = curve[:, 0], curve[:, 1]
    best = None
    for d in decays:
        f = A * ramp(t, hold, d)
        # lag = b + c·f, least squares (b: a fixed offset, not the draw)
        m = np.vstack([np.ones_like(f), f]).T
        (b, c), *_ = np.linalg.lstsq(m, lag, rcond=None)
        r = float(np.sum((lag - b - c * f) ** 2))
        if best is None or r < best[0]:
            best = (r, c, d)
    # ours is later (+lag) where Kontakt runs ahead: Kontakt sharp by c
    return float(best[1]), best[2] * 1000


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    binary, section, mid, kon, out_wav = args[:5]
    hold = 0.333
    lo, hi = 333.0, 666.0
    for i, a in enumerate(sys.argv):
        if a == "--hold-ms":
            hold = float(sys.argv[i + 1]) / 1000
        if a == "--decay":
            lo, hi = map(float, sys.argv[i + 1].split(":"))
    decays = np.arange(lo, hi + 1e-9, 8.0) / 1000
    k = load(kon)
    zero = [(0.0, lo)] * MAX_DRAWS
    flag = "--pitch-envs"
    fmt = lambda draws: ",".join(f"{c:.3f}:{d:.1f}" for c, d in draws)  # noqa: E731
    o, voices = render_pe(binary, section, mid, flag, fmt(zero), out_wav)
    # strikes: the frames our engine drew at (any voice of a pitch_env_random
    # articulation; Marcato for CSW) — the order the draws are consumed in
    strikes = sorted({round(t, 4) for t, desc in voices if "/Marcato/" in desc})
    draws = []
    for i, t0 in enumerate(strikes):
        t1 = min(strikes[i + 1] if i + 1 < len(strikes) else t0 + FIT_SPAN, t0 + FIT_SPAN)
        draws.append(fit(track(k, o, t0 + 0.01, t1), hold, decays))
    windows = []
    for i, t0 in enumerate(strikes):
        t1 = strikes[i + 1] if i + 1 < len(strikes) else t0 + FIT_SPAN
        windows.append((int(t0 * SR), min(int(t1 * SR), len(k))))
    tmp = os.path.dirname(os.path.abspath(out_wav))
    for _ in range(REFINE_ROUNDS):
        ours, _ = render_pe(binary, section, mid, flag, fmt(draws), out_wav)
        best = [(c, energy(k[a:b] - ours[a:b])) for (c, _), (a, b) in zip(draws, windows)]
        trials = []
        for i, (c, d) in enumerate(draws):
            for v in sorted({round(c + dc, 3) for dc in REFINE_CENTS} | {0.0}):
                if v != round(c, 3):
                    trials.append((i, v))
        for n in range(0, len(trials), REFINE_CHUNK):
            chunk = trials[n:n + REFINE_CHUNK]
            jobs = []
            for j, (i, v) in enumerate(chunk):
                trial = draws[:i] + [(v, draws[i][1])] + draws[i + 1:]
                jobs.append((trial, f"{tmp}/.pe_{os.getpid()}_{j}.wav", False, windows[i]))
            for (i, v), (y, _) in zip(chunk, render_pe_batch(binary, section, mid, flag, fmt, jobs)):
                a, b = windows[i]
                e = energy(k[a:b] - y[: b - a])
                if e < best[i][1]:
                    best[i] = (v, e)
        changed = [i for i in range(len(draws)) if best[i][0] != draws[i][0]]
        for i in changed:
            draws[i] = (best[i][0], draws[i][1])
        if not changed:
            break
    render_pe(binary, section, mid, flag, fmt(draws), out_wav)
    print(fmt(draws))
    print(f"{len(draws)} draws", file=sys.stderr)


def render_pe(binary, section, mid, flag, values, out):
    """One traced render with the draws string forced through `flag`."""
    extra = os.environ.get("FIT_EXTRA", "")
    os.environ["FIT_EXTRA"] = f"{extra} {flag} {values}".strip()
    try:
        return render(binary, section, mid, None, None, out, trace=True)
    finally:
        os.environ["FIT_EXTRA"] = extra


def render_pe_batch(binary, section, mid, flag, fmt, jobs):
    """Windowed renders `(draws, out, trace, window)`, each with its own draws
    forced through `flag` (a one-item values list: the draws string)."""
    return render_batch(binary, section, mid, flag, [([fmt(d)], o, t, w) for d, o, t, w in jobs], keep=False)


if __name__ == "__main__":
    main()
