#!/usr/bin/env python3
"""Fit Kontakt's random draws in a probe render, for render_css_live to replay.

    python3 fit_draws.py --tunes|--variants|--attacks|--rrs <render_css_live> <Section> <probe.mid> <kontakt.wav> <out.wav>

Kontakt draws some choices at random: a short's +-5 c detune (a key not
struck in the last 500 ms), a zone-2 legato body variant (2..4) and a loud
first note's attack velocity (an index into 5-7 / 8-9). They
cannot be reproduced, so they are fitted from the render and forced into
ours (render_css_live --tunes / --variants take them in the order they are
drawn).

Our engine is rendered once per candidate value with EVERY draw forced to
that value. A note drew where the voices our engine started differ between
the lowest and highest candidate (render_css_live --trace: a drawn detune
changes the voice's rate, a drawn variant its sample); each such note gets
the candidate that leaves the least residual against Kontakt between its
onset and the next note-on. Prints the fitted list and writes our render
with it to <out.wav>. No knowledge of which notes draw is needed.
"""
import os, re, struct, subprocess, sys, tempfile

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from null_test import SR, load  # noqa: E402

MAX_DRAWS = 400
REFINE_DB = -30.0
REFINE_ROUNDS = 3
# trial renders per batch (each ~27 MB, held until the chunk is scored)
REFINE_CHUNK = 32


def note_ons(path):
    """Note-on times (s) of a single-track SMF written by gen_probe.py."""
    d = open(path, "rb").read()
    ppq = struct.unpack(">H", d[12:14])[0]
    p = 14 + 8
    end = len(d)
    t = 0
    tempo = 500000
    run = None
    out = []
    while p < end:
        v = 0
        while True:
            b = d[p]
            p += 1
            v = (v << 7) | (b & 0x7F)
            if b < 0x80:
                break
        t += v
        st = d[p]
        if st == 0xFF:
            ty, ln = d[p + 1], d[p + 2]
            if ty == 0x51:
                tempo = int.from_bytes(d[p + 3 : p + 6], "big")
            if ty == 0x2F:
                break
            p += 3 + ln
            continue
        if st & 0x80:
            run = st
            p += 1
        n = 1 if (run & 0xF0) in (0xC0, 0xD0) else 2
        if run & 0xF0 == 0x90 and d[p + 1] > 0:
            out.append(t * tempo / ppq / 1e6)
        p += n
    return sorted(out)


VOICE = re.compile(r"^\[v\d+\]\s+([\d.]+)s\s+(.*)$")


def render_batch(binary, section, mid, flag, jobs, keep=True):
    """Our renders of `mid`, one per `(values, out, trace[, window])` job — `values`
    forced through `flag`; with `trace`, also the voices started as (time,
    description) in order. One `render_css_live --batch` process: the
    instrument loads once and the jobs render side by side, each on a fresh
    rig (the same WAV a single run gives)."""
    jobs = [j if len(j) == 4 else (*j, None) for j in jobs]
    lines = []
    for values, out, trace, window in jobs:
        args = []
        if window is not None:
            args += ["--window-frames", f"{window[0]},{window[1]}"]
        if values is not None:
            args += [flag, ",".join(str(v) for v in values)]
        # FIT_EXTRA: other draws held fixed for every render ("--tunes 0,0,…")
        args += os.environ.get("FIT_EXTRA", "").split()
        if trace:
            args.append("--trace")
        lines.append("\t".join([mid, out, " ".join(args)]))
    fd, listing = tempfile.mkstemp(suffix=".tsv")
    with os.fdopen(fd, "w") as f:
        f.write("\n".join(lines) + "\n")
    try:
        r = subprocess.run(
            [binary, "--batch", listing, "--section", section, "--library", os.environ.get("CS_LIBRARY", "strings")],
            check=True, capture_output=True, text=True, timeout=1800,
        )
    finally:
        os.unlink(listing)
    # each job prints its trace, then `wrote <out> …`
    voices, block = {}, []
    for line in r.stdout.splitlines():
        if line.startswith("wrote "):
            out = next((o for _, o, _, _ in jobs if line.startswith(f"wrote {o} ")), None)
            voices[out] = [(float(m.group(1)), m.group(2)) for m in map(VOICE.match, block) if m]
            block = []
        else:
            block.append(line)
    results = []
    for _, out, _, _ in jobs:
        results.append((load(out), voices.get(out, [])))
        if not keep:
            os.unlink(out)
    return results


