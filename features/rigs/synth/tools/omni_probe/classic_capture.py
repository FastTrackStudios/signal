#!/usr/bin/env python3
"""Capture Omnisphere's classic (DSP) oscillator as wavetables.

A layer with an empty <WAVES> plays the built-in oscillator picked by `OSC
type` (a 0.02-step index), shaped by `pdepth` (Shape) and `pwidth`
(Symmetry). For each type and each Symmetry step this renders the plugin at
every Shape step (filters, drift, unison, Harmonia off), extracts one cycle by
harmonic analysis (phase-aligned to the fundamental so frames morph without
combing) and writes `<out>/t<idx>_s<sym>.wav`: SHAPES frames of 4096 float
samples, Shape 0 → 1. Levels are absolute: scaled by the same plugin-output /
table ratio measured on a real wavetable (the init part's JP-8 Saw), so the
tables sit in the units the imported WAVES tables do.

usage: classic_capture.py <out-dir> [type-idx ...]   env: JOBS (default 3)
"""
import os, re, struct, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
H = "/Volumes/dev-drive/daw/target/release/examples/omni_render"
SR, NOTE, N = 48000, 24, 4096
SYMS = [0.0, 0.25, 0.5, 0.75, 1.0]
SHAPES = [i / 16 for i in range(17)]
hexf = lambda v: struct.pack(">f", v).hex()
base = open(os.path.join(HERE, "init_part.prt_omn"), encoding="latin1").read()
REF_TABLE = ("/Volumes/dev-drive/AudioHaven/Sampled/Synth/Omnisphere-Wavetables/"
             "1 - Classic Waveforms/Sawtooth/JP-8 Saw/Jupiter 8 Saw.wav")

def osc_edit(xml, **kv):
    i = xml.index("<OSC "); j = xml.index(">", i)
    tag = xml[i:j]
    for k, v in kv.items():
        tag = re.sub(rf'\b{k}="[^"]*"', f'{k}="{v}"', tag) if re.search(rf'\b{k}="', tag) else tag + f' {k}="{v}" '
    return xml[:i] + tag + xml[j:]

def patch(t, shape, sym, classic=True):
    x = osc_edit(base, type=hexf(t / 50), pdepth=hexf(shape), pwidth=hexf(sym), odrft="0", hrmOn="0", unsOn="0")
    if classic:
        k = x.index("<WAVES", x.index("<OSC ")); e = x.index(">", k)
        x = x[:k] + "<WAVES" + x[e:]
    return x

def render(xml):
    fd, p = tempfile.mkstemp(suffix=".prt_omn"); os.close(fd)
    w = p[:-8] + ".wav"
    open(p, "w", encoding="latin1").write(xml)
    subprocess.run([H, p, w, "--note", str(NOTE), "--hold", "1.4", "--tail", "0.05"],
                   capture_output=True, stdin=subprocess.DEVNULL, timeout=300)
    d = open(w, "rb").read(); i = 12
    while i < len(d):
        cid, sz = d[i:i+4], struct.unpack("<I", d[i+4:i+8])[0]
        if cid == b"data": data = d[i+8:i+8+sz]
        i += 8 + sz + (sz & 1)
    os.remove(p); os.remove(w)
    return np.frombuffer(data, "<f4").reshape(-1, 2).mean(1).astype(np.float64)

def harmonics(x):
    seg = x[int(0.45 * SR):int(1.35 * SR)]
    n = np.arange(len(seg)); win = np.hanning(len(seg)); ws = win.sum()
    f_nom = 440 * 2 ** ((NOTE - 69) / 12)
    def amp(f, k):
        return 2 * np.sum(seg * win * np.exp(-2j * np.pi * k * f * n / SR)) / ws
    # Refine f0 on the energy of the first harmonics.
    best = max(np.linspace(f_nom * 0.995, f_nom * 1.005, 201),
               key=lambda f: sum(abs(amp(f, k)) for k in range(1, 9)))
    K = int(min(20000, SR / 2 - 200) // best)
    c = np.array([amp(best, k) for k in range(1, K + 1)])
    return best, c

def cycle(c):
    k = np.arange(1, len(c) + 1)
    ph = np.angle(c) - k * np.angle(c[0])          # align the fundamental to phase 0
    t = np.arange(N) / N
    return (np.abs(c)[:, None] * np.cos(2 * np.pi * k[:, None] * t[None, :] + ph[:, None])).sum(0)

def write_wav(path, frames):
    data = np.concatenate(frames).astype("<f4").tobytes()
    hdr = b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 3, 1, SR, SR * 4, 4, 32)
    open(path, "wb").write(hdr + b"data" + struct.pack("<I", len(data)) + data)

def ref_scale():
    """Plugin output per table unit: the JP-8 Saw frame 0 vs its render."""
    tab = np.frombuffer(open(REF_TABLE, "rb").read()[-64 * N * 4:], "<f4")[:N].astype(np.float64)
    f0, c = harmonics(render(patch(50, 0.0, 0.0, classic=False)))
    got = cycle(c)
    k = np.fft.rfft(tab)[1:len(c) + 1]
    tab_bl = np.fft.irfft(np.concatenate([[0], k, np.zeros(N // 2 + 1 - len(c) - 1)]), N)
    return np.sqrt(np.mean(got ** 2)) / np.sqrt(np.mean(tab_bl ** 2))

def main():
    out = sys.argv[1]; os.makedirs(out, exist_ok=True)
    types = [int(t) for t in sys.argv[2:]] or [0, 1, 2, 3, 5, 50]
    scale = ref_scale()
    print(f"plugin output per table unit: {scale:.4f}", flush=True)
    jobs = [(t, si, s, sh) for t in types for si, s in enumerate(SYMS) for sh in SHAPES]
    def one(j):
        t, si, s, sh = j
        f0, c = harmonics(render(patch(t, sh, s)))
        return j, cycle(c) / scale
    res = {}
    with ThreadPoolExecutor(int(os.environ.get("JOBS", "3"))) as ex:
        for (t, si, s, sh), cyc in ex.map(one, jobs):
            res[(t, si, sh)] = cyc
    for t in types:
        for si, s in enumerate(SYMS):
            frames = [res[(t, si, sh)] for sh in SHAPES]
            write_wav(os.path.join(out, f"t{t:02d}_s{si}.wav"), frames)
            peaks = [f"{np.max(np.abs(f)):.2f}" for f in frames[::4]]
            print(f"type {t:2d} sym {s:.2f}: peaks {peaks}", flush=True)

if __name__ == "__main__":
    main()
