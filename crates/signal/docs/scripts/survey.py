#!/usr/bin/env python3
"""Comprehensive survey of every Keyscape .db to enumerate what the signalpack
format must support: mics, trigger layers, roles, RR, velocity layers, tuning,
gain, key-range styles, multi-articulation, combos."""
import re, struct, glob, os, collections, math
SRC="/run/media/AudioHaven/SourceLibraries/Keyscape/Keyboards"
def hexf(s):
    try: return struct.unpack('>f', bytes.fromhex(s))[0]
    except: return 1.0

# Global aggregates
G=dict(mics=collections.Counter(), roles=collections.Counter(), triggers=collections.Counter(),
       max_rr=0, vel_layers=set(), tuned_patches=set(), gained_patches=set(),
       combo_patches=[], single_note=[], key_styles=collections.Counter())

def role(ss):
    l=ss.lower()
    if 'pedal' in l: return 'pedal'
    if 'mechanical' in l: return 'mechanical'
    if 'release' in l: return 'release'
    if 'mute' in l: return 'body:mute'
    if 'tremolo' in l: return 'body:tremolo'
    if 'vibra' in l: return 'body:vibra'
    if 'wah' in l: return 'body:wah'
    if 'amp' in l: return 'body:amp'
    if 'suitcase' in l: return 'body:suitcase'
    if 'fast' in l: return 'body:fast'
    if 'slow' in l: return 'body:slow'
    return 'body'
def norm_mic(n):
    l=n.lower()
    if l in ('default layer','mic','mono mic'): return 'Main'
    if 'room' in l or l=='microphone': return 'Room'
    if 'stereo' in l: return 'Stereo'
    if l.startswith('direct'): return 'Direct'
    return n

def survey(db):
    data=open(db,'rb').read(); end=data.find(b'</FileSystem>'); xml=data[:end+13].decode('latin1')
    bin_start=end+len(b'</FileSystem>')
    stack=[]; files=[]
    for m in re.finditer(r'<(/?DIR|FILE)([^>]*)>', xml):
        k=m.group(1); a=m.group(2)
        if k=='DIR':
            nm=re.search(r'name="([^"]*)"',a); stack.append(nm.group(1) if nm else '')
        elif k=='/DIR':
            if stack: stack.pop()
        elif k=='FILE':
            nm=re.search(r'name="([^"]*)"',a); off=re.search(r'offset="(\d+)"',a); sz=re.search(r'size="(\d+)"',a)
            if nm and off and sz: files.append((list(stack),nm.group(1),int(off.group(1)),int(sz.group(1))))
    R=dict(mics=set(),roles=set(),triggers=set(),max_rr=0,vel=set(),tuned=False,gained=False,
           roots=set(),audio=0,ss=set())
    for st,name,o,s in files:
        if 'AudioFiles' in st and name.endswith('.wav'): R['audio']+=1
        if not name.endswith('.xml') or name=='HitBundle.xml': continue
        if not any('Pitch ' in x for x in st): continue
        blob=data[bin_start+o:bin_start+o+s].decode('latin1',errors='replace')
        if '<LayerHitStack' not in blob: continue
        ss=st[0]; R['ss'].add(ss); R['roles'].add(role(ss))
        base=name[:-4]
        # trigger from layer name / role
        if 'pedal down' in base.lower(): R['triggers'].add('pedal-down')
        elif 'pedal up' in base.lower(): R['triggers'].add('pedal-up')
        elif 'release' in ss.lower(): R['triggers'].add('release')
        else: R['triggers'].add('attack')
        # mic (skip pedal trigger layers as mics)
        if 'pedal' not in base.lower(): R['mics'].add(norm_mic(base))
        for hv in re.finditer(r'<HitVelocity([^>]*)>(.*?)</HitVelocity>', blob, re.S):
            va=hv.group(1); body=hv.group(2)
            vmin=int((re.search(r'Minimum="(\d+)"',va) or [0,'0'])[1])
            vmax=int((re.search(r'Maximum="(\d+)"',va) or [0,'127'])[1])
            R['vel'].add((vmin,vmax))
            for sw in re.finditer(r'<SampleWaveform([^>]*)>', body):
                aa=sw.group(1)
                root=int((re.search(r'BaseNote="(\d+)"',aa) or [0,'0'])[1]); R['roots'].add(root)
                rr=int((re.search(r'RoundRobinSequenceNum="(\d+)"',aa) or [0,'0'])[1]); R['max_rr']=max(R['max_rr'],rr)
                if abs(hexf((re.search(r'A440="([0-9a-fA-F]+)"',aa) or [0,'3f800000'])[1])-1.0)>1e-4: R['tuned']=True
                if abs(hexf((re.search(r'Level="([0-9a-fA-F]+)"',aa) or [0,'3f800000'])[1])-1.0)>1e-3: R['gained']=True
    return R

rows=[]
for db in sorted(glob.glob(f"{SRC}/*.db")):
    patch=os.path.basename(db)[:-3]
    R=survey(db)
    rows.append((patch,R))
    for m in R['mics']: G['mics'][m]+=1
    for r in R['roles']: G['roles'][r]+=1
    for t in R['triggers']: G['triggers'][t]+=1
    G['max_rr']=max(G['max_rr'],R['max_rr'])
    G['vel_layers'].add(len(R['vel']))
    if R['tuned']: G['tuned_patches'].add(patch)
    if R['gained']: G['gained_patches'].add(patch)
    if R['audio']==0 and R['roles']: G['combo_patches'].append(patch)
    if len(R['roots'])<=2 and R['roots']: G['single_note'].append(patch)
    nroots=len(R['roots'])
    style = 'full' if nroots>=80 else ('sparse' if nroots>=10 else 'few')
    G['key_styles'][style]+=1

print("========== PER-PATCH ==========")
for patch,R in rows:
    print(f"{patch:28s} mics={sorted(R['mics'])} rr={R['max_rr']+1} vel={len(R['vel'])} roles={sorted(R['roles'])} tune={'Y' if R['tuned'] else '.'} gain={'Y' if R['gained'] else '.'}")
print("\n========== LIBRARY-WIDE FEATURE SET ==========")
print("MICS:", dict(G['mics']))
print("ROLES:", dict(G['roles']))
print("TRIGGERS:", dict(G['triggers']))
print("MAX RR (across library):", G['max_rr']+1)
print("VELOCITY-LAYER COUNTS seen:", sorted(G['vel_layers']))
print("PATCHES WITH PER-SAMPLE TUNING:", sorted(G['tuned_patches']))
print("PATCHES WITH PER-SAMPLE GAIN:", len(G['gained_patches']), "of 44")
print("COMBO (0-audio, reference) PATCHES:", G['combo_patches'])
print("SINGLE/FEW-NOTE PATCHES:", G['single_note'])
print("KEY-RANGE STYLES:", dict(G['key_styles']))
