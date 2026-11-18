"""lsfit.py <render> <t0> <len> <skip> <file:rate>...: joint least squares of render[t0+skip, +len] by each file
(resampled by rate, started at t0); prints each file's gain (dB) and the residual."""
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
import numpy as np
from null_test import load, SR
r = load(sys.argv[1])[:, 0]; t0, ln, skip = map(float, sys.argv[2:5])
a, n = int((t0 + skip) * SR), int(ln * SR)
seg = r[a:a + n]
cols, names = [], []
for fr in sys.argv[5:]:
    f, rate = fr.rsplit(':', 1); x = load(f)[:, 0]; rate = float(rate)
    pos = (np.arange(n) + int(skip * SR)) * rate
    cols.append(np.interp(pos, np.arange(len(x)), x)); names.append(f.split('/')[-1])
M = np.stack(cols, 1)
g, *_ = np.linalg.lstsq(M, seg, rcond=None)
res = seg - M @ g
for nm, gi in zip(names, g): print(f"  {nm:20} {20*np.log10(abs(gi)+1e-9):7.1f} dB {'-' if gi<0 else ''}")
print(f"  residual {10*np.log10(np.sum(res**2)/np.sum(seg**2)):.1f} dB")