def render(binary, section, mid, flag, values, out, trace=False):
    """One render (see [`render_batch`])."""
    return render_batch(binary, section, mid, flag, [(values, out, trace)])[0]


def energy(x):
    return float(np.sum(x * x))


def main():
    kind = sys.argv[1]
    binary, section, mid, kon, out_wav = sys.argv[2:7]
    flag, cands = {
        "--tunes": ("--tunes", range(-5, 6)),
        "--variants": ("--variants", (2, 3, 4)),
        "--attacks": ("--attacks", (0, 1, 2)),
        # CSSS marcato: the body's random extra entry, ms (random(0, $a4bsp))
        "--offsets": ("--offsets", range(0, 251, 25)),
        # random short round robins (woodwinds / brass): 0-based RR, up to
        # the largest RR count (brass Repetitions: 7)
        "--rrs": ("--rrs", range(int(os.environ.get("CS_RR_MAX", "8")))),
    }[kind]
    cands = list(cands)
    k = load(kon)
    tmp = tempfile.mkdtemp()
    # the candidates render side by side, in one batch
    traced = dict(zip(cands, render_batch(
        binary, section, mid, flag, [([c] * MAX_DRAWS, f"{tmp}/c{c}.wav", True) for c in cands])))
    renders = {c: v[0] for c, v in traced.items()}
    n = min(len(k), *(len(r) for r in renders.values()))
    ons = note_ons(mid)
    lo = traced[cands[0]][1]
    # the onsets whose voices changed with the forced value drew (against
    # every candidate: a forced RR past the count wraps to the first one)
    drawn = sorted({
        round(a[0], 4)
        for c in cands[1:]
        for a, b in zip(lo, traced[c][1])
        if a != b
    })
    windows = []
    for t0 in drawn:
        nxt = [t for t in ons if t > t0 + 1e-4]
        a = int(t0 * SR)
        windows.append((a, min(int(nxt[0] * SR) if nxt else n, n)))
    draws = []
    for a, b in windows:
        res = {c: energy(k[a:b] - renders[c][a:b]) for c in cands}
        draws.append(min(res, key=res.get))

    # A window can still hold the tail of an earlier drawn note (the second
    # key of an alternating burst), rendered above with the same forced
    # value: re-search such a draw (another drawn note within the second
    # before it) that leaves more than REFINE_DB, every other draw held at
    # its fit. Every such window's candidates render in the same batches
    # (each against the current draws), the improvements apply together, and
    # the search repeats until nothing changes (at most REFINE_ROUNDS).
    for _ in range(REFINE_ROUNDS):
        ours, _ = render(binary, section, mid, flag, draws, out_wav)
        todo = []
        for i, (a, b) in enumerate(windows):
            if not any(0 < a - a0 <= SR for a0, _ in windows[:i]):
                continue
            rel = energy(k[a:b] - ours[a:b]) / (energy(k[a:b]) + 1e-20)
            if 10 * np.log10(rel + 1e-20) > REFINE_DB:
                todo.append(i)
        if not todo:
            break
        trials = [(i, c) for i in todo for c in cands if c != draws[i]]
        best = {i: (draws[i], energy(k[a:b] - ours[a:b])) for i in todo for a, b in [windows[i]]}
        for n in range(0, len(trials), REFINE_CHUNK):
            chunk = trials[n:n + REFINE_CHUNK]
            # each trial renders only up to its window and keeps just that
            jobs = [(draws[:i] + [c] + draws[i + 1:], f"{tmp}/r{i}_{c}.wav", False, windows[i]) for i, c in chunk]
            for (i, c), (y, _) in zip(chunk, render_batch(binary, section, mid, flag, jobs, keep=False)):
                a, b = windows[i]
                e = energy(k[a:b] - y[: b - a])
                if e < best[i][1]:
                    best[i] = (c, e)
        changed = [i for i in todo if best[i][0] != draws[i]]
        for i in changed:
            draws[i] = best[i][0]
        if not changed:
            break
    ours, _ = render(binary, section, mid, flag, draws, out_wav)
    print(",".join(str(v) for v in draws))
    print(f"{len(draws)} draws", file=sys.stderr)


if __name__ == "__main__":
    main()
