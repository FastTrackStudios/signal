#!/usr/bin/env python3
"""The `ui.fps` readings a WebView run produced in a given time window.

Two shapes, because the window logs to whichever subscriber it has: the
console (`tracing`'s fmt layer, ANSI-coloured) with the probe off, and
the probe's JSONL with it on — the probe installs itself as the global
subscriber, so it takes the fmt layer out of the process.

Selection is by TIMESTAMP, not line number. The probe rotates its log
every 10 MB and writes ~17 MB/s, so a scroll of a few seconds can push
three rotations through; a mark taken as a line offset then points into
a file that has since been truncated, and the run reports "no readings"
as though the gesture never happened.
"""

import glob
import json
import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
# `2026-09-09T22:57:58.150271028Z` — both writers stamp RFC 3339.
TS = re.compile(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?)")
CONSOLE = re.compile(r"ui\.fps=([0-9.]+)\s+ui\.worst_frame_ms=([0-9.]+)")


def readings(paths, since):
    out = []
    for path in paths:
        for raw in open(path, errors="replace"):
            if "ui.fps" not in raw:
                continue
            line = ANSI.sub("", raw)
            stamp = TS.search(line)
            if not stamp or (since and stamp.group(1) < since):
                continue
            if line.lstrip().startswith("{"):
                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    continue
                out.append((stamp.group(1), event["ui.fps"], event["ui.worst_frame_ms"]))
            elif (m := CONSOLE.search(line)):
                out.append((stamp.group(1), float(m.group(1)), float(m.group(2))))
    out.sort()
    return out


def main():
    label, since = sys.argv[1], sys.argv[2]
    paths = [p for arg in sys.argv[3:] for p in sorted(glob.glob(arg))]
    rows = readings(paths, since)
    if not rows:
        print(f"{label}: no readings since {since} — did the window log?")
        return 1
    fps = sorted(r[1] for r in rows)
    worst = max(r[2] for r in rows)
    # The floor and the worst frame, not the mean. A drag that stutters
    # averages well and feels terrible, and a window that idles between
    # gestures averages better still.
    print(
        f"{label:<12} n={len(fps):<3d} min {fps[0]:5.1f}  "
        f"p50 {fps[len(fps) // 2]:5.1f}  max {fps[-1]:5.1f} fps   "
        f"worst frame {worst:6.1f} ms"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
