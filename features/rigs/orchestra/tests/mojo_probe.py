#!/usr/bin/env python3
"""MOJO 2 probe MIDI + manifest for the Kontakt null test.

    python3 mojo_probe.py <mode> <instrument> <out.mid> <out_manifest.tsv> [--root DIR]

Same timing and manifest as gen_probe.py (120 BPM, 600 PPQ; units isolated
by silence, windowed by null_test.py at the same frames). Each articulation
block starts with its keyswitch (Key mode, `%jqdqz`: C1 = MIDI 24 …), then
one unit per (key, velocity). Keys are spread over that articulation's
shipped range (`key_range` in the instrument's section.styx), so no unit
falls outside it.

Modes:
  oneshots  Staccato and Stabs at velocities across all four layers
            (20 50 80 110 127), the one-layer FX at 60 and 110; three keys each.
  stacc     Staccato only (the first null).
  sustains  Sustains, legato off (shipped): held notes at several velocities
            (body loop + normal release sample), re-struck keys, notes 300 ms
            apart (the previous sustain's fade).
  legato    Sustains with legato on (CC26 = 127): overlapping pairs at several
            intervals and speeds, a fast run, natural legato (key C#2 = 37,
            latched), octave legato (CC64 held), a held note under a line.
  stablegato  Stabs with stab legato on (CC32 = 127; Alto ships it on):
            overlapping stabs within the 273/291 ms bypass at several
            intervals, a fast stab line, and stabs further apart (fresh,
            still monophonic).
  reltypes  Sustain release types: each latched type (keys 96-100), fall
            lengths (CC18 0/64/127) and trill intervals (CC42), the
            >= 120 keyswitch override over a held chord, a note outside a
            type's range (Alto), back to Normal.
  keytable  Sustains on every key of the range at one velocity (the sax
            groups' Key Position EQ routes move a band on some keys).
  phrases   Musical horn lines mixing articulations: stabs into staccato, a
            sustain ended by a fall (Falls keyswitch at velocity 127 while held),
            doit / shake releases (release-type keys 97 / 98, then 96 back).
  swells    Swells and crescendos (keys G1 / G#1) at 120 BPM, several keys.
  players   The Ensemble: the player count by CC15 (2, 3, 5), Auto Divisi by
            CC16. Stacked: sustains (mid key, the range's top and bottom:
            the extra players' neighbour keys), a re-strike, a stab, two
            staccatos. Divisi: one note, chords smaller than, equal to and
            larger than the count (a steal), a new note over a held chord
            (a steal), two notes 2 ms apart, a freed player taking a new note,
            a staccato chord; CC15 changed under held notes (stacked and divisi).
"""
import os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_probe import Smf  # noqa: E402  (same SMF writer and tick grid)

KEYSWITCH = {"Sus": 24, "Stacc": 25, "Stabs": 26, "BendDown": 27, "RunDown": 28, "RunUp": 29,
             "Rise": 30, "Swells": 31, "Cresc": 32, "Doits": 33, "Shakes": 34, "Trills": 35, "Falls": 36}
NATURAL_LEGATO = 37
RELEASE_KEY = {"normal": 96, "doit": 97, "shake": 98, "trill": 99, "fall": 100}
# script articulation order (`%ArtRange` = persistent `%yqd5p`, lo/hi pairs)
SCRIPT_ARTS = ["Sus", "Stacc", "Stabs", "BendDown", "RunDown", "RunUp", "Doits",
               "Rise", "Shakes", "Trills", "Swells", "Cresc", "Falls"]
LAYERED = (20, 50, 80, 110, 127)
ONE_LAYER = (60, 110)
MODES = {
    "stacc": [("Stacc", LAYERED)],
    "oneshots": [("Stacc", LAYERED), ("Stabs", LAYERED)]
    + [(a, ONE_LAYER) for a in ("BendDown", "RunDown", "RunUp", "Doits", "Rise", "Shakes", "Trills", "Falls")],
}
HOLD = 2.0   # key held this long (every one-shot sample here ends sooner or rings under the release)
WIN = 3.5    # window scored from the note-on
GAP = 1.5    # silence after the window


