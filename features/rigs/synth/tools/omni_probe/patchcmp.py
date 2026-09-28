#!/usr/bin/env python3
"""Whole-patch comparison: render real patches through Omnisphere and
Signal, compare loudness (dB, over time) and the held note's 1/3-octave
spectrum. usage: patchcmp.py <patch.prt_omn>... (env NOTE, HOLD)"""
import os, re, struct, subprocess, sys, tempfile
import numpy as np
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
HARNESS = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
OURS = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
SR = 48000; NOTE = os.environ.get("NOTE", "60"); HOLD = float(os.environ.get("HOLD", "2.0"))

def render(exe, patch):
    wav = os.path.join(tempfile.gettempdir(), f"pc_{os.getpid()}.wav")
    r = subprocess.run([exe, patch, wav, "--note", NOTE, "--hold", str(HOLD), "--tail", "1.0"],
                       capture_output=True, text=True, timeout=300, stdin=subprocess.DEVNULL)
    if r.returncode: return None
    d = open(wav, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)

def loud(x):
    w = int(0.25*SR)
    return np.array([20*np.log10(np.sqrt(np.mean(x[i:i+w]**2))+1e-9) for i in range(0, len(x)-w, w)])

def bands(x):
    seg = x[int(0.3*SR):int(min(HOLD, 1.8)*SR)]
    sp = np.abs(np.fft.rfft(seg*np.hanning(len(seg))))**2; f = np.fft.rfftfreq(len(seg), 1/SR)
    edges = 50 * 2 ** (np.arange(0, 28) / 3)
    e = np.array([sp[(f >= a) & (f < b)].sum() for a, b in zip(edges[:-1], edges[1:])])
    db = 10*np.log10(e + 1e-12)
    return db - db.max()

def features(patch):
    x = open(patch, encoding="utf-8", errors="replace").read()
    fx = [m for m in re.findall(r'<EFFMODULE[^>]*Type="([^"]+)"[^>]*Active="(?:3f[0-9a-f]+|1)"', x) if m != "No Effect"]
    routes = len(re.findall(r'source\d+="(?!off)[^"]+"', x))
    return f"fx {len(fx)} ({', '.join(sorted(set(fx))[:4])}) routes {routes}"

for p in sys.argv[1:]:
    if os.environ.get("NOFX"):
        # Every effect bypassed in both (a copy of the patch).
        x = open(p, encoding="utf-8", errors="replace").read()
        x = re.sub(r'(<EFFMODULE\b[^>]*?\bActive=")[^"]*(")', r"\g<1>0\g<2>", x)
        q = os.path.join(tempfile.gettempdir(), f"pc_nofx_{os.getpid()}.prt_omn")
        open(q, "w").write(x)
    else:
        q = p
    a, b = render(HARNESS, q), render(OURS, q)
    name = os.path.basename(p)[:40]
    if a is None or b is None:
        print(f"{name:40s} render failed"); continue
    la, lb = loud(a), loud(b); n = min(len(la), len(lb))
    ba, bb = bands(a), bands(b)
    m = ba > -40
    print(f"{name:40s} level {np.mean(lb[:n]-la[:n]):+5.1f} dB (env rms {np.sqrt(np.mean(((lb[:n]-lb[:n].mean())-(la[:n]-la[:n].mean()))**2)):4.1f})  "
          f"spectrum rms {np.sqrt(np.mean((ba[m]-bb[m])**2)):4.1f} dB  | {features(p)}", flush=True)
