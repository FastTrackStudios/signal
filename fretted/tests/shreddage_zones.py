#!/usr/bin/env python3
"""Shreddage 3.5: one decoded NKI → `_patches/<instrument>/library.styx`.

usage: shreddage_zones.py <library root> <NKI name> <instrument>
  e.g. shreddage_zones.py ".../Shreddage Abyss" "Shreddage 3.5 Abyss" Abyss

Reads `_ksp/<NKI>/{zones.tsv, groups.tsv}` and
`_notes/raw_to_organized.tsv` (shreddage_organize.py). Writes every zone the
script can play, tagged the way the engine's fretboard stage looks them up:

- channel (`mic`), articulation and string come from the zone's GROUP:
  "<channel> <articulation> [Str <n>]" (Abyss "Blend Sustain Str 1",
  Argent "Neck PM Str 9"); string 1 = the highest string, as the script's
  string index 0 is (`%str___string_meta`, _notes/engine-model.md §3).
- the zone's velocity slot is an index the script computes, never a
  dynamic (s3___play_event, script_0.ksp L8306-8336); decoded per trigger
  type (TRIGGER, engine-model.md §1d):
    1  layer + 1 + 16·rr (+64 for an up stroke when the articulation is
       picked) → `dynamic v<layer+1>`, `rr_index`, `stroke`
    2  hammer/pull: 16·rr + layer, +64 ascending → `direction up|down`
    3  slide: interval + 64·ascending + 32·rr → `interval`, `direction`
    0  pop: 1 + rr;  6  pitched release: 1 + 16·rr
  each zone is written once per slot with velocity 0..127 (the engine picks
  layer and RR itself).
- gain_db = 20·log10(zvol), tune_cents = 1200·log2(tune), sample_start, start_mod and
  loops verbatim.
"""
import csv, json, math, os, re, sys

# Group articulation word(s) → (our id, trigger type, picked).
TRIGGER = {
    "Sustain": ("Sustain", 1, False),
    "Staccato": ("Staccato", 1, False),
    "Harmonics": ("Harmonics", 1, False),
    "Pops": ("Pop", 0, False),
    "Hammer-on/Pull-off": ("HammerPull", 2, False),
    "Slides": ("Slide", 3, False),
    "Slides Lite": ("SlideLite", 3, False),
    "Pitched Release": ("PitchedRelease", 6, False),
    "FX": ("FX", 1, False),
    "DI Noise": ("DINoise", 1, False),
}
# Groups another articulation plays as its own (Abyss Tapping: TACT 3's
# group lookup is the Hammer-on/Pull-off groups 24-29, trigger type 8 =
# the hammer's velocity layout): those zones again under its name.
ALSO = {"Hammer-on/Pull-off": ["Tapping"]}

# Libraries that ship their TACT meta (`%_<lib>_meta`, loaded from the NKI's
# resources and kept in persistent_0): the groups, trigger type, RR count and
# picking of every articulation come from it, by TACT index → our id (the
# order of `!<lib>_artic_names`, engine-model.md §1b).
META_NAMES = {
    "argent": ["Sustain", "Mute", "Staccato", "PowerChordSustain", "PowerChordMute",
               "PowerChordStaccato", "Choke", "Tapping", "NaturalHarmonics", "ArtificialHarmonics",
               "PinchHarmonics", "Rake", "Tremolo", "HammerPull", "Slide", "PitchedRelease",
               "PowerChordPitchedRelease", "UnpitchedRelease", "FX"],
    "darkwall": ["Sustain", "SustainShort", "StaccatoLong", "StaccatoShort", "Mute", "Harmonics",
                 "Tapping", "BuzzTrill", "HammerPull", "Slide", "PitchedRelease", "UnpitchedRelease"],
}
# Groups outside TACT, by the words of their name (after the pickup).
EXTRA_GROUPS = {
    "Fret Squeaks": ("FretSqueak", 1, False),
    "Fret Noise": ("FretNoise", 1, False),
    "Extra Pick": ("ExtraPick", 1, False),
    # 16·rr + 64·dir + layer + 1 (L25834): tagged as a hammer's slots
    "Wide Chokes": ("WideChoke", 2, False),
    "DI Noise": ("DINoise", 1, False),
}
# Groups outside TACT whose name is also an articulation's in other
# libraries: only when no TACT articulation claims the group (Darkwall's
# key FX, keys 10/11).
EXTRA_FALLBACK = {"FX": ("KeyFX", 1, False)}


