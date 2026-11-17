#!/usr/bin/env python3
"""Kontakt's effective playback rate per key (gen_probe.py --tunes render) against
the exact zone product (group tune × zone tune × semitones).

    python3 rate_sweep.py <kontakt.wav> <manifest.tsv> <css_root>

Measured on CSS 1st Violins: Kontakt's rate is a repeatable function of the ideal
rate (same product → same rate) but off by up to ±1.8 ppm (±0.003 c) in a
pattern that is neither a fixed-point step nor a cents grid — the known floor
(−38…−41 dB on a held note) of every null test.
"""
import sys, csv, os, math, numpy as np
sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, load
from identify import resample, TAPS
kon = load(sys.argv[1]); man = sys.argv[2]; root = sys.argv[3]
zones = list(csv.DictReader(open(os.path.join(root, '_ksp', 'CSS 1st Violins', 'zones.tsv')), delimiter='\t'))
def frac_lag(a, b):
    n = len(a); nfft = 1 << (2*n-1).bit_length(); acc = 0
    for c in range(2): acc = acc + np.fft.irfft(np.fft.rfft(b[:, c], nfft)*np.conj(np.fft.rfft(a[:, c], nfft)), nfft)
    acc = np.concatenate([acc[-50:], acc[:51]]); i = int(np.argmax(acc)); y0, y1, y2 = acc[i-1], acc[i], acc[i+1]
    return (i-50) + 0.5*(y0-y2)/(y0-2*y1+y2)
print("key  zone_tune  semis  ideal_rate    kontakt_rate   diff_cents  diff_ppm  offset_frames")
for r in csv.DictReader(open(man), delimiter='\t'):
    key = int(r['pitch']); t0 = float(r['t_start'])
    z = [z for z in zones if int(z['key_lo']) <= key <= int(z['key_hi']) and z['vel_lo'] == '1' and '_Nonvib_Mix_ff_' in z['sample']]
    if not z: continue
    z = z[0]; note = os.path.basename(z['sample'])[len('1v_Nonvib_Mix_ff_'):-4]
    f = os.path.join(root, 'Mix', '1st Violins', 'Sustain', 'Nonvib', f'ff_{note}.wav')
    ideal = 1.0052122*float(z['tune'])*2**((key-int(z['root']))/12)
    y = resample(load(f)[int(z['sample_start']):], ideal, int(3.0*SR))
    s = int(round(t0*SR)); K = kon[s:s+len(y)]
    ts, ls = [], []
    for t in np.arange(0.3, 2.8, 0.25):
        a0 = int(t*SR); ls.append(frac_lag(K[a0:a0+4800], y[a0:a0+4800])); ts.append(t)
    p = np.polyfit(ts, ls, 1); kr = ideal*(1+p[0]/SR)
    print(f"{key:3d}  {float(z['tune']):.7f}  {key-int(z['root']):+d}    {ideal:.9f}  {kr:.9f}  {1200*math.log2(kr/ideal):+.5f}   {p[0]/SR*1e6:+7.3f}   {p[1]:+.3f}")
