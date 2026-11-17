#!/usr/bin/env python3
"""Probe a Kontakt percussion instrument for a KONTRA-vs-Kontakt null, keys found by KONTRA.

usage: perc_null_probe.py <instrument.nki|multi.nkm> <ksp dir | -> <out dir> [--kontra BIN] [--repeats N]
                          [--max-gap-ms MS] [--vels 20,64,100,127] [--max-hits N]

A script decides what a key plays (1000Drums' zones sit on key 60, its script plays 48..107),
so the zones alone cannot say which keys and velocities matter. Discovery runs the instrument in
KONTRA (`kontra-inspect play`, which runs the real KSP): every key 0..127 at each of `--vels`,
one hit per 0.5 s, and reads the voices each hit starts (`stage_events` type `voice`,
`event: Start`). A (key, velocity) is kept when its samples, round-robin suffix removed, differ
from every lower kept velocity of that key; keys that start nothing are dropped.

The probe plays each kept (key, velocity) `--repeats` times (default 2), one hit every gap:
the longest sample those hits start (zones.tsv frames/rate) + 500 ms, at most `--max-gap-ms`
(default 8000). Note length 200 ms. Times are whole MIDI ticks at 120 BPM (480 TPQ: one tick is
50 frames at 48 kHz), so probe.mid and scn.json agree to the frame.

Writes <out>/probe.mid, <out>/scn.json (kontra-inspect's scenario), <out>/manifest.tsv
(null_test.py's: idx, t_start, win, pitch, category, description) and <out>/discovery.tsv.
"""
import csv
import json
import os
import re
import struct
import subprocess
import sys
import tempfile

TPQ, US_PER_Q, FRAMES_PER_TICK = 480, 500000, 50  # 120 BPM, 48 kHz
NOTE_TICKS = 192                                  # 200 ms
# The first hit waits 2 s: a script that finishes its tables from timers after `on init`
# (STRIKEFORCE's velocity curves) is done in Kontakt's render only if it had time, and how
# long Kontakt had since loading varies; KONTRA's pre-roll settles it too.
LEAD_TICKS = 1920
RR = re.compile(r"(?i)[_ -]?rr[_ -]?\d+")


def vlq(n):
    b = [n & 0x7F]
    n >>= 7
    while n:
        b.insert(0, (n & 0x7F) | 0x80)
        n >>= 7
    return bytes(b)


def write_mid(path, hits, end):
    """`end`: the end-of-track tick; the render stops there, so it must cover the last tail."""
    ev = []
    for tick, key, vel in hits:
        ev += [(tick, bytes([0x90, key, vel])), (tick + NOTE_TICKS, bytes([0x80, key, 0]))]
    ev.sort(key=lambda e: e[0])
    trk, last = b"\x00\xff\x51\x03" + US_PER_Q.to_bytes(3, "big"), 0
    for t, e in ev:
        trk += vlq(t - last) + e
        last = t
    trk += vlq(max(end - last, 0)) + b"\xff\x2f\x00"
    with open(path, "wb") as f:
        f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, TPQ) + b"MTrk" + struct.pack(">I", len(trk)) + trk)


def scenario(hits):
    ev = []
    for tick, key, vel in hits:
        f = tick * FRAMES_PER_TICK
        ev += [{"frame": f, "type": "note_on", "key": key, "velocity": vel},
               {"frame": f + NOTE_TICKS * FRAMES_PER_TICK, "type": "note_off", "key": key}]
    ev.sort(key=lambda e: e["frame"])
    return {"events": ev}


def voices(kontra, nki, hits, env):
    """For each hit, the samples of the voices started between it and the next hit."""
    with tempfile.TemporaryDirectory() as d:
        scn, out = os.path.join(d, "scn.json"), os.path.join(d, "out.json")
        json.dump(scenario(hits), open(scn, "w"))
        subprocess.run([kontra, "play", nki, "--scenario", scn, "--json", out], env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3000, check=False)
        r = json.load(open(out))["result"]
    for k in ("unhandled_actions", "unanswered_pars"):
        if r.get(k):
            print(f"KONTRA {k}: {r[k]}", file=sys.stderr)
    starts = [(e["frame"], e["sample"]) for e in r["stage_events"]
              if e.get("type") == "voice" and e.get("event") == "Start"]
    frames = [t * FRAMES_PER_TICK for t, _, _ in hits] + [float("inf")]
    got = [[] for _ in hits]
    for f, s in starts:
        for i in range(len(hits)):
            if frames[i] <= f < frames[i + 1]:
                got[i].append(s)
                break
    return got


