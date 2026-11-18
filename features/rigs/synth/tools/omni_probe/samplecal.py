#!/usr/bin/env python3
"""Sample-layer level calibration: one layer of a patch, cleaned (other
layers off, filters off, Harmonia off, mod-matrix rows off, effects off),
rendered through Omnisphere and Signal across notes and velocities; prints
ours − Omnisphere (dB, RMS over the first 0.6 s after the attack).
usage: samplecal.py <patch> <layer 0..3>   env: NOTES=36,48,60,72,84 VELS=40,80,120
KEEP=filter,harm,matrix,fx keeps those parts."""
import os, re, struct, subprocess, sys, tempfile
import numpy as np
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
os.environ.setdefault("RENDER_PREROLL_MS", "3000")
H = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
O = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
src, layer = sys.argv[1], int(sys.argv[2])
x = open(src, encoding="latin1").read()
n = [0]
def only(m):
    k = n[0]; n[0] += 1
    return m.group(0) if k == layer else re.sub(r'onOff="[^"]*"', 'onOff="0"', m.group(0))
x = re.sub(r'<AENVPARAMS [^>]*>', only, x)
KEEP = set(filter(None, os.environ.get("KEEP", "").split(",")))
if "filter" not in KEEP:
    x = re.sub(r'<FILTER ([^>]*)>', lambda m: "<FILTER " + re.sub(r'\bact="[^"]*"', 'act="0"', m.group(1), count=1) + ">", x)
if "harm" not in KEEP:
    x = re.sub(r'\bhrmOn="[^"]*"', 'hrmOn="0"', x)
if "matrix" not in KEEP:
    x = re.sub(r'\bsource(\d+)="[^"]*"', r'source\1="off"', x)
if "fx" not in KEEP:
    x = re.sub(r'(<EFFMODULE\b[^>]*?\bActive=")[^"]*(")', r'\g<1>0\g<2>', x)
fd, P = tempfile.mkstemp(suffix=".prt_omn"); os.close(fd)
open(P, "w", encoding="latin1").write(x)

def level(exe, note, vel):
    w = P[:-8] + ".wav"
    subprocess.run([exe, P, w, "--note", str(note), "--vel", str(vel), "--hold", "0.8", "--tail", "0.1"],
                   capture_output=True, stdin=subprocess.DEVNULL, timeout=300)
    d = open(w, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    s = np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)[int(0.01 * 48000):int(0.61 * 48000)]
    return 20 * np.log10(np.sqrt(np.mean(s.astype(np.float64) ** 2)) + 1e-12)

notes = [int(v) for v in os.environ.get("NOTES", "36,48,60,72,84").split(",")]
vels = [int(v) for v in os.environ.get("VELS", "40,80,120").split(",")]
print(f"{os.path.basename(src)} layer {layer}   rows: velocity, cols: notes {notes}")
allv = []
for v in vels:
    row = []
    for nt in notes:
        a = np.mean([level(H, nt, v) for _ in range(2)])
        b = level(O, nt, v)
        row.append(b - a); allv.append(b - a)
        print(f"  vel {v:3d} note {nt:3d}: omni {a:6.1f}  ours-omni {b - a:+5.1f}", flush=True)
print(f"  mean {np.mean(allv):+.1f} dB  spread {np.std(allv):.1f}")
