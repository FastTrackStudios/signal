import json, math, os
f=json.load(open('fit2.json')); rows=[l.rstrip('\n').split('\t') for l in open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'types.txt'))]
names={h:nm for h,n,nm in rows}; uses={h:int(n) for h,n,nm in rows}
def kcrit(n): return 8.0 if n<=2 else 1/math.cos(math.pi/n)**n
print("const FILTER_MODELS: &[(u32, FilterModel)] = &[")
for t,v in sorted(f.items(), key=lambda kv:-uses.get(kv[0],0)):
    r5,r9=v['res']['0.5'],v['res']['0.9']; ladder=v['topo']=='ladder'; poles=v['poles']
    if ladder:
        kmax=1.2*kcrit(poles) if poles>2 else 8.0
        a=max(min(r5['p'],kmax),1e-3); b=min(max(r9['p'],a),kmax)
        if b<=a*1.001: hi,c=b,0.3
        else: c=math.log(b/a)/math.log(1.8); hi=a/0.5**c
        lo=0.0; hi=min(hi,kmax); comp=(r5['comp']+r9['comp'])/2
    else:
        lo=max(v['q0'],0.5); a=max(r5['p'],lo); b=max(r9['p'],a)
        if a<=lo*1.001 and b<=lo*1.001: hi,c=lo,1.0
        elif a<=lo*1.001: hi,c=lo*(b/lo)**(1/0.9**4),4.0
        else:
            la,lb=math.log(a/lo),math.log(b/lo); c=math.log(lb/la)/math.log(1.8) if lb>la else 1.0
            hi=lo*math.exp(la/0.5**c)
        hi=min(hi,40.0); comp=0.0
    c=min(max(c,0.3),6.0)
    s9=r9['shift'] if r9['err']<4 else 1.0; s5=r5['shift'] if r5['err']<4 else 1.0
    if abs(math.log(s9))<1e-3: sh,g=1.0,1.0
    else:
        ratio=math.log(s5)/math.log(s9) if abs(math.log(s5))>1e-3 else 0.3
        g=math.log(ratio)/math.log(0.5/0.9) if ratio>0 else 1.0
        g=min(max(g,0.15),4.0); sh=min(max(s9**(1/0.9**g),0.4),2.5)
    tap=v['taper']; pts=[]
    for k in ['0.15','0.3','0.45','0.6','0.75']:
        x=tap.get(k); sc=x if isinstance(x,float) else (x[0] if x and x[1]<3 else None)
        pts.append(sc)
    base=tap['0.3']; pts=[p if p is not None else base for p in pts]
    mode={'lp':'lowpass','hp':'highpass','bp':'bandpass','notch':'notch'}[v['mode']]
    terr=' '.join(f"{k}:{tap[k][1]:.1f}" for k in ['0.15','0.45','0.6','0.75'] if isinstance(tap.get(k),list))
    print(f"    // {names.get(t,'?')} — {uses.get(t,0)} factory uses; fit {v['err']:.1f} dB at res 0, {r5['err']:.1f}/{r9['err']:.1f} at res 0.5/0.9; taper fits {terr}")
    print(f"    (0x{int(t,16):08x}, FilterModel {{ name: \"{names.get(t,'?')}\", mode: \"{mode}\", poles: {poles}, ladder: {str(ladder).lower()}, taper: [{', '.join(f'{p:.4f}' for p in pts)}], res: ({lo:.3f}, {hi:.3f}, {c:.3f}), res_shift: {sh:.3f}, res_shift_curve: {g:.3f}, gain_db: {v['gain']:.1f}, comp: {comp:.2f} }}),")
print("];")
