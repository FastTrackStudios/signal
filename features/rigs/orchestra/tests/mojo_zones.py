#!/usr/bin/env python3
"""MOJO 2 Horn Section: one instrument's decoded NKI → `_patches/<instrument>/library.styx`.

usage: mojo_zones.py <library root> [<NKI name> …]   (default: every full instrument)

Reads `_ksp/<NKI name>/{zones.tsv, groups.json, persistent_1.tsv}` and
`_notes/raw_to_organized.tsv` (zone sample path → organized WAV). Writes the
zones the engine plays, one per logical round robin:

- articulation / dynamic / mic come from the zone's GROUP ("<Art>_<Layer> - BusN";
  Bus1..4 = Mix/Close/Near/Room), never from the file name — a few groups play
  another mic's file, and Kontakt plays what the group holds.
- MOJO's `play_note` velocity is a round-robin index, not a dynamic: the
  script plays `(127 / NumRR) * rr + 3` (script_1.ksp:25228, NumRR = `$1f1tu`
  in persistent_1), so the zone whose velocity range holds that value is
  logical RR `rr`. Each is written with `rr_index rr` and velocity 0..127.
  (Trumpet ships NumRR 4 over 3 recorded zones: RR 0 and 1 share a file.)
- legato groups encode the interval in the velocity: up `1 + i·10.5`,
  down `127 − i·10.5` for i = 1..12 (script_1.ksp:26029-26050); written with
  `direction` and `interval`.
- the keyclick group keeps its own 11 RR zones (the script plays a random
  velocity 1..127 into it).
- gain_db = 20·log10(zvol), tune_cents = 1200·log2(tune), loops verbatim.
- a Time Machine II group's zones (source mode 5: legato, octave, unison
  tails, swells, crescendos) carry `tm2_block`, the analysis block of the
  group's source record (`tm2_blocks`).
"""
import csv, json, math, os, re, shutil, struct, sys, time

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section"
BUS_MIC = {1: "Mix", 2: "Close", 3: "Near", 4: "Room"}
TRILL = {1: "ht", 2: "wt", 3: "min3", 4: "maj3"}

RULES = [
    (r"^Sustains_Non_Vibrato_Velocity_(\d)_R$", "SusRel", "v{}"),
    (r"^Sustains_Vibrato_Velocity_(\d)_R$", "SusVibRel", "v{}"),
    (r"^Sustains_Non_Vibrato_Velocity_(\d)$", "Sus", "v{}"),
    (r"^Sustains_Vibrato_Velocity_(\d)$", "SusVib", "v{}"),
    (r"^Staccato_Velocity_(\d)_AttkAcnt$", "AttkAcnt", "v{}"),
    (r"^Staccato_Velocity_(\d)$", "Stacc", "v{}"),
    (r"^Stabs_Velocity_(\d)$", "Stabs", "v{}"),
    (r"^Bend_Downs?$", "BendDown", "v1"),
    (r"^Octave_Run_Down$", "RunDown", "v1"),
    (r"^Octave_Run_Up$", "RunUp", "v1"),
    (r"^Doits$", "Doits", "v1"),
    (r"^Rise_[Tt]o_Hit$", "Rise", "v1"),
    (r"^Shakes$", "Shakes", "v1"),
    (r"^Trills \((\d)\)$", "Trills", "trill"),
    (r"^Swells_Length_(\d)$", "Swells", "L{}"),
    (r"^Crescendo(?:e)?s_Length_(\d)$", "Cresc", "L{}"),
    (r"^Falls_Length_(\d)$", "Falls", "L{}"),
    (r"^Doits_Releases$", "DoitRel", "v1"),
    (r"^Shakes_Releases$", "ShakeRel", "v1"),
    (r"^Falls_Length_(\d)_Releases$", "FallRel", "L{}"),
    (r"^Trills \((\d)\)_Releases$", "TrillRel", "trill"),
    (r"^Key(?:clicks?|noise)$", "Keyclick", "v1"),
    (r"^Legato_Up_Velocity_(\d)$", "LegUp", "v{}"),
    (r"^Legato_Down_Velocity_(\d)$", "LegDown", "v{}"),
    (r"^Legato_UpDown_Unison_Tails_Velocity_(\d)$", "LegTail", "v{}"),
    (r"^Octave_Legato_Up_Velocity_(\d)$", "OctLegUp", "v{}"),
    (r"^Octave_Legato_Down_Velocity_(\d)$", "OctLegDown", "v{}"),
]
LEGATO = ("LegUp", "LegDown", "LegTail", "OctLegUp", "OctLegDown")


