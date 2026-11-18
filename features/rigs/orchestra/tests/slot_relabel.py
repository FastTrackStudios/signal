#!/usr/bin/env python3
"""Give zones the articulation of the Kontakt slot that plays them.

    python3 slot_relabel.py <_ksp/<instrument> dir> <_patches/<label>/library.styx>

Zones are labelled by their file name (`Sbcl_NVsus_Mix_ff_C3` → "NVsus"), but
Kontakt plays a recording by the group slot it sits in. CSW's vibrato-less
instruments (the clarinets, the contrabassoon) put their NVsus recordings in
the *vibsus* groups and leave the nonvib groups empty: Kontakt plays them as
the vibrato side (always, when the instrument has no vibrato CC; at CC2 ≥ 64
on Solo Bass Clarinet, silent below; its legato transitions likewise sit
in the vibrato-side legato groups). CSB 4 Horns plays its Stopped
recordings from the mute slots (CC58 31-40 / 56-60). So a slot that holds none of its own
articulation's recordings but all of another's relabels those zones to its
own articulation. Rewrites library.styx in place (a .pre-slot-<date> copy is
kept) and prints what it changed.
"""
import collections, csv, datetime, json, os, re, shutil, sys

# group family (after the mic) → the articulation that slot stands for
SLOTS = {
    "vibsus": "Vibsus", "nonvib": "NVsus", "vibsus rel": "Vibsusrel", "nonvib rel": "NVsusrel",
    # CSB: the mute slots (4 Horns keeps its stopped recordings there)
    "tremolo": "Mutesus", "tremolo rel": "Mutesusrel", "pizzicato": "Mutereps",
    # CSW legato: Solo Bass Clarinet keeps its NVLeg transitions in the
    # vibrato-side "legato" groups (the NVlegato groups are empty)
    "legato": "Leg", "NVlegato": "NVLeg", "legato zero": "Legzero", "NVlegato zero": "NVLegzero",
}
# slots that only matter with a vibrato controller: without one the engine
# plays the non-vibrato side alone, and the clarinets' transitions must stay
# there (moving them silenced every transition, null −11 → −2 dB)
VIB_ONLY = {"legato", "NVlegato", "legato zero", "NVlegato zero"}
# recordings that may stand in for a slot's own (not the borrowed attack /
# short zones a vibsus group also holds)
STAND_INS = set(SLOTS.values()) | {"Stoppedsus", "Stoppedsusrel", "Stoppedreps"}
MICS = ("Mix", "Main", "Room", "Close", "OH")


def file_artic(sample):
    parts = os.path.basename(sample)[:-4].split("_")
    return parts[1] if len(parts) > 1 else ""


def main():
    ksp, styx = sys.argv[1], sys.argv[2]
    groups = json.load(open(os.path.join(ksp, "groups.json")))
    section = os.path.join(os.path.dirname(styx), "section.styx")
    has_vib = os.path.exists(section) and re.search(
        r'^vibrato_controller\s+"[^"]+"', open(section).read(), re.M) is not None
    held = collections.defaultdict(set)  # slot artic → file artics it holds
    where = collections.defaultdict(set)  # file artic → slots holding it
    for r in csv.DictReader(open(os.path.join(ksp, "zones.tsv")), delimiter="\t"):
        name = groups[int(r["group"])]["name"]
        mic, _, fam = name.partition(" ")
        fam = re.sub(r" (pp|p|mp|mf|f|ff|fff)$", "", fam)
        if mic not in MICS or fam not in SLOTS or (fam in VIB_ONLY and not has_vib):
            continue
        a = file_artic(r["sample"])
        held[SLOTS[fam]].add(a)
        where[a].add(SLOTS[fam])
    relabel = {}
    for slot, artics in held.items():
        if slot in artics:
            continue
        for a in artics:
            if a in STAND_INS and where[a] == {slot}:
                relabel[a] = slot
    if not relabel:
        print("no slot holds another articulation's recordings")
        return
    text = open(styx).read()
    n = collections.Counter()

    def sub(m):
        a = m.group(2)
        if a in relabel:
            n[a] += 1
            return f'{m.group(1)}"{relabel[a]}"'
        return m.group(0)

    out = re.sub(r'(\n\s+articulation\s+)"([^"]+)"', sub, text)
    backup = f"{styx}.pre-slot-{datetime.date.today().isoformat()}"
    if not os.path.exists(backup):
        shutil.copy2(styx, backup)
    open(styx, "w").write(out)
    for a, slot in sorted(relabel.items()):
        print(f"{a} → {slot}: {n[a]} zone(s)")


if __name__ == "__main__":
    main()
