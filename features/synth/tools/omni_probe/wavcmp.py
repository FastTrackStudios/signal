#!/usr/bin/env python3
"""Compare two renders of the same note: a reference (any plug-in, e.g.
Kontakt via daw's `plugin_render`) and ours (`render_patch`). Prints level
(ours − ref, over time), 1/3-octave spectrum of the held note and of the
attack, and the pitch of each. usage: wavcmp.py <ref.wav> <ours.wav>
(env HOLD — the note's hold, default 2.0; BANDS=1 for per-band detail)."""
import os, struct, sys
import numpy as np
SR = 48000; HOLD = float(os.environ.get("HOLD", "2.0"))
EDGES = 50 * 2 ** (np.arange(0, 28) / 3)

def read(path):
    d = open(path, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)

def loud(x, w=0.25):
    w = int(w*SR)
    return np.array([20*np.log10(np.sqrt(np.mean(x[i:i+w]**2))+1e-9) for i in range(0, len(x)-w, w)])

def bands(seg):
    sp = np.abs(np.fft.rfft(seg*np.hanning(len(seg))))**2; f = np.fft.rfftfreq(len(seg), 1/SR)
    e = np.array([sp[(f >= a) & (f < b)].sum() for a, b in zip(EDGES[:-1], EDGES[1:])])
    db = 10*np.log10(e + 1e-12)
    return db - db.max()

def pitch(x):
    # Strongest spectral peak of the held note, refined parabolically.
    seg = x[int(0.3*SR):int(min(HOLD, 1.8)*SR)]
    sp = np.abs(np.fft.rfft(seg*np.hanning(len(seg)), 8*len(seg)))
    f = np.fft.rfftfreq(8*len(seg), 1/SR); lo = np.searchsorted(f, 25)
    k = lo + int(np.argmax(sp[lo:np.searchsorted(f, 5000)]))
    a, b, c = np.log(sp[k-1:k+2] + 1e-12); p = 0.5*(a-c)/(a-2*b+c)
    return (k + p) * (f[1]-f[0])

def onset(x):
    return int(np.argmax(np.abs(x) > 0.05*np.abs(x).max()))

ref, ours = read(sys.argv[1]), read(sys.argv[2])
# Align the onsets (a plug-in may add latency).
ref, ours = ref[onset(ref):], ours[onset(ours):]
n = min(len(ref), len(ours)); ref, ours = ref[:n], ours[:n]
la, lb = loud(ref), loud(ours)
held = slice(int(0.3*SR), int(min(HOLD, 1.8)*SR)); atk = slice(0, int(0.1*SR))
ba, bb = bands(ref[held]), bands(ours[held]); aa, ab = bands(ref[atk]), bands(ours[atk])
m, ma = ba > -40, aa > -40
fa, fb = pitch(ref), pitch(ours)
cents = 1200*np.log2(fb/fa) if fa > 0 and fb > 0 else float("nan")
print(f"level {np.mean(lb-la):+5.1f} dB (env rms {np.sqrt(np.mean(((lb-lb.mean())-(la-la.mean()))**2)):4.1f})  "
      f"held spectrum rms {np.sqrt(np.mean((ba[m]-bb[m])**2)):4.1f} dB  attack {np.sqrt(np.mean((aa[ma]-ab[ma])**2)):4.1f} dB  "
      f"peak {fa:7.1f} vs {fb:7.1f} Hz ({cents:+.0f}c)")
if os.environ.get("BANDS"):
    print("  band Hz  " + " ".join(f"{int(e):>5}" for e in EDGES[:-1][m]))
    print("  ref      " + " ".join(f"{v:5.0f}" for v in ba[m]))
    print("  ours-ref " + " ".join(f"{v:5.0f}" for v in (bb - ba)[m]))
    print("  atk band " + " ".join(f"{int(e):>5}" for e in EDGES[:-1][ma]))
    print("  ours-ref " + " ".join(f"{v:5.0f}" for v in (ab - aa)[ma]))
    print("  t(.25s)  " + " ".join(f"{v:5.0f}" for v in la))
    print("  ours     " + " ".join(f"{v:5.0f}" for v in lb))
