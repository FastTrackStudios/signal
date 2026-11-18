"""slide.py <render.wav> <t0> <t1> <step> <len> <cents> <file>...: per window, best (corr, implied file start time in the render) per file."""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import numpy as np
from null_test import load, SR
r = load(sys.argv[1])[:, 0]
t0, t1, step, ln, cents = map(float, sys.argv[2:7])
files = sys.argv[7:]
ys = []
for f in files:
    x = load(f)[:, 0]
    ratio = 2 ** (cents / 1200)
    pos = np.arange(0, len(x) - 2, ratio)
    ys.append(np.interp(pos, np.arange(len(x)), x)[: int(4 * SR)])
for t in np.arange(t0, t1, step):
    seg = r[int(t * SR): int((t + ln) * SR)]
    seg = seg - seg.mean()
    out = []
    for f, y in zip(files, ys):
        n = len(y) + len(seg); nfft = 1 << (n - 1).bit_length()
        c = np.fft.irfft(np.fft.rfft(y, nfft) * np.conj(np.fft.rfft(seg, nfft)), nfft)[: len(y) - len(seg)]
        cs = np.concatenate([[0.0], np.cumsum(y * y)]); e = (cs[len(seg):] - cs[:-len(seg)])[: len(c)]
        nc = c / np.sqrt(e * np.sum(seg * seg) + 1e-20)
        i = int(np.argmax(nc))
        out.append(f"{f.split('/')[-1]}:{nc[i]:.2f}@{t - i / SR:.3f}")
    print(f"{t:7.3f}", "  ".join(out))
