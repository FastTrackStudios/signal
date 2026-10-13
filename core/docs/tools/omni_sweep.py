#!/usr/bin/env python3
"""Omnisphere calibration sweep driver (probe tooling — findings land in Rust).

Builds injected state files from a base patch + per-case attribute rewrites /
AENV replacements, ships them to voyager, renders through the real Omnisphere
plugin, pulls the WAVs, and measures. Modes: curve | curve2 | filter | types |
lfo | lfo2 | unison | unison2 (see __main__). Measured constants + traps are
recorded in docs/omnisphere-calibration-log.md — READ IT before probing
(unanchored-regex traps in here cost real hours).

Needs: the voyager build (rustc shim), a template state dump (omni_state.bin
next to this script or adjust TEMPLATE), and the omni_state example built.
"""
import re, struct, subprocess, math, os, sys, glob

# Work dir for generated cases + pulled WAVs (override via OMNI_SWEEP_DIR).
SCRATCH = os.environ.get("OMNI_SWEEP_DIR", os.path.dirname(os.path.abspath(__file__)))
SIGNAL = "/run/media/Development/FastTrackStudio/signal"
PATCH = "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere-Voyager/Settings Library/Patches/User/My Category/1975 Attempt.prt_omn"
TEMPLATE = f"{SCRATCH}/omni_state.bin"
OMNI = "/Library/Audio/Plug-Ins/VST3/Omnisphere.vst3"

def hexf(v):
    return format(struct.unpack('>I', struct.pack('>f', float(v)))[0], '08x') if v else "0"

def rewrite(xml, attr_regex, value):
    return re.sub(rf'({attr_regex})="[^"]*"', rf'\1="{value}"', xml)

def insert_attrs(xml, tag, attrs):
    """Insert (or overwrite) attrs on every `<tag ` element."""
    for k, v in attrs.items():
        xml = re.sub(rf'{k}="[^"]*"\s*', '', xml)  # drop existing
    add = " ".join(f'{k}="{v}"' for k, v in attrs.items())
    return xml.replace(f"<{tag} ", f"<{tag}  {add} ")

def make_aenv(points):
    body = "\n".join(
        f' <p  l="{hexf(l)}"  t="{hexf(t)}"  s="{s}"  c="{hexf(c)}" >\n</p>'
        for l, t, s, c in points)
    return f'<AENV  c="3"  pan="0"  zoom="3e449d2c" >\n{body}\n </AENV>'

def replace_aenv(xml, points):
    out, rest = [], xml
    while "<AENV " in rest:
        i = rest.index("<AENV "); j = rest.index("</AENV>", i) + 7
        out.append(rest[:i]); out.append(make_aenv(points)); rest = rest[j:]
    out.append(rest)
    return "".join(out)

def static_base():
    """The base patch with every motion source neutralized."""
    x = open(PATCH, errors="replace").read()
    x = rewrite(x, r"ArpOnOff", "0")
    for attr in ["unsOn", "udrft", "uanalg", "hrmOn", "fm", "am", "grnOn", "gran", "odrft"]:
        x = rewrite(x, attr, "0")
    x = rewrite(x, r"Active", "0")            # all EFFMODULE FX off
    x = rewrite(x, r"source\d+", "off")       # mod matrix silenced
    # Solo the first voice: zero the OSC level in every subsequent VOICE so
    # inter-layer detune beating doesn't wobble the measurements.
    parts = x.split("<VOICE")
    for i in range(2, len(parts)):
        parts[i] = re.sub(r'level="[^"]*"', 'level="0"', parts[i], count=1)
    x = "<VOICE".join(parts)
    return x

def build(name, xml):
    p = f"{SCRATCH}/cal/{name}.prt_omn"
    open(p, "w").write(xml)
    subprocess.run([f"{SIGNAL}/target/debug/examples/omni_state", "patch", p,
                    TEMPLATE, f"{SCRATCH}/cal/{name}.bin"],
                   check=True, capture_output=True)

def run_remote(names, note=48, secs=10):
    subprocess.run(["scp"] + [f"{SCRATCH}/cal/{n}.bin" for n in names] +
                   ["voyager:/tmp/"], check=True, capture_output=True)
    cmds = "; ".join(
        f'./target/release/examples/load_plugin "{OMNI}" --load-state /tmp/{n}.bin '
        f"--note {note} --secs {secs} --render /tmp/cal_{n}.wav >/dev/null 2>&1"
        for n in names)
    subprocess.run(["ssh", "voyager",
                    f"cd ~/Development/FastTrackStudio/signal && {cmds}"], check=True)
    subprocess.run(["scp"] + [f"voyager:/tmp/cal_{n}.wav" for n in names] +
                   [f"{SCRATCH}/cal/"], check=True, capture_output=True)

