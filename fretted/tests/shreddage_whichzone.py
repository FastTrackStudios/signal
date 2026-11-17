#!/usr/bin/env python3
"""Which zone did Kontakt play for one probe note?

usage: shreddage_whichzone.py <render.wav> <manifest.tsv> <library.styx> <samples root> <articulation> <i> [<i> …]

For probe note i (key k, start s): takes the render over [s + 30 ms, s + 430 ms]
and every zone of <articulation> whose key range holds k − 2, k or k + 2 (a
neighbour borrow plays the zone two frets away retuned to k), resamples each
zone's file to the played pitch at the render's rate, and ranks them by
normalized cross-correlation (offset search ±60 ms). Prints the top five:
score, string, dynamic, rr, file, and where in the file the segment starts.
"""
import os, re, sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shreddage_null import read_wav  # noqa: E402


def zones(path, artic):
    t = open(path).read()
    out = []
    for b in re.findall(r"\{(.*?)\}", t, re.S):
        d = dict(re.findall(r'(\w+)\s+("[^"]*"|\S+)', b))
        if d.get("articulation", "").strip('"') != artic:
            continue
        out.append({k: v.strip('"') for k, v in d.items()})
    return out


def main():
    wav, man, lib, root, artic = sys.argv[1:6]
    idxs = [int(x) for x in sys.argv[6:]]
    r, sr = read_wav(wav)
    r = r.sum(axis=1)
    rows = {int(l.split("\t")[0]): l.split("\t") for l in open(man).read().splitlines()[1:]}
    zs = zones(lib, artic)
    cache = {}
    for i in idxs:
        _, s, e, key, vel = rows[i]
        key = int(key)
        t0 = float(s)
        seg = r[int((t0 + 0.03) * sr): int((t0 + 0.43) * sr)]
        seg = seg - seg.mean()
        res = []
        for z in zs:
            lo, hi = int(z["key_min"]), int(z["key_max"])
            if not any(lo <= k <= hi for k in (key - 2, key, key + 2)):
                continue
            f = os.path.join(root, z["file"])
            if f not in cache:
                x, fsr = read_wav(f)
                cache[f] = (x[:, 0], fsr)
            x, fsr = cache[f]
            step = (fsr / sr) * 2 ** ((key - int(z["root_key"])) / 12)
            n = min(len(x) - 2, int(0.6 * fsr))
            pos = np.arange(0, n, step)
            y = np.interp(pos, np.arange(len(x)), x)
            m = len(seg)
            if len(y) <= m:
                continue
            nfft = 1 << (len(y) + m).bit_length()
            c = np.fft.irfft(np.fft.rfft(y, nfft) * np.conj(np.fft.rfft(seg, nfft)), nfft)[: len(y) - m]
            cs = np.concatenate([[0.0], np.cumsum(y * y)])
            en = (cs[m:] - cs[:-m])[: len(c)]
            nc = c / np.sqrt(en * np.sum(seg * seg) + 1e-20)
            j = int(np.argmax(nc[: int(0.1 * sr)]))
            res.append((float(nc[j]), z.get("string", "-"), z.get("dynamic", ""), z.get("rr_index", "0"),
                        os.path.basename(z["file"]), j / sr * 1000))
        res.sort(reverse=True)
        print(f"note {i}: key {key} vel {vel} at {t0:.2f}s")
        for sc, st, dy, rr, fn, off in res[:5]:
            print(f"   {sc:6.3f}  str {st:>2} {dy:3} rr {rr}  {fn:44} seg at {off:6.1f} ms into the file")


if __name__ == "__main__":
    main()