def load_meta(kdir):
    """(lib, meta ints) when the instrument ships `%_<lib>_meta`, else None."""
    with open(os.path.join(kdir, "persistent_0.tsv")) as f:
        for line in f:
            k, _, v = line.rstrip("\n").partition("\t")
            m = re.match(r"^%_([a-z]+)_meta$", k)
            if m and m.group(1) in META_NAMES:
                vals = [int(x) for x in v.split()]
                return m.group(1), vals + [0] * (64 * len(META_NAMES[m.group(1)]) - len(vals))
    return None


def classify(gidx, name, meta, ngroups, npickups=2):
    """[(mic, articulation, trigger, picked, string)] a group's zones play as."""
    g = parse_group(name)
    if not g:
        return []
    mic, word, string = g
    if word in EXTRA_GROUPS:
        art, trig, picked = EXTRA_GROUPS[word]
        return [(mic, art, trig, picked, string)]
    if meta is None:
        if word not in TRIGGER:
            return []
        art, trig, picked = TRIGGER[word]
        return [(mic, a, trig, picked, string) for a in [art] + ALSO.get(word, [])]
    lib, vals = meta
    # one block of groups per pickup (Argent Neck/Bridge, Darkwall
    # Neck/Middle/Bridge), each laid out by the meta
    local = gidx % (ngroups // npickups)
    out = []
    for a, art in enumerate(META_NAMES[lib]):
        m = vals[64 * a: 64 * a + 64]
        first, count = m[1], m[0]
        if first <= local < first + count:
            s = local - first + 1 if m[38] == 1 else 0
            out.append((mic, art, m[21], m[35] == 1, s))
    if not out and word in EXTRA_FALLBACK:
        art, trig, picked = EXTRA_FALLBACK[word]
        out.append((mic, art, trig, picked, string))
    return out


def fmt(v):
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, str):
        return json.dumps(v)
    if isinstance(v, float):
        s = f"{v:.6f}".rstrip("0").rstrip(".")
        return s if "." in s else s + ".0"
    return str(v)


def zone_block(z):
    body = "\n".join(f"        {k:<12} {fmt(v)}" for k, v in z.items())
    return f"    {{\n{body}\n    }}"


def parse_group(name):
    """'Blend Sustain Str 1' → ('Blend', 'Sustain', 1); 'Blend FX' → (…, 0)."""
    m = re.match(r"^(\S+) (.+?)(?: Str (\d+))?$", name)
    if not m:
        return None
    return m.group(1), m.group(2), int(m.group(3) or 0)


