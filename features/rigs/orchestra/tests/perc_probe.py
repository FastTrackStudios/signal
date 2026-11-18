#!/usr/bin/env python3
"""Probe MIDI for a percussion kit: every key, a hit in each velocity layer, each repeated.

usage: perc_probe.py <kits.tsv> <kit> <out.mid> <out_manifest.tsv> [--repeats N] [--gap-ms MS] [--ksp DIR]

`kits.tsv` is damage_kits.py's table (one row per playable key with its velocity layers).
For each key of <kit>: for each velocity layer `lo-hi` a hit at the layer's middle velocity,
`--repeats` times (default 3, so a random round robin shows several draws), one hit every
`--gap-ms` at 120 BPM, note length 200 ms (kits are one-shots; the note-off only matters to
scripts). Without --gap-ms the gap is the kit's longest sample (`--ksp`, the kit's decoded
`_ksp` folder: zones.tsv frames/rate) + 500 ms, so every hit is isolated by silence.
The manifest is null_test.py's: idx, t_start (s), win (s), pitch, category, description.
"""
import csv, struct, sys

TPQ, US_PER_Q = 480, 500000          # 120 BPM: 1 ms = 0.96 ticks


def vlq(n):
    b = [n & 0x7F]; n >>= 7
    while n:
        b.insert(0, (n & 0x7F) | 0x80); n >>= 7
    return bytes(b)


def main():
    a = sys.argv[1:]
    def opt(name, default):
        if name in a:
            i = a.index(name); v = a[i + 1]; del a[i:i + 2]; return v
        return default
    repeats = int(opt("--repeats", "3")); gap = opt("--gap-ms", None); ksp = opt("--ksp", None)
    kits, kit, out_mid, out_man = a
    rows = [r for r in csv.DictReader(open(kits), delimiter="\t") if r["kit"] == kit]
    if not rows:
        sys.exit(f"no kit {kit!r} in {kits}")
    if gap is None:
        if not ksp:
            sys.exit("give --gap-ms or --ksp <kit _ksp folder>")
        zs = list(csv.DictReader(open(f"{ksp}/zones.tsv"), delimiter="\t"))
        gap = 1000 * max(int(z["frames"]) / int(z["rate"]) for z in zs) + 500
    gap = float(gap)
    hits = []
    for r in rows:
        layers = r["vel_layers"].split() or ["1-127"]
        for lay in layers:
            lo, hi = (int(x) for x in lay.split("-"))
            for _ in range(repeats):
                hits.append((int(r["key"]), (lo + hi) // 2, lay))
    ticks = lambda ms: round(ms * TPQ * 1000 / US_PER_Q)
    ev = []
    with open(out_man, "w") as m:
        m.write("idx\tt_start\twin\tpitch\tcategory\tdescription\n")
        for i, (k, v, lay) in enumerate(hits):
            t = i * gap
            ev.append((ticks(t), bytes([0x90, k, max(1, v)])))
            ev.append((ticks(t + 200), bytes([0x80, k, 0])))
            m.write(f"{i}\t{t / 1000:.3f}\t{gap / 1000:.3f}\t{k}\tHIT\tkey{k} v{v} layer {lay}\n")
    ev.sort(key=lambda e: e[0])
    trk = b"\x00\xff\x51\x03" + US_PER_Q.to_bytes(3, "big"); last = 0
    for t, e in ev:
        trk += vlq(t - last) + e; last = t
    trk += b"\x00\xff\x2f\x00"
    open(out_mid, "wb").write(b"MThd" + struct.pack(">IHHH", 6, 0, 1, TPQ) + b"MTrk" + struct.pack(">I", len(trk)) + trk)
    print(f"{kit}: {len(hits)} hits, {len(hits) * gap / 1000:.0f} s")


if __name__ == "__main__":
    main()
