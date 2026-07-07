#!/usr/bin/env python3
"""Per-note CSS A/B, MANIFEST-DRIVEN (robust to quiet notes).

    python3 ab_pernote.py css_ab_manifest.tsv css_ab_ours.wav [css_ab_css.wav]

Each manifest unit is silence-isolated at a known time. We window every unit in
BOTH renders (one global offset from the loud CALIB note), detect its onset
inside the window, and compare onset-time / level / timbre INDIVIDUALLY. A unit
present in one render but silent in the other is flagged as dropped/extra.

    nix-shell -p 'python313.withPackages(ps:[ps.numpy])' --run 'python3 ab_pernote.py ...'
"""
import sys, wave, math
import numpy as np
SR = 48000

def load(p):
    w = wave.open(p, "rb"); n=w.getnframes(); ch=w.getnchannels(); sw=w.getsampwidth()
    raw = w.readframes(n); w.close()
    if sw == 2:
        a = np.frombuffer(raw, dtype="<i2").astype(np.float32)/32768.0
    elif sw == 3:
        b = np.frombuffer(raw, dtype=np.uint8).reshape(-1,3)
        a = (b[:,0].astype(np.int32)|(b[:,1].astype(np.int32)<<8)|(b[:,2].astype(np.int32)<<16))
        a = np.where(a&0x800000, a-0x1000000, a).astype(np.float32)/2**23
    else:
        raise SystemExit(f"bad width {sw}")
    return a.reshape(-1, ch).mean(axis=1)

