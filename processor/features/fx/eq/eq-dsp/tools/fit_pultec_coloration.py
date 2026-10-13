#!/usr/bin/env python3
"""Fit the normalized Pultec transfer residual at its captured default panel.

Uses -12 and 0 dBFS at 1 kHz; -24 and -6 dBFS are validation levels. This fits
coloration only. The archive's scan/saturation data do not identify full EQ
frequency responses, and this script does not claim to fit those responses.
"""
import argparse
import hashlib
import json
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--archive',type=Path,default=Path('/run/media/AudioHaven/Plugin Analysis'))
parser.add_argument('--output',type=Path,default=Path('features/fx/eq/eq-dsp/tests/fixtures/pultec_coloration.csv'))
args=parser.parse_args()
p=args.archive/'UADx Pultec EQP-1A/saturation/mode-Low_Freq-60_CPS/saturation.json'
d=json.loads(p.read_text())
points=[]
for shape in d['shapes']:
    assert shape['freq_hz']==1000.0 and shape['pass']=='default'
    points.extend((shape['level_db'],shape['peak']*x,shape['peak']*y) for x,y in zip(shape['x'],shape['y']))
# Solve for a fifth-order residual with unit small-signal derivative. The
# capture removes DC: center even powers by their sine-cycle means before fitting.
a=[[0.0]*5 for _ in range(4)]
for level,x,y in points:
    if level not in [-12.0,0.0]:continue
    amplitude=10**(level/20)
    basis=[x*x-amplitude*amplitude/2,x**3,x**4-3*amplitude**4/8,x**5]
    for i in range(4):
        for j in range(4):a[i][j]+=basis[i]*basis[j]
        a[i][4]+=basis[i]*(y-x)
for i in range(4):
    k=max(range(i,4),key=lambda j:abs(a[j][i]));a[i],a[k]=a[k],a[i]
    pivot=a[i][i];assert abs(pivot)>1e-15
    a[i]=[x/pivot for x in a[i]]
    for j in range(4):
        if j!=i:
            scale=a[j][i];a[j]=[x-scale*y for x,y in zip(a[j],a[i])]
c=[row[4] for row in a]
print('coefficients:',', '.join(format(x,'.16g') for x in c))
for level in sorted(set(p[0] for p in points)):
    errors=[abs(x+sum(v*b for v,b in zip(c,[x*x-10**(level/10)/2,x**3,x**4-3*10**(level/5)/8,x**5]))-y) for l,x,y in points if l==level]
    print(f'{level:g} dBFS max normalized transfer error {max(errors):.9g}')
lines=['# Normalized transfer shape: excludes captured linear gain/phase.',
       '# Source SHA256: '+hashlib.sha256(p.read_bytes()).hexdigest(),
       '# Pinned controls: '+json.dumps(d['pinned']),
       '# frequency=1000 Hz sample_rate='+str(d['sample_rate']),
       '# input_level_db,input_sample,normalized_output_sample']
lines.extend(f'{level:g},{x:.16g},{y:.16g}' for level,x,y in points)
args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text('\n'.join(lines)+'\n')