def classify(group_name):
    """Group name → (articulation id, dynamic, bus), or None for the dummy group."""
    m = re.match(r"^(.*?)(?:\s*-\s*|_)Bu[sa]\s*(\d+)$", group_name.strip())
    if not m:
        if group_name.upper().startswith("DUMMY"):
            return None
        raise SystemExit(f"unrecognised group name {group_name!r}")
    base, bus = m.group(1).strip(), int(m.group(2))
    # The library's own group-name typos (the script addresses groups by
    # index, so the names are labels only; check_starts cross-checks them).
    base = re.sub(r"Val?ocity|Velovity", "Velocity", base)
    base = re.sub(r"^Sustain_", "Sustains_", base)
    base = re.sub(r"Velocity (\d)", r"Velocity_\1", base)
    base = re.sub(r"_Release$", "_Releases", base)
    for pat, art, dyn in RULES:
        mm = re.match(pat, base)
        if mm:
            if dyn == "trill":
                return art, TRILL[int(mm.group(1))], bus
            return art, dyn.format(*mm.groups()), bus
    raise SystemExit(f"unrecognised group base {base!r} (group {group_name!r})")


def persistent(path):
    out = {}
    for row in csv.reader(open(path), delimiter="\t"):
        if len(row) >= 2 and row[0] != "name":
            out[row[0]] = row[1].split()
    return out


def raw_key(sample):
    """Zone sample path → the raw path used in raw_to_organized.tsv."""
    rel = sample.split("Samples/", 1)[1]  # <C>.nkx/Samples/<name>.ncw
    c = rel.split(".nkx/")[0]
    return os.path.join(c + ".nkx", c, rel.split(".nkx/", 1)[1])[:-4] + ".wav"


def fmt(v):
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, float):
        s = f"{v:.6f}".rstrip("0").rstrip(".")
        return s if "." in s else s + ".0"
    return str(v)


def zone_block(z):
    body = "\n".join(f"        {k:<12} {fmt(v)}" for k, v in z.items())
    return f"    {{\n{body}\n    }}"


# Script articulation order (script_1.ksp:185-197) → our id; `%hr4to[i]` is
# its first group, and group = start + 4·index + (bus − 1).
SCRIPT_ARTS = ["Sus", "Stacc", "Stabs", "BendDown", "RunDown", "RunUp", "Doits",
               "Rise", "Shakes", "Trills", "Swells", "Cresc", "Falls"]


# Time Machine II's analysis setting → its analysis block, frames
# (`BSampleInfo::FindPitchMarks`, Kontakt 8.7.1 0x43764c; 0 asks a
# pre-analysis, not decoded).
TM2_BLOCK = {1: 512, 2: 512, 3: 1024, 4: 2048}


def tm2_blocks(nki, ksp, gjson):
    """Every Time Machine II group's analysis block: `{group index: frames}`.

    A group's source record (`BParSrcMode`, in the group's private data of
    `inner.bin`) for source mode 5 is `u8 0 · u16 version · u32 5 · f32 ·
    flag · u32 · flag · f32` (+ `f32 f32 flag` from v0x102), then the mode's
    own `flag · u32 · u32`: the transient-copy flag, the transient size and
    the analysis setting (KONTRA af3e766: `BParSrcMode::doRead` 0x7d151c,
    `BGroup + 0xc18`). The records sit in group order, so the n-th one is
    the n-th mode-5 group's.
    """
    d = open(os.path.join(ksp, "inner.bin"), "rb").read()
    recs = []
    for m in re.finditer(rb"\x00[\x00-\x06]\x01\x05\x00\x00\x00", d):
        e = m.start()
        if e < 1 or d[e - 1] > 1:
            continue
        e -= 1  # the record's leading flag byte
        ver = struct.unpack_from("<H", d, e + 2)[0]
        if ver < 0x102 or any(d[e + k] > 1 for k in (12, 17, 30, 31)):
            continue
        recs.append((d[e + 31], struct.unpack_from("<I", d, e + 36)[0]))
    groups = [g["index"] for g in gjson if g.get("source_mode") == 5]
    if len(recs) != len(groups):
        raise SystemExit(f"{nki}: {len(groups)} Time Machine II groups, {len(recs)} source records")
    out = {}
    for gi, (transient, setting) in zip(groups, recs):
        if transient:
            raise SystemExit(f"{nki}: group {gi} copies transients (not modelled)")
        if setting not in TM2_BLOCK:
            raise SystemExit(f"{nki}: group {gi} analysis setting {setting} (pre-analysis, not decoded)")
        out[gi] = TM2_BLOCK[setting]
    return out