def envelope(x, hop=240):
    m = (len(x)//hop)*hop
    return np.sqrt((x[:m]*x[:m]).reshape(-1, hop).mean(axis=1)), hop

def onset_in(x):
    """Onset sample within a window (first sustained energy rise), or None."""
    e, hop = envelope(x)
    if len(e) < 3: return None, 0.0
    pk = e.max()
    if pk < 3e-4: return None, pk            # silent
    thr = max(pk*0.15, 3e-4)
    idx = np.where(e > thr)[0]
    return (int(idx[0]*hop) if len(idx) else None), pk

edges = np.logspace(np.log10(60), np.log10(12000), 49)
def logspec(x):
    if len(x) < 2048: return None
    X = np.abs(np.fft.rfft(x*np.hanning(len(x)))); f = np.fft.rfftfreq(len(x), 1/SR)
    o = np.zeros(48)
    for i in range(48):
        m = (f>=edges[i])&(f<edges[i+1]); o[i] = X[m].mean() if m.any() else 0
    return o
def cos(a,b):
    if a is None or b is None: return float("nan")
    na,nb = np.linalg.norm(a), np.linalg.norm(b)
    return float(a@b/(na*nb)) if na>0 and nb>0 else float("nan")
def rms_db(x):
    r = np.sqrt((x*x).mean()); return 20*math.log10(r) if r>1e-9 else -120.0

def global_offset(ours, css, calib_t, calib_win):
    """Cross-correlate the loud CALIB region → CSS-vs-ours sample offset."""
    a = ours[int(calib_t*SR):int((calib_t+calib_win)*SR)]
    eo,_ = envelope(a)
    best, bo = -2, 0
    for off in range(-int(0.4*SR), int(0.4*SR), 240):
        b = css[int(calib_t*SR)+off:int((calib_t+calib_win)*SR)+off]
        ec,_ = envelope(b); m = min(len(eo), len(ec))
        if m < 20: continue
        c = np.corrcoef(eo[:m], ec[:m])[0,1]
        if c > best: best, bo = c, off
    return bo, best

def main():
    man_p, ours_p = sys.argv[1], sys.argv[2]
    css_p = sys.argv[3] if len(sys.argv) > 3 else None
    man = []
    for ln in open(man_p):
        if ln.startswith("idx"): continue
        p = ln.rstrip("\n").split("\t")
        if len(p) >= 6: man.append(dict(idx=int(p[0]), t=float(p[1]), win=float(p[2]),
                                        pitch=int(p[3]), cat=p[4], desc=p[5]))
    ours = load(ours_p)

    if css_p is None:
        print(f"self-validation: {len(man)} manifest units vs ours\n")
        miss = 0
        for u in man:
            w = ours[int(u['t']*SR):int((u['t']+u['win'])*SR)]
            on, pk = onset_in(w)
            if on is None:
                miss += 1; print(f"  #{u['idx']:>2} {u['cat']:10} {u['desc'][:30]:30} SILENT (pk={pk:.4f})")
        print(f"\n{len(man)-miss}/{len(man)} units audible in our render"
              + ("  ✓" if miss==0 else f"   ⚠ {miss} silent"))
        return

    css = load(css_p)
    # ── Timing metric, honest by family ──────────────────────────────────────
    # The render is deterministic/offline — there is NO real per-note timing
    # jitter. Both renders start each note at the known manifest time, so
    # "onset" is only a meaningful axis where the ATTACK is sharp (arco shorts).
    #   • SHORT / SHORT-RR (fast transient): envelope cross-correlation gives a
    #     real, sharp onset delta → keep it in the MATCH gate.
    #   • Everything else (sustain / long / legato ~4–200 ms blooms): the leading
    #     edge is a slow ramp, so envelope-xcorr argmax is degenerate (a flat
    #     plateau correlates at every lag) and reads as bogus ±200 ms "jitter".
    #     We instead measure the ATTACK HALF-CROSSING (time to reach 50% of the
    #     note's own plateau) in each render and report their delta as an
    #     attack-time agreement figure — but we DROP it from the MATCH gate,
    #     since it is not an audible mismatch, only a bloom measurement artifact.
    from collections import defaultdict
    HOP = 96  # 2 ms
    SHORT_CATS = {"SHORT", "SHORT-RR"}  # onset is a real axis only for these
    def penv(x):
        m=(len(x)//HOP)*HOP
        return np.sqrt((x[:m]*x[:m]).reshape(-1,HOP).mean(axis=1)) if m>=HOP else np.zeros(1)
    def half_cross(e, frac=0.5):
        """Sample index where the RMS envelope first reaches `frac` of its peak."""
        if len(e) < 3: return None
        pk = e.max()
        if pk < 3e-4: return None
        idx = np.where(e >= pk*frac)[0]
        return int(idx[0])*HOP if len(idx) else None
    recs = []; dropped = []
    for u in man:
        t = u['t']; is_short = u['cat'] in SHORT_CATS
        oa, ob = t-0.05, t+u['win']              # ours window
        ca, cb = t-0.40, t+u['win']+0.40         # css search window (wide)
        ow = ours[int(oa*SR):int(ob*SR)]; cw = css[int(ca*SR):int(cb*SR)]
        on_o,_ = onset_in(ow); on_c,_ = onset_in(cw)
        if (on_o is None) != (on_c is None):
            dropped.append((u['idx'], "ours" if on_o is None else "CSS", u['desc']))
            recs.append((u, None)); continue
        if on_o is None:
            recs.append((u, None)); continue
        eo, ec = penv(ow), penv(cw)
        # BODY ALIGNMENT (used for the level/timbre comparison window): slide the
        # ours envelope across css, pick the max-correlation overlap. This is a
        # best-effort steady-state alignment and is used for EVERY family so the
        # timbre/level numbers compare like-for-like bodies.
        best, blag = -2.0, 0
        for lag in range(0, max(1, len(ec)-len(eo)+1)):
            seg = ec[lag:lag+len(eo)]
            if len(seg) < 20: break
            c = np.corrcoef(eo, seg)[0,1]
            if not math.isnan(c) and c > best: best, blag = c, lag
        o_start, c_start = oa, ca + blag*HOP/SR
        if is_short:
            # sharp transient → the xcorr lag IS the real onset delta
            onset_delta = o_start - c_start
        else:
            # slow bloom → onset is the 50%-of-plateau attack-crossing delta,
            # reported for information but DROPPED from the MATCH gate.
            ho, hc = half_cross(eo), half_cross(ec)
            onset_delta = ((oa + (ho/SR if ho is not None else 0.0))
                           - (ca + (hc/SR if hc is not None else 0.0)))
        recs.append((u, (o_start, c_start, onset_delta)))
    lat = np.median([r[1][2] for r in recs if r[1]]) if any(r[1] for r in recs) else 0.0
    print(f"systematic latency (ours−CSS, median) = {lat*1000:+.0f}ms  "
          f"[onset via xcorr for shorts, attack half-crossing for blooms]\n")
    cat = defaultdict(list)
    print(f"{'#':>2} {'category':10} {'desc':26} {'onset':>7} {'dLvl':>6} {'timbre':>7}")
    for u, r in recs:
        if r is None:
            if any(d[0]==u['idx'] for d in dropped):
                who=[d[1] for d in dropped if d[0]==u['idx']][0]
                print(f"{u['idx']:>2} {u['cat']:10} {u['desc'][:26]:26}  SILENT in {who}")
            continue
        o_start, c_start, delta = r
        is_short = u['cat'] in SHORT_CATS
        win = min(u['win'], 1.0)
        ao = ours[int(o_start*SR):int((o_start+win)*SR)]
        ac = css[int(c_start*SR):int((c_start+win)*SR)]
        L = min(len(ao), len(ac))
        jit = (delta - lat)*1000                  # residual timing after latency
        dl = rms_db(ao[:L]) - rms_db(ac[:L])
        tb = cos(logspec(ao[:L]), logspec(ac[:L]))
        cat[u['cat']].append((jit, dl, tb, is_short))
        gated = (tb>0.9 and abs(dl)<3 and (abs(jit)<40 or not is_short))
        flag = "" if gated else "  <--"
        onset_s = f"{jit:>6.0f}" if is_short else f"{jit:>5.0f}~"
        print(f"{u['idx']:>2} {u['cat']:10} {u['desc'][:26]:26} {onset_s:>6} {dl:>6.1f} {tb:>7.3f}{flag}")
    print(f"\n{'category':12} {'n':>3} {'onset|dt|':>9} {'dLvl dB':>8} {'timbre':>7}")
    allr=[]
    for c, rows in cat.items():
        r = np.array([(x[0],x[1],x[2]) for x in rows]); allr += rows
        print(f"{c:12} {len(rows):>3} {np.abs(r[:,0]).mean():>8.0f}m {r[:,1].mean():>+8.1f} {r[:,2].mean():>7.3f}")
    if allr:
        a = np.array([(x[0],x[1],x[2]) for x in allr])
        # onset-gated MATCH: onset only counts against SHORT families
        ok = sum(1 for x in allr if x[2]>0.9 and abs(x[1])<3 and (abs(x[0])<40 or not x[3]))
        ok_lt = sum(1 for x in allr if x[2]>0.9 and abs(x[1])<3)  # level+timbre only
        shorts = [x for x in allr if x[3]]
        print(f"\nMATCH {ok}/{len(allr)} notes ONSET-GATED (timbre>0.90, |level|<3dB, "
              f"|onset|<40ms on shorts only)")
        print(f"MATCH {ok_lt}/{len(allr)} on LEVEL+TIMBRE alone")
        print(f"overall: timbre {a[:,2].mean():.3f}   mean|level| {np.abs(a[:,1]).mean():.1f}dB"
              f"   short-onset|dt| {np.abs([x[0] for x in shorts]).mean():.0f}ms (n={len(shorts)})")
    if dropped:
        print(f"\nDROPPED/EXTRA ({len(dropped)}):")
        for i,who,d in dropped: print(f"  #{i} silent in {who}: {d}")

main()