def read_wav(path):
    b = open(path, "rb").read(); i = 12; data = None; ch = 2
    while i < len(b):
        cid = b[i:i+4]; size = struct.unpack_from("<I", b, i+4)[0]
        if cid == b"fmt ":
            _, ch, _ = struct.unpack_from("<HHI", b, i+8)
        elif cid == b"data":
            data = b[i+8:i+8+size]; break
        i += 8 + size + (size & 1)
    n = len(data) // 4
    smp = struct.unpack(f"<{n}f", data)
    return [(smp[j] + smp[j+1]) / 2 for j in range(0, n - 1, ch)]

def envelope(x, sr=48000, win=480):
    return [(s / sr, math.sqrt(sum(v * v for v in x[s:s+win]) / win))
            for s in range(0, len(x) - win, win)]

def decay_marks(name, off_s):
    env = envelope(read_wav(f"{SCRATCH}/cal/cal_{name}.wav"))
    pre = [e for t, e in env if off_s - 1.0 < t < off_s - 0.1]
    ref = sum(pre) / len(pre) if pre else 0
    cv = (max(pre) - min(pre)) / ref if ref else 9  # staticness check
    marks = {}
    for db, frac in [(-3, 0.708), (-6, 0.5), (-12, 0.25), (-20, 0.1), (-26, 0.05)]:
        marks[db] = next((round(t - off_s, 2) for t, e in env
                          if t > off_s + 0.02 and e < ref * frac), None)
    return ref, cv, marks

def band_energy(name, sr=48000):
    """Goertzel energy at probe frequencies (for filter sweeps)."""
    x = read_wav(f"{SCRATCH}/cal/cal_{name}.wav")
    seg = x[sr*1:int(sr*2.8)]  # sustained region (note-off at secs/2)
    out = {}
    for f in [65, 131, 262, 523, 1046, 2093, 4186, 8372]:
        w = 2 * math.pi * f / sr
        s0 = s1 = s2 = 0.0
        cw = 2 * math.cos(w)
        for v in seg:
            s0 = v + cw * s1 - s2
            s2 = s1; s1 = s0
        p = s2*s2 + s1*s1 - cw*s1*s2
        out[f] = 10 * math.log10(p + 1e-12)
    return out

