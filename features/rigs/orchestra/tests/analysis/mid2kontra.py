"""mid2kontra.py <probe.mid> [rate] > scenario.json : a Standard MIDI File as a
`kontra-inspect play --scenario` file (note_on / note_off / cc at sample
frames, 48 kHz by default). Every track is merged; tempo changes apply."""
import json
import struct
import sys


def vlq(b, i):
    v = 0
    while True:
        c = b[i]
        i += 1
        v = (v << 7) | (c & 0x7F)
        if c < 0x80:
            return v, i


def events(path):
    b = open(path, "rb").read()
    assert b[:4] == b"MThd", "not a MIDI file"
    _, ntracks, division = struct.unpack(">HHH", b[8:14])
    assert division < 0x8000, "SMPTE time is not supported"
    pos = 8 + struct.unpack(">I", b[4:8])[0]
    raw = []  # (tick, order, kind, data)
    for _ in range(ntracks):
        assert b[pos:pos + 4] == b"MTrk"
        end = pos + 8 + struct.unpack(">I", b[pos + 4:pos + 8])[0]
        i, tick, status = pos + 8, 0, 0
        while i < end:
            d, i = vlq(b, i)
            tick += d
            if b[i] == 0xFF:
                kind, i = b[i + 1], i + 2
                n, i = vlq(b, i)
                if kind == 0x51:
                    raw.append((tick, len(raw), "tempo", int.from_bytes(b[i:i + 3], "big")))
                i += n
                continue
            if b[i] in (0xF0, 0xF7):
                n, i = vlq(b, i + 1)
                i += n
                continue
            if b[i] & 0x80:
                status, i = b[i], i + 1
            hi = status & 0xF0
            if hi in (0xC0, 0xD0):
                i += 1
                continue
            d1, d2 = b[i], b[i + 1]
            i += 2
            if hi == 0x90 and d2 > 0:
                raw.append((tick, len(raw), "note_on", (d1, d2)))
            elif hi in (0x80, 0x90):
                raw.append((tick, len(raw), "note_off", (d1, 0)))
            elif hi == 0xB0:
                raw.append((tick, len(raw), "cc", (d1, d2)))
        pos = end
    raw.sort()
    return division, raw


def main():
    rate = int(sys.argv[2]) if len(sys.argv) > 2 else 48000
    division, raw = events(sys.argv[1])
    out, us_per_q, last_tick, secs = [], 500000, 0, 0.0
    for tick, _, kind, data in raw:
        secs += (tick - last_tick) * us_per_q / 1e6 / division
        last_tick = tick
        frame = round(secs * rate)
        if kind == "tempo":
            us_per_q = data
        elif kind == "cc":
            out.append({"frame": frame, "type": "cc", "cc": data[0], "value": data[1]})
        elif kind == "note_on":
            out.append({"frame": frame, "type": "note_on", "key": data[0], "velocity": data[1]})
        else:
            out.append({"frame": frame, "type": "note_off", "key": data[0]})
    json.dump({"events": out}, sys.stdout)


if __name__ == "__main__":
    main()
