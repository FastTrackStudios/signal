#!/usr/bin/env python3
"""Probe MIDI for a Shreddage null test.

usage: shreddage_probe.py <out.mid> <mode> [--notes 1.2:0.4] [--lib abyss|argent|darkwall]

single-note modes (keyswitch = the shipped TACT key):
  sus   Sustain (key 12)      stac  Staccato (key 13)      harm  Harmonics (key 15)
phrase modes (Sustain selected; one manifest row per phrase):
  rel     notes of many lengths (the pitched release fires on notes up to
          1071 ms) and a note followed at once by another on its string
  legato  overlapping pairs at intervals ±1…±5, the second note soft
          (velocity ≤ 60: hammer-on / pull-off) or loud (a fresh note)
  slide   overlapping pairs with key 17 held (Legato (Slide)), ±1…±12
  chord   chords of 2 to 5 notes struck together

Single notes: the keyswitch sounds at 0.5 s, the first note at 1.0 s; each
note lasts `on` seconds with `gap` seconds after it (default 1.2:0.4); the
walk covers the range at velocities in every layer and repeats some notes
(RR order, borrow draws). Phrases: 0.8 s of silence after each. 120 BPM,
960 PPQ. A manifest `<out>.tsv` lists each unit: index, start s, end s,
first key, first velocity.
"""
import struct, sys

# Per library: the shipped TACT keyswitches, the slide key, the playable
# range and the loudest velocity a mode's articulation keeps (Argent's KS 0
# plays Rake at 116-126 and Pinch Harmonics at 127, engine-model §1b).
LIBS = {
    "abyss": dict(ks={"sus": 12, "stac": 13, "harm": 15, "fx": 16, "tap": 14}, slide=17,
                  lo=21, hi=72, vmax={}),
    "argent": dict(ks={"sus": 0, "mute": 1, "stac": 2, "pcsus": 3, "pcmute": 4, "pcstac": 5,
                       "trem": 6, "choke": 7, "tap": 8, "nharm": 9, "aharm": 10, "fx": 11,
                       "rake": 0, "pinch": 0},
                   slide=21, lo=25, hi=88, vmax={"sus": 115},
                   side=[22] * 6 + [12, 13, 14, 15] * 3, slide_in=23),
    "darkwall": dict(ks={"sus": 0, "ssus": 1, "stacl": 2, "stacs": 3, "mute": 4, "harm": 5,
                         "tap": 6, "buzz": 7},
                     slide=12, lo=16, hi=72, vmax={},
                     side=[13] * 6 + [10, 11] * 6, slide_in=14),
}
SINGLE = {"sus", "stac", "harm", "fx", "mute", "pcsus", "pcmute", "pcstac", "trem", "choke",
          "nharm", "aharm", "rake", "pinch", "ssus", "stacl", "stacs", "buzz"}
PHRASE = {"rel", "legato", "slide", "chord", "tap", "slidein", "slideout"}
# side keys (fret noise, wide chokes, key FX): pressed alone, held 0.3 s
LIB = LIBS["abyss"]
PPQ, US_PER_Q = 960, 500_000


