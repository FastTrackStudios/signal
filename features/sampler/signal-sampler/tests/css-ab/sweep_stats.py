#!/usr/bin/env python3
"""Aggregate a `match-ref --sweep` run into numbers an engine can be tuned to.

Reads the sweep's stdout and the parameter-test manifest, and reports the
skips grouped the way the decoded KSP indexes them (interval, IOI, velocity
zone). Its real job, though, is the two guards that decide whether ANY of that
is trustworthy:

  * **Scan edges.** A fit at either end of the scan is clipped, not measured —
    the search wanted to go further and could not. Those rows read as
    confident numbers, and dropping them took the phrase-start spread from
    146 ms to 64 ms.
  * **Phrase starts.** A fresh note plays its body from the head, so it has no
    skip at all and every one of them must land on a single constant. They are
    the control: when they scatter, nothing else in the table means anything.

Usage:  python3 sweep_stats.py <sweep-output.txt> [--lead-ms 30] [--scan-ms 150]
"""

from __future__ import annotations

import argparse
import json
import re
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = HERE.parent.parent / "scripts" / "css-param-test.manifest.json"

ROW = re.compile(
    r"^OK\s*([\d.]+)s\s+(\d+)\s+([\d.]+)s\s+([+-][\d.]+)ms\s+([\d.]+)%\s+(\S+)"
)


def median(xs: list[float]) -> float:
    s = sorted(xs)
    return s[len(s) // 2] if s else float("nan")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("sweep", type=Path)
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument("--lead-ms", type=float, default=30.0)
    ap.add_argument("--scan-ms", type=float, default=150.0)
    ap.add_argument("--refine-ms", type=float, default=0.5)
    args = ap.parse_args()

    man = json.loads(args.manifest.read_text())
    notes = [
        dict(t=n["t"], pitch=n["pitch"], vel=n["vel"], sec=s["name"])
        for s in man["sections"]
        for n in s["notes"]
    ]
    notes.sort(key=lambda x: x["t"])

    # What each note IS, from the MIDI: a step from the note before it, or a
    # phrase start when the line had fallen silent.
    ctx = {}
    for i, n in enumerate(notes):
        prev = notes[i - 1] if i else None
        fresh = not (prev and (n["t"] - prev["t"]) < 1.5 and prev["pitch"] != n["pitch"])
        ctx[round(n["t"], 3)] = dict(
            kind="fresh" if fresh else "trans",
            iv=0 if fresh else abs(n["pitch"] - prev["pitch"]),
            ioi=None if fresh else (n["t"] - prev["t"]) * 1000.0,
            vel=n["vel"],
            sec=n["sec"],
        )

    rows, clipped = [], 0
    for line in args.sweep.read_text().splitlines():
        m = ROW.match(line)
        if not m:
            continue
        c = ctx.get(round(float(m.group(1)), 3))
        if not c:
            continue
        drift = float(m.group(4))
        # The sweep windows each note at `lead` past its nominal onset, so the
        # sample offset it recovered is `lead - drift`.
        off = args.lead_ms - drift
        if off <= args.refine_ms * 48 or off + args.refine_ms * 48 >= args.scan_ms:
            clipped += 1
            continue
        rows.append(dict(drift=drift, artic=m.group(6), share=float(m.group(5)), **c))

    if not rows:
        print("no clean rows — every fit was clipped at a scan edge")
        return 1
    print(f"{len(rows)} clean rows ({clipped} clipped at a scan edge)\n")

    fresh = [r for r in rows if r["kind"] == "fresh"]
    print("PHRASE STARTS — a fresh body has no skip, so these must agree")
    by = defaultdict(list)
    for r in fresh:
        by[r["artic"]].append(r["drift"])
    for a, v in sorted(by.items(), key=lambda kv: -len(kv[1])):
        print(
            f"  {a:16s} n={len(v)}  median {median(v):+7.1f} ms  "
            f"spread {max(v) - min(v):5.1f}  {[round(x) for x in sorted(v)]}"
        )
    base = median([r["drift"] for r in fresh]) if fresh else 0.0
    print(f"\n  reference constant = {base:+.1f} ms\n")

    tr = [r for r in rows if r["kind"] == "trans"]
    print("TRANSITIONS, relative to that constant (Overlap-Delay minus the $1fvjk skip)")
    groups = [
        ("interval 1 st", lambda r: r["iv"] == 1),
        ("interval 2 st", lambda r: r["iv"] == 2),
        ("interval 3+ st", lambda r: r["iv"] >= 3),
        ("IOI < 200 ms", lambda r: r["ioi"] and r["ioi"] < 200),
        ("IOI > 400 ms", lambda r: r["ioi"] and r["ioi"] > 400),
        ("vel zone 1", lambda r: r["vel"] <= 64),
        ("vel zone 2", lambda r: 65 <= r["vel"] <= 100),
        ("vel zone 3", lambda r: r["vel"] >= 101),
    ]
    for label, pred in groups:
        v = [r["drift"] - base for r in tr if pred(r)]
        if v:
            print(
                f"  {label:15s} n={len(v):3d}  median {median(v):+7.1f} ms  "
                f"range {min(v):+.0f}..{max(v):+.0f}"
            )
    print(
        "\n  A median is only as good as its range. Where the range is ±50 ms the\n"
        "  median is not a number to tune to, however tidy it looks."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
