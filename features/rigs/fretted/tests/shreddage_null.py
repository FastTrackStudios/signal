#!/usr/bin/env python3
"""Null one render against a Kontakt reference, note by note.

usage: shreddage_null.py <ours.wav> <kontakt.wav> <manifest.tsv> [--tail 0.5] [--json out.json]

The manifest is shreddage_probe.py's (i, start, end, key, vel). For each
note, over [start − 20 ms, end + tail]:
  strict_db  residual ours − Kontakt, energy relative to Kontakt's
  lag        the lag (samples, ours later = +) that minimises the residual,
             searched ±960 samples on the whole file first, then ±48 per note
  at_lag_db  the residual at that lag
  gain_db    the least-squares gain that maps ours onto Kontakt at that lag
Reading (nkx skill): gain ≈ −3…−20 dB with strict ≈ 0 → a different sample
or pitch; a whole-ms lag → a script wait; a flat ratio → level or pan.
"""
import json, struct, sys

import numpy as np


def read_wav(path):
    d = open(path, "rb").read()
    assert d[:4] == b"RIFF" and d[8:12] == b"WAVE", path
    p, fmt, data = 12, None, None
    while p + 8 <= len(d):
        cid, n = d[p:p + 4], struct.unpack("<I", d[p + 4:p + 8])[0]
        body = d[p + 8:p + 8 + n]
        if cid == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif cid == b"data":
            data = body
        p += 8 + n + (n & 1)
    tag, ch, sr, _, _, bits = fmt
    if tag == 0xFFFE:
        tag = 3 if bits == 32 and struct.unpack("<H", d[d.find(b"fmt ") + 32:d.find(b"fmt ") + 34])[0] == 3 else 1
    if tag == 3:
        x = np.frombuffer(data, dtype="<f4" if bits == 32 else "<f8").astype(np.float64)
    elif bits == 24:
        b = np.frombuffer(data[: len(data) // 3 * 3], dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        v = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16)
        v = np.where(v >= 1 << 23, v - (1 << 24), v)
        x = v / float(1 << 23)
    else:
        x = np.frombuffer(data, dtype={16: "<i2", 32: "<i4"}[bits]).astype(np.float64) / float(1 << (bits - 1))
    return x.reshape(-1, ch), sr


def shift(x, lag):
    """x delayed by `lag` samples (negative = advanced), same length."""
    y = np.zeros_like(x)
    if lag >= 0:
        y[lag:] = x[: len(x) - lag]
    else:
        y[:lag] = x[-lag:]
    return y


def resid_db(a, k):
    ek = float(np.sum(k * k))
    return 10 * np.log10(max(float(np.sum((a - k) ** 2)), 1e-20) / max(ek, 1e-20))


def best_lag(a, k, span):
    best = (0, resid_db(a, k))
    for lag in range(-span, span + 1):
        r = resid_db(shift(a, lag), k)
        if r < best[1]:
            best = (lag, r)
    return best


def main():
    ours_p, kon_p, man_p = sys.argv[1:4]
    tail = float(sys.argv[sys.argv.index("--tail") + 1]) if "--tail" in sys.argv else 0.5
    ours, sr = read_wav(ours_p)
    kon, sr2 = read_wav(kon_p)
    assert sr == sr2, (sr, sr2)
    n = min(len(ours), len(kon))
    ours, kon = ours[:n], kon[:n]
    a, k = ours.sum(axis=1), kon.sum(axis=1)
    rows = [l.rstrip("\n").split("\t") for l in open(man_p)][1:]

    # whole-file lag by cross-correlation of the first 20 s
    m = min(n, 20 * sr)
    spec = np.fft.rfft(k[:m], 2 * m) * np.conj(np.fft.rfft(a[:m], 2 * m))
    cc = np.fft.irfft(spec)
    cc = np.concatenate([cc[-960:], cc[:961]])
    glag = int(np.argmax(cc)) - 960
    print(f"whole-file lag {glag} samples ({glag / sr * 1000:.2f} ms), ours→Kontakt")

    out = []
    print(f"{'i':>3} {'key':>3} {'vel':>3} {'strict':>7} {'lag':>5} {'at_lag':>7} {'gain':>7}")
    for i, s, e, key, vel in rows:
        lo, hi = int((float(s) - 0.02) * sr), min(n, int((float(e) + tail) * sr))
        ka = k[lo:hi]
        aa = shift(a, glag)[lo:hi]
        strict = resid_db(a[lo:hi], ka)
        lag, at = best_lag(aa, ka, 48)
        al = shift(aa, lag)
        g = float(np.dot(al, ka) / max(np.dot(al, al), 1e-20))
        gain = 20 * np.log10(abs(g)) if g else -999
        lag += glag
        out.append({"i": int(i), "key": int(key), "vel": int(vel), "strict_db": round(strict, 2),
                     "lag": lag, "at_lag_db": round(at, 2), "gain_db": round(gain, 2)})
        print(f"{i:>3} {key:>3} {vel:>3} {strict:7.2f} {lag:5d} {at:7.2f} {gain:7.2f}")
    at = sorted(r["at_lag_db"] for r in out)
    print(f"median at_lag {at[len(at) // 2]:.2f} dB, worst {at[-1]:.2f}, best {at[0]:.2f}")
    if "--json" in sys.argv:
        json.dump(out, open(sys.argv[sys.argv.index("--json") + 1], "w"), indent=1)


if __name__ == "__main__":
    main()
