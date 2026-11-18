#!/usr/bin/env python3
"""Write single-voice PROBE MIDI + manifest for the Kontakt null test.

    python3 gen_probe.py <out.mid> <out_manifest.tsv>

Each unit isolates as few Kontakt voices as the CSS group curves allow
(_ksp/CSS 1st Violins/groups.json): CC1=127 leaves only the ff dynamic
groups audible, CC2=0 only the nonvib groups (CC2=127 only vibsus), and a
velocity <= 64 takes the zone-1 first-note path (body only, sample offset 0,
no attack voice, no fade). CC59=0 first resets every round-robin counter.
Units are separated by silence like gen_css_ab.rs, so null_test.py and
identify.py window them the same way. 120 BPM, 600 PPQ: 1200 ticks/s, so
every time on the 5 ms grid is an exact tick (480 PPQ put 8.08 s 0.2 ms late,
enough to wreck a waveform fit).
"""
import struct, sys

PPQ = 600
TPS = 1200  # ticks per second at 120 BPM
GAP = 2.0


def vlq(v):
    out = [v & 0x7F]
    v >>= 7
    while v:
        out.append(0x80 | (v & 0x7F))
        v >>= 7
    return bytes(reversed(out))


# --transpose=N shifts every key (the probes are written for violin range,
# G4 = 67; Violas -7, Cellos -19, Basses -24 keep them inside the section).
TRANSPOSE = next((int(a.split("=", 1)[1]) for a in sys.argv if a.startswith("--transpose=")), 0)


def finish(m, rows, t):
    m.write(sys.argv[1])
    with open(sys.argv[2], "w") as f:
        f.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for r in rows:
            f.write("\t".join(map(str, (*r[:3], r[3] + TRANSPOSE, *r[4:]))) + "\n")
    print(f"{len(rows)} units, {t:.1f} s")


class Smf:
    def __init__(self):
        self.ev = []  # (tick, order, bytes)

    def cc(self, t, num, val):
        self.ev.append((round(t * TPS), 0, bytes([0xB0, num, val])))

    def note(self, t, dur, key, vel):
        key += TRANSPOSE
        self.ev.append((round(t * TPS), 1, bytes([0x90, key, vel])))
        self.ev.append((round((t + dur) * TPS), -1, bytes([0x80, key, 0])))

    def write(self, path):
        body = vlq(0) + b"\xff\x51\x03" + (500000).to_bytes(3, "big")
        last = 0
        for t, _, b in sorted(self.ev, key=lambda e: (e[0], e[1])):
            body += vlq(t - last) + b
            last = t
        body += vlq(0) + b"\xff\x2f\x00"
        hdr = b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ)
        open(path, "wb").write(hdr + b"MTrk" + struct.pack(">I", len(body)) + body)


