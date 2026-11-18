#!/usr/bin/env python3
"""Refit a short's round robin and detune draws together, note by note.

    python3 fit_joint.py <render_css_live> <Section> <probe.mid> <kontakt.wav> <out.wav> <rrs.txt> <tunes.txt>

fit_draws.py fits the RRs with the detune held, then the detune with the
RRs held. A note can settle in the wrong pair that way: the wrong file at
the detune that suits it beats the right file at the wrong detune (2 Flutes
detunes every note ±5 c; a staccato burst there left −1 dB where Kontakt's
own RRs null at −37). Each note whose window (onset to the next drawn note)
still leaves more than JOINT_DB against Kontakt is searched over every
(RR, detune) pair at once — RRs that play the same file as another are
dropped, a note that draws no detune keeps its detune — in windowed
batches, every other draw held; JOINT_ROUNDS passes. Rewrites both files
(first line: the draws) and writes our render with them to <out.wav>.

Env: FIT_EXTRA other forced draws; CS_LIBRARY the library; CS_RR_MAX the
largest RR count.
"""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fit_draws import MAX_DRAWS, energy, render_batch  # noqa: E402
from null_test import SR, load  # noqa: E402

JOINT_DB = -25.0
JOINT_ROUNDS = 2
CHUNK = 32
TUNES = range(-5, 6)


def drawn_onsets(traced_lo, traced_others):
    """The onsets (s, sorted) whose voices change with the forced value."""
    return sorted({
        round(a[0], 4)
        for other in traced_others
        for a, b in zip(traced_lo, other)
        if a != b
    })


def main():
    binary, section, mid, kon, out_wav, rr_file, tune_file = sys.argv[1:8]
    rrs = [int(v) for v in open(rr_file).readline().strip().split(",") if v]
    tunes = [int(v) for v in open(tune_file).readline().strip().split(",") if v]
    k = load(kon)
    tmp = os.path.dirname(os.path.abspath(out_wav))
    pid = os.getpid()
    rr_max = int(os.environ.get("CS_RR_MAX", "8"))

    def batch(jobs, keep=False):
        # each job's draws ride in its own values slot (flag + values = args)
        return render_batch(binary, section, mid, "--rrs",
                            [([",".join(map(str, r)) + " --tunes " + ",".join(map(str, t))], o, tr, w)
                             for r, t, o, tr, w in jobs], keep=keep)

    # which onsets draw an RR / a detune, and the file each RR plays there
    probes = [([c] * MAX_DRAWS, [0] * MAX_DRAWS, f"{tmp}/.j{pid}_rr{c}.wav", True, None) for c in range(rr_max)]
    probes += [([0] * MAX_DRAWS, [t] * MAX_DRAWS, f"{tmp}/.j{pid}_t{t}.wav", True, None) for t in (-5, 5)]
    res = batch(probes)
    rr_voices = [v for _, v in res[:rr_max]]
    rr_on = drawn_onsets(rr_voices[0], rr_voices[1:])
    t_on = drawn_onsets(res[rr_max][1], [res[rr_max + 1][1]])
    t_index = {t: i for i, t in enumerate(t_on)}
    # per RR onset, the candidates that play distinct voices
    distinct = []
    for t0 in rr_on:
        seen, cands = set(), []
        for c, vs in enumerate(rr_voices):
            sig = tuple(d for t, d in vs if abs(t - t0) < 1e-4)
            if sig not in seen:
                seen.add(sig)
                cands.append(c)
        distinct.append(cands)
    ons = sorted(set(rr_on) | set(t_on))
    windows = []
    for t0 in rr_on:
        nxt = [t for t in ons if t > t0 + 1e-4]
        windows.append((int(t0 * SR), min(int(nxt[0] * SR) if nxt else len(k), len(k))))

    for _ in range(JOINT_ROUNDS):
        (ours, _), = batch([(rrs, tunes, out_wav, False, None)], keep=True)
        todo = []
        for i, (a, b) in enumerate(windows):
            if i >= len(rrs):
                break
            rel = energy(k[a:b] - ours[a:b]) / (energy(k[a:b]) + 1e-20)
            if rel > 10 ** (JOINT_DB / 10):
                todo.append(i)
        if not todo:
            break
        best = {i: (rrs[i], None, energy(k[a:b] - ours[a:b])) for i in todo for a, b in [windows[i]]}
        trials = []
        for i in todo:
            ti = t_index.get(rr_on[i])
            for c in distinct[i]:
                for t in (TUNES if ti is not None and ti < len(tunes) else (None,)):
                    if c == rrs[i] and (t is None or t == tunes[ti]):
                        continue
                    trials.append((i, c, ti, t))
        for n in range(0, len(trials), CHUNK):
            chunk = trials[n:n + CHUNK]
            jobs = []
            for j, (i, c, ti, t) in enumerate(chunk):
                r = rrs[:i] + [c] + rrs[i + 1:]
                tt = tunes if t is None else tunes[:ti] + [t] + tunes[ti + 1:]
                jobs.append((r, tt, f"{tmp}/.j{pid}_{j}.wav", False, windows[i]))
            for (i, c, ti, t), (y, _) in zip(chunk, batch(jobs)):
                a, b = windows[i]
                e = energy(k[a:b] - y[: b - a])
                if e < best[i][2]:
                    best[i] = (c, (ti, t) if t is not None else None, e)
        changed = 0
        for i in todo:
            c, tt, _ = best[i]
            if c != rrs[i] or tt is not None:
                rrs[i] = c
                if tt is not None:
                    tunes[tt[0]] = tt[1]
                changed += 1
        if not changed:
            break
    batch([(rrs, tunes, out_wav, False, None)], keep=True)
    with open(rr_file, "w") as f:
        f.write(",".join(map(str, rrs)) + "\n")
    with open(tune_file, "w") as f:
        f.write(",".join(map(str, tunes)) + "\n")
    print(f"{len(rrs)} rr draws, {len(tunes)} detune draws", file=sys.stderr)


if __name__ == "__main__":
    main()
