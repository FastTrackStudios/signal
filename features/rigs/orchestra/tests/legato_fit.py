#!/usr/bin/env python3
"""Explain one Kontakt legato transition as its voices, window by window.

    python3 legato_fit.py <kontakt.wav> <manifest.tsv> <css_root> <unit> [--od MS] [--lt-off MS]
                          [--a-start FRAMES] [--search]

Probe units come from `gen_probe.py --legato` (CC1=127, CC2=0 → only the
ff NONVIB groups sound). Description "LL v40 67->69 ioi80": mode, velocity,
first note a, second note b, inter-onset interval. With t0 = the second
note-on, the decoded KSP timeline (target/orchestra-ab/css_ksp_decode.md §1.2)
predicts four voices:

  A   old body  — Nonvib ff of a (its first-note zone; --a-start for a body
                  variant), playing since the first note-on
  B   new body  — Nonvib ff of b from its zone start (+75 ms beyond an octave),
                  starting at t0 + OD
  LT  transition — NVLeg ff zone of the LOWER note, velocity = interval (up)
                  or interval+12 (down), starting at t0 + OD, entered at
                  lt_off = base + OD ms (LL 100/148/177, EX 0/83/177 by zone)
  R   in-phrase release of a — NVrel ff, at t0 + OD + 2 ms

OD (overlap delay) from the IOI tables. Each voice is read at its exact
Kontakt rate (group tune × zone tune × semitones). In every 10 ms window the
four gains are solved jointly by least squares; printed relative to the
instrument's full level (1.3414), with the window residual. --search
re-fits OD and lt_off by grid search on the summed residual first.
"""
import csv, os, re, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load  # noqa: E402
from release_fit import read  # noqa: E402

GROUP = 1.0052122
FULL = 1.3414
NOTE = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def ni_name(midi):  # NI naming, C3 = 60
    return f"{NOTE[midi % 12]}{midi // 12 - 2}"


def od_ms(mode, zone, ioi):
    """KSP legtrans_OD (decode §1.2 step 2), integer lerp."""
    if zone != 1:
        return 0
    if mode == "LL":
        a, b, top = 75, 100, 77
    else:
        a, b, top = 200, 300, 83
    if ioi < a:
        return top
    if ioi < b:
        return top + int(-top * (ioi - a) / (b - a))  # KSP integer division truncates toward zero
    return 0


def lt_base(mode, zone):
    return {"LL": (100, 148, 177), "EX": (0, 83, 177)}[mode][zone - 1]


class Zones:
    def __init__(self, path, root):
        self.rows = list(csv.DictReader(open(path), delimiter="\t"))
        self.root = root

    def find(self, pattern, key, vel):
        for z in self.rows:
            if (re.search(pattern, os.path.basename(z["sample"])) and int(z["key_lo"]) <= key <= int(z["key_hi"])
                    and int(z["vel_lo"]) <= vel <= int(z["vel_hi"])):
                return z
        raise SystemExit(f"no zone {pattern} key {key} vel {vel}")

    def voice(self, z, key, cat):
        n = os.path.basename(z["sample"])[3:-4]  # 1v_<Art>_<Mic>_<rest>.ncw
        art, mic, rest = n.split("_", 2)
        path = os.path.join(self.root, mic, "1st Violins", cat, art, rest + ".wav")
        rate = GROUP * float(z["tune"]) * 2 ** ((key - int(z["root"])) / 12)
        return load(path), rate, int(z["sample_start"]), float(z["zvol"])


