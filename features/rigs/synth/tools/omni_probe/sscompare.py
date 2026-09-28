#!/usr/bin/env python3
"""A/B a soundsource against Omnisphere: a probe patch (one sample layer,
filters / Harmonia / matrix / effects off) naming the soundsource, rendered
through the plugin and Signal at a few notes; prints per note the pitch of
each (cents re the note, from the strongest partial), ours − Omnisphere
level, and the 1/3-octave spectrum rms difference.

usage: sscompare.py <name> [<name> ...]   (or --list <pack-dir> [filter])
env: NOTES=36,48,60,72,84 VEL=100 LIBRARY="Keyscape Library"
"""
import os, re, struct, subprocess, sys, tempfile, glob
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
os.environ.setdefault("FTS_PACK_LIBRARY", "/Volumes/dev-drive/AudioHaven/Signal/Libraries")
os.environ.setdefault("FTS_OMNISPHERE_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled/Keys/Omnisphere")
os.environ.setdefault("RENDER_PREROLL_MS", "3000")
H = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
O = "/Volumes/dev-drive/signal/target/release/examples/render_patch"
SR = 48000
PROBE = os.path.join(HERE, "..", "..", "..", "..", "..", "..", "AudioHaven")  # unused
BASE_PATCH = os.environ.get("PROBE_PATCH",
    "/Volumes/dev-drive/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches/User/Worship Gig 3/Hammered Dolceola.prt_omn")
LIB = os.environ.get("LIBRARY", "Keyscape Library")
E = 50 * 2 ** (np.arange(0, 28) / 3)

def probe(name):
    x = open(BASE_PATCH, encoding="latin1").read()
    n = [0]
    def only(m):
        k = n[0]; n[0] += 1
        return m.group(0) if k == 0 else re.sub(r'onOff="[^"]*"', 'onOff="0"', m.group(0))
    x = re.sub(r'<AENVPARAMS [^>]*>', only, x)
    x = re.sub(r'<FILTER ([^>]*)>', lambda m: "<FILTER " + re.sub(r'\bact="[^"]*"', 'act="0"', m.group(1), count=1) + ">", x)
    x = re.sub(r'\bhrmOn="[^"]*"', 'hrmOn="0"', x)
    x = re.sub(r'\bsource(\d+)="[^"]*"', r'source\1="off"', x)
    x = re.sub(r'(<EFFMODULE\b[^>]*?\bActive=")[^"]*(")', r'\g<1>0\g<2>', x)
    k = [0]
    def ss(m):
        i = k[0]; k[0] += 1
        return f'<MS_IM_0 name="{name}" library="{LIB}">' if i == 0 else m.group(0)
    return re.sub(r'<MS_IM_0 [^>]*>', ss, x)

def render(exe, path, note, vel):
    w = path[:-8] + (".o.wav" if exe == H else ".s.wav")
    subprocess.run([exe, path, w, "--note", str(note), "--vel", str(vel), "--hold", "0.9", "--tail", "0.1"],
                   capture_output=True, stdin=subprocess.DEVNULL, timeout=300)
    d = open(w, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1).astype(np.float64)[int(0.05 * SR):int(0.85 * SR)]

def pitch(x, note):
    f0 = 440 * 2 ** ((note - 69) / 12)
    n = 1 << 18
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)), n)); fr = np.fft.rfftfreq(n, 1 / SR)
    best = None
    for p in range(1, 5):
        lo, hi = f0 * p * 2 ** (-0.5 / 12), f0 * p * 2 ** (0.5 / 12)   # ±50 cents? widen to ±100
        lo, hi = f0 * p * 2 ** (-1 / 12), f0 * p * 2 ** (1 / 12)
        m = (fr >= lo) & (fr <= hi)
        if not m.any(): continue
        i = np.argmax(sp[m]); v = sp[m][i]
        if best is None or v > best[0]:
            best = (v, fr[m][i], p)
    v, hz, p = best
    return 1200 * np.log2(hz / (f0 * p))

def bands(x):
    sp = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2; fr = np.fft.rfftfreq(len(x), 1 / SR)
    b = 10 * np.log10(np.array([sp[(fr >= a) & (fr < c)].sum() for a, c in zip(E[:-1], E[1:])]) + 1e-20)
    return b - b.max()

def compare(name, notes, vel):
    fd, p = tempfile.mkstemp(suffix=".prt_omn"); os.close(fd)
    open(p, "w", encoding="latin1").write(probe(name))
    rows = []
    for nt in notes:
        a, b = render(H, p, nt, vel), render(O, p, nt, vel)
        la = 20 * np.log10(np.sqrt(np.mean(a ** 2)) + 1e-12); lb = 20 * np.log10(np.sqrt(np.mean(b ** 2)) + 1e-12)
        if la < -90:
            rows.append((nt, None)); continue
        if lb < -90:
            rows.append((nt, ("SILENT", la))); continue
        ba, bb = bands(a), bands(b); m = ba > -40
        rows.append((nt, (pitch(a, nt), pitch(b, nt), lb - la, float(np.sqrt(np.mean((ba[m] - bb[m]) ** 2))))))
    os.remove(p)
    return rows

def main():
    args = sys.argv[1:]
    if args and args[0] == "--list":
        flt = args[2].lower() if len(args) > 2 else ""
        names = []
        for f in sorted(glob.glob(os.path.join(args[1], "*.soundsources.txt"))):
            names += [l.strip() for l in open(f) if l.strip() and flt in l.lower()]
    else:
        names = args
    notes = [int(v) for v in os.environ.get("NOTES", "36,48,60,72,84").split(",")]
    vel = int(os.environ.get("VEL", "100"))
    for name in names:
        rows = compare(name, notes, vel)
        cells = []
        worst = 0
        pitched = not any(w in name.lower() for w in ("noise", "mechanical", "release", "pedal"))
        for nt, r in rows:
            if r is None: cells.append(f"{nt}: omni silent"); continue
            if r[0] == "SILENT": cells.append(f"{nt}: OURS SILENT"); worst = 99; continue
            po, ps, dl, spec = r
            cells.append(f"{nt}: pitch {po:+.0f}/{ps:+.0f}c lvl {dl:+.1f} spec {spec:.1f}")
            # FAIL: a wrong pitch (pitched sources) or a different sound;
            # level is reported for calibration, a WARN beyond ±4 dB.
            bad = (pitched and abs(ps - po) > 20) or spec > 12
            warn = (pitched and abs(ps - po) > 10) or spec > 7 or abs(dl) > 4
            worst = max(worst, 2 if bad else (1 if warn else 0))
        tag = "ok  " if worst < 1 else ("WARN" if worst < 2 else "FAIL")
        print(f"{tag} {name}: " + " | ".join(cells), flush=True)

if __name__ == "__main__":
    main()
