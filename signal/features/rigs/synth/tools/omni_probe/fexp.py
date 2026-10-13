#!/usr/bin/env python3
"""Filter response probe: render the init part (JP-8 saw, note 36) through
real Omnisphere (and optionally ours) with the filter set per spec, and
report |H| at each harmonic relative to the unfiltered render.

usage: fexp.py <name>=<edit;edit> ...   (edits as in exp.py; applied to FILTER #0)
env: OURS=1 also renders ours; JSON=path dumps curves.
"""
import json, os, re, struct, subprocess, sys, tempfile
import numpy as np
sys.path.insert(0, os.path.dirname(__file__))
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
HARNESS = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
OURS = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
HERE = os.path.dirname(os.path.abspath(__file__))
# Renders and dumps go to the working directory (the measurement dir).
OUTDIR = os.getcwd()
NOTE = int(os.environ.get("NOTE", "24")); SR = 48000
F0 = 440 * 2 ** ((NOTE - 69) / 12)

def hexf(v): return struct.pack(">f", v).hex()

def apply(xml, edit):
    m = re.match(r"(\w+)\.(\w+)=(f:|h:)?([^@]*)(?:@(\d+))?$", edit)
    tag, attr, kind, val, nth = m.groups()
    val = hexf(float(val)) if kind == "f:" else val
    pos = [mm.start() for mm in re.finditer(r"<" + tag + r"\b", xml)][int(nth or 0)]
    end = xml.index(">", pos); el = xml[pos:end]
    el2 = re.sub(r"\b" + attr + r'="[^"]*"', f'{attr}="{val}"', el, count=1) if re.search(r"\b" + attr + r'="', el) else el + f' {attr}="{val}"'
    return xml[:pos] + el2 + xml[end:]

def render(xml, exe, tag):
    with tempfile.NamedTemporaryFile("w", suffix=".prt_omn", delete=False) as f:
        f.write(xml); p = f.name
    wav = os.path.join(OUTDIR, f"f_{tag}.wav")
    r = subprocess.run([exe, p, wav, "--note", str(NOTE), "--vel", os.environ.get("VEL", "100"), "--hold", "1.2", "--tail", "0.1", "--sr", str(SR)], capture_output=True, text=True, timeout=180, stdin=subprocess.DEVNULL)
    os.unlink(p)
    if r.returncode: raise SystemExit(r.stderr[-1500:])
    d = open(wav, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    x = np.frombuffer(data, "<f4").reshape(-1, 2).mean(1)
    return x

def harm(x):
    """Harmonic magnitudes by FFT peak-picking (robust to pitch drift)."""
    seg = x[int(0.6*SR):int(1.1*SR)]
    n = len(seg); w = np.hanning(n); N = 1 << 20
    spec = np.abs(np.fft.rfft(seg * w, N)) / w.sum() * 2
    df = SR / N
    hs = np.arange(1, int(20000 / F0))
    out = []
    for h in hs:
        c = h * F0; half = min(0.02 * c, 0.4 * F0)
        lo, hi = int((c - half) / df), int((c + half) / df) + 1
        out.append(spec[lo:hi].max())
    return hs * F0, np.array(out)

def summary(f, h, lvl_abs=None):
    db = 20*np.log10(np.maximum(h, 1e-12))
    if lvl_abs is not None:  # drop harmonics under the noise floor
        ok = lvl_abs > -100
        f, db = f[ok], db[ok]
    pb = float(np.median(db[:4]))
    pk = int(np.argmax(db))
    def cross(lvl):
        idx = np.where(db < pb + lvl)[0]
        if not len(idx): return None
        i = idx[0]
        if i == 0: return float(f[0])
        # log-frequency interpolation between the bracketing harmonics
        a, b = db[i-1]-pb-lvl, db[i]-pb-lvl
        t = a / (a - b)
        return float(np.exp(np.log(f[i-1]) + t*(np.log(f[i]) - np.log(f[i-1]))))
    a, b = cross(-12), cross(-30)
    slope = -18 / np.log2(b / a) if a and b and b > a else 0.0
    r = lambda v: None if v is None else int(round(v))
    return dict(pass_db=round(pb,1), peak_db=round(float(db[pk]-pb),1), peak_hz=int(f[pk]), f3=r(cross(-3)), f12=r(cross(-12)), slope=round(float(slope),1))

base = open(os.path.join(HERE, "init_part.prt_omn")).read()
refs = {}
def ref(exe, tag):
    if tag not in refs:
        refs[tag] = harm(render(base, exe, "ref_" + tag))[1]
    return refs[tag]

dump = {}
for spec in sys.argv[1:]:
    name, _, edits = spec.partition("=")
    xml = apply(base, "FILTER.act=f:1")
    if not os.environ.get("KEEPENV"):
        xml = apply(xml, "FILTER.envdpth=f:0")
    for e in filter(None, edits.split(";")):
        xml = apply(xml, e if "." in e.split("=")[0] else "FILTER." + e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag == "omni" and os.environ.get("ONLYOURS"): continue
        f, h = harm(render(xml, exe, tag)); r = ref(exe, tag)
        if os.environ.get("QUIET"):
            dump[f"{name}/{tag}"] = [float(v) for v in 20*np.log10(np.maximum(h / np.maximum(r, 1e-12),1e-9))]
            continue
        H = h / np.maximum(r, 1e-12)
        s = summary(f, H, 20*np.log10(np.maximum(h,1e-12)/max(h[0],1e-12))); print(f"{name:14s} {tag} {s}", flush=True)
        dump[f"{name}/{tag}"] = [float(v) for v in 20*np.log10(np.maximum(H,1e-9))]
if os.environ.get("JSON"):
    json.dump(dict(f0=F0, curves=dump), open(os.environ["JSON"], "w"))
