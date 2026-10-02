#!/usr/bin/env python3
"""Spectral peaks of the held note (as ratios to the note's f0, dB re the
strongest) — for FM/AM/sync sidebands that need not be harmonic.
usage: peaks.py <patch> name=edit;edit ...   env OURS=1, NOTE"""
import os, sys, subprocess, struct
import numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
PK_ARGS = sys.argv[1:]; sys.argv = sys.argv[:1]
_f = open(os.path.join(HERE, "exp.py")).read()
exec(_f[:_f.index("def main():")])
NOTE = int(os.environ.get("NOTE", "48")); F0 = 440*2**((NOTE-69)/12); SR = 48000
os.environ.setdefault("FTS_SAMPLED_ROOT", "/Volumes/dev-drive/AudioHaven/Sampled")
base = open(PK_ARGS[0], "rb").read().decode("utf-8", "replace")
for spec in PK_ARGS[1:]:
    name, _, edits = spec.partition("=")
    xml = base
    for e in filter(None, edits.split(";")):
        xml = apply(xml, e)
    for tag, exe in (("omni", HARNESS), ("ours", OURS)):
        if tag == "ours" and not os.environ.get("OURS"): continue
        x = np.array(wav_mono(render(xml, name, NOTE, exe=exe)))[int(0.6*SR):int(1.4*SR)]
        sp = np.abs(np.fft.rfft(x*np.hanning(len(x)))); f = np.fft.rfftfreq(len(x), 1/SR)
        db = 20*np.log10(sp/sp.max() + 1e-12)
        pk = [i for i in range(2, len(sp)-2) if sp[i] >= sp[i-1] and sp[i] >= sp[i+1] and sp[i] > sp[i-2] and sp[i] > sp[i+2] and db[i] > -40 and f[i] < 12*F0]
        pk = sorted(pk, key=lambda i: -sp[i])[:10]
        print(f"{name:12s} {tag} " + " ".join(f"{f[i]/F0:.2f}:{db[i]:.0f}" for i in sorted(pk)), flush=True)