def vlq(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append(0x80 | (n & 0x7F))
        n >>= 7
    return bytes(reversed(out))


def notes():
    """(key, velocity): the walk the single-note modes play."""
    lo, hi = LIB["lo"], LIB["hi"]
    if LIB is not LIBS["abyss"]:
        hi = min(hi, lo + 51)
    keys = list(range(lo, hi + 1, 3)) + list(range(hi, lo - 1, -5))
    vels = [20, 50, 80, 110, 100, 40, 70, 127, 10, 60]
    seq = [(k, vels[i % len(vels)]) for i, k in enumerate(keys)]
    seq += [(40, 90)] * 6 + [(57, 64)] * 4 + [(33, 30)] * 4
    return seq


def velocities(mode, seq):
    """Fit a walk's velocities to the mode's articulation (KS 0 splits)."""
    if mode == "rake":
        return [(k, 116 + v % 11) for k, v in seq]
    if mode == "pinch":
        return [(k, 127) for k, v in seq]
    top = LIB["vmax"].get(mode)
    return [(k, min(v, top)) for k, v in seq] if top else seq


def phrases(mode):
    """[(notes, extra)]: notes (t, dur, key, vel) from the phrase start."""
    out = []
    if mode == "rel":
        for i, d in enumerate([0.08, 0.2, 0.4, 0.7, 1.0, 1.06, 1.1, 1.5]):
            out.append(([(0.0, d, 33 + 5 * (i % 6), 70)], []))
        # a note let go by the next on its string (no gap), soft and loud
        for k1, k2, v in [(45, 47, 80), (50, 48, 40), (40, 41, 100)]:
            out.append(([(0.0, 0.5, k1, v), (0.5, 0.5, k2, v)], []))
    elif mode == "legato":
        for base in (40, 52):
            for iv in (1, -1, 2, -2, 3, -3, 4, -4, 5):
                for v2 in (40, 90):
                    out.append(([(0.0, 0.5, base, 90), (0.4, 0.8, base + iv, v2)], []))
    elif mode == "slide":
        for base in (40, 52):
            for iv in (1, -1, 3, -3, 5, -5, 7, 12, -12):
                for v2 in (40, 100):
                    ns = [(0.0, 0.5, base, 90), (0.4, 0.8, base + iv, v2)]
                    out.append((ns, [(-0.05, 1.3, LIB["slide"], 100)]))
    elif mode == "slidein":
        # the slide-in key held around a lone note: one stitch per fret from
        # the open string
        for k, v in [(41, 90), (43, 60), (47, 110), (52, 80), (36, 100), (45, 40), (57, 90), (50, 70)]:
            out.append(([(0.0, 1.0, k, v)], [(-0.05, 1.2, LIB["slide_in"], 100)]))
    elif mode == "slideout":
        # the slide-in/out key pressed while a note rings (fret 0 cuts it)
        for k, v in [(41, 90), (47, 70), (52, 110), (38, 90), (45, 60), (57, 100)]:
            out.append(([(0.0, 1.4, k, v)], [(0.5, 0.6, LIB["slide_in"], 100)]))
    elif mode == "tap":
        # taps from idle strings (open and fretted), then on ringing ones
        for k, v in [(33, 80), (35, 60), (28, 100), (45, 40), (52, 90)]:
            out.append(([(0.0, 0.8, k, v)], []))
        for base in (40, 52):
            for iv in (1, -1, 2, -3, 5):
                out.append(([(0.0, 0.5, base, 90), (0.4, 0.8, base + iv, 70)], []))
    elif mode == "chord":
        for ch in ([33, 40, 45], [28, 35, 40, 43], [45, 52], [38, 45, 50, 55, 59],
                   [21, 28, 33], [43, 47, 50, 55], [52, 57, 62, 67]):
            out.append(([(0.0, 1.0, k, 90) for k in ch], []))
    else:
        raise SystemExit(f"unknown mode {mode}")
    return out


def main():
    global LIB
    out, mode = sys.argv[1], sys.argv[2]
    if "--lib" in sys.argv:
        LIB = LIBS[sys.argv[sys.argv.index("--lib") + 1]]
    on, gap = 1.2, 0.4
    if "--notes" in sys.argv:
        on, gap = (float(x) for x in sys.argv[sys.argv.index("--notes") + 1].split(":"))
    tick = lambda s: round(s * PPQ * 1_000_000 / US_PER_Q)
    ks = LIB["ks"].get(mode, LIB["ks"]["sus"])
    ev = [(tick(0.5), 0x90, ks, 100), (tick(0.6), 0x80, ks, 0)]
    rows, t = [], 1.0
    if mode == "side":
        for i, k in enumerate(LIB["side"]):
            v = (25, 60, 90, 120, 45, 100, 75)[i % 7]
            ev += [(tick(t), 0x90, k, v), (tick(t + 0.3), 0x80, k, 0)]
            rows.append(f"{i}\t{t:.3f}\t{t + 0.3:.3f}\t{k}\t{v}")
            t += 1.6
    elif mode == "fx":
        for i, k in enumerate(range(21, 121, 7)):
            ev += [(tick(t), 0x90, k, 30 + (i * 13) % 97), (tick(t + 0.8), 0x80, k, 0)]
            rows.append(f"{i}\t{t:.3f}\t{t + 0.8:.3f}\t{k}\t{30 + (i * 13) % 97}")
            t += 2.5
    elif mode in SINGLE:
        for i, (k, v) in enumerate(velocities(mode, notes())):
            ev += [(tick(t), 0x90, k, v), (tick(t + on), 0x80, k, 0)]
            rows.append(f"{i}\t{t:.3f}\t{t + on:.3f}\t{k}\t{v}")
            t += on + gap
    else:
        for i, (ns, extra) in enumerate(phrases(mode)):
            end = t
            for dt, d, k, v in ns + extra:
                ev += [(tick(t + dt), 0x90, k, v), (tick(t + dt + d), 0x80, k, 0)]
                end = max(end, t + dt + d)
            rows.append(f"{i}\t{t:.3f}\t{end:.3f}\t{ns[0][2]}\t{ns[0][3]}")
            t = end + 0.8
    ev.sort(key=lambda e: (e[0], e[1] != 0x80))
    body, last = bytearray(b"\x00\xff\x51\x03" + US_PER_Q.to_bytes(3, "big")), 0
    for tk, st, d1, d2 in ev:
        body += vlq(tk - last) + bytes([st, d1, d2])
        last = tk
    body += b"\x00\xff\x2f\x00"
    with open(out, "wb") as f:
        f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ))
        f.write(b"MTrk" + struct.pack(">I", len(body)) + body)
    with open(out[:-4] + ".tsv", "w") as f:
        f.write("i\tstart\tend\tkey\tvel\n" + "\n".join(rows) + "\n")
    print(f"{out}: {len(rows)} units, {t + 1:.1f} s")


if __name__ == "__main__":
    main()
