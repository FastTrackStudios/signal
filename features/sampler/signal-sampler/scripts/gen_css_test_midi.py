#!/usr/bin/env python3
"""Generate the CSS parameter-space test MIDI + manifest.

One track, 120 BPM, tpq 960. Sections separated by 2 bars of silence, each
section's CCs set 100 ms before its first note. Legato notes overlap 54 ms
(matching the hand-played reference). Render through Kontakt CSS (default
patch state, no keyswitches, Mix mic, reverb 0) in one pass; the manifest
gives exact per-section parameters for our engine to replay and diff.

Usage: gen_css_test_midi.py <out.mid> <out.manifest.json>
"""
import json
import struct
import sys

TPQ = 960
BPM = 120.0
SEC_PER_QN = 60.0 / BPM  # 0.5 s
OVERLAP = 0.054  # s, legato note overlap (matches the reference export)


def t2tick(sec: float) -> int:
    return round(sec / SEC_PER_QN * TPQ)


events = []  # (tick, priority, bytes)


def cc(t, num, val):
    events.append((t2tick(t), 0, bytes([0xB0, num, val])))


def marker(t, text):
    b = text.encode()
    events.append((t2tick(t), 0, bytes([0xFF, 0x06, len(b)]) + b))


def note(t, dur, pitch, vel):
    events.append((t2tick(t), 1, bytes([0x90, pitch, vel])))
    events.append((t2tick(t + dur), 0, bytes([0x80, pitch, 64])))


SCALE = [60, 62, 64, 65, 67, 65, 64, 62, 60]  # C up to G and back

manifest = {"bpm": BPM, "tpq": TPQ, "overlap_s": OVERLAP, "sections": []}
cursor = 1.0  # start at 1 s


def section(name, cc1, cc2, notes, note_dur=0.5, legato=True, gap=0.0):
    """notes: list of (offset_s, pitch, vel) or legato line [(pitch, vel), ...]."""
    global cursor
    start = cursor
    marker(start - 0.5, name)
    cc(start - 0.25, 1, cc1)
    cc(start - 0.25, 2, cc2)
    seq = []
    if legato:
        for i, (p, v) in enumerate(notes):
            t = start + i * note_dur
            note(t, note_dur + OVERLAP, p, v)
            seq.append({"t": round(t, 3), "pitch": p, "vel": v, "dur": round(note_dur + OVERLAP, 3)})
        end = start + len(notes) * note_dur + OVERLAP
    else:
        for off, p, v in notes:
            t = start + off
            note(t, note_dur, p, v)
            seq.append({"t": round(t, 3), "pitch": p, "vel": v, "dur": note_dur})
        end = start + max(off for off, _, _ in notes) + note_dur
    manifest["sections"].append(
        {"name": name, "start_s": round(start, 3), "end_s": round(end, 3),
         "cc1": cc1, "cc2": cc2, "notes": seq}
    )
    cursor = end + 4.0 + gap  # 2 bars of silence


# ── Velocity zones over the same line (CC1=80, non-vib) ──────────────────────
section("S01 porta vel1", 80, 0, [(p, 1) for p in SCALE])
section("S02 legato vel40 z1", 80, 0, [(p, 40) for p in SCALE])
section("S03 legato vel90 z2", 80, 0, [(p, 90) for p in SCALE])
section("S04 legato vel115 z3", 80, 0, [(p, 115) for p in SCALE])
# ── CC1 dynamic layers (vel 90) ──────────────────────────────────────────────
section("S05 cc1=20 soft", 20, 0, [(p, 90) for p in SCALE])
section("S06 cc1=127 full", 127, 0, [(p, 90) for p in SCALE])
# ── Vibrato ──────────────────────────────────────────────────────────────────
section("S07 vibrato cc2=127", 80, 127, [(p, 90) for p in SCALE])
# ── Intervals (vel 90, cc1 80, non-vib) ──────────────────────────────────────
section("S08 intervals", 80, 0,
        [(60, 90), (64, 90), (60, 90), (65, 90), (60, 90), (67, 90), (60, 90), (72, 90), (60, 90)])
section("S09 big leaps >12", 80, 0, [(60, 90), (76, 90), (60, 90), (79, 90), (60, 90)])
section("S10 rebows same note", 80, 0, [(60, 90)] * 5)
# ── Fast runs (IOI interpolation) ────────────────────────────────────────────
section("S11 fast run vel90", 80, 0, [(p, 90) for p in SCALE], note_dur=0.125)
section("S12 fast run vel40", 80, 0, [(p, 40) for p in SCALE], note_dur=0.125)
# ── Fresh attacks + release tails (detached, no overlap) ─────────────────────
section("S13 detached attacks", 80, 0,
        [(0.0, 60, 90), (2.0, 64, 90), (4.0, 67, 40), (6.0, 72, 115)],
        note_dur=1.0, legato=False)
# ── CC sweeps on a held note (lag/crossfade behavior) ────────────────────────
start = cursor
marker(start - 0.5, "S14 cc1 sweep held C4")
cc(start - 0.25, 1, 0)
cc(start - 0.25, 2, 0)
note(start, 8.0, 60, 90)
for i in range(33):
    cc(start + 0.5 + i * 6.0 / 32.0, 1, min(127, i * 4))
manifest["sections"].append({"name": "S14 cc1 sweep held C4", "start_s": round(start, 3),
                             "end_s": round(start + 8.0, 3), "cc1": "sweep 0..127 over 0.5-6.5s",
                             "cc2": 0, "notes": [{"t": round(start, 3), "pitch": 60, "vel": 90, "dur": 8.0}]})
cursor = start + 12.0
start = cursor
marker(start - 0.5, "S15 cc2 sweep held C4")
cc(start - 0.25, 1, 80)
cc(start - 0.25, 2, 0)
note(start, 8.0, 60, 90)
for i in range(33):
    cc(start + 0.5 + i * 6.0 / 32.0, 2, min(127, i * 4))
manifest["sections"].append({"name": "S15 cc2 sweep held C4", "start_s": round(start, 3),
                             "end_s": round(start + 8.0, 3), "cc1": 80,
                             "cc2": "sweep 0..127 over 0.5-6.5s",
                             "notes": [{"t": round(start, 3), "pitch": 60, "vel": 90, "dur": 8.0}]})
cursor = start + 12.0

# ── Serialize SMF format 1 ───────────────────────────────────────────────────
def vlq(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append((n & 0x7F) | 0x80)
        n >>= 7
    return bytes(reversed(out))


events.sort(key=lambda e: (e[0], e[1]))
tempo = struct.pack(">I", int(60_000_000 / BPM))[1:]
trk0 = vlq(0) + bytes([0xFF, 0x51, 0x03]) + tempo + vlq(0) + bytes([0xFF, 0x2F, 0x00])
body = b""
last = 0
for tick, _, ev in events:
    body += vlq(tick - last) + ev
    last = tick
body += vlq(TPQ * 4) + bytes([0xFF, 0x2F, 0x00])

def chunk(tag, data):
    return tag + struct.pack(">I", len(data)) + data

out = chunk(b"MThd", struct.pack(">HHH", 1, 2, TPQ)) + chunk(b"MTrk", trk0) + chunk(b"MTrk", body)
open(sys.argv[1], "wb").write(out)
json.dump(manifest, open(sys.argv[2], "w"), indent=1)
print(f"wrote {sys.argv[1]} ({len(out)} bytes), {len(manifest['sections'])} sections, "
      f"total {cursor:.1f}s")
