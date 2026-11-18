"""resprof.py <kontakt.wav> <ours.wav> <manifest.tsv> <idx> [step_ms] : residual and both levels through one unit."""
import sys, csv
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))
from null_test import load, SR
import numpy as np
k=load(sys.argv[1]); o=load(sys.argv[2])
r=list(csv.DictReader(open(sys.argv[3]),delimiter='\t'))[int(sys.argv[4])]
st=int(sys.argv[5]) if len(sys.argv)>5 else 100
s0=int(float(r['t_start'])*SR); n=int(float(r['win'])*SR)
print(r['description'])
tot=np.sum(k[s0:s0+n]**2)
for a in range(-50, int(n/SR*1000), st):
    s=s0+a*SR//1000; e=s+st*SR//1000
    kk=k[s:e]; oo=o[s:e]
    ek=np.sum(kk**2); er=np.sum((kk-oo)**2)
    print(f"  {a:6}ms kon {10*np.log10(np.mean(kk**2)+1e-20):6.1f} ours {10*np.log10(np.mean(oo**2)+1e-20):6.1f} res/local {10*np.log10(er/(ek+1e-20)+1e-20):6.1f} share-of-total {100*er/tot:6.2f}%")
