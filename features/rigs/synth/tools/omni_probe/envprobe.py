#!/usr/bin/env python3
"""Amp-envelope probe: write breakpoints into Layer A's AENV (or FENV) of
the init part, render, and print the level (dB) every 50 ms.

usage: envprobe.py name=l:t:s:c,...[|edits] ...   env: TAG (AENV), OURS, HOLD, TAIL
"""
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
_args = sys.argv[1:]; sys.argv = sys.argv[:1]
_src = open(os.path.join(HERE, "fexp.py")).read()
exec(_src[:_src.index("def render(")])
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
TAG = os.environ.get("TAG", "AENV")
HOLD, TAIL = os.environ.get("HOLD", "1.5"), os.environ.get("TAIL", "2.0")
base = open(os.path.join(HERE, "init_part.prt_omn")).read()

def set_env(xml, spec):
    a = xml.index("<" + TAG + " ") if ("<" + TAG + " ") in xml else xml.index("<" + TAG + ">")
    b = xml.index("</" + TAG + ">", a)
    head = xml[a:xml.index(">", a) + 1]
    pts = [p.split(":") for p in spec.split(",")]
    head = re.sub(r'\bc="[^"]*"', f'c="{len(pts)-1:x}"', head)
    body = "".join(f'<p l="{hexf(float(l))}"  t="{hexf(float(t))}"  s="{int(s):x}"  c="{hexf(float(c))}" ></p> ' for l, t, s, c in pts)
    return xml[:a] + head + body + xml[b:]

def run(xml, exe):
    open("/tmp/ep.prt_omn", "w").write(xml)
    subprocess.run([exe, "/tmp/ep.prt_omn", "/tmp/ep.wav", "--note", "60", "--hold", HOLD, "--tail", TAIL],
                   capture_output=True, stdin=subprocess.DEVNULL)
    d = open("/tmp/ep.wav", "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    x = np.frombuffer(data, "<f4").reshape(-1, 2).mean(1); W = 2400
    return [20*np.log10(np.sqrt(np.mean(x[k:k+W]**2)) + 1e-9) for k in range(0, len(x) - W, W)]

for spec in _args:
    name, _, rest = spec.partition("=")
    pts, _, edits = rest.partition("|")
    xml = set_env(base, pts)
    for e in filter(None, edits.split(";")):
        xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        lv = run(xml, exe); top = max(lv)
        print(f"{name:10s} {tag} " + " ".join(f"{v - top:4.0f}" for v in lv), flush=True)
