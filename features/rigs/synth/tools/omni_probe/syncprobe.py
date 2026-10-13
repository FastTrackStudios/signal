#!/usr/bin/env python3
"""Tempo-sync probe: LFO1 (synced, sine) onto layer A's amplitude in the init
part; prints the tremolo period (ms) per `rate` value at each tempo.
usage: syncprobe.py [rate ...]   env: BPMS=120,90  OURS=1"""
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
H = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
O = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
hexf = lambda v: struct.pack(">f", v).hex()
base = open(os.path.join(HERE, "init_part.prt_omn")).read()

def patch(rate):
    x = base
    x = re.sub(r'<LFO ([^>]*)>', lambda m: "<LFO " + re.sub(r'\bsync="[^"]*"', 'sync="3f800000"' if os.environ.get("NOSYNC") is None else 'sync="0"',
              re.sub(r'\brate="[^"]*"', f'rate="{hexf(rate)}"', re.sub(r'\bunidir="[^"]*"', 'unidir="0"', m.group(1)))) + ">", x, count=1)
    x = re.sub(r'\bsource0="off" target0="off"', 'source0="LFO1" target0="A atrm"', x, count=1)
    x = re.sub(r'\bhi0="[^"]*"', f'hi0="{hexf(0.5)}"', x, count=1)
    x = re.sub(r'(<OSC [^>]*?)\batrm="[^"]*"', lambda m: m.group(1) + f'atrm="{hexf(0.5)}"', x, count=1)
    return x

def period(wav):
    d = open(wav, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    x = np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)
    w = 96
    e = np.array([np.sqrt(np.mean(x[k:k+w]**2)) for k in range(int(0.3*48000), len(x) - w, w)])
    e = (e - e.mean()) * np.hanning(len(e))
    n = 1 << 16
    sp = np.abs(np.fft.rfft(e, n)); fr = np.fft.rfftfreq(n, w / 48000)
    m = (fr > float(os.environ.get("FMIN", "0.1"))) & (fr < 60)
    return 1000.0 / fr[m][np.argmax(sp[m])]

for bpm in [float(b) for b in os.environ.get("BPMS", "120,90").split(",")]:
    for r in [float(v) for v in (sys.argv[1:] or ["0", "0.1", "0.2", "0.3", "0.4", "0.5", "0.6", "0.8", "1"])]:
        open("/tmp/sp.prt_omn", "w").write(patch(r))
        out = []
        for tag, exe in (("omni", H), ("ours", O)):
            if tag == "ours" and not os.environ.get("OURS"): continue
            subprocess.run([exe, "/tmp/sp.prt_omn", "/tmp/sp.wav", "--note", "48", "--hold", os.environ.get("HOLD", "6"), "--tail", "0.1",
                            "--bpm", str(bpm)], capture_output=True, stdin=subprocess.DEVNULL)
            p = period("/tmp/sp.wav")
            out.append(f"{tag} {p:7.0f} ms = {p/ (60000/bpm):5.3f} beats")
        print(f"bpm {bpm:5.0f} rate {r:4.2f}  " + "   ".join(out), flush=True)
