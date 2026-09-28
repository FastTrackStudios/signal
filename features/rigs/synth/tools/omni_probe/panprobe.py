#!/usr/bin/env python3
"""Pan-law probe: held-note L and R levels (dB) through Omnisphere (and ours)."""
import os, re, struct, subprocess, sys, tempfile
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
_args = sys.argv[1:]; sys.argv = sys.argv[:1]
_src = open(os.path.join(HERE, "fexp.py")).read()
exec(_src[:_src.index("def render(")])
base = open(os.path.join(HERE, "init_part.prt_omn")).read()
for spec in _args:
    name, _, edits = spec.partition("=")
    xml = base
    for e in filter(None, edits.split(";")):
        xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        with tempfile.NamedTemporaryFile("w", suffix=".prt_omn", delete=False) as f:
            f.write(xml); p = f.name
        wav = os.path.join(os.getcwd(), "pan.wav")
        subprocess.run([exe, p, wav, "--note", "48", "--hold", "1.2", "--tail", "0.1"], capture_output=True, stdin=subprocess.DEVNULL)
        os.unlink(p)
        d = open(wav, "rb").read(); i = 12
        while i < len(d):
            cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
            if cid == b"data": data = d[i+8:i+8+sz]
            i += 8 + sz + (sz & 1)
        x = np.frombuffer(data, "<f4").reshape(-1, 2)[int(0.5*48000):int(1.1*48000)]
        l, r = (20*np.log10(np.sqrt(np.mean(x[:, c]**2))+1e-9) for c in (0, 1))
        print(f"{name:10s} {tag} L {l:6.1f} R {r:6.1f}", flush=True)
