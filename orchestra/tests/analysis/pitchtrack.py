"""pitchtrack.py <render> <file> <t,...> <lo> <hi> <step>: best cents (and corr) per time, file scanned at any offset."""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import numpy as np
from null_test import load, SR
r = load(sys.argv[1])[:, 0]; x = load(sys.argv[2])[:, 0][: int(5 * SR)]
ts = [float(v) for v in sys.argv[3].split(',')]
grid = np.arange(float(sys.argv[4]), float(sys.argv[5]) + 1e-9, float(sys.argv[6]))
ys = {c: np.interp(np.arange(0, len(x) - 2, 2 ** (c / 1200)), np.arange(len(x)), x) for c in grid}
for t in ts:
    seg = r[int(t * SR): int((t + 0.05) * SR)]; seg = seg - seg.mean(); best = None
    for c, y in ys.items():
        n = len(y) + len(seg); nfft = 1 << (n - 1).bit_length()
        cc = np.fft.irfft(np.fft.rfft(y, nfft) * np.conj(np.fft.rfft(seg, nfft)), nfft)[: len(y) - len(seg)]
        cs = np.concatenate([[0.0], np.cumsum(y * y)]); e = (cs[len(seg):] - cs[:-len(seg)])[: len(cc)]
        nc = cc / np.sqrt(e * np.sum(seg * seg) + 1e-20); i = int(np.argmax(nc))
        if best is None or nc[i] > best[0]: best = (nc[i], c, t - i / SR)
    print(f"{t:7.3f}  {best[1]:6.1f} c  corr {best[0]:.2f}  start@{best[2]:.3f}")
