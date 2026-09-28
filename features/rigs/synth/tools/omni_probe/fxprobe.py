#!/usr/bin/env python3
"""Effect probe: put an Omnisphere effect in the part's first Common FX slot
of the init part, feed it a 30 ms click (or a held note, SRC=hold) and
report what comes out: echo peaks (time ms, level dB re the click), the time
the tail takes to fall 30/60 dB, and wet/dry energy.

usage: fxprobe.py name="Type|P0=0.5,P3=0.2,..." ...    env: OURS=1, SRC=click|hold, ENV=1
"""
import os, re, struct, subprocess, sys
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
_args = sys.argv[1:]; sys.argv = sys.argv[:1]
_src = open(os.path.join(HERE, "fexp.py")).read()
exec(_src[:_src.index("def render(")])
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
SR = 48000
base = open(os.path.join(HERE, "init_part.prt_omn")).read()

def set_click(xml):
    a = xml.index("<AENV "); b = xml.index("</AENV>", a)
    head = re.sub(r'\bc="[^"]*"', 'c="3"', xml[a:xml.index(">", a) + 1])
    pts = [(0, 0), (1, 0.00001), (0, 0.0003), (0, 0.0004)]
    body = "".join(f'<p l="{hexf(l)}"  t="{hexf(t)}"  s="14"  c="{hexf(0.5)}" ></p> ' for l, t in pts)
    return xml[:a] + head + body + xml[b:]

def set_fx(xml, typ, params, slot=16):
    ms = [m.start() for m in re.finditer(r"<EFFMODULE\b", xml)]
    a = ms[slot]; b = xml.index(">", a)
    el = f'<EFFMODULE Type="{typ}" ' + " ".join(f'P{i}="{hexf(params.get(i, 0.0))}" ' for i in range(15)) + ' Active="3f800000"  MixLock="0" '
    return xml[:a] + el + xml[b:]

def render(xml, exe):
    open("/tmp/fx.prt_omn", "w").write(xml)
    hold = "0.05" if os.environ.get("SRC", "click") == "click" else "2.0"
    r = subprocess.run([exe, "/tmp/fx.prt_omn", "/tmp/fx.wav", "--note", os.environ.get("NOTE", "60"), "--hold", hold,
                        "--tail", os.environ.get("TAIL", "4.0")], capture_output=True, text=True, stdin=subprocess.DEVNULL)
    d = open("/tmp/fx.wav", "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2)

REF = {}

def analyse(x, ref_key):
    m = x.mean(1)
    W = 240  # 5 ms energy
    e = np.array([np.mean(m[k:k+W]**2) for k in range(0, len(m) - W, W)]) + 1e-20
    db = 10*np.log10(e)
    # Levels are re the DRY click's loudest 5 ms (a render with no effect).
    if ref_key not in REF:
        REF[ref_key] = None
    ref = REF[ref_key] if REF[ref_key] is not None else db[:8].max()
    db -= ref
    # peaks after 40 ms that stand 6 dB above their neighbours
    pk = [(k*5, round(db[k], 1)) for k in range(9, len(db)-2)
          if db[k] > -60 and db[k] >= db[k-1] and db[k] >= db[k+1] and db[k] > min(db[k-2], db[k+2]) + 6][:8]
    tail = np.where(db[8:] > -30)[0]; t30 = (tail[-1] + 8) * 5 if len(tail) else 0
    tail = np.where(db[8:] > -60)[0]; t60 = (tail[-1] + 8) * 5 if len(tail) else 0
    wet = 10*np.log10(e.sum() / 10**(ref/10) / 9)
    side = 10*np.log10(np.mean((x[:, 0]-x[:, 1])**2)/max(np.mean((x[:, 0]+x[:, 1])**2), 1e-20) + 1e-12)
    out = dict(dry_db=round(float(db[:8].max()), 1), peaks=pk, t30_ms=t30, t60_ms=t60, wet_vs_click_db=round(float(wet), 1), side_db=round(float(side), 1))
    if os.environ.get("ENV"):
        out["env"] = " ".join(f"{v:.0f}" for v in db[:400:10])
    return out

def chorus(x):
    """Held note, wet only: the pitch wobble (± cents, rate Hz) of the
    fundamental at note 60, per channel."""
    out = []
    for c in (0, 1):
        m = x[:, c]; n, hop, N = 4096, 240, 1 << 16; F = 261.63; w = np.hanning(n); p = []
        for st in range(int(0.3*SR), int(1.9*SR) - n, hop):
            sp = np.abs(np.fft.rfft(m[st:st+n]*w, N)); df = SR/N
            lo, hi = int(F*0.9/df), int(F*1.1/df); k = lo + int(np.argmax(sp[lo:hi]))
            a, b, g = np.log(sp[k-1]+1e-12), np.log(sp[k]+1e-12), np.log(sp[k+1]+1e-12)
            p.append(1200*np.log2((k + 0.5*(a-g)/(a-2*b+g))*df/F))
        p = np.array(p) - np.median(p); q = p - p.mean()
        ac = np.correlate(q, q, "full")[len(q)-1:]; ac /= ac[0] + 1e-12
        z = np.argmax(ac < 0) if (ac < 0).any() else 0
        rate = 0.0
        if z:
            k = z + int(np.argmax(ac[z:])); rate = SR/hop/k if ac[k] > 0.2 else 0.0
        out.append((round(float((np.percentile(p, 98) - np.percentile(p, 2))/2), 1), round(rate, 2)))
    return out

def dry_ref(exe):
    xml = base if os.environ.get("SRC", "click") != "click" else set_click(base)
    m = render(xml, exe).mean(1); W = 240
    e = np.array([np.mean(m[k:k+W]**2) for k in range(0, len(m) - W, W)]) + 1e-20
    return 10*np.log10(e[:8].max())

for spec in _args:
    name, _, rest = spec.partition("=")
    typ, _, ps = rest.partition("|")
    params = {int(k[1:]): float(v) for k, v in (p.split("=") for p in filter(None, ps.split(",")))}
    xml = base if os.environ.get("SRC", "click") != "click" else set_click(base)
    if typ:
        xml = set_fx(xml, typ, params, int(os.environ.get("SLOT", "16")))
    for e in filter(None, os.environ.get("EDITS", "").split(";")):
        xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        if tag not in REF or REF[tag] is None:
            REF[tag] = dry_ref(exe)
        x = render(xml, exe)
        if os.environ.get("SRC") == "hold":
            print(f"{name:12s} {tag} chorus L/R (±cents, Hz) {chorus(x)}", flush=True)
            continue
        r = analyse(x, tag)
        r["peaks"] = [(int(t), float(v)) for t, v in r["peaks"]]
        r = {k: (int(v) if isinstance(v, np.integer) else v) for k, v in r.items()}
        print(f"{name:12s} {tag} {r}", flush=True)
