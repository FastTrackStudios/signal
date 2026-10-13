"""Compare two screenshots structurally. No third-party deps (no PIL, no numpy).

For porting a pane from DOM to a painted scene, this is the check that
matters: render the SAME fixture both ways and diff them. `just ee-shots
shoot_stack` writes into target/gui-shots/expression-editor/, so keeping a
copy before the port and diffing after tells you whether the picture
survived. It caught two real defects in the drum stack port that reading
the code did not: labels dropped by one ascent, and lane content bleeding
into the lane below.

Read the output as two populations. Differences of one or two levels are
the two rasterizers disagreeing about antialiasing and are not worth a
second look; the threshold argument filters them out. What you are hunting
is CLUSTERS - a band of rows where a shape moved, grew, or vanished. Two
clusters about eight pixels apart over the same columns is one label that
shifted, not two that changed.

Usage:
    python3 scripts/ui-stress/imagediff.py BEFORE.png AFTER.png [threshold]
"""
import struct, zlib, sys

def read(path):
    d = open(path,'rb').read()
    assert d[:8] == b'\x89PNG\r\n\x1a\n'
    i, idat, w, h, bd, ct = 8, b'', 0,0,0,0
    while i < len(d):
        ln = struct.unpack('>I', d[i:i+4])[0]; typ = d[i+4:i+8]; body = d[i+8:i+8+ln]
        if typ == b'IHDR':
            w,h,bd,ct = struct.unpack('>IIBB', body[:10])
        elif typ == b'IDAT': idat += body
        i += 12 + ln
    assert bd == 8 and ct in (2,6), (bd,ct)
    ch = 3 if ct==2 else 4
    raw = zlib.decompress(idat)
    out, prev, pos = [], bytearray(w*ch), 0
    for _ in range(h):
        f = raw[pos]; pos += 1
        line = bytearray(raw[pos:pos+w*ch]); pos += w*ch
        for x in range(len(line)):
            a = line[x-ch] if x >= ch else 0
            b = prev[x]; c = prev[x-ch] if x >= ch else 0
            if f==1: line[x] = (line[x]+a)&255
            elif f==2: line[x] = (line[x]+b)&255
            elif f==3: line[x] = (line[x]+(a+b)//2)&255
            elif f==4:
                p = a+b-c; pa,pb,pc = abs(p-a),abs(p-b),abs(p-c)
                pr = a if (pa<=pb and pa<=pc) else (b if pb<=pc else c)
                line[x] = (line[x]+pr)&255
        out.append(bytes(line)); prev = line
    return w,h,ch,out

def main(pa, pb, threshold=24):
    w1,h1,c1,A = read(pa); w2,h2,c2,B = read(pb)
    print(f'{w1}x{h1} vs {w2}x{h2}, threshold {threshold}/255')
    if (w1,h1)!=(w2,h2):
        print('DIFFERENT SIZE')
        return 1
    rows=[]
    for y in range(h1):
        ra,rb = A[y],B[y]
        rows.append([x for x in range(w1)
                     if max(abs(ra[x*c1+k]-rb[x*c2+k]) for k in range(3)) > threshold])
    total=sum(len(r) for r in rows)
    print(f'structural differences: {total} px of {w1*h1} ({100*total/(w1*h1):.3f}%)')
    band=None
    for y,xs in enumerate(rows):
        if xs and band is None:
            band=y
        elif not xs and band is not None:
            span=[x for yy in range(band,y) for x in rows[yy]]
            n=sum(len(rows[yy]) for yy in range(band,y))
            print(f'  y {band}-{y-1}: x {min(span)}-{max(span)}, {n} px')
            band=None
    if band is not None:
        span=[x for yy in range(band,h1) for x in rows[yy]]
        print(f'  y {band}-{h1-1}: x {min(span)}-{max(span)}')
    return 0


if __name__ == '__main__':
    th = int(sys.argv[3]) if len(sys.argv) > 3 else 24
    raise SystemExit(main(sys.argv[1], sys.argv[2], th))
