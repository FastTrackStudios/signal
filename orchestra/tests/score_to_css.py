#!/usr/bin/env python3
"""MusicXML score -> one CSS-ready SMF per part (render_css_live input).

    python3 score_to_css.py <score.xml|.mxl> <outdir> [--swing=0.60]

Writes <outdir>/<part id>.mid (P1, P2, ...) and prints each part's name.
Eighths are straight unless --swing=<ratio> (0.60 = MuseScore's default);
a "Swing" marking in the score is NOT applied by itself.

Per part, channel 0, single track, 1 s per second (tempo 120, ticks = ms):
  - 2 s pre-roll: CC58=2 (legato sustain), CC1=90 (sustain dynamic; also the
    Staccato short type), CC11=127, CC2=90 (vibrato).
  - ties merged; plain notes detached (gap), tenuto/slurred notes overlap the
    next note (legato transition); staccato notes switch to the shorts group
    (CC58=21) and back.
"""
import io, os, struct, sys, zipfile
import xml.etree.ElementTree as ET

PRE = 2.0
TPQ = 500  # with 120 bpm: 1 tick = 1 ms
SWING = next((float(a.split("=")[1]) for a in sys.argv if a.startswith("--swing=")), 0.5)
STEP = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def parts(root):
    tempo = 110.0
    for s in root.iter("sound"):
        if s.get("tempo"):
            tempo = float(s.get("tempo"))
            break
    beat = 60.0 / tempo
    out = {}
    for p in root.iter("part"):
        div = 1
        pos = 0.0  # quarters
        notes = []  # [start_q, dur_q, key, flags]
        for m in p.findall("measure"):
            for el in m:
                if el.tag == "attributes" and el.findtext("divisions"):
                    div = int(el.findtext("divisions"))
                elif el.tag == "backup":
                    pos -= int(el.findtext("duration")) / div
                elif el.tag == "forward":
                    pos += int(el.findtext("duration")) / div
                elif el.tag == "note":
                    d = int(el.findtext("duration") or 0) / div
                    if el.find("grace") is not None:
                        continue
                    if el.find("chord") is not None:
                        continue  # parts are monophonic
                    if el.find("rest") is None:
                        pt = el.find("pitch")
                        key = 12 * (int(pt.findtext("octave")) + 1) + STEP[pt.findtext("step")] + int(float(pt.findtext("alter") or 0))
                        ties = {t.get("type") for t in el.findall("tie")}
                        fl = set()
                        for n in el.iter("notations"):
                            for a in n.iter():
                                if a.tag in ("staccato", "staccatissimo", "spiccato"):
                                    fl.add("stacc")
                                elif a.tag == "tenuto":
                                    fl.add("ten")
                                elif a.tag == "accent" or a.tag == "strong-accent":
                                    fl.add("acc")
                                elif a.tag == "slur":
                                    fl.add("slur-" + a.get("type", ""))
                        if "stop" in ties and notes and notes[-1][2] == key and abs(notes[-1][0] + notes[-1][1] - pos) < 1e-6:
                            notes[-1][1] += d
                        else:
                            notes.append([pos, d, key, fl])
                    pos += d
        # slurs: mark notes inside a slur span
        inside = False
        for n in notes:
            if "slur-start" in n[3]:
                inside = True
            if inside:
                n[3].add("slur")
            if "slur-stop" in n[3]:
                inside = False
        out[p.get("id")] = (notes, beat)
    return out


def swing_q(q):
    """Swing eighths: the off-beat eighth moves to SWING of the beat."""
    b = int(q // 1)
    f = q - b
    if f <= 0.5:
        f = f / 0.5 * SWING
    else:
        f = SWING + (f - 0.5) / 0.5 * (1 - SWING)
    return b + f


def vlq(v):
    b = [v & 0x7F]
    v >>= 7
    while v:
        b.append(0x80 | (v & 0x7F))
        v >>= 7
    return bytes(reversed(b))


def write_smf(path, events):
    events.sort(key=lambda e: (e[0], e[1]))
    trk = bytearray(vlq(0) + b"\xff\x51\x03" + (500000).to_bytes(3, "big"))
    last = 0
    for t, _, msg in events:
        trk += vlq(t - last) + bytes(msg)
        last = t
    trk += vlq(0) + b"\xff\x2f\x00"
    with open(path, "wb") as f:
        f.write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, TPQ) + b"MTrk" + struct.pack(">I", len(trk)) + trk)


def build(notes, beat):
    ev = []
    ms = lambda s: int(round(s * 1000))
    cc = lambda t, c, v, o=0: ev.append((ms(t), o, [0xB0, c, v]))
    cc(0.0, 58, 2)
    cc(0.0, 1, 90)
    cc(0.0, 11, 127)
    cc(0.0, 2, 90)
    art = 2
    for i, (q, d, key, fl) in enumerate(notes):
        t0 = PRE + swing_q(q) * beat
        t1 = PRE + swing_q(q + d) * beat
        nxt = notes[i + 1] if i + 1 < len(notes) else None
        joined = nxt is not None and abs(q + d - nxt[0]) < 1e-6
        want = 21 if "stacc" in fl else 2
        if want != art:
            cc(t0 - 0.03, 58, want)
            art = want
        if want == 21:
            vel = 100 if "acc" in fl else 88
            end = t0 + min(t1 - t0, 0.18)
        else:
            vel = 96 if "acc" in fl else 80
            if joined and (fl & {"ten", "slur"}):
                end = t1 + 0.04  # overlap: legato transition into the next note
            else:
                end = t1 - min(0.06, 0.15 * (t1 - t0))
        ev.append((ms(t0), 2, [0x90, key, vel]))
        ev.append((ms(end), 1, [0x80, key, 0]))
    ev.append((ms(ev[-1][0] / 1000 + 4), 0, [0xB0, 64, 0]))
    return ev


def load_score(path):
    """A MusicXML root from .xml/.musicxml or compressed .mxl."""
    if path.endswith(".mxl"):
        z = zipfile.ZipFile(path)
        cont = ET.fromstring(z.read("META-INF/container.xml"))
        rf = next(cont.iter("rootfile")).get("full-path")
        return ET.parse(io.BytesIO(z.read(rf))).getroot()
    return ET.parse(path).getroot()


def main():
    src, outdir = sys.argv[1], sys.argv[2]
    os.makedirs(outdir, exist_ok=True)
    root = load_score(src)
    names = {sp.get("id"): sp.findtext("part-name") for sp in root.iter("score-part")}
    for pid, (notes, beat) in parts(root).items():
        ev = build(notes, beat)
        write_smf(os.path.join(outdir, f"{pid}.mid"), ev)
        print(pid, names.get(pid, ""), len(notes), "notes", f"{ev[-1][0] / 1000:.1f}s", sep="\t")


if __name__ == "__main__":
    main()
