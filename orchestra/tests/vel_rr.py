#!/usr/bin/env python3
"""Give each round-robin slot the recording Kontakt plays there.

    python3 vel_rr.py <_ksp/<instrument> dir> <_patches/<label>/library.styx> [articulation ...]

The CS scripts pick a marcato body's round robin by velocity index
(`play_note(note, $jpvdn + $d3x3j)`, CSB 11021 / 14060): RR n plays the
zone whose velocity range holds n. The shipped NKIs do not always map the
files in suffix order — CSB 2 Trombones `Marcato_f_D3_1` spans velocities
1-2 and `_2` sits at 14, so both RRs play `_1`. Our zones are labelled RR by
file suffix; this rewrites, per (mic, articulation, dynamic, key range),
the zone of each RR slot to carry the recording Kontakt plays at that index
(its file and every per-recording field). Default articulation: Marcato.
Rewrites library.styx in place (a .pre-velrr-<date> copy is kept) and prints
what it changed.
"""
import collections, csv, datetime, os, re, shutil, sys


def blocks(text):
    """(start, end, body) of every top-level zone block `    {…    }`."""
    out = []
    for m in re.finditer(r"\n    \{\n(.*?)\n    \}", text, re.S):
        out.append((m.start(), m.end(), m.group(1)))
    return out


def field(body, key):
    m = re.search(rf"^\s+{key}\s+(.+)$", body, re.M)
    return m.group(1).strip().strip('"') if m else None


def main():
    ksp, styx = sys.argv[1], sys.argv[2]
    artics = set(sys.argv[3:]) or {"Marcato"}
    # Kontakt: per sample name, its velocity range (= the RR indices it serves)
    vel = {}
    for z in csv.DictReader(open(os.path.join(ksp, "zones.tsv")), delimiter="\t"):
        name = os.path.basename(z["sample"]).rsplit(".", 1)[0]
        vel[name] = (int(z["vel_lo"]), int(z["vel_hi"]))
    text = open(styx).read()
    bl = blocks(text)
    # our zones: (mic, artic, dyn, kmin, kmax) -> {rr: (block idx, file)}
    groups = collections.defaultdict(dict)
    for i, (_, _, body) in enumerate(bl):
        a = field(body, "articulation")
        if a not in artics:
            continue
        key = (field(body, "mic"), a, field(body, "dynamic"), field(body, "key_min"), field(body, "key_max"))
        groups[key][int(field(body, "rr_index") or 0)] = (i, field(body, "file"))
    # a library file "Mix/2 Trombones/Marcato/f_D3_1.wav" ↔ Kontakt "2tb_Marcato_Mix_f_D3_1"
    def kname(f):
        mic, _, artic, base = f.split("/")[-4:]
        dyn, rest = base[:-4].split("_", 1)
        hits = [n for n in vel if n.endswith(f"_{artic}_{mic}_{dyn}_{rest}")]
        return hits[0] if len(hits) == 1 else None
    new_bodies = {}
    changed = 0
    for key, slots in groups.items():
        n = len(slots)
        if n < 2:
            continue
        files = {rr: f for rr, (_, f) in slots.items()}
        for rr, (i, f) in slots.items():
            idx = rr + 1  # Kontakt velocity index of this RR slot
            src = [r for r, g in files.items() if (kn := kname(g)) and vel[kn][0] <= idx <= vel[kn][1]]
            if len(src) != 1 or src[0] == rr:
                continue
            j = slots[src[0]][0]
            body = bl[j][2]
            body = re.sub(r"(^\s+rr_index\s+)\S+", rf"\g<1>{rr}", body, flags=re.M)
            new_bodies[i] = body
            changed += 1
            print(f"{key[0]} {key[1]} {key[2]} keys {key[3]}-{key[4]}: RR {idx} plays {os.path.basename(files[src[0]])}")
    if not changed:
        print("every RR slot already plays its velocity's recording")
        return
    out, pos = [], 0
    for i, (s, e, body) in enumerate(bl):
        if i in new_bodies:
            out.append(text[pos:s])
            out.append("\n    {\n" + new_bodies[i] + "\n    }")
            pos = e
    out.append(text[pos:])
    backup = f"{styx}.pre-velrr-{datetime.date.today().isoformat()}"
    if not os.path.exists(backup):
        shutil.copy2(styx, backup)
    open(styx, "w").write("".join(out))
    print(f"{changed} zone(s) rewritten")


if __name__ == "__main__":
    main()