def key_ranges(ksp_dir):
    """Articulation id → (lo, hi): the shipped `%ArtRange` (persistent `%yqd5p`)."""
    for line in open(os.path.join(ksp_dir, "persistent_1.tsv")):
        if line.startswith("%yqd5p\t"):
            v = [int(x) for x in line.split("\t")[1].split()]
            return {a: (v[2 * i], v[2 * i + 1]) for i, a in enumerate(SCRIPT_ARTS)}
    raise SystemExit("no %yqd5p")


class Probe:
    """Units on a timeline: each starts after the last one's window and a gap."""

    def __init__(self):
        self.m, self.rows, self.t = Smf(), [], 1.0

    def ks(self, art, vel=100, gap=0.6):
        self.m.note(self.t, 0.1, KEYSWITCH[art], vel)
        self.t += gap

    def unit(self, cat, desc, notes, ccs=(), tail=3.0, pitch=0):
        """`notes` = [(t_rel, dur, key, vel)], `ccs` = [(t_rel, cc, val)], relative to the
        unit start; the window runs to the last note-off + `tail`."""
        t0 = self.t
        for dt, num, val in ccs:
            self.m.cc(t0 + dt, num, val)
        end = 0.0
        for dt, dur, key, vel in notes:
            self.m.note(t0 + dt, dur, key, vel)
            end = max(end, dt + dur)
        win = end + tail
        self.rows.append((len(self.rows), f"{t0:.3f}", f"{win:.3f}", pitch or notes[0][2], cat, desc))
        self.t = t0 + win + 1.5


def scale(lo, hi):
    """A major-scale ladder around the middle of a range (tonic a fifth below it)."""
    mid = (lo + hi) // 2
    root = mid - 7
    steps = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16]
    return [root + s for s in steps if lo <= root + s <= hi]


