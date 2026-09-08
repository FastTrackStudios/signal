#!/usr/bin/env python3
"""Fit stretched-exponential release to normalized applied-gain references.

Uses four Peak Reduction settings; 4/7 and 6/7 are withheld for interpolation
validation. Normalization removes makeup and the nonzero final compression.
The end-to-end release_verify example measures remaining absolute gain error.
"""
import math,csv
from collections import defaultdict
from pathlib import Path
rows=defaultdict(list)
for line in Path('features/fx/comp/comp-dsp/tests/fixtures/la2a_release.csv').read_text().splitlines():
 if not line.startswith('#'):
  pr,t,g=map(float,line.split(','));rows[pr].append((t,g))
for pr,row in rows.items():
 if pr<.2 or abs(pr-4/7)<1e-9 or abs(pr-6/7)<1e-9:continue
 before=next(g for t,g in row if t==-1);end=row[-1][1];depth=4.941176471-before
 curve=[(t,(end-g)/(end-before)) for t,g in row if 0<=t<=1500]
 def score(a,b):
  tau=math.exp(a);shape=math.exp(b)
  return sum((math.exp(-(t/tau)**shape)-r)**2 for t,r in curve)/len(curve)
 best=(1e9,0,0)
 for tau in [15,30,60,120,240,480,960]:
  for shape in [.4,.7,1.,1.5,2.]:
   a,b=math.log(tau),math.log(shape)
   for step in [.5,.2,.1,.05,.02,.01,.005,.001]:
    for _ in range(20):
     options=[(score(x,y),x,y) for x,y in [(a,b),(a+step,b),(a-step,b),(a,b+step),(a,b-step)]]
     err,x,y=min(options)
     if x==a and y==b:break
     a,b=x,y
   best=min(best,(score(a,b),a,b))
 err,a,b=best
 print(f'({depth:.6f}, {math.exp(a):.6f}, {math.exp(b):.6f}), // PR {pr:.3f}, normalized RMSE {math.sqrt(err):.4f}, residual GR {4.941176471-end:.4f}')
