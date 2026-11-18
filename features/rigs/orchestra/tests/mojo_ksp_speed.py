#!/usr/bin/env python3
"""MOJO 2's `_Speed_to_ep` (script_1.ksp 20353) in KSP's 32-bit integers.

The script turns a stretch speed percent into the Time Machine parameter
through its own fixed-point maths: `_Log2` (the 32767-entry log table its
init fills, 24425-24452, and `_Lg`/`_NLZ`), `_Root3` (`%otiqs`), then
`(145·R + (145·R mod 4091))/4091 + 500000`. Every value KONTRA traces
(76 % → 254509, 90 → 320780, 102 → 602974, 106 → 647691, 113 → 689328,
114 → 693810, 115 → 698062, 118 → 709667, 153 → 788682) comes out exact.

    python3 mojo_ksp_speed.py [--styx N]   # checks; or the styx table 0..N
"""
import sys

SFVEY = (1000000000, 584962501, 321928095, 169925001, 87462841, 44394119, 22367813, 11227256, 5624549, 2815016, 1408195, 704269, 352178, 176099, 88052, 44028, 22014, 11006, 5504, 2751, 1376)
OTIQS = (671, 717, 758, 795, 829, 861, 890, 917, 956, 956, 1002, 1002, 1045, 1045, 1084, 1084, 1121, 1121, 1156, 1156, 1204, 1204, 1204, 1204, 1263, 1263, 1263, 1263)
M = 1 << 32
def i32(x):
    x &= M - 1
    return x - M if x >= 1 << 31 else x
def div(a, b):  # truncates toward zero
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q
def mod(a, b):
    return a - div(a, b) * b
def shl(a, n): return i32(a << n) if n >= 0 else a >> -n
def shr(a, n): return a >> n
def nlz(x):
    b = 0
    if 0 <= x <= 31: b = 27
    elif 32 <= x <= 255: b = 24
    elif 256 <= x <= 16383: b = 18
    elif 16384 <= x <= 1048575: b = 12
    elif 1048576 <= x <= 67108863: b = 6
    y = shl(x, b)
    if 1073741824 <= y <= 2147483647: b += 1
    elif 536870912 <= y <= 1073741823: b += 2
    elif 268435456 <= y <= 536870911: b += 3
    elif 134217728 <= y <= 268435455: b += 4
    elif 0 <= y <= 134217727: b += 5
    return b
LOG = []
for z in range(1, 32768):
    b = nlz(z) - 1
    v = shl(z, b)
    piq = 30 - b
    if v == 1073741824:
        r = 0
    else:
        r = 999999312
        c = 1
        while c <= 20 and v >= 0:
            g = shr(v, c)
            if i32(-v - g) < 0:
                v = i32(v + g)
                r -= SFVEY[c]
            c += 1
        r = div(r + 500, 1000)
    LOG.append(i32(r + 1000000 * piq))
def lg(x):
    if x <= 0: return -2147483648
    b = nlz(x)
    if b >= 17: piq = 0
    else:
        piq = 17 - b
        x = div(shr(x, piq - 1) + 1, 2)
    return LOG[x - 1], piq
def log2(x):
    r, piq = lg(x)
    return r + piq * 1000000 if r >= 0 else r
def root3(x):
    if x < -2147483647: return -129015915
    if abs(x) < 2: return 100000 * x
    neg = x < 0
    x = abs(x)
    m = div(nlz(x) - 1, 3)
    x = shl(x, 3 * m)
    f = OTIQS[shr(x, 26) - 4]
    f = div(2 * f + div(x, f * f), 3)
    d = div(x, f) - f * f
    f = 400000 * f + div(400000 * d, 3 * f + div(d, f))
    r = shr(shr(f, m) + 2, 2)
    return -r if neg else r
def speed_ep(pct):
    r = root3(log2(3 * (pct * 10) + 500) - 11773139)
    return div(145 * r + mod(145 * r, 4091), 4091) + 500000
if __name__ == "__main__":
    if "--styx" in sys.argv:
        n = int(sys.argv[sys.argv.index("--styx") + 1])
        print(" ".join(str(speed_ep(p)) if p > 0 else "500000" for p in range(n + 1)))
        raise SystemExit
    # every value KONTRA traced running MOJO's script (Trumpet, Alto Sax)
    traced = ((76, 254509), (90, 320780), (100, 500000), (102, 602974), (106, 647691), (113, 689328),
              (114, 693810), (115, 698062), (118, 709667), (153, 788682))
    for p, want in traced:
        print(p, speed_ep(p), want, "OK" if speed_ep(p) == want else "DIFF")
