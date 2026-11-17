"""Build a one-track REAPER project: <fxchain file> + a MIDI file as an inline item, rendering to <out wav>."""
import struct,sys
fx_path, mid_path, out_wav, rpp_path = sys.argv[1:5]
d=open(mid_path,'rb').read()
assert d[:4]==b'MThd'
fmt,ntrk,div=struct.unpack('>HHH',d[8:14]); p=14
evs=[]  # (abs_tick, bytes)
tempo_us=500000
track_end=0  # the latest end-of-track: a file may end after its last note to keep a tail
for _ in range(ntrk):
    assert d[p:p+4]==b'MTrk'; ln=struct.unpack('>I',d[p+4:p+8])[0]; q=p+8; end=q+ln; t=0; run=None
    while q<end:
        v=0
        while True:
            b=d[q]; q+=1; v=(v<<7)|(b&0x7f)
            if b<0x80: break
        t+=v; st=d[q]
        if st==0xFF:
            ty=d[q+1]; q+=2; l=0
            while True:
                b=d[q]; q+=1; l=(l<<7)|(b&0x7f)
                if b<0x80: break
            if ty==0x51 and t==0: tempo_us=int.from_bytes(d[q:q+3],'big')
            elif ty==0x51: sys.exit('tempo change mid-file not supported')
            q+=l; continue
        if st in (0xF0,0xF7):
            q+=1; l=0
            while True:
                b=d[q]; q+=1; l=(l<<7)|(b&0x7f)
                if b<0x80: break
            q+=l; continue
        if st&0x80: run=st; q+=1
        n=2 if (run&0xF0) not in (0xC0,0xD0) else 1
        evs.append((t,bytes([run])+d[q:q+n])); q+=n
    track_end=max(track_end,t)
    p=end
evs.sort(key=lambda e:e[0])
bpm=60_000_000/tempo_us
ppq=div; scale=1  # keep the file's own resolution: exact ticks stay exact
last=max(evs[-1][0],track_end)*scale
lines=[]; prev=0
for t,b in evs:
    tt=round(t*scale); lines.append(f"        E {tt-prev} "+" ".join(f"{x:02x}" for x in b) + (" 00" if len(b)==2 else "")); prev=tt
length_s=last/ppq*60/bpm+4
fx=open(fx_path).read()
rpp=f'''<REAPER_PROJECT 0.1 "7.0" 1700000000
  RENDER_FILE "{out_wav}"
  RENDER_PATTERN ""
  RENDER_FMT 0 2 48000
  RENDER_1X 0
  RENDER_RANGE 1 0 0 18 1000
  RENDER_RESAMPLE 3 0 1
  RENDER_ADDTOPROJ 0
  RENDER_STEMS 0
  RENDER_DITHER 0
  <RENDER_CFG
    ZXZhdxgAAQ==
  >
  SAMPLERATE 48000 0 0
  TEMPO {bpm:.6f} 4 4 0
  <TRACK
    NAME "AB"
    VOLPAN 1 0 -1 -1 1
    MUTESOLO 0 0 0
    NCHAN 2
{fx.rstrip()}
    <ITEM
      POSITION 0
      LENGTH {length_s:.6f}
      LOOP 0
      NAME "ab"
      <SOURCE MIDI
        HASDATA 1 {ppq} QN
{chr(10).join(lines)}
        E {round((length_s*bpm/60)*ppq)-prev} b0 7b 00
      >
    >
  >
>
'''
open(rpp_path,'w').write(rpp)
print(f"{len(evs)} events, {bpm} bpm, {length_s:.1f}s")
