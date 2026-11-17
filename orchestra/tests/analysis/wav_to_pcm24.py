#!/usr/bin/env python3
"""32-bit float (or any PCM) WAV -> 24-bit PCM WAV, for null_test.py (which reads integer PCM only).

usage: wav_to_pcm24.py <in.wav> <out.wav>

KONTRA's `KONTRA_INSPECT_WAV` writes IEEE float (format 3, often WAVE_FORMAT_EXTENSIBLE), which
python's `wave` module refuses; this reads the RIFF chunks itself. Samples are clipped to
[-1, 1) and rounded to 24 bits; no dither, no gain.
"""
import struct, sys
import numpy as np


def read_any(path):
    d = open(path, "rb").read()
    assert d[:4] == b"RIFF" and d[8:12] == b"WAVE", "not a WAV"
    o, fmt, data = 12, None, None
    while o + 8 <= len(d):
        cid, ln = d[o:o + 4], struct.unpack_from("<I", d, o + 4)[0]
        body = d[o + 8:o + 8 + ln]
        if cid == b"fmt ":
            tag, ch, sr, _, _, bits = struct.unpack_from("<HHIIHH", body)
            if tag == 0xFFFE:
                tag = struct.unpack_from("<H", body, 24)[0]
            fmt = (tag, ch, sr, bits)
        elif cid == b"data":
            data = body
        o += 8 + ln + (ln & 1)
    tag, ch, sr, bits = fmt
    if tag == 3 and bits == 32:
        x = np.frombuffer(data, "<f4").astype(np.float64)
    elif tag == 3 and bits == 64:
        x = np.frombuffer(data, "<f8")
    elif tag == 1 and bits == 16:
        x = np.frombuffer(data, "<i2") / 32768.0
    elif tag == 1 and bits == 24:
        a = np.frombuffer(data, np.uint8).reshape(-1, 3).astype(np.int32)
        v = a[:, 0] | (a[:, 1] << 8) | (a[:, 2] << 16)
        x = np.where(v >= 1 << 23, v - (1 << 24), v) / float(1 << 23)
    elif tag == 1 and bits == 32:
        x = np.frombuffer(data, "<i4") / float(1 << 31)
    else:
        sys.exit(f"unsupported WAV format tag {tag}, {bits} bits")
    return x.reshape(-1, ch), sr


def write24(path, x, sr):
    ch = x.shape[1]
    v = np.clip(np.round(x * (1 << 23)), -(1 << 23), (1 << 23) - 1).astype(np.int32).reshape(-1)
    b = np.empty((v.size, 3), np.uint8)
    b[:, 0] = v & 0xFF; b[:, 1] = (v >> 8) & 0xFF; b[:, 2] = (v >> 16) & 0xFF
    data = b.tobytes()
    hdr = struct.pack("<4sI4s4sIHHIIHH4sI", b"RIFF", 36 + len(data), b"WAVE", b"fmt ", 16, 1, ch, sr,
                      sr * ch * 3, ch * 3, 24, b"data", len(data))
    open(path, "wb").write(hdr + data)


if __name__ == "__main__":
    x, sr = read_any(sys.argv[1])
    write24(sys.argv[2], x, sr)
    print(f"{sys.argv[2]}: {len(x)} frames x {x.shape[1]} ch @ {sr}, peak {np.abs(x).max():.4f}")