def decode_slot(trig, picked, v):
    """Velocity slot → the zone's tags (engine-model.md §1d)."""
    i = v - 1
    if trig == 10:
        # layer + 1 + 8·rr (+64 up stroke when picked), L7600
        tags = {"dynamic": f"v{i % 8 + 1}", "rr_index": (i % 64) // 8}
        if picked:
            tags["stroke"] = "up" if i >= 64 else "down"
        return tags
    if trig in (1, 5, 9):
        tags = {"dynamic": f"v{i % 16 + 1}"}
        if picked:
            tags["rr_index"] = (i % 64) // 16
            tags["stroke"] = "up" if i >= 64 else "down"
        else:
            tags["rr_index"] = i // 16
        return tags
    if trig in (2, 8):
        return {"dynamic": f"v{i % 16 + 1}", "rr_index": (i % 64) // 16,
                "direction": "up" if i >= 64 else "down"}
    if trig == 3:
        return {"interval": v % 32, "rr_index": (v % 64) // 32,
                "direction": "up" if v >= 64 else "down"}
    if trig == 0:
        return {"rr_index": i}
    if trig in (6, 7):
        return {"rr_index": i // 16}
    raise ValueError(trig)


def reachable(trig, picked, v):
    """Whether the script can send velocity slot `v` (engine-model §2):
    a release (6) plays layer 0 down-stroke only; an unpicked one-shot never
    sends the +64 up stroke."""
    i = v - 1
    if trig == 6:
        return i % 16 == 0 and i < 64
    return True


def zones_of(r, inst, organized, nki, mic, art, trig, picked, string):
    """The zones one decoded zone row becomes for one articulation."""
    raw = r["sample"].split("Samples/", 1)[1]  # <C>.nkx/Samples/<name>
    raw = os.path.splitext(raw)[0] + ".wav"
    if raw not in organized:
        sys.exit(f"{nki}: no organized file for {raw}")
    lo, hi = int(r["vel_lo"]), int(r["vel_hi"])
    slots = [None] if (lo, hi) == (1, 127) else range(lo, hi + 1)
    out = []
    for v in slots:
        if v is not None and not reachable(trig, picked, v):
            continue
        z = {
            "section": inst,
            "file": organized[raw],
            "key_min": int(r["key_lo"]),
            "key_max": int(r["key_hi"]),
            "root_key": int(r["root"]),
            "vel_min": 0,
            "vel_max": 127,
            "rr_index": 0,
            "mic": mic,
            "articulation": art,
        }
        if string:
            z["string"] = string
        if v is not None:
            z.update(decode_slot(trig, picked, v))
        gain = float(r["zvol"])
        z["gain_db"] = round(20 * math.log10(gain), 6) if gain > 0 else -144.0
        z["tune_cents"] = round(1200 * math.log2(float(r["tune"])), 6)
        if int(r["sample_start"]) > 0:
            z["sample_start"] = int(r["sample_start"])
        if int(r["start_mod"]) != 0:  # −1: no start offset (the script's is ignored)
            z["start_mod"] = int(r["start_mod"])
        ls, le = int(r["loop_start"]), int(r["loop_end"])
        if le > ls >= 0:
            z["loop_start"], z["loop_end"] = ls, le
        out.append(z)
    return out


def main():
    root, nki, inst = sys.argv[1:4]
    kdir = os.path.join(root, "_ksp", nki)
    with open(os.path.join(root, "_notes", "raw_to_organized.tsv")) as f:
        organized = dict(line.rstrip("\n").split("\t") for line in f if line.strip())
    with open(os.path.join(kdir, "groups.tsv")) as f:
        groups = {r["idx"]: r["name"] for r in csv.DictReader(f, delimiter="\t")}
    with open(os.path.join(kdir, "zones.tsv")) as f:
        rows = list(csv.DictReader(f, delimiter="\t"))
    meta = load_meta(kdir)
    # source-module Tracking per group (groups.json `key_tracking`)
    gj = json.load(open(os.path.join(kdir, "groups.json")))
    fixed = {str(g["index"]) for g in (gj if isinstance(gj, list) else gj["groups"])
             if not g.get("key_tracking", True)}
    ngroups = len(groups)
    npickups = len({n.split(" ", 1)[0] for n in groups.values() if n})

    out, skipped = [], {}
    for r in rows:
        name = groups.get(r["group"], "")
        kinds = classify(int(r["group"]), name, meta, ngroups, npickups)
        if not kinds:
            skipped[name or r["group"]] = skipped.get(name or r["group"], 0) + 1
            continue
        for mic, art, trig, picked, string in kinds:
            zs = zones_of(r, inst, organized, nki, mic, art, trig, picked, string)
            if r["group"] in fixed:
                for z in zs:
                    z["fixed_pitch"] = True
            out.extend(zs)
    pdir = os.path.join(root, "_patches", inst)
    os.makedirs(pdir, exist_ok=True)
    path = os.path.join(pdir, "library.styx")
    with open(path, "w") as f:
        f.write(f"// {nki} — sample zones.\n"
                "// Generated by features/rigs/fretted/tests/shreddage_zones.py from the decoded NKI — do not edit.\n\n"
                f'name "{nki}"\nvendor "Impact Soundworks"\nversion "3.5"\n\nzones (\n')
        f.write("\n".join(zone_block(z) for z in out))
        f.write("\n)\n")
    print(f"{path}: {len(out)} zones")
    for k, n in sorted(skipped.items()):
        print(f"  skipped group {k!r}: {n} zones")


if __name__ == "__main__":
    main()