def check_starts(nki, groups, starts):
    """The script's group index for every core articulation group → its bus.

    The script addresses a group as start + 4·index + (bus − 1), never by
    name, so the index decides the mic. A name naming another articulation is
    an error; a name naming another bus is a label slip (Trumpet Muted group
    96 "Rise_To_Hit - Bus2" holds the Mix files) — reported, index wins.
    """
    bus_of = {}
    for i, art in enumerate(SCRIPT_ARTS):
        lo = starts[i]
        for gi in range(lo, lo + 4):
            cls = classify(groups.get(gi, "?"))
            if cls is None or cls[0] != art:
                raise SystemExit(f"{nki}: group {gi} {groups.get(gi)!r} is not {art} "
                                 f"(%hr4to[{i}] = {lo})")
            if cls[2] != gi - lo + 1:
                print(f"  {nki}: group {gi} {groups[gi]!r} plays as Bus{gi - lo + 1} (by index)")
            bus_of[gi] = gi - lo + 1
    return bus_of


def group_stage(fx_slots, gi):
    """A group's active kind-9 insert as `(gain_db, param)`: its output gain
    goes on the zones; its parameter (the float at byte 9, −0.3 on Trumpet
    Stabs) is NOT a balance — Kontakt's render keeps both channels level
    (measured: L/R gain 0.98/0.99 against an unpanned voice) — so it is
    returned for the caller to report, not applied."""
    gain_db, param = 0.0, 0.0
    for s in fx_slots.get(gi, []):
        if s["kind"] != 9 or s.get("bypass", True):
            continue
        gain_db += 20 * math.log10(s.get("output_gain") or 1.0)
        ch = bytes.fromhex(s["children_hex"])
        if len(ch) >= 13:
            param += struct.unpack("<f", ch[9:13])[0]
    return gain_db, param


