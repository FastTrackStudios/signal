#!/usr/bin/env python3
"""Pitch-modulation probe: render the init part (note 48) with edits through
Omnisphere (and optionally Signal) and report the vibrato the held note
carries — rate (Hz), depth (± cents), and the waveform's shape.

usage: pexp.py name=edit;edit ...   (edits as in fexp.py, tag required)
env: OURS=1, ONLYOURS=1, CC="1=127" (sent before the note), NOTE, HOLD
"""
import os, re, struct, subprocess, sys, tempfile
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
sys.argv, _args = [sys.argv[0]], sys.argv[1:]
_src = open(os.path.join(HERE, "fexp.py")).read()
exec(_src[:_src.index("def render(")])
SR = 48000
NOTE = int(os.environ.get("NOTE", "48")); F0 = 440 * 2 ** ((NOTE - 69) / 12)
HOLD = float(os.environ.get("HOLD", "4.0"))

def render(xml, exe, tag):
    with tempfile.NamedTemporaryFile("w", suffix=".prt_omn", delete=False) as f:
        f.write(xml); p = f.name
    wav = os.path.join(os.getcwd(), f"p_{tag}.wav")
    cmd = [exe, p, wav, "--note", str(NOTE), "--vel", os.environ.get("VEL", "100"),
           "--hold", str(HOLD), "--tail", os.environ.get("TAIL", "0.1"), "--sr", str(SR)]
    for cc in filter(None, os.environ.get("CC", "").split(",")):
        cmd += ["--cc", cc]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=300, stdin=subprocess.DEVNULL)
    os.unlink(p)
    if r.returncode: raise SystemExit(r.stderr[-1500:])
    d = open(wav, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)

def track(x, h=int(os.environ.get("H", "4"))):
    """Instantaneous pitch (cents vs nominal) from harmonic h, hop 5 ms."""
    n, hop, N = 4096, 240, 1 << 16
    w = np.hanning(n); c = F0 * h; out = []
    for s in range(int(0.3*SR), len(x) - n - int(0.1*SR), hop):
        sp = np.abs(np.fft.rfft(x[s:s+n]*w, N)); df = SR / N
        wide = float(os.environ.get("WIDE", "0.06"))
        lo, hi = int(c*(1-wide)/df), int(c*(1+wide)/df)
        k = lo + int(np.argmax(sp[lo:hi]))
        a, b, g = np.log(sp[k-1]+1e-12), np.log(sp[k]+1e-12), np.log(sp[k+1]+1e-12)
        k2 = k + 0.5*(a-g)/(a-2*b+g)
        out.append(1200*np.log2(k2*df/h/F0))
    return np.array(out), hop / SR

def describe(p, dt):
    p = p - np.median(p)
    depth = (np.percentile(p, 98) - np.percentile(p, 2)) / 2
    q = p - p.mean(); ac = np.correlate(q, q, "full")[len(q)-1:]
    ac /= ac[0] + 1e-12
    # first peak after the first zero crossing
    z = np.argmax(ac < 0) if (ac < 0).any() else 0
    rate = 0.0
    if z:
        k = z + int(np.argmax(ac[z:z+int(2/dt)]))
        rate = 1 / (k*dt) if ac[k] > 0.3 else 0.0
    out = dict(depth_cents=round(float(depth), 1), rate_hz=round(rate, 3))
    if os.environ.get("SHAPE") and rate > 0:
        # One cycle, 16 points, normalized to ±1 (starting at the curve's minimum).
        per = int(round(1 / rate / dt)); i0 = int(np.argmin(p[:per]))
        cyc = p[i0:i0+per]
        if len(cyc) == per:
            cyc = (cyc - cyc.min()) / max(cyc.max() - cyc.min(), 1e-9) * 2 - 1
            out["shape"] = " ".join(f"{v:+.1f}" for v in cyc[:: max(1, per // 16)][:16])
    return out

base = open(os.path.join(HERE, "init_part.prt_omn")).read()

def set_modenv(xml, n, spec, zoom=None):
    """Replace the Nth part MODENV's points with `l:t:s:c,…` (floats; s an
    int code, written as hex like the plugin)."""
    starts = [m.start() for m in re.finditer(r"<MODENV\b", xml)]
    a = starts[n]; b = xml.index("</MODENV>", a)
    head = xml[a:xml.index(">", a) + 1]
    pts = [tuple(v for v in p.split(":")) for p in spec.split(",")]
    head = re.sub(r'\bc="[^"]*"', f'c="{len(pts):x}"', head)
    if zoom is not None:
        head = re.sub(r'\bzoom="[^"]*"', f'zoom="{hexf(float(zoom))}"', head)
    body = "".join(f'<p l="{hexf(float(l))}"  t="{hexf(float(t))}"  s="{int(sc):x}"  c="{hexf(float(c))}" ></p> '
                   for l, t, sc, c in pts)
    return xml[:a] + head + body + xml[b:]
for spec in _args:
    name, _, edits = spec.partition("=")
    xml = base
    for e in filter(None, edits.split(";")):
        if re.match(r"MODENV\d*=", e):
            # MODENV<n>=l:t:s:c,…[|zoom]
            k, _, v = e.partition("=")
            v, _, z = v.partition("|")
            xml = set_modenv(xml, int(k[6:] or 0), v, z or None)
        else:
            xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag == "omni" and os.environ.get("ONLYOURS"): continue
        p, dt = track(render(xml, exe, tag))
        if os.environ.get("TRAJ"):
            # Pitch (cents) every 0.1 s from 0.3 s.
            step = int(0.1 / dt)
            print(f"{name:14s} {tag} " + " ".join(f"{v:5.0f}" for v in p[::step]), flush=True)
        else:
            print(f"{name:14s} {tag} {describe(p, dt)}", flush=True)