def main():
    if len([a for a in sys.argv[1:] if not a.startswith("--")]) != 2:
        raise SystemExit(__doc__)
    m = Smf()
    rows = []
    t = 0.5
    m.cc(0.0, 59, 0)  # reset round-robins
    m.cc(0.0, 11, 127)
    m.cc(0.0, 64, 0)

    def unit(cat, desc, key, vel, cc1, cc2, hold, cc58=2):
        nonlocal t
        # controllers 1.5 s ahead: CC1/CC11 lag 120 ms, CC2 lags 1000 ms
        m.cc(t, 58, cc58)
        m.cc(t, 1, cc1)
        m.cc(t, 2, cc2)
        start = t + 1.5
        m.note(start, hold, key, vel)
        win = hold + GAP
        rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", key, cat, desc))
        t = start + win

    if "--winds" in sys.argv:
        # Woodwinds' / brass' own articulations (CSW / CSB CC58 maps): held
        # notes and connected pairs of Flutter (and brass Mutesus), trills
        # (two keys together), and brass' repeated / ornament shorts (Rips,
        # Mutereps, Taka). CC1 for dynamics; velocity sets the attack.
        def held(cat, desc, keys, vel, cc58, cc1, hold=2.0, ioi=None):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, cc1)
            start = t + 1.5
            if ioi is None:
                for key in keys:
                    m.note(start, hold, key, vel)
                win = hold + 2.5
            else:
                m.note(start, ioi + 0.05, keys[0], vel)
                m.note(start + ioi, hold, keys[1], vel)
                win = ioi + hold + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", keys[-1], cat, desc))
            t = start + win

        sustained = [("Flutter", 51)]
        if "--brass" in sys.argv:
            sustained.append(("Mutesus", 56))
        for name, cc58 in sustained:
            for cc1 in (40, 110):
                held(name, f"{name} key67 cc{cc1} v80", [67], 80, cc58, cc1)
            held(name, f"{name} key72 cc80 v80", [72], 80, cc58, 80)
            held(name, f"{name} 67->69 ioi500 cc80", [67, 69], 80, cc58, 80, ioi=0.5)
        held("Trills", "Trills 67+68 (half) cc60", [67, 68], 80, 46, 60)
        held("Trills", "Trills 67+69 (whole) cc110", [67, 69], 80, 46, 110)
        if "--brass" in sys.argv:
            for name, cc58 in (("Rips", 41), ("Mutereps", 31), ("Taka", 61)):
                for vel in (60, 120):
                    m.cc(t, 58, cc58)
                    m.cc(t, 1, 100)
                    start = t + 1.5
                    n = 1 if name == "Rips" else 4
                    for i in range(n):
                        m.note(start + i * 0.4, 0.2 if n > 1 else 1.0, 67, vel)
                    win = 0.4 * (n - 1) + 3.0
                    rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", 67, name, f"{name} key67 v{vel} x{n}"))
                    t = start + win
        finish(m, rows, t)
        return

    if "--tremolo" in sys.argv:
        # Tremolo / harmonics (mono path, legato on) and trills (two keys
        # within 25 ms). CC1 127; velocity 40 = zone 1 (no random re-bow
        # attack on transitions).
        def single(cat, desc, keys, vel, cc58, hold=2.0):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, 127)
            start = t + 1.5
            for key in keys:
                m.note(start, hold, key, vel)
            win = hold + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", keys[0], cat, desc))
            t = start + win

        def phrase(cat, desc, a, b, vel, ioi, cc58):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, 127)
            start = t + 1.5
            m.note(start, ioi + 0.05, a, vel)
            m.note(start + ioi, 2.0, b, vel)
            win = ioi + 2.0 + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", b, cat, desc))
            t = start + win

        if "--rebow" in sys.argv:
            # Zone-2/3 tremolo transitions: the random re-bow attack voice.
            for vel in (80, 80, 120, 120):
                phrase("Tremolo", f"Tremolo 67->69 ioi500 v{vel}", 67, 69, vel, 0.5, 56)
            finish(m, rows, t)
            return
        for name, cc58 in (("Tremolo", 56), ("Harm", 51)):
            single(name, f"{name} key67 v40", [67], 40, cc58)
            single(name, f"{name} key72 v40", [72], 40, cc58)
            for a, b in ((67, 69), (67, 72), (69, 66)):
                phrase(name, f"{name} {a}->{b} ioi500 v40", a, b, 40, 0.5, cc58)
        single("Trills", "Trills 67+68 (half) v40", [67, 68], 40, 46)
        single("Trills", "Trills 67+69 (whole) v40", [67, 69], 40, 46)
        finish(m, rows, t)
        return

    if "--shorts" in sys.argv:
        # Shorts (CC58 picks the type). A key repeated within 500 ms plays
        # untuned; the first note of each burst draws Kontakt's random ±5 c
        # (fitted from the render and forced into ours). Bursts walk the RR
        # sequence of one velocity band; the alternating burst shares a
        # note-class (67/68), exercising the no-repeat rule.
        # The short TYPE follows CC1 (Kontakt's MIDI-learned type knob):
        # 0-5 Spiccato, 6-64 Staccatissimo, 65-121 Staccato, 122+ Sfz.
        cc1_for = {11: 2, 16: 40, 21: 100, 26: 127}

        def burst(cat, desc, keys, vel, gap, cc58):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, cc1_for[cc58])
            start = t + 1.5
            for i, key in enumerate(keys):
                m.note(start + i * gap, 0.1, key, vel)
            win = gap * (len(keys) - 1) + 3.0
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", keys[0], cat, desc))
            t = start + win

        types = (("Spiccato", 11, (40, 100, 120)), ("Staccatissimo", 16, (40, 100, 120)),
                 ("Staccato", 21, (40, 100, 120)), ("Sfz", 26, (60, 100, 120)))
        if "--pizz" in sys.argv:
            # Pizzicato family: the type knob (CC1) <11 / 11-117 / >117.
            cc1_for.update({31: 5, 36: 60, 41: 127})
            types = (("Pizzicato", 31, (30, 60, 90, 120)), ("Bartokpizz", 36, (40, 100)),
                     ("Clegno", 41, (30, 70, 110)))
        for name, cc58, vels in types:
            for vel in vels:
                burst(name, f"{name} key67 v{vel} x6", [67] * 6, vel, 0.3, cc58)
            burst(name, f"{name} alt67/68 v100 x6", [67, 68] * 3, 100, 0.2, cc58)
        finish(m, rows, t)
        return

    if "--marcato" in sys.argv:
        # Marcato (CC58 66 = overlay off) — CC1=127, CC2 has no say here.
        # Single notes (soft/hard) and connected pairs at slow gaps (no run
        # mode, which only engages within 175 ms).
        if "--overlay" in sys.argv:
            # Marcato WITH overlay (CC58 71) single notes, and runs (IOI <=
            # 175 ms: run mode's extra voice) without and with overlay.
            for vel in (40, 120):
                for key in (67, 72):
                    unit("MARC-OV", f"marcato+overlay key{key} v{vel}", key, vel, 127, 0, 2.0, cc58=71)

            def run(desc, keys, vel, gap, cc58):
                nonlocal t
                m.cc(t, 58, cc58)
                m.cc(t, 1, 127)
                start = t + 1.5
                for i, key in enumerate(keys):
                    last = i == len(keys) - 1
                    m.note(start + i * gap, 2.0 if last else gap + 0.05, key, vel)
                win = gap * (len(keys) - 1) + 2.0 + 2.5
                rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", keys[-1], "MARC-RUN", desc))
                t = start + win

            for vel in (40, 120):
                for gap in (0.06, 0.12):
                    run(f"run 67-69-71-72 v{vel} gap{int(gap*1000)}", [67, 69, 71, 72], vel, gap, 66)
            run("run 67-69-71-72 v120 gap120 +overlay", [67, 69, 71, 72], 120, 0.12, 71)
            finish(m, rows, t)
            return
        for vel in (40, 120):
            for key in (67, 72):
                unit("MARC-1", f"marcato key{key} v{vel}", key, vel, 127, 0, 2.0, cc58=66)

        def mphrase(desc, a, b, vel, ioi):
            nonlocal t
            m.cc(t, 58, 66)
            m.cc(t, 1, 127)
            start = t + 1.5
            m.note(start, ioi + 0.05, a, vel)
            m.note(start + ioi, 2.0, b, vel)
            win = ioi + 2.0 + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", b, "MARC-LEG", desc))
            t = start + win

        for vel in (40, 120):
            for ioi in (0.3, 0.5, 1.0):
                mphrase(f"MC v{vel} 67->69 ioi{int(ioi*1000)}", 67, 69, vel, ioi)
            for a, b in ((67, 72), (69, 66), (67, 81)):
                mphrase(f"MC v{vel} {a}->{b} ioi500", a, b, vel, 0.5)
        finish(m, rows, t)
        return

    if "--attack" in sys.argv:
        # Loud first notes with vibrato up: CSS plays an attack voice from
        # the vibsus groups' attack zones (a Staccato at velocity 5-7 in
        # zone 2, an Sfz at 8-9 in zone 3, drawn at random), which nonvib
        # (CC2=0) never shows. Twice each, to see the draw vary.
        for vel in (80, 80, 120, 120):
            for key in (67, 72):
                unit("SUS-VATK", f"vibsus ff key{key} v{vel} (attack voice)", key, vel, 127, 127, 2.0)
        unit("SUS-VATK", "vib/nonvib CC2=64 key67 v120", 67, 120, 127, 64, 2.0)
        finish(m, rows, t)
        return

    if "--tunes" in sys.argv:
        # One single-voice note (nonvib ff, CC1=127, CC2=0, velocity 40) per
        # playable key: Kontakt's effective playback rate per zone, to find
        # how it turns group × zone tune × semitones into a rate.
        for key in range(55, 99):
            unit("TUNE", f"nonvib ff only key{key} v40", key, 40, 127, 0, 3.0)
        finish(m, rows, t)
        return

    if "--legato" in sys.argv:
        # Two-note connected phrases (second note-on before the first
        # note-off) — CC1=127/CC2=0 leaves only the ff nonvib groups (NV
        # sustains + NV legato transitions). Zone 2 (vel 65-100) draws a random
        # body variant, so only zones 1 and 3 are probed (deterministic).
        def phrase(desc, a, b, vel, ioi, cc58):
            nonlocal t
            m.cc(t, 58, cc58)
            m.cc(t, 1, 127)
            m.cc(t, 2, 0)
            start = t + 1.5
            m.note(start, ioi + 0.05, a, vel)  # overlaps the next note-on by 50 ms
            m.note(start + ioi, 2.0, b, vel)
            win = ioi + 2.0 + 2.5
            rows.append((len(rows), f"{start:.3f}", f"{win:.3f}", b, "LEG", desc))
            t = start + win

        vels = (80,) if "--zone2" in sys.argv else (40, 120)
        for mode, cc58 in (("LL", 2), ("EX", 8)):
            for vel in vels:
                for ioi in (0.08, 0.2, 1.0):
                    phrase(f"{mode} v{vel} 67->69 ioi{int(ioi*1000)}", 67, 69, vel, ioi, cc58)
                for a, b in ((67, 72), (69, 66), (67, 79), (67, 81)):
                    phrase(f"{mode} v{vel} {a}->{b} ioi500", a, b, vel, 0.5, cc58)
        finish(m, rows, t)
        return

    if "--curves" in sys.argv:
        # CC→volume law of the dynamic groups: one note per CC value, short
        # holds (only the steady body is fitted), nonvib only (CC2=0).
        for cc1 in list(range(0, 128, 4)) + [127]:
            unit("CURVE-CC1", f"nonvib key67 v40 cc1={cc1}", 67, 40, cc1, 0, 1.2)
        for cc11 in (0, 16, 32, 48, 64, 80, 96, 112, 127):
            m.cc(t, 11, cc11)
            unit("CURVE-CC11", f"nonvib ff key67 v40 cc11={cc11}", 67, 40, 127, 0, 1.2)
        m.cc(t, 11, 127)
        for cc2 in (0, 16, 32, 48, 64, 80, 96, 112, 127):
            unit("CURVE-CC2", f"vib/nonvib ff key67 v40 cc2={cc2}", 67, 40, 127, cc2, 1.2)
        finish(m, rows, t)
        return

    for key in (67, 69, 72, 79):
        unit("SUS-1V", f"nonvib ff only key{key} v40", key, 40, 127, 0, 4.0)
    for key in (67, 72):
        unit("SUS-1V", f"vibsus ff only key{key} v40", key, 40, 127, 127, 4.0)
    for key in (68, 70):
        unit("SUS-SHIFT", f"nonvib ff only key{key} v40 (resampled)", key, 40, 127, 0, 4.0)
    unit("SUS-LAYER", "nonvib CC1=90 key67 v40 (2 layers)", 67, 40, 90, 0, 4.0)
    unit("SUS-ATK", "nonvib ff key67 v100 (attack voice)", 67, 100, 127, 0, 4.0)
    unit("SUS-ATK", "nonvib ff key67 v120 (zone 3 attack)", 67, 120, 127, 0, 4.0)

    finish(m, rows, t)


if __name__ == "__main__":
    main()