def main():
    a = sys.argv[1:]

    def opt(name, default):
        if name in a:
            i = a.index(name)
            v = a[i + 1]
            del a[i:i + 2]
            return v
        return default
    kontra = opt("--kontra", "/run/media/Development/KONTRA-perc/target/release/kontra-inspect")
    repeats = int(opt("--repeats", "2"))
    max_gap = float(opt("--max-gap-ms", "8000"))
    vels = [int(v) for v in opt("--vels", "20,64,100,127").split(",")]
    max_hits = int(opt("--max-hits", "240"))
    nki, ksp, out = a
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ, KONTRA_KSP_SEED=os.environ.get("KONTRA_KSP_SEED", "777"),
               KONTRA_INSPECT_BLOCK="512")
    env.pop("KONTRA_INSPECT_WAV", None)

    # sample lengths for the gap; `-` (a multi: no one decoded folder) leaves the gap at its maximum
    length = {}
    if ksp != "-":
        for z in csv.DictReader(open(os.path.join(ksp, "zones.tsv")), delimiter="\t"):
            length[os.path.basename(z["sample"])] = int(z["frames"]) / int(z["rate"])

    probe = [(k, v) for k in range(128) for v in vels]
    step = 480  # 0.5 s
    got = voices(kontra, nki, [(i * step, k, v) for i, (k, v) in enumerate(probe)], env)
    kept, last_sig = [], {}
    with open(os.path.join(out, "discovery.tsv"), "w") as f:
        f.write("key\tvel\tvoices\tkept\tsamples\n")
        for (k, v), samples in zip(probe, got):
            sig = tuple(sorted({RR.sub("", os.path.basename(s)) for s in samples}))
            keep = bool(sig) and sig not in last_sig.get(k, set())
            if keep:
                last_sig.setdefault(k, set()).add(sig)
                longest = max((length.get(os.path.basename(s), max_gap / 1000) for s in samples), default=0.0)
                kept.append((k, v, longest))
            f.write(f"{k}\t{v}\t{len(samples)}\t{int(keep)}\t{' '.join(sig)[:300]}\n")
    if not kept:
        sys.exit("no key starts a voice")
    hits_per = repeats
    while len(kept) * hits_per > max_hits and hits_per > 1:
        hits_per -= 1
    if len(kept) * hits_per > max_hits:
        kept = kept[:max_hits]
        print(f"capped at {max_hits} (key, velocity) cases", file=sys.stderr)
    gap_ms = min(max(lg for _, _, lg in kept) * 1000 + 500, max_gap)
    gap = max(1, round(gap_ms * TPQ * 1000 / US_PER_Q))
    hits, rows = [], []
    for k, v, _ in kept:
        for r in range(hits_per):
            t = LEAD_TICKS + len(hits) * gap
            hits.append((t, k, v))
            rows.append((t, k, f"key{k} v{v} rep{r + 1}"))
    write_mid(os.path.join(out, "probe.mid"), hits, LEAD_TICKS + len(hits) * gap)
    json.dump(scenario(hits), open(os.path.join(out, "scn.json"), "w"))
    win = gap * FRAMES_PER_TICK / 48000
    with open(os.path.join(out, "manifest.tsv"), "w") as m:
        m.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for i, (t, k, d) in enumerate(rows):
            m.write(f"{i}\t{t * FRAMES_PER_TICK / 48000:.6f}\t{win:.6f}\t{k}\tHIT\t{d}\n")
    print(f"{len(kept)} (key, velocity) cases x {hits_per}: {len(hits)} hits, gap {win * 1000:.0f} ms, "
          f"{len(hits) * win:.0f} s")


if __name__ == "__main__":
    main()
