#!/usr/bin/env python3
"""Per-note NULL TEST: our render vs a real Kontakt render of the same MIDI.

    python3 null_test.py <manifest.tsv> <ours.wav> <kontakt.wav> [kontakt_run2.wav] [--json out.json]

For every manifest unit (silence-isolated, see gen_css_ab.rs) both renders are
windowed at the SAME frames — no global alignment, no onset detection, no
mono downmix: both channels count. Reported per unit:

  strict    residual energy / Kontakt energy with NO adjustment. This is the
            number that has to reach -60..-100 dB; the others only explain it.
  lag       the integer sample shift (ours later = +) that minimises the
            residual, searched over +-SEARCH_MS.
  @lag      residual at that lag (timing removed).
  gain      least-squares gain of ours onto Kontakt at that lag, in dB.
  @lag+g    residual at that lag and gain (timing and level removed): what is
            left is shape — envelope, interpolation, wrong sample, mic mix.

With a second Kontakt render, a unit whose run-to-run residual is below
DET_DB is DETERMINISTIC (Kontakt drew no random choice); only those are held
to the null target. The others need the drawn round-robin identified first.

Units are judged on the strict number only. A unit that "matches" after lag
and gain but not strictly does NOT match.
"""
import csv, json, sys, wave
import numpy as np

SR = 48000
SEARCH_MS = 400
DET_DB = -100.0  # run-to-run residual below this = Kontakt was deterministic
TARGET_DB = -60.0


def load(path):
    w = wave.open(path, "rb")
    n, ch, sw, sr = w.getnframes(), w.getnchannels(), w.getsampwidth(), w.getframerate()
    raw = w.readframes(n)
    w.close()
    if sr != SR:
        raise SystemExit(f"{path}: {sr} Hz, expected {SR}")
    if sw == 2:
        a = np.frombuffer(raw, dtype="<i2").astype(np.float64) / 32768.0
    elif sw == 3:
        b = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        a = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16)
        a = (np.where(a & 0x800000, a - 0x1000000, a)).astype(np.float64) / 2**23
    elif sw == 4:  # 32-bit integer PCM (render_css_live writes this)
        a = np.frombuffer(raw, dtype="<i4").astype(np.float64) / 2**31
    else:
        raise SystemExit(f"{path}: unsupported sample width {sw}")
    a = a.reshape(-1, ch)
    if ch == 1:
        a = np.repeat(a, 2, axis=1)
    return a[:, :2]


def db(x):
    return 10.0 * np.log10(max(x, 1e-30))


def window(a, s, e):
    out = np.zeros((e - s, 2))
    lo, hi = max(s, 0), min(e, len(a))
    if hi > lo:
        out[lo - s : hi - s] = a[lo:hi]
    return out


def best_lag(ref, x, max_lag):
    """Lag (x later = +) maximising cross-correlation summed over channels."""
    n = len(ref) + len(x)
    nfft = 1 << (n - 1).bit_length()
    acc = np.zeros(nfft)
    for c in range(2):
        acc += np.fft.irfft(np.fft.rfft(x[:, c], nfft) * np.conj(np.fft.rfft(ref[:, c], nfft)), nfft)
    lags = np.concatenate([np.arange(0, max_lag + 1), np.arange(-max_lag, 0)])
    vals = np.concatenate([acc[: max_lag + 1], acc[nfft - max_lag :]])
    return int(lags[int(np.argmax(vals))])


def shift(x, lag):
    """Move x EARLIER by lag frames (undo a +lag delay)."""
    out = np.zeros_like(x)
    if lag >= 0:
        out[: len(x) - lag] = x[lag:]
    else:
        out[-lag:] = x[: len(x) + lag]
    return out


def residual(ref, x):
    return db(np.sum((ref - x) ** 2) / max(np.sum(ref**2), 1e-30))


def score(rows, ours, kon, kon2=None, show=print):
    """Score every manifest unit; `show` gets the table and summary lines.
    Returns the per-unit dicts (what --json writes) and the summary lines."""
    max_lag = SEARCH_MS * SR // 1000
    show(f"{'#':>2} {'category':9} {'desc':28} {'det':>4} {'strict':>8} {'lag':>7} {'@lag':>7} {'gain':>6} {'@lag+g':>7}")
    out = []
    for r in rows:
        s = int(round(float(r["t_start"]) * SR))
        e = s + int(round(float(r["win"]) * SR))
        k = window(kon, s, e)
        o = window(ours, s, e)
        ek = np.sum(k**2)
        if ek < 1e-12:
            show(f"{r['idx']:>2} {r['category']:9} {r['description'][:28]:28}  Kontakt silent")
            continue
        det = None
        if kon2 is not None:
            det = bool(residual(k, window(kon2, s, e)) < DET_DB)
        strict = residual(k, o)
        lag = best_lag(k, o, max_lag)
        ol = shift(o, lag)
        at_lag = residual(k, ol)
        g = float(np.sum(k * ol) / max(np.sum(ol**2), 1e-30))
        at_lag_g = residual(k, g * ol)
        gdb = 20 * np.log10(abs(g)) if g != 0 else -999
        flag = "" if strict <= TARGET_DB else "  <--"
        dets = "-" if det is None else ("DET" if det else "rnd")
        show(f"{r['idx']:>2} {r['category']:9} {r['description'][:28]:28} {dets:>4} {strict:8.1f} {lag/SR*1000:6.1f}m {at_lag:7.1f} {gdb:6.1f} {at_lag_g:7.1f}{flag}")
        out.append(dict(idx=int(r["idx"]), category=r["category"], desc=r["description"], deterministic=det,
                        strict_db=strict, lag_frames=lag, at_lag_db=at_lag, gain_db=gdb, at_lag_gain_db=at_lag_g))

    show("")
    summary = []
    for label, sel in (("deterministic", [u for u in out if u["deterministic"]]),
                       ("random", [u for u in out if u["deterministic"] is False]),
                       ("all", out)):
        if not sel:
            continue
        st = np.array([u["strict_db"] for u in sel])
        lg = np.array([u["at_lag_gain_db"] for u in sel])
        ok = int(np.sum(st <= TARGET_DB))
        line = (f"{label:14} n={len(sel):3}  strict median {np.median(st):7.1f} dB  worst {st.max():7.1f}  "
                f"@lag+gain median {np.median(lg):7.1f}  NULLED (<= {TARGET_DB:.0f} dB) {ok}/{len(sel)}")
        summary.append(line)
        show(line)
    return out, summary


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    jpath = sys.argv[sys.argv.index("--json") + 1] if "--json" in sys.argv else None
    if jpath in args:
        args.remove(jpath)
    if len(args) < 3:
        raise SystemExit(__doc__)
    rows = list(csv.DictReader(open(args[0]), delimiter="\t"))
    ours, kon = load(args[1]), load(args[2])
    kon2 = load(args[3]) if len(args) > 3 else None
    out, _ = score(rows, ours, kon, kon2)
    if jpath:
        json.dump(out, open(jpath, "w"), indent=1)


if __name__ == "__main__":
    main()
