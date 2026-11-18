"""findseg.py <render.wav> <t0> <len> <cents> <file>... : where in each file (any offset) a render segment comes from.
Resamples each candidate by `cents` (linear interp; fine for a correlation peak), normalized cross-correlation of the
segment (left channel) against the whole file; prints the best match per file, best first."""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import numpy as np
import null_test
from null_test import load, SR

r = load(sys.argv[1]); t0 = float(sys.argv[2]); ln = float(sys.argv[3]); cents = float(sys.argv[4])
seg = r[int(t0 * SR):int((t0 + ln) * SR), 0]
seg = seg - seg.mean()
out = []
for f in sys.argv[5:]:
    x = load(f)[:, 0]
    ratio = 2 ** (cents / 1200)
    pos = np.arange(0, len(x) - 2, ratio)
    y = np.interp(pos, np.arange(len(x)), x)
    n = len(y) + len(seg)
    nfft = 1 << (n - 1).bit_length()
    c = np.fft.irfft(np.fft.rfft(y, nfft) * np.conj(np.fft.rfft(seg, nfft)), nfft)[: len(y) - len(seg)]
    cs = np.concatenate([[0.0], np.cumsum(y * y)]); e = (cs[len(seg):] - cs[:-len(seg)])[: len(c)]
    nc = c / np.sqrt(e * np.sum(seg * seg) + 1e-20)
    i = int(np.argmax(nc))
    out.append((nc[i], f, i * ratio, i))
for v, f, src, i in sorted(out, reverse=True):
    print(f"{v:6.3f}  {f:40} file frame {src:9.0f} ({src / SR * 1000:7.1f} ms)  render-aligned start = t0 - {i / SR * 1000:.1f} ms")
