#!/usr/bin/env python3
"""Explain one Kontakt MARCATO transition as its voices, window by window.

    python3 marcato_fit.py <kontakt.wav> <manifest.tsv> <css_root> <unit>
                           [--a-rr N] [--b-rr N] [--b-off MS] [--lt-off MS] [--search]

Units from `gen_probe.py --marcato` ("MC v40 67->69 ioi1000"; CC1=127, so only
the fff marcato groups sound). Voices (KSP decode §2.2):

  A   old body   Marcato fff of a, round-robin --a-rr (1/2), from sample 0
  B   new body   Marcato fff of b, round-robin --b-rr, entered --b-off ms in
                 (decoded: 60 soft / 20 hard; 10 / 0 beyond an octave)
  LT  transition the `marc leg fff` zone: Leg ff of the LOWER note,
                 interval-coded, from its zone start (3984) + --lt-off ms
                 (decoded: 177 below 100 ms IOI → 117 from 500 ms)
  R   in-phrase release of a (Marcrel fff)

All start at the second note-on (marcato has no overlap delay). Gains are
solved per 10 ms window; --search grids B/LT offsets first.

--model predicts the whole transition with FIXED gains instead (the
engine's model, null-tested 2026-11-12: −34 to −43 dB on all 12 units):

  A   linear fade_out 225 ms × a sink toward 1 − 0.5946 from 45 ms over
      270 ms (CC19 leaving 127 drops the CC19=127 step mod; flat across
      bands, so a gain), cut at 225; glide 0 → +J cents over 60 ticks
  B   fade_in stage 1 (225·igmiu/x444h ms) then, at 225·igmiu/100 ticks,
      stage 2 from where stage 1 is to full over 225·(100−igmiu)/100 ms;
      × the body flex envelopes with CC19 → level 1 and CC18 → levels 1-2
      (`marc_ccs`); glide −J → 0 over 60 ticks
  LT  the zone's own level × group +0.04 dB × "LT Vol" (−9/−1.5 dB below
      150 ms IOI → 0 dB at 300) × the marc-leg flex envelope
  R   −8 dB × the release AHD (250 ms) × release flex, at +2 ms

Flex segments use the shape law a = (0.5 − tension)·15 (FLEXK).
Debug hooks: FREE_EARLY=1|all (per-5 ms free gains next to the model),
BANDS=1 (per-band A ratio), SAVE_RES=<.npy> (the residual, for hunting).
"""
import csv, os, re, sys
import numpy as np

sys.path.insert(0, os.path.dirname(__file__))
from null_test import SR, db, load  # noqa: E402
from release_fit import read  # noqa: E402
from legato_fit import GROUP, FULL, ni_name, Zones  # noqa: E402


