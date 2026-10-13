#!/usr/bin/env python3
"""A project with the studio's SHAPE but none of its weight.

Opening Set in Stone costs ~25 s before a frame is drawn: parsing is
quick, but materializing 5.6 GB of takes through the media bay is not.
None of that is what a UI bisect measures — the window's cost is the
tracks, items, folders and markers it has to lay out, and those are
free to fabricate.

So this writes a `.rpp` with the same counts as the real session — 65
tracks with the same folder nesting, 877 items, 16 markers — where every
item's source is an EMPTY MIDI take. There is no media to find, so the
project opens instantly and the layout the studio builds is the same
size it would be for the real thing.

It is not a substitute for the real session as a number of record: real
takes carry peaks, and the arrangement draws waveforms from them. Use it
to HUNT, and confirm on Set in Stone.

    scripts/ui-stress/make-synthetic-rpp.py > /tmp/synthetic.rpp
"""

import random
import sys

# Overridable, so the same fixture can stand in for a drum session or an
# orchestral template: `make-synthetic-rpp.py 200 20000`.
TRACKS = int(sys.argv[1]) if len(sys.argv) > 1 else 65
ITEMS = int(sys.argv[2]) if len(sys.argv) > 2 else 877
MARKERS = 16
# 500 measures at 180 BPM, 4/4 — a full-length arrangement rather than a
# ten-minute excerpt.
MEASURES = 500
BPM = 180.0
SECS_PER_BAR = 4.0 * 60.0 / BPM
LENGTH = MEASURES * SECS_PER_BAR
random.seed(20260911)  # a fixture that changes between runs is not one


def guid() -> str:
    h = "%032X" % random.getrandbits(128)
    return f"{{{h[:8]}-{h[8:12]}-{h[12:16]}-{h[16:20]}-{h[20:32]}}}"


def main() -> None:
    out = sys.stdout.write
    out('<REAPER_PROJECT 0.1 "7.65/synthetic" 1788749173 0\n')
    # `TEMPO bpm num den 0` — the trailing field matters; without it the
    # loader falls back to 120 and the ruler draws half the bars.
    out("  RIPPLE 0 0\n  AUTOXFADE 128\n  TEMPO 180 4 4 0\n")

    # Markers, spread across the song, so the ruler has its flags.
    for i in range(MARKERS):
        at = LENGTH * (i + 1) / (MARKERS + 1)
        out(f'  MARKER {i} {at:.6f} "section {i + 1}" 0 0 1 B {{{i}}} 0\n')

    # Items per track, distributed the way a real session is: a few
    # tracks carry many takes and most carry a handful.
    per = [1] * TRACKS
    for _ in range(ITEMS - TRACKS):
        per[random.randrange(TRACKS)] += 1

    for t in range(TRACKS):
        # The same folder nesting the real project has: a bus every
        # eighth track, closed two tracks later.
        depth = 1 if t % 8 == 0 else (-1 if t % 8 == 3 else 0)
        out(f"  <TRACK {guid()}\n")
        out(f'    NAME "Track {t + 1}"\n')
        out(f"    PEAKCOL {16576 + t * 997}\n")
        out("    BEAT -1\n    AUTOMODE 0\n")
        # Volume and pan spread across the session rather than parked at
        # unity and centre. A fixture where every fader reads the same
        # cannot show whether a fader reads at all — which is the whole
        # question for the collapsed rows, where the bar IS the value.
        # Two different periods so the pair never lines up into a moire.
        vol = 0.25 + 1.35 * (t % 17) / 16.0
        pan = -1.0 + 2.0 * (t % 11) / 10.0
        out(f"    VOLPAN {vol:.4f} {pan:.4f} -1 -1 1\n")
        out(f"    MUTESOLO {1 if t % 11 == 0 else 0} 0 0\n")
        out("    IPHASE 0\n")
        out(f"    ISBUS {1 if depth > 0 else 0} {depth}\n")
        out("    SHOWINMIX 1 0.6667 0.5 1 0.5 0 0 0 0\n")
        out("    SEL 0\n    REC 0 5088 1 0 0 0 0 0\n")
        # Track heights, as a real session has them: a bus opened up to
        # see its automation, a handful of focused tracks, and the long
        # tail collapsed. A fixture where every track is 70 tall would
        # never exercise the panel's density tiers, and those are most of
        # what makes a two-thousand-track session legible.
        if depth > 0:
            height = 100          # a bus, opened
        elif t % 8 == 1:
            height = 24           # collapsed to a band
        elif t % 8 == 2:
            height = 12           # collapsed further
        elif t % 16 == 5:
            height = 160          # one track being worked on
        elif t % 8 == 6:
            height = 40           # compact
        else:
            height = 0            # unset: the user's default height
        out(f"    TRACKHEIGHT {height} 0 0 0 0 0 0\n")

        # Items are laid out as a real arrangement is, not scattered:
        # takes that START and STOP on bar lines, separated by rests, and
        # COMPED — several overlapping takes across the same passage,
        # which is what a tracking session actually leaves behind and
        # what makes an arrangement expensive to draw (overlapping
        # geometry in the same rows).
        emitted = 0
        bar = random.randrange(0, 8)
        while emitted < per[t] and bar < MEASURES:
            take_bars = random.choice((2, 4, 4, 8, 8, 16))
            # A third of passages are comped: 2-4 alternate takes over
            # the same bars, the way a punch-in leaves them.
            comps = random.choice((1, 1, 1, 2, 3, 4))
            for c in range(comps):
                if emitted >= per[t]:
                    break
                start = bar * SECS_PER_BAR
                # Comps drift a beat either way, as punches do.
                jitter = 0.0 if c == 0 else random.uniform(-0.5, 0.5) * (SECS_PER_BAR / 4)
                length = take_bars * SECS_PER_BAR
                out("    <ITEM\n")
                out(f"      POSITION {max(0.0, start + jitter):.6f}\n")
                out(f"      LENGTH {length:.6f}\n")
                out("      LOOP 0\n      ALLTAKES 0\n")
                out("      FADEIN 1 0.01 0 1 0 0 0\n      FADEOUT 1 0.01 0 1 0 0 0\n")
                out(f"      MUTE {1 if c > 0 and random.random() < 0.6 else 0} 0\n")
                out("      SEL 0\n")
                out(f"      IGUID {guid()}\n")
                out(f'      NAME "t{t + 1} bar{bar + 1}{"" if c == 0 else f" comp{c}"}"\n')
                out("      VOLPAN 1 0 1 -1\n      SOFFS 0\n")
                out("      PLAYRATE 1 1 0 -1 0 0.0025\n")
                out(f"      GUID {guid()}\n")
                out("      <SOURCE MIDI\n        HASDATA 1 960 QN\n        E 1 b0 7b 00\n      >\n")
                out("    >\n")
                emitted += 1
            # A rest, then the next passage.
            bar += take_bars + random.choice((0, 0, 2, 4, 8))
        out("  >\n")
    out(">\n")


if __name__ == "__main__":
    main()
