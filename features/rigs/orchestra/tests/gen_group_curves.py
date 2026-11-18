#!/usr/bin/env python3
"""Emit an articulation's `group_curves` styx block from a decoded groups.json.

    python3 gen_group_curves.py <groups.json> <group-prefix> <dyn>... [--ccs 1,2]

e.g. `gen_group_curves.py groups.json nonvib p mf ff` reads the Mix-mic groups
"Mix nonvib p" / "Mix nonvib mf" / "Mix nonvib ff" (the tables are identical
on every mic) and prints one `{ dynamic …, cc …, points (…) }` entry per
CC_VOLUME table, to paste under the articulation in the spec. CC11 tables
are linear (value/127) and are applied engine-wide, so they are skipped.
"""
import json, sys


def main():
    argv = sys.argv[1:]
    ccs = {1, 2}
    if "--ccs" in argv:
        i = argv.index("--ccs")
        ccs = {int(c) for c in argv[i + 1].split(",")}
        del argv[i : i + 2]
    if len(argv) < 3:
        raise SystemExit(__doc__)
    groups = {g["name"]: g for g in json.load(open(argv[0]))}
    prefix, dyns = argv[1], argv[2:]
    print("        group_curves (")
    for d in dyns:
        name = f"Mix {prefix} {d}"
        g = groups.get(name)
        if g is None:
            raise SystemExit(f"no group {name!r}")
        for m in g["mods"]:
            # Release groups scale their flex-envelope LEVELS by CC36
            # (`CC_TIME1` → fm7_level1/2, the same curve on both): a volume
            # curve in effect. Emit it once, as cc 36.
            if m["source"] == "CC_TIME1" and m["cc"] == 36 and m["target"] == "fm7_level1" and m["curve"]:
                pts = " ".join(f"{v:.8g}" for v in m["curve"])
                print(f"            {{ dynamic {d}, cc 36, points ({pts}) }}")
                continue
            if m["source"] == "CC_VOLUME" and m["cc"] in ccs and m["curve"]:
                pts = " ".join(f"{v:.8g}" for v in m["curve"])
                print(f"            {{ dynamic {d}, cc {m['cc']}, points ({pts}) }}")
    print("        )")


if __name__ == "__main__":
    main()