def one_shots(p, ranges, arts):
    for art, vels in arts:
        lo, hi = ranges[art]
        p.ks(art, gap=1.0)  # the spacing the oneshots references were rendered with
        for key in sorted({lo + 2, (lo + hi) // 2, hi - 3}):
            for vel in vels:
                p.unit(art.upper(), f"{art} key{key} v{vel}", [(0, HOLD, key, vel)], tail=WIN - HOLD)


def sustains(p, ranges):
    lo, hi = ranges["Sus"]
    sc = scale(lo, hi)
    p.ks("Sus")
    for key in (sc[1], sc[4], sc[-2]):
        for vel in (30, 70, 115):
            p.unit("SUS", f"Sus key{key} v{vel} hold2.5", [(0, 2.5, key, vel)])
    k = sc[4]
    p.unit("SUS-REP", f"Sus key{k} restrike x3 every 0.6s", [(0, 0.5, k, 90), (0.6, 0.5, k, 90), (1.2, 0.5, k, 90)])
    p.unit("SUS-REP", f"Sus key{k} restrike while held", [(0, 1.2, k, 90), (0.8, 1.2, k, 90)])
    p.unit("SUS-SEP", "Sus two notes 300 ms apart", [(0, 0.7, sc[2], 90), (1.0, 0.7, sc[4], 90)])
    p.unit("SUS-SHORT", f"Sus key{k} 150 ms note", [(0, 0.15, k, 100)])


def legato(p, ranges):
    lo, hi = ranges["Sus"]
    sc = scale(lo, hi)
    base = sc[3]
    p.ks("Sus")
    p.m.cc(p.t, 26, 127)  # sustain legato on (CC26 toggle)
    p.t += 0.5
    for iv in (1, 2, 4, 7, -2, -5):
        for ioi, vel in ((0.30, 90), (0.80, 60)):
            a, b = base, base + iv
            p.unit("LEG", f"Leg {a}->{b} ioi{int(ioi*1000)} v{vel}",
                   [(0, ioi + 0.10, a, vel), (ioi, 1.2, b, vel)])
    run = sc[:8]
    p.unit("LEG-RUN", "Leg run 8 notes ioi150", [(i * 0.15, 0.20, k, 95) for i, k in enumerate(run)])
    p.unit("LEG-HOLD", "Leg held note under a line", [(0, 2.0, base, 90)]
           + [(0.4 + i * 0.35, 0.40, k, 90) for i, k in enumerate((base + 2, base + 4, base + 2))])
    p.m.note(p.t, 0.1, NATURAL_LEGATO, 100)  # natural legato (latched toggle)
    p.t += 0.5
    p.unit("LEG-NAT", "Natural legato run ioi200", [(i * 0.2, 0.25, k, 95) for i, k in enumerate(sc[:6])])
    p.m.note(p.t, 0.1, NATURAL_LEGATO, 100)  # back to regular legato
    p.t += 0.5
    p.unit("LEG-OCT", "Octave legato +7 (pedal)", [(0, 0.5, base, 95), (0.4, 1.2, base + 7, 95)],
           ccs=[(-0.2, 64, 127), (2.0, 64, 0)])
    p.unit("LEG-OCT", "Octave legato -12 (pedal)", [(0, 0.5, base + 7, 95), (0.4, 1.2, base - 5, 95)],
           ccs=[(-0.2, 64, 127), (2.0, 64, 0)])
    p.m.cc(p.t, 26, 0)


def stablegato(p, ranges):
    lo, hi = ranges["Stabs"]
    sc = scale(lo, hi)
    base = sc[3]
    p.ks("Stabs")
    p.m.cc(p.t, 32, 127)  # stab legato on
    p.t += 0.5
    for iv in (1, 3, -2, 7):
        for ioi, vel in ((0.15, 100), (0.25, 70)):
            p.unit("STLEG", f"Stab leg {base}->{base + iv} ioi{int(ioi*1000)} v{vel}",
                   [(0, ioi + 0.05, base, vel), (ioi, 0.4, base + iv, vel)])
    p.unit("STLEG-RUN", "Stab line 6 notes ioi120", [(i * 0.12, 0.15, k, 105) for i, k in enumerate(sc[2:8])])
    p.unit("STLEG-FRESH", "Stabs 400 ms apart (beyond the bypass)",
           [(i * 0.4, 0.2, k, 100) for i, k in enumerate((base, base + 2, base + 4))])
    p.m.cc(p.t, 32, 0)


def reltypes(p, ranges):
    lo, hi = ranges["Sus"]
    sc = scale(lo, hi)
    k = sc[5]
    p.ks("Sus")
    for name in ("doit", "shake", "trill", "fall"):
        p.m.note(p.t, 0.1, RELEASE_KEY[name], 100)
        p.t += 0.5
        p.unit("REL-TYPE", f"Sus with {name} release", [(0, 1.0, k, 100)], tail=3.0)
    for cc18 in (64, 127):
        p.m.cc(p.t, 18, cc18)
        p.t += 0.3
        p.unit("REL-FALL", f"Fall release CC18 {cc18}", [(0, 1.0, k, 100)], tail=3.0)
    p.m.note(p.t, 0.1, RELEASE_KEY["trill"], 100)
    p.t += 0.5
    for cc42 in (0, 80):
        p.m.cc(p.t, 42, cc42)
        p.t += 0.3
        p.unit("REL-TRILL", f"Trill release CC42 {cc42}", [(0, 1.0, k, 100)], tail=3.0)
    p.m.note(p.t, 0.1, RELEASE_KEY["normal"], 100)
    p.t += 0.5
    p.unit("REL-LOW", f"Doit/fall outside range at key {lo}", [(0, 1.0, lo, 100)], tail=3.0)
    # the orange override: a two-note chord, Shakes keyswitch at 127 while held
    p.m.note(p.t + 1.0, 0.1, KEYSWITCH["Shakes"], 127)
    p.unit("REL-OVR", "Chord, Shakes KS v127 while held", [(0, 2.0, k, 100), (0, 2.0, k + 4, 100)], tail=3.0)
    p.m.note(p.t, 0.1, KEYSWITCH["Sus"], 100)
    p.t += 0.5


def keytable(p, ranges):
    lo, hi = ranges["Sus"]
    p.ks("Sus")
    for key in range(lo, min(hi, lo + 24) + 1):
        p.unit("KEYTAB", f"Sus key{key} v100", [(0, 0.8, key, 100)], tail=1.2)


def phrases(p, ranges):
    lo, hi = ranges["Sus"]
    sc = scale(lo, hi)
    # stabs into staccato: a typical horn-section punch
    p.ks("Stabs")
    p.unit("PHR-STAB", "Stab hits on the beat", [(i * 0.5, 0.2, sc[4 + (i % 2)], 110) for i in range(4)])
    p.ks("Stacc")
    p.unit("PHR-STACC", "Staccato eighths line", [(i * 0.25, 0.12, k, v) for i, (k, v) in
                                                  enumerate(zip(sc[2:8], (100, 80, 90, 80, 100, 110)))])
    # a sustain ended by a fall: Falls keyswitch at 127 while the note is held
    p.ks("Sus")
    k = sc[5]
    p.m.note(p.t + 1.2, 0.1, KEYSWITCH["Falls"], 127)
    p.unit("PHR-FALL", "Sus then fall release via KS v127", [(0, 2.0, k, 100)], tail=3.0)
    # release types: doit, then shake, then back to normal
    for rel in ("doit", "shake"):
        p.m.note(p.t, 0.1, RELEASE_KEY[rel], 100)
        p.t += 0.5
        p.unit("PHR-REL", f"Sus with {rel} release", [(0, 1.0, k, 100)], tail=3.0)
    p.m.note(p.t, 0.1, RELEASE_KEY["normal"], 100)
    p.t += 0.5
    # mixed line: stab, two staccatos, held sustain
    p.ks("Stabs")
    p.unit("PHR-MIX", "Stab", [(0, 0.2, sc[4], 115)], tail=1.0)
    p.ks("Stacc")
    p.unit("PHR-MIX", "Two staccatos", [(0, 0.12, sc[3], 95), (0.25, 0.12, sc[2], 95)], tail=1.5)
    p.ks("Sus")
    p.unit("PHR-MIX", "Held sustain", [(0, 1.5, sc[4], 85)])


def swells(p, ranges):
    for art in ("Swells", "Cresc"):
        lo, hi = ranges[art]
        p.ks(art)
        for key in sorted({lo + 3, (lo + hi) // 2, hi - 4}):
            for vel in (60, 110):
                p.unit(art.upper(), f"{art} key{key} v{vel} hold4", [(0, 4.0, key, vel)], tail=3.0)


PLAYERS_CC = {1: 0, 2: 20, 3: 30, 5: 55}  # CC15 bands of 12 (decode-players-divisi §1)


def players(p, ranges):
    lo, hi = ranges["Sus"]
    sc = scale(lo, hi)
    mid = sc[4]
    chord = [sc[0], sc[2], sc[4], sc[6], sc[8], sc[1]]

    def setp(n, divisi):
        p.m.cc(p.t, 15, PLAYERS_CC[n])
        p.m.cc(p.t + 0.05, 16, 127 if divisi else 0)
        p.t += 0.3

    for n in (2, 3, 5):
        p.ks("Sus")
        setp(n, False)
        p.unit("PL-SUS", f"P{n} stacked Sus key{mid}", [(0, 1.5, mid, 100)])
        p.unit("PL-EDGE", f"P{n} stacked Sus top key{hi}", [(0, 1.0, hi, 90)])
        p.unit("PL-EDGE", f"P{n} stacked Sus bottom key{lo}", [(0, 1.0, lo, 90)])
        p.unit("PL-REP", f"P{n} stacked Sus restrike", [(0, 0.5, mid, 90), (0.6, 0.5, mid, 90)])
        p.ks("Stabs")
        p.unit("PL-STAB", f"P{n} stacked stab key{mid}", [(0, 0.4, mid, 110)], tail=2.0)
        p.ks("Stacc")
        p.unit("PL-STACC", f"P{n} stacked staccatos", [(0, 0.3, mid, 100), (0.5, 0.3, mid + 2, 80)], tail=2.0)
    p.ks("Sus")
    for n in (2, 3, 5):
        setp(n, True)
        p.unit("DIV-ONE", f"P{n} divisi one note key{mid}", [(0, 1.0, mid, 100)])
        if n > 2:
            p.unit("DIV-LT", f"P{n} divisi chord of {n - 1}", [(0, 1.5, k, 100) for k in chord[:n - 1]])
        p.unit("DIV-EQ", f"P{n} divisi chord of {n}", [(0, 1.5, k, 100) for k in chord[:n]])
        p.unit("DIV-GT", f"P{n} divisi chord of {n + 1}", [(0, 1.5, k, 100) for k in chord[:n + 1]])
        p.unit("DIV-STEAL", f"P{n} divisi held chord, new note", [(0, 2.0, k, 100) for k in chord[:n]]
               + [(0.8, 1.0, chord[n - 1] + 3, 100)])
        p.unit("DIV-NEAR", f"P{n} divisi two notes 2 ms apart", [(0, 1.2, mid, 100), (0.002, 1.2, mid + 4, 100)])
        p.unit("DIV-FREE", f"P{n} divisi player freed, new note",
               [(0, 2.0, chord[0], 100), (0, 0.8, chord[1], 100), (1.2, 0.8, chord[1] + 2, 100)])
    setp(3, True)
    p.ks("Stacc")
    p.unit("DIV-STACC", "P3 divisi staccato chord of 3", [(0, 0.3, k, 100) for k in chord[:3]], tail=2.0)
    p.ks("Sus")
    p.unit("DIV-CC15", "P3 divisi chord held, CC15 to 5 players, new note",
           [(0, 2.0, k, 100) for k in chord[:3]] + [(1.2, 1.0, chord[3], 100)], ccs=[(0.8, 15, PLAYERS_CC[5])])
    setp(2, False)
    p.unit("PL-CC15", "P2 stacked note held, CC15 to 3 players, new note",
           [(0, 2.0, mid, 100), (1.0, 1.0, mid + 4, 100)], ccs=[(0.6, 15, PLAYERS_CC[3])])
    setp(1, False)


MODE_FN = {"players": players, "sustains": sustains, "legato": legato, "stablegato": stablegato, "reltypes": reltypes,
           "keytable": keytable,
           "phrases": phrases,
           "swells": swells}


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    if len(args) != 4 or (args[0] not in MODES and args[0] not in MODE_FN):
        raise SystemExit(__doc__)
    mode, inst, mid, manifest = args
    root = next((a.split("=", 1)[1] for a in sys.argv if a.startswith("--root=")),
                "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section")
    ranges = key_ranges(os.path.join(root, "_ksp", f"{inst} 2.0"))
    p = Probe()
    if mode in MODES:
        one_shots(p, ranges, MODES[mode])
    else:
        MODE_FN[mode](p, ranges)
    p.m.write(mid)
    with open(manifest, "w") as f:
        f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for r in p.rows:
            f.write("\t".join(map(str, r)) + "\n")
    print(f"{len(p.rows)} units, {p.t:.1f} s")


if __name__ == "__main__":
    main()