def build(nki, organized):
    ksp = os.path.join(ROOT, "_ksp", nki)
    inst = re.sub(r"\s+2\.0$", "", nki)
    gjson = json.load(open(os.path.join(ksp, "groups.json")))
    groups = {g["index"]: g["name"] for g in gjson}
    # the source mode (nkx-extract source_mode): 3 DFD, 5 Time Machine II, …
    modes = {g["index"]: g.get("source_mode") for g in gjson}
    tm2 = tm2_blocks(nki, ksp, gjson)
    pers = persistent(os.path.join(ksp, "persistent_1.tsv"))
    num_rr = int(pers["$1f1tu"][0])
    bus_of = check_starts(nki, groups, [int(x) for x in pers["%hr4to"]])
    fx_path = os.path.join(ksp, "group_fx.json")
    fx_slots = {x["group"]: x["slots"] for x in json.load(open(fx_path))} if os.path.exists(fx_path) else {}

    by_group = {}
    for r in csv.DictReader(open(os.path.join(ksp, "zones.tsv")), delimiter="\t"):
        by_group.setdefault(int(r["group"]), []).append(r)

    out, skipped = [], 0
    for gi, rs in sorted(by_group.items()):
        cls = classify(groups[gi])
        if cls is None:
            skipped += len(rs)
            continue
        art, dyn, bus = cls
        bus = bus_of.get(gi, bus)
        stage_db, _stage_param = group_stage(fx_slots, gi)

        def emit(r, **extra):
            raw = raw_key(r["sample"])
            if raw not in organized:
                raise SystemExit(f"{nki}: no organized file for {raw}")
            z = {
                "section": inst,
                "file": organized[raw],
                "key_min": int(r["key_lo"]),
                "key_max": int(r["key_hi"]),
                "root_key": int(r["root"]),
                "vel_min": 0,
                "vel_max": 127,
                "rr_index": 0,
                "mic": BUS_MIC[bus],
                "articulation": art,
                "dynamic": dyn,
            }
            z.update(extra)
            gain = float(r["zvol"])
            z["gain_db"] = round(20 * math.log10(gain) + stage_db, 6) if gain > 0 else -144.0
            z["tune_cents"] = round(1200 * math.log2(float(r["tune"])), 6)
            # Kontakt limits a script's offset by S.Mod only in DFD (KSP
            # manual, play_note): there it is always written (−1, no start
            # modulation, ignores the offset); other modes (the Time Machine
            # II transitions) take the offset whole
            if modes.get(gi) in (3, None):
                z["start_mod"] = int(r["start_mod"])
            # a Time Machine II group's voices play grain by grain, at any
            # speed, from pitch marks found with its analysis block
            if gi in tm2:
                z["tm2_block"] = tm2[gi]
            # the zone's own sample window: a play_note offset counts from
            # sample_start (KONTRA trace: a release 78 ms in starts at frame
            # 3744 + the zone's 1500)
            if int(r["sample_start"]) > 0:
                z["sample_start"] = int(r["sample_start"])
            if int(r["sample_end"]) > 0:
                z["sample_end"] = int(r["sample_end"])
            ls, le = int(r["loop_start"]), int(r["loop_end"])
            if le > ls >= 0:
                z["loop_start"], z["loop_end"] = ls, le
            out.append(z)

        # Each zone keeps its own key range: within one group the velocity
        # slots of a note can span different keys (Alto Sax vibrato G3:
        # slots 1-2 cover 67-68, slot 3 covers 67-69). Where no zone holds a
        # key at the played velocity, Kontakt plays nothing; so do we.
        def holding(v):
            return [r for r in rs if int(r["vel_lo"]) <= v <= int(r["vel_hi"])]

        if art in LEGATO:
            # play_note's velocity is the interval (26031/26036): up
            # 1 + ⌊i·126/12⌋, down 127 − ⌊i·126/12⌋; a repeated key is down
            # at 0 (127, the unison band the tail groups hold)
            up = art in ("LegUp", "OctLegUp")
            for i in range(0 if not up else 1, 13):
                v = 1 + (i * 126) // 12 if up else 127 - (i * 126) // 12
                for r in holding(v):
                    emit(r, direction="up" if up else "down", interval=i)
        elif art == "Keyclick":
            # play_note(64, random(1, 127)): the zone by its velocity band
            for i, r in enumerate(sorted(rs, key=lambda r: int(r["vel_lo"]))):
                emit(r, rr_index=i, vel_min=int(r["vel_lo"]), vel_max=int(r["vel_hi"]))
        else:
            for rr in range(num_rr):
                for r in holding((127 // num_rr) * rr + 3):
                    emit(r, rr_index=rr)

    patch = os.path.join(ROOT, "_patches", inst)
    os.makedirs(patch, exist_ok=True)
    dst = os.path.join(patch, "library.styx")
    if os.path.exists(dst):
        shutil.copy2(dst, dst + time.strftime(".pre-%Y-%m-%d-%H%M%S"))
    with open(dst, "w") as f:
        f.write(f"// MOJO 2 Horn Section — {inst} sample zones.\n")
        f.write("// Generated by features/rigs/orchestra/tests/mojo_zones.py from the decoded NKI — do not edit.\n")
        f.write("// Load specs/mojo2-horns.styx alongside for the engine config.\n\n")
        f.write(f'name "MOJO 2 Horn Section — {inst}"\nvendor "Vir2"\nversion "2.0"\n\nzones (\n')
        f.write("\n".join(zone_block(z) for z in out))
        f.write("\n)\n")
    arts = sorted({z["articulation"] for z in out})
    print(f"{inst}: {len(out)} zones (NumRR {num_rr}, {skipped} dummy rows skipped), "
          f"{len(arts)} articulations: {' '.join(arts)}")


def main():
    organized = {}
    with open(os.path.join(ROOT, "_notes", "raw_to_organized.tsv")) as f:
        for row in csv.DictReader(f, delimiter="\t"):
            organized[row["raw"]] = row["organized"]
    names = sys.argv[2:] or sorted(
        d for d in os.listdir(os.path.join(ROOT, "_ksp")) if " - " not in d and d.endswith("2.0")
    )
    for n in names:
        build(n, organized)


if __name__ == "__main__":
    main()
