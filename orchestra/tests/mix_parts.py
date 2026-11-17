#!/usr/bin/env python3
"""Mix rendered parts into one stereo file: equal-power pan, -1 dBFS peak, 24-bit.

    python3 mix_parts.py <out.wav> <part.wav>:<pan -1..1> [...]

Drops the first second (half of score_to_css.py's 2 s pre-roll).
"""
import os, sys, wave

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from null_test import SR, load  # noqa: E402

out = sys.argv[1]
parts = [(a.rsplit(":", 1)[0], float(a.rsplit(":", 1)[1])) for a in sys.argv[2:]]
stems = [(load(p), pan) for p, pan in parts]
n = max(len(x) for x, _ in stems)
mix = np.zeros((n, 2))
for x, pan in stems:
    th = (pan + 1) * np.pi / 4
    mix[: len(x), 0] += x[:, 0] * np.cos(th) * np.sqrt(2)
    mix[: len(x), 1] += x[:, 1] * np.sin(th) * np.sqrt(2)
mix *= 10 ** (-1 / 20) / (np.abs(mix).max() + 1e-12)
mix = mix[SR:]
pcm = (np.clip(mix, -1, 1) * (2**23 - 1)).astype(np.int32)
b = np.zeros((len(pcm), 2, 3), np.uint8)
for i in range(3):
    b[..., i] = (pcm >> (8 * i)) & 0xFF
with wave.open(out, "wb") as w:
    w.setnchannels(2)
    w.setsampwidth(3)
    w.setframerate(SR)
    w.writeframes(b.tobytes())
print(f"wrote {out} ({len(mix) / SR:.1f}s)")