def env_period(name, lo_s=1.2, hi_s=3.8):
    """Dominant periodicity of the amplitude envelope (autocorrelation)."""
    env = envelope(read_wav(f"{SCRATCH}/cal/cal_{name}.wav"), win=240)  # 5 ms hop
    seg = [e for t, e in env if lo_s < t < hi_s]
    if len(seg) < 40: return None
    mean = sum(seg) / len(seg)
    x = [v - mean for v in seg]
    n = len(x)
    best_lag, best = None, 0.0
    e0 = sum(v * v for v in x) or 1e-12
    prev = 1.0
    rising = False
    for lag in range(2, n // 2):
        r = sum(x[i] * x[i + lag] for i in range(n - lag)) / e0
        if rising and r < prev:
            best_lag, best = lag - 1, prev
            break
        rising = r > prev
        prev = r
    if best_lag is None or best < 0.15: return None
    return 1.0 / (best_lag * 0.005)  # Hz

if __name__ == "__main__":
    os.makedirs(f"{SCRATCH}/cal", exist_ok=True)
    mode = sys.argv[1] if len(sys.argv) > 1 else "curve"
    base = static_base()
    if mode == "lfo2":
        # LFO rate via PITCH vibrato (route LFO1 → A tune ±, measured as
        # periodic f0 wobble): defV/lo/hi carry the base + range.
        cases = {}
        lbase = insert_attrs(base, "MOD_MATRIX", {
            "source0": "LFO1", "target0": "A tune",
            "defV0": hexf(0.5), "lo0": hexf(0.40), "hi0": hexf(0.60),
            "mute0": hexf(1.0), "damp0": "0",
        })
        lbase = rewrite(lbase, r"sync", "0")  # free-running, not tempo-synced
        for rv in [0.2, 0.4, 0.6]:
            cases[f"lfo2_{rv}"] = rewrite(lbase, r"rate", hexf(rv))
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=60, secs=8)
        for n in cases:
            hz = env_period(n)
            print(f"{n:<10} periodicity={hz and round(hz,2)} Hz")
    elif mode == "unison2":
        # Unison with INSERTED enable attrs (v2 patches lack them entirely).
        cases = {}
        for dv in [0.1, 0.4, 0.8]:
            u = insert_attrs(base, "OSC", {
                "unsOn": hexf(1.0), "unsLv": hexf(0.75),
                "ucnt": hexf(1/7), "udpth": hexf(dv),
                "uwdth": "0", "uoct": "0", "udrft": "0", "uanalg": "0",
            })
            cases[f"uni2_{dv}"] = u
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=60, secs=8)
        for n in cases:
            hz = env_period(n)
            print(f"{n:<10} beat={hz and round(hz,2)} Hz")
    elif mode == "lfo":
        # LFO rate curve: route LFO1 → layer A amp trem at full depth,
        # sweep the LFO rate attr, measure tremolo Hz via autocorrelation.
        cases = {}
        lbase = rewrite(base, r"source0", "LFO1")
        lbase = rewrite(lbase, r"target0", "A atrm")
        lbase = rewrite(lbase, r"hi0", hexf(1.0))
        lbase = rewrite(lbase, r"lo0", "0")
        lbase = rewrite(lbase, r"mute0", "3f800000")  # corpus: 1 on ACTIVE routes
        lbase = rewrite(lbase, r"type", "0")  # sine LFOs (OSC kept: kind attr)
        for rv in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]:
            cases[f"lfo_{rv}"] = rewrite(lbase, r"rate", hexf(rv))
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=48, secs=8)
        for n in cases:
            hz = env_period(n)
            print(f"{n:<10} trem={hz and round(hz,2)} Hz")
    elif mode == "unison":
        # Unison detune: 2 voices, no width → beat rate = Δf Hz.
        cases = {}
        ubase = rewrite(base, r"unsOn", "3f800000")
        ubase = rewrite(ubase, r"umix", "3f800000")
        ubase = rewrite(ubase, r"ucnt", hexf(1/7))    # 2 voices
        ubase = rewrite(ubase, r"uwdth|uoct|udrft|uanalg|udprg", "0")
        for dv in [0.1, 0.2, 0.4, 0.6, 0.8]:
            cases[f"uni_{dv}"] = rewrite(ubase, r"udpth", hexf(dv))
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=60, secs=8)  # C4 ≈ 261.6 Hz
        for n in cases:
            hz = env_period(n)
            cents = hz and round(1731.2 * hz / 261.63, 1)
            print(f"{n:<10} beat={hz and round(hz,2)} Hz  ≈{cents} cents total")
    elif mode == "filter":
        # Filter freq curve: engage filter 1 (LP, the patch's own type1),
        # kill resonance + env depth, sweep the master freq.
        cases = {}
        fbase = rewrite(base, r"envdpth", "0")
        fbase = rewrite(fbase, r"res|res1|res2", "0")
        fbase = rewrite(fbase, r"act|act1", "3f800000")
        fbase = rewrite(fbase, r"act2", "0")
        fbase = rewrite(fbase, r"dpth", "0")  # waveshaper depth (shares 'act')
        fbase = rewrite(fbase, r"key|keyinv|keymp", "0")  # no keytracking
        cases["filt_off"] = rewrite(base, r"act", "0")  # unfiltered reference
        for fv in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]:
            cases[f"freq_{fv}"] = rewrite(fbase, r"freq", hexf(fv))
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=36, secs=6)
        for n in cases:
            be = band_energy(n)
            print(n, " ".join(f"{f}:{v:.0f}" for f, v in be.items()))
    elif mode == "types":
        # type1 enum sweep at fixed mid freq, no resonance.
        cases = {}
        fbase = rewrite(base, r"envdpth", "0")
        fbase = rewrite(fbase, r"res|res1|res2", "0")
        fbase = rewrite(fbase, r"act|act1", "3f800000")
        fbase = rewrite(fbase, r"act2", "0")
        fbase = rewrite(fbase, r"dpth", "0")
        fbase = rewrite(fbase, r"freq", hexf(0.5))
        for k in range(1, 51):
            tv = round(0.02 * k, 4)
            cases[f"type_{k:02d}"] = rewrite(fbase, r"type1", hexf(tv))
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases), note=36, secs=6)
        for n in cases:
            be = band_energy(n)
            print(n, " ".join(f"{f}:{v:.0f}" for f, v in be.items()))
    elif mode == "curve2":
        # Curve attaches to the segment STARTING at a point (terminal-point c
        # was a no-op): sweep the SUSTAIN point's c across a 2 s release.
        cases = {}
        for c in [0.0, 0.25, 0.5, 0.75, 1.0]:
            cases[f"crv2_{c}"] = replace_aenv(base, [(0,0,14,0.5),(1,0.0001,14,0.5),(0.8,0.005,14,c),(0,0.025,18,0.5)])
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases))
        for n in cases:
            ref, cv, m = decay_marks(n, 5.0)
            print(f"{n:<12} ref={ref:.4f} wobble={cv*100:.1f}%  " +
                  "  ".join(f"{db}dB@{t}" for db, t in m.items()))
    elif mode == "curve":
        cases = {}
        cases["static_chk"] = replace_aenv(base, [(0,0,14,0.5),(1,0.0001,14,0.5),(0.8,0.005,14,0.5),(0,0.025,18,0.5)])
        cases["static_chk2"] = cases["static_chk"]  # determinism check
        for c in [0.0, 0.25, 0.5, 0.75, 1.0]:
            cases[f"crv_{c}"] = replace_aenv(base, [(0,0,14,0.5),(1,0.0001,14,0.5),(0.8,0.005,14,0.5),(0,0.025,18,c)])
        for n, xml in cases.items():
            build(n, xml)
        run_remote(list(cases))
        for n in cases:
            ref, cv, m = decay_marks(n, 5.0)
            print(f"{n:<12} ref={ref:.4f} wobble={cv*100:.1f}%  " +
                  "  ".join(f"{db}dB@{t}" for db, t in m.items()))
