#!/usr/bin/env python3
"""Damage (Heavyocity): every Perc Kit's playable map, from its decode.

usage: damage_kits.py [<library root>] > kits.tsv

Reads `_ksp/02 Percussive Kits/**/{script_0.ksp, persistent_0.tsv, zones.tsv,
groups.json}`. The Perc Kit engine (one script with a per-kit header table,
`{-- input data start --}` … `{-- input data end --}`) plays, for a key in
`$start_note .. $start_note + NUM_OF_NOTES - 1` (persistent `$start_note`):

- drum = `%drum_id_map[key - start_note]`;
- groups close/OH/room = drum*3 + 0/2/1 (`%close_groups`, `%oh_groups`,
  `%room_groups`); the room group is skipped when `$purge_Room` is 1;
- `play_note(%drum_note[drum] + rr, vel + (127 - $key_sens) capped at 127)`,
  rr random in 0..%roundrobin_num[drum]-1, never the key's last pick.

One row per (kit, key): drum index and name, RR count, the zone key range
that RR spans, and the velocity layers the close group holds there.
Menu kits (`$NUM_OF_DRUMS := ($NUM_GROUPS - 1)/3`: one drum per key from
$start_note, 1 RR) are `perc_menu`; kits on another script (Hit Impacts,
Tails, Hits MW: `$first_note`/`$FirstNote`) are listed once as `other`.
"""
import csv, json, os, re, sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/run/media/AudioHaven/Sampled/Percussion/Damage"
KITS = os.path.join(ROOT, "_ksp", "02 Percussive Kits")


def persistent(path):
    out = {}
    for row in csv.reader(open(path), delimiter="\t"):
        if len(row) >= 2 and row[0] != "name":
            out[row[0]] = row[1].split()
    return out


def ksp_array(src, name, n):
    """`declare %name[...] := (a, b, …)` (with `...` continuations); a short
    list fills the rest with its last value, as KSP does."""
    m = re.search(r"declare\s+%" + name + r"\[[^\]]*\]\s*:=\s*\((.*?)\)", src, re.S)
    if not m:
        return None
    vals = [int(v) for v in re.findall(r"-?\d+", m.group(1).replace("...", ""))]
    return (vals + [vals[-1]] * n)[:n]


def ksp_const(src, name):
    m = re.search(r"declare\s+const\s+\$" + name + r"\s*:=\s*(-?\d+)", src)
    return int(m.group(1)) if m else None


def drum_names(src, n):
    names = [""] * n
    for i, s in re.findall(r'!drum_names_long\[(\d+)\]\s*:=\s*"([^"]*)"', src):
        if int(i) < n:
            names[int(i)] = s
    return names


def drum_note_loop(src, n, start):
    """%drum_note filled in a loop: `A + ($count*B)` or `$FIRST_NOTE + $count`."""
    m = re.search(r"%drum_note\[\$count\]\s*:=\s*(\d+)\s*\+\s*\(\$count\s*\*\s*(\d+)\)", src)
    if m:
        return [int(m.group(1)) + i * int(m.group(2)) for i in range(n)]
    if re.search(r"%drum_note\[\$count\]\s*:=\s*\$FIRST_NOTE\s*\+\s*\$count", src):
        return [start + i for i in range(n)]
    return None


def drum_map_loop(src, n):
    """%drum_id_map filled in a loop: `$count/2` or `$count`."""
    m = re.search(r"%drum_id_map\[\$count\]\s*:=\s*\$count\s*(/\s*(\d+))?\s*$", src, re.M)
    if not m:
        return None
    div = int(m.group(2)) if m.group(2) else 1
    return [i // div for i in range(n)]


def main():
    w = csv.writer(sys.stdout, delimiter="\t", lineterminator="\n")
    w.writerow(["kit", "engine", "key", "drum", "drum_name", "rr", "zone_key_lo", "zone_key_hi",
                "close_group", "oh_group", "room_group", "vel_layers", "start_note", "key_sens", "purge_room"])
    for dirpath, _, files in sorted(os.walk(KITS)):
        if "script_0.ksp" not in files:
            continue
        kit = os.path.relpath(dirpath, KITS)
        src = open(os.path.join(dirpath, "script_0.ksp"), errors="replace").read()
        per = persistent(os.path.join(dirpath, "persistent_0.tsv"))
        zones = list(csv.DictReader(open(os.path.join(dirpath, "zones.tsv")), delimiter="\t"))
        nd, nn = ksp_const(src, "NUM_OF_DRUMS"), ksp_const(src, "NUM_OF_NOTES")
        engine = "perc_kit"
        if nd is None and re.search(r"\$NUM_OF_DRUMS\s*:=\s*\(\$NUM_GROUPS\s*-\s*1\)\s*/\s*3", src):
            # menu kits: one drum per key from $start_note, groups = 3 per drum + dummy
            ngroups = len(json.load(open(os.path.join(dirpath, "groups.json"))))
            nd = nn = (ngroups - 1) // 3
            engine = "perc_menu"
        if nd is None:
            first = per.get("$FirstNote", ["?"])[0]
            w.writerow([kit, "other", first, "", "", "", "", "", "", "", "", "", first,
                        per.get("$key_sens", ["?"])[0], ""])
            continue
        start = int(per["$start_note"][0])
        rr = ksp_array(src, "roundrobin_num", nd)
        dnote = ksp_array(src, "drum_note", nd) or drum_note_loop(src, nd, start)
        dmap = ksp_array(src, "drum_id_map", nn) or drum_map_loop(src, nn)
        names = drum_names(src, nd)
        if None in (rr, dnote, dmap):
            w.writerow([kit, "unparsed", "", "", "", "", "", "", "", "", "", "", start, "", ""])
            continue
        for k in range(nn):
            d = dmap[k]
            lo, hi = dnote[d], dnote[d] + rr[d] - 1
            cg, rg, og = d * 3, d * 3 + 1, d * 3 + 2
            layers = sorted({(int(z["vel_lo"]), int(z["vel_hi"])) for z in zones
                             if int(z["group"]) == cg and lo <= int(z["key_lo"]) <= hi})
            w.writerow([kit, engine, start + k, d, names[d], rr[d], lo, hi, cg, og, rg,
                        " ".join(f"{a}-{b}" for a, b in layers), start,
                        per.get("$key_sens", ["?"])[0], per.get("$purge_Room", ["?"])[0]])


if __name__ == "__main__":
    main()