def main():
    argv = sys.argv[1:]
    opt = {}
    for k in ("--a-rr", "--b-rr", "--b-off", "--lt-off"):
        if k in argv:
            i = argv.index(k)
            opt[k] = float(argv[i + 1])
            del argv[i : i + 2]
    search = "--search" in argv
    model = "--model" in argv
    if model:
        argv.remove("--model")
    stage2 = "restart" if "--stage2-restart" in argv else "continue"
    if "--stage2-restart" in argv:
        argv.remove("--stage2-restart")
    if search:
        argv.remove("--search")
    kon_p, man_p, root, unit = argv[0], argv[1], argv[2], int(argv[3])
    kon = load(kon_p)
    row = [r for r in csv.DictReader(open(man_p), delimiter="\t") if int(r["idx"]) == unit][0]
    vel, a, b, ioi = map(int, re.match(r"MC v(\d+) (\d+)->(\d+) ioi(\d+)", row["description"]).groups())
    hard = vel > 75 and ioi > 50
    t1 = float(row["t_start"])
    t0 = t1 + ioi / 1000.0
    on1, on0 = int(round(t1 * SR)), int(round(t0 * SR))
    zt = Zones(os.path.join(root, "_ksp", "CSS 1st Violins", "zones.tsv"), root)
    interval = abs(b - a)
    lower = min(a, b)
    # Round-robin alternates 1/2 per key (zone) from the CC59 reset at the top.
    uses = {}
    for r in csv.DictReader(open(man_p), delimiter="\t"):
        if int(r["idx"]) >= unit:
            break
        m_ = re.match(r"MC v\d+ (\d+)->(\d+)", r["description"])
        for key in (map(int, m_.groups()) if m_ else [int(r["pitch"])]):
            uses[key] = uses.get(key, 0) + 1
    a_rr = int(opt.get("--a-rr", uses.get(a, 0) % 2 + 1))
    uses[a] = uses.get(a, 0) + 1
    b_rr = int(opt.get("--b-rr", uses.get(b, 0) % 2 + 1))

    def marc(key, rr):
        z = zt.find(r"_Marcato_Mix_fff_", key, rr)
        return zt.voice(z, key, "Short")

    A, ra, sa, _ = marc(a, a_rr)
    B, rb, sb, _ = marc(b, b_rr)
    R, rr, sr_, _ = zt.voice(zt.find(r"_Marcrel_Mix_fff_", a, 1), a, "Releases")
    LT, sl, rl, lt_zvol = None, 0, 1.0, 1.0
    if interval <= 12:
        code = interval if b > a else interval + 12
        # The marc-leg fff zones: Leg ff, start 3984, the LAST of the marc-leg
        # zone sets (mf, f, fff in zone order); keyed at the lower note,
        # interval-coded; each zone carries its own level (1.2599, 1.4142, …).
        cands = [z for z in zt.rows if "_Leg_Mix_ff_" in z["sample"] and z["sample_start"] == "3984"
                 and int(z["key_lo"]) <= lower <= int(z["key_hi"]) and int(z["vel_lo"]) <= code <= int(z["vel_hi"])]
        if cands:
            cands = [max(cands, key=lambda z: int(z["idx"]))]
            LT, rl, sl, lt_zvol = zt.voice(cands[0], lower, "Legato")
            lt_name = os.path.basename(cands[0]["sample"])
    b_off = opt.get("--b-off", (10.0 if not hard else 0.0) if interval > 12 else (20.0 if hard else 60.0))
    # KSP integer lerp: 177 below 150 ms IOI → 117 at 500 (19815-19840).
    lt_off = opt.get("--lt-off", 177.0 if ioi < 150 else (117.0 if ioi >= 500 else
                     177 + int(-60 * ((ioi - 150) * 10000 // 350) / 10000)))

    def columns(n, b_off, lt_off):
        cols = [read(A, sa + (n - on1) * ra)]
        cols.append(read(B, sb + b_off * SR / 1000 + (n - on0) * rb) * ((n >= on0)[:, None]))
        if LT is not None:
            cols.append(read(LT, sl + lt_off * SR / 1000 + (n - on0) * rl) * ((n >= on0)[:, None]))
        rs = on0 + 0.002 * SR
        cols.append(read(R, sr_ + (n - rs) * rr) * ((n >= rs)[:, None]))
        return cols

    def fit(n, b_off, lt_off):
        M = np.stack([c.ravel() for c in columns(n, b_off, lt_off)], 1)
        t = kon[n].ravel()
        keep = [i for i in range(M.shape[1]) if np.dot(M[:, i], M[:, i]) > 1e-14]
        g = np.zeros(M.shape[1])
        if keep:
            sol, *_ = np.linalg.lstsq(M[:, keep], t, rcond=None)
            g[keep] = sol
        return g, np.sum((t - M @ g) ** 2), np.sum(t * t)

    W = int(0.01 * SR)
    if model:
        global zt_
        zt_ = zt
        return run_model(kon, on1, on0, A, sa, ra, B, sb, rb, LT, sl, rl, R, sr_, rr, b_off, lt_off,
                         vel, ioi, interval, hard, stage2, a, b, lt_zvol)
    span = range(on0 - 5 * W, on0 + int(1.4 * SR), W)
    if search:
        best = None
        for bo in range(0, 130, 10):
            for lo in range(60, 260, 10):
                e = sum(fit(np.arange(w, w + W), bo, lo)[1] for w in span[::4])
                if best is None or e < best[0]:
                    best = (e, bo, lo)
        _, b_off, lt_off = best
    names = ["A old", "B new"] + (["LT"] if LT is not None else []) + ["R rel"]
    print(f"unit {unit}: {row['description']}  hard {hard}  A rr{a_rr} B rr{b_rr} off {b_off:.0f} ms  "
          f"LT {lt_name if LT is not None else '-'} off {lt_off:.0f} ms")
    print("   t ms  " + "  ".join(f"{x:>7}" for x in names) + "   residual")
    te = ts = 0.0
    for w0 in span:
        n = np.arange(w0, w0 + W)
        g, e, s = fit(n, b_off, lt_off)
        te += e
        ts += s
        if s > 1e-14:
            print(f"{(w0 - on0) / SR * 1000:7.0f}  " + "  ".join(f"{x / FULL:7.3f}" for x in g) + f"   {db(e / s):6.1f}")
    print(f"whole-window residual {db(te / ts):.1f} dB")


def shape(t, tension):
    a = (0.5 - tension) * float(os.environ.get("FLEXK", 15.0))
    return t if abs(a) < 1e-3 else (np.exp(a * t) - 1.0) / (np.exp(a) - 1.0)


def flex(t_ms, segs, start=0.0):
    """Kontakt ENV_FLEX level at t_ms (array) from (ms, level, tension) segments."""
    out = np.full(t_ms.shape, segs[-1][1], dtype=float)
    acc, frm = 0.0, start
    done = np.zeros(t_ms.shape, bool)
    for ms, lvl, ten in segs:
        m = (~done) & (t_ms < acc + ms)
        u = np.clip((t_ms[m] - acc) / ms, 0, 1)
        out[m] = frm + (lvl - frm) * shape(u, ten)
        done |= m
        acc += ms
        frm = lvl
    return out


def ksp_lerp(x, xs, ys):
    """The script's integer lerp over breakpoints (flat outside)."""
    if x < xs[0]:
        return ys[0]
    for i in range(len(xs) - 1):
        if x < xs[i + 1]:
            l, span = x - xs[i], xs[i + 1] - xs[i]
            d = ys[i + 1] - ys[i]
            return ys[i] + int(d * (l * 10000 // span) / 10000)
    return ys[-1]


def bend_cents(ioi, interval):
    """$ruv02: glide depth (cents) by interval 1 / 2 / >2 over IOI 75/100/500."""
    row = (40, 10, 10) if interval == 1 else ((30, 10, 0) if interval == 2 else (20, 0, 0))
    return 0 if interval == 0 else ksp_lerp(ioi, (75, 100, 500), row)


def bent(n, n0, start, rate, cents_of_ms):
    """Read positions for a voice whose tune follows cents_of_ms(t) from n0."""
    t = (n - n0) / SR * 1000.0
    step = rate * 2 ** (cents_of_ms(t) / 1200.0)
    pos = start + np.concatenate([[0.0], np.cumsum(step[:-1])])
    return pos


def lerp_tab(x, xs, ys):
    return float(np.interp(x, xs, ys))


def marc_ccs(ioi, interval, hard):
    """CC19 ($313em) and CC18 ($eaghu) the script sets at a marcato transition."""
    if hard:
        return (96 if interval > 12 else 100), (127 if interval > 12 else 105)
    if interval > 12:
        return 0, 52
    cc19 = int(lerp_tab(ioi, (150, 300, 500), (0, 60, 0)))
    cc18 = int(lerp_tab(ioi, (150, 300, 500), (96 if interval > 2 else 127, 52, 52)))
    return cc19, cc18


def run_model(kon, on1, on0, A, sa, ra, B, sb, rb, LT, sl, rl, R, sr_, rr, b_off, lt_off,
              vel, ioi, interval, hard, stage2, a, b, lt_zvol):
    n = np.arange(on0 - int(0.05 * SR), on0 + int(1.8 * SR))
    t = (n - on0) / SR * 1000.0
    after = t >= 0
    igmiu = 60 if hard else 50
    x444h = lerp_tab(ioi, (150, 300), (90, 60))
    xt = 225.0
    s1 = xt * igmiu / x444h  # stage-1 fade-in ms
    s2 = xt * (100 - igmiu) / 100.0
    s2_at = int(xt * igmiu / 100) - int(os.environ.get("S2M1", 0))  # integer ticks
    fade_a = np.where(after, np.clip(1 - t / xt, 0, 1), 1.0)
    # Old body sinks toward 1 − 0.5946 (the CC19=127 step mod's depth) once
    # CC19 leaves 127: measured, flat across bands (a gain, not the filter).
    a_on = float(os.environ.get("A_ON", 45.0))
    a_len = float(os.environ.get("A_LEN", 270.0))
    fade_a = fade_a * (1 - 0.5946 * np.clip((t - a_on) / a_len, 0, 1))
    g1 = np.clip(t / s1, 0, 1)
    if stage2 == "restart":
        fin = np.where(t < s2_at, g1, np.clip((t - s2_at) / s2, 0, 1))
    else:
        v0 = min(s2_at / s1, 1.0)
        fin = np.where(t < s2_at, g1, np.clip(v0 + (1 - v0) * (t - s2_at) / s2, 0, 1))
    cc19, cc18 = marc_ccs(ioi, interval, hard)
    l19, l18 = cc19 / 127.0, cc18 / 127.0
    f1 = flex(t, [(1, l19, 0.505), (299, 1, 0.67), (1200, 1, 0.5)])
    T2 = float(os.environ.get("F2T", 0.67))
    TL = float(os.environ.get("LTT", 0.33))
    f2 = flex(t, [(1, l18, 0.5), (559, l18, 0.505), (1440, 1, T2), (484, 1, 0.5)])
    gb = np.where(after, fin * f1 * f2, 0.0)
    gl = np.zeros_like(t)
    if LT is not None:
        lt_db = lerp_tab(ioi, (150, 300), (-9.0 if interval <= 2 else -1.5, 0.0))
        gl = np.where(after, lt_zvol * 1.0042866 * 10 ** (lt_db / 20)
                      * flex(t, [(68, 0.49315068, 0.499), (492, 1, 0.72), (1440, 0, TL)]), 0.0)
    tr = t - 2.0
    gr = np.where(tr >= 0, 10 ** (-8.0 / 20) * flex(tr, [(250, 1, 0.675), (25000, 0.25, 0.808)])
                  * flex(tr, [(1, 0.98630136, 0.125), (4007, 1, 0.9)]), 0.0)
    J = bend_cents(ioi, interval) * (1 if b > a else -1)
    tick = lambda tt: np.clip(np.floor(tt) + 1, 0, 60)
    cents_a = lambda tt: np.where(tt >= 0, J * tick(tt) / 60.0, 0.0)
    # Incoming voice from −J back to 0, outgoing 0 → +J, over 60 one-ms ticks
    # (20278-20291); the read position is anchored at the onset.
    cents_b = lambda tt: np.where(tt >= 0, -J * (60 - tick(tt)) / 60.0, 0.0)
    pa = read(A, bent(n, on0, sa + (n[0] - on1) * ra, ra, cents_a)) * (fade_a * (t < xt))[:, None]
    pb = read(B, bent(n, on0, sb + b_off * SR / 1000 + (n[0] - on0) * rb, rb, cents_b)) * gb[:, None]
    pl = read(LT, sl + lt_off * SR / 1000 + (n - on0) * rl) * gl[:, None] if LT is not None else 0
    pr = read(R, sr_ + (n - on0 - 0.002 * SR) * rr) * gr[:, None]
    pred = FULL * (pa + pb + pl + pr)
    k = kon[n]
    if os.environ.get("SAVE_RES"):
        res = np.zeros_like(kon)
        res[n] = k - pred
        np.save(os.environ["SAVE_RES"], res)
    if os.environ.get("NOTEOFF"):
        # Kontakt's end of the phrase note b (note-off 2.0 s after t0): free
        # gains of B, LT and b's release per 10 ms window.
        off = on0 + int(2.0 * SR)
        Rb, rrb, srb, _ = zt_.voice(zt_.find(r"_Marcrel_Mix_fff_", b, 1), b, "Releases")
        w10 = int(0.01 * SR)
        for w in range(off - 3 * w10, off + int(0.7 * SR), 2 * w10):
            m = np.arange(w, w + w10)
            tt = (m - on0) / SR * 1000.0
            cols = [read(B, bent(m, on0, sb + b_off * SR / 1000 + (m[0] - on0) * rb, rb, lambda q: 0 * q)).ravel(),
                    read(Rb, srb + (m - off - 0.002 * SR) * rrb).ravel() * (m >= off + 0.002 * SR).repeat(2)]
            if LT is not None:
                cols.append(read(LT, sl + lt_off * SR / 1000 + (m - on0) * rl).ravel())
            M = np.stack(cols, 1) * FULL
            ok = [i for i in range(M.shape[1]) if np.dot(M[:, i], M[:, i]) > 1e-12]
            g = np.zeros(M.shape[1])
            g[ok], *_ = np.linalg.lstsq(M[:, ok], kon[m].ravel(), rcond=None)
            res = kon[m].ravel() - M @ g
            print(f"off+{(w - off) / SR * 1000:6.0f}  B {g[0]:6.3f}  Rb {g[1]:6.3f}" + (f"  LT {g[2]:6.3f}" if LT is not None else "")
                  + f"   res {db(np.sum(res**2) / (np.sum(kon[m]**2) + 1e-30)):6.1f}")
        return
    if os.environ.get("BANDS"):
        rest = k - FULL * (pb + pl + pr)
        ua = read(A, sa + (n - on1) * ra) * FULL * (fade_a * (t < xt))[:, None]
        for lo_ms, hi_ms in ((-40, 0), (0, 40), (40, 80), (80, 120), (120, 160), (160, 200)):
            m = (t >= lo_ms) & (t < hi_ms)
            X = np.fft.rfft(ua[m].sum(1) * np.hanning(m.sum()))
            Y = np.fft.rfft(rest[m].sum(1) * np.hanning(m.sum()))
            f = np.fft.rfftfreq(m.sum(), 1 / SR)
            row = []
            for b0, b1 in ((100, 400), (400, 800), (800, 1600), (1600, 3200), (3200, 6400), (6400, 12800)):
                bm = (f >= b0) & (f < b1)
                row.append(np.sqrt(np.sum(np.abs(Y[bm]) ** 2) / (np.sum(np.abs(X[bm]) ** 2) + 1e-30)))
            print(f"{lo_ms:5d}..{hi_ms:<5d} " + " ".join(f"{v:6.3f}" for v in row))
        return
    if os.environ.get("FREE_EARLY"):
        allfree = os.environ.get("FREE_EARLY") == "all"
        rest = k if allfree else k - FULL * (pl + pr)
        cb = float(os.environ.get("ABEND", 0))
        bend = np.clip((n - on0) / (0.06 * SR), 0, 1) * cb
        pos_a = sa + (on0 - on1) * ra + np.concatenate([[0], np.cumsum(ra * 2 ** (bend[:-1] / 1200))]) - (on0 - n[0]) * ra * 0
        pos_a = sa + (n - on1) * ra + np.cumsum(np.concatenate([[0], ra * (2 ** (bend[:-1] / 1200) - 1)]))
        ua = read(A, pos_a) * FULL
        ub = read(B, sb + b_off * SR / 1000 + (n - on0) * rb) * FULL
        w5 = int(0.005 * SR)
        for w in range(0, int(0.3 * SR) + int(0.05 * SR), w5):
            sl_ = slice(w, w + w5)
            cols = [ua[sl_].ravel(), ub[sl_].ravel()]
            if allfree:
                cols += [(FULL * read(LT, sl + lt_off * SR / 1000 + (n - on0) * rl))[sl_].ravel(),
                         (FULL * read(R, sr_ + (n - on0 - 0.002 * SR) * rr))[sl_].ravel()]
            M = np.stack(cols, 1)
            ok = [i for i in range(M.shape[1]) if np.dot(M[:, i], M[:, i]) > 1e-12]
            g = np.zeros(M.shape[1])
            g[ok], *_ = np.linalg.lstsq(M[:, ok], rest[sl_].ravel(), rcond=None)
            c = w + w5 // 2
            mb = gb[c]
            ma = fade_a[c] * (t[c] < xt)
            extra = f"  LT {g[2]:6.3f} ({gl[c]:5.3f})  R {g[3]:6.3f} ({gr[c]:5.3f})" if allfree else ""
            ra_ = g[0] / ma if ma > 0.02 else float("nan")
            print(f"{t[w] + 2.5:7.1f}  A/model {ra_:5.3f}  A {g[0]:6.3f} (model {ma:5.3f})  B {g[1]:6.3f} (model {mb:5.3f})" + extra)
        return
    print(f"model: bend {J} c  cc19 {cc19} cc18 {cc18} stage1 {s1:.1f} ms stage2 {s2:.1f} ms @ {s2_at}  ({stage2})")
    W = int(0.02 * SR)
    for w in range(0, len(n), W):
        e = np.sum((k[w:w + W] - pred[w:w + W]) ** 2)
        s = np.sum(k[w:w + W] ** 2) + 1e-30
        if (w // W) % 3 == 0:
            print(f"{t[w]:7.0f}  {db(e / s):6.1f}")
    early = after & (t < 250)
    print(f"early residual {db(np.sum((k[early] - pred[early]) ** 2) / np.sum(k[early] ** 2)):.1f} dB")
    late = t >= 600
    print(f"late residual {db(np.sum((k[late] - pred[late]) ** 2) / np.sum(k[late] ** 2)):.1f} dB")
    print(f"model residual {db(np.sum((k - pred) ** 2) / np.sum(k * k)):.1f} dB")


if __name__ == "__main__":
    main()