def main():
    argv = sys.argv[1:]
    opt = {}
    for k in ("--od", "--lt-off", "--a-start", "--b-start", "--b-delay", "--lt-delay", "--r-delay", "--r2"):
        if k in argv:
            i = argv.index(k)
            opt[k] = float(argv[i + 1])
            del argv[i : i + 2]
    search = "--search" in argv
    if search:
        argv.remove("--search")
    if "--dump" in argv:
        i = argv.index("--dump")
        del argv[i : i + 2]
    kon_p, man_p, root, unit = argv[0], argv[1], argv[2], int(argv[3])
    kon = load(kon_p)
    row = [r for r in csv.DictReader(open(man_p), delimiter="\t") if int(r["idx"]) == unit][0]
    mode, vel, a, b, ioi = re.match(r"(LL|EX) v(\d+) (\d+)->(\d+) ioi(\d+)", row["description"]).groups()
    vel, a, b, ioi = int(vel), int(a), int(b), int(ioi)
    zone = 1 if vel <= 64 else (2 if vel <= 100 else 3)
    t1 = float(row["t_start"])  # first note-on
    t0 = t1 + ioi / 1000.0  # second note-on
    zt = Zones(os.path.join(root, "_ksp", "CSS 1st Violins", "zones.tsv"), root)
    interval = abs(b - a)
    lower = min(a, b)
    lt_vel = interval if b > a else interval + 12
    za = zt.find(r"_Nonvib_Mix_ff_", a, 1)
    zb = zt.find(r"_Nonvib_Mix_ff_", b, 1)
    zr = zt.find(r"_NVrel_Mix_ff_", a, 1)
    A, ra, sa, _ = zt.voice(za, a, "Sustain")
    B, rb, sb, _ = zt.voice(zb, b, "Sustain")
    R, rr, sr_, _ = zt.voice(zr, a, "Releases")
    if "--a-start" in opt:
        sa = int(opt["--a-start"])
    if "--b-start" in opt:
        sb = int(opt["--b-start"])
    LT = None
    if interval <= 12:
        zl = zt.find(r"_NVLeg_Mix_ff_", lower, lt_vel)
        LT, rl, sl, _ = zt.voice(zl, lower, "Legato")
        lt_name = os.path.basename(zl["sample"])
    on1, on0 = int(round(t1 * SR)), int(round(t0 * SR))
    od = opt.get("--od", od_ms(mode, zone, ioi))
    lt_off = opt.get("--lt-off", lt_base(mode, zone) + od)
    b_off = 0.0  # measured: Kontakt enters the >octave body at frame 0 (the KSP 75 ms is not applied)

    # Extra start delays in frames past t0 + OD (each KSP wait(1) may cost a
    # processing block): body after 4 waits, LT after 5, release +2 ms.
    bd = opt.get("--b-delay", 0.0)
    ld = opt.get("--lt-delay", 0.0)
    rd = opt.get("--r-delay", 0.002 * SR)

    def columns(n, od, lt_off):
        start = on0 + od * SR / 1000.0
        bs, ls_ = start + bd, start + ld
        cols = [read(A, sa + (n - on1) * ra)]
        cols.append(read(B, sb + b_off * SR / 1000 + (n - bs) * rb) * ((n >= bs)[:, None]))
        if LT is not None:
            cols.append(read(LT, sl + lt_off * SR / 1000 + (n - ls_) * rl) * ((n >= ls_)[:, None]))
        rs = start + rd
        cols.append(read(R, sr_ + (n - rs) * rr) * ((n >= rs)[:, None]))
        if "--r2" in opt:
            # A second release of a: its own key-up release, opt ms after t0.
            r2 = on0 + opt["--r2"] * SR / 1000.0
            cols.append(read(R, sr_ + (n - r2) * rr) * ((n >= r2)[:, None]))
        return cols

    def fit(n, od, lt_off):
        cols = columns(n, od, lt_off)
        M = np.stack([c.ravel() for c in cols], 1)
        t = kon[n].ravel()
        keep = [i for i in range(M.shape[1]) if np.dot(M[:, i], M[:, i]) > 1e-14]
        g = np.zeros(M.shape[1])
        if keep:
            sol, *_ = np.linalg.lstsq(M[:, keep], t, rcond=None)
            g[keep] = sol
        return g, np.sum((t - M @ g) ** 2), np.sum(t * t)

    W = int(0.01 * SR)
    span = range(on0 - 5 * W, on0 + int(1.4 * SR), W)
    if search:
        best = None
        for od_t in sorted({od, *range(0, 100, 5)}):
            for lo in np.arange(max(0, lt_off - 120), lt_off + 121, 10):
                e = sum(fit(np.arange(w, w + W), od_t, lo)[1] for w in span[::4])
                if best is None or e < best[0]:
                    best = (e, od_t, lo)
        _, od, lt_off = best
    names = ["A old", "B new"] + (["LT"] if LT is not None else []) + ["R rel"] + (["R2 keyup"] if "--r2" in opt else [])
    print(f"unit {unit}: {row['description']}  zone {zone}  OD {od} ms  LT {lt_name if LT is not None else '-'} off {lt_off:.0f} ms  B off {b_off:.0f} ms")
    print("   t ms  " + "  ".join(f"{n:>7}" for n in names) + "   residual")
    tot_e = tot_s = 0.0
    resid = [] if "--dump" in sys.argv else None
    for w0 in span:
        n = np.arange(w0, w0 + W)
        g, e, s = fit(n, od, lt_off)
        if resid is not None:
            cols = columns(n, od, lt_off)
            M = np.stack([c.ravel() for c in cols], 1)
            resid.append((kon[n].ravel() - M @ g).reshape(-1, 2))
        tot_e += e
        tot_s += s
        if s < 1e-14:
            continue
        print(f"{(w0 - on0) / SR * 1000:7.0f}  " + "  ".join(f"{x / FULL:7.3f}" for x in g) + f"   {db(e / s):6.1f}")
    print(f"whole-window residual {db(tot_e / tot_s):.1f} dB")
    if resid is not None:
        # The unexplained part, from 50 ms before t0, for residual hunts.
        np.save(sys.argv[sys.argv.index("--dump") + 1], np.concatenate(resid))


if __name__ == "__main__":
    main()
