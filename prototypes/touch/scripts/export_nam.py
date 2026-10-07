#!/usr/bin/env python3
"""Which NAM models each Amp / Drive / Core variation loads, from the rig's
own config (~/.config/signal/rig/modules.styx + drive-presets.styx), into
src/data/nam.json for the prototype's browser. Only model names are kept —
no paths. Core variations are resolved through the Amp variation and the
drives they pick (from the fixture's snapshot_info)."""

import json, os, re

RIG = os.path.expanduser("~/.config/signal/rig")
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "src", "data", "nam.json")
FIXTURE = os.path.join(HERE, "..", "src", "data", "fixture.json")


def model(path):
    return os.path.splitext(os.path.basename(path))[0] if path else None


def unq(x):
    return x.strip().strip('"')


def block(s, start):
    depth, j = 1, start
    while depth and j < len(s):
        depth += {"(": 1, ")": -1}.get(s[j], 0)
        j += 1
    return s[start : j - 1]


drives = {}
dp = open(os.path.join(RIG, "drive-presets.styx")).read()
for m in re.finditer(r'\{name "([^"]+)", options \(', dp):
    opts = re.findall(r'\{name "([^"]+)", nam "([^"]*)"', block(dp, m.end()))
    drives[m.group(1)] = [(n, model(p)) for n, p in opts]

mods = open(os.path.join(RIG, "modules.styx")).read()
out = {}
for m in re.finditer(r"\{module (\w+), name ([^,]+), snapshots \(", mods):
    kind, name = m.group(1), unq(m.group(2))
    if kind not in ("Amp", "Drive"):
        continue
    body = block(mods, m.end())
    for sm in re.finditer(r'\{name ([^,]+), nam "([^"]*)", cab "([^"]*)", nam2 "([^"]*)", cab2 "([^"]*)"[^}]*?drives \(([^)]*)\)', body):
        snap, nam, cab, nam2, cab2, dr = sm.groups()
        models = []
        for role, p in (("amp", nam), ("cab", cab), ("amp", nam2), ("cab", cab2)):
            if p:
                models.append({"role": role, "name": model(p)})
        for b, preset, opt in re.findall(r'\{block "([^"]+)", preset "([^"]+)", option (\d+)\}', dr):
            options = drives.get(preset, [])
            o = options[int(opt)] if int(opt) < len(options) else None
            models.append({"role": "drive", "name": o[1] if o and o[1] else preset, "pedal": preset, "option": o[0] if o else None})
        out[f"{kind}/{name}/{unq(snap)}"] = models

# Core variations: their Amp variation's models, plus their Drive's.
fx = json.load(open(FIXTURE))
for m in fx["compositions"]["modules"]:
    if m["module"] != "Core":
        continue
    for snap, info in zip(m["snapshots"], m["snapshot_info"]):
        models = []
        for pick in info.get("modules", []):
            models += out.get(f'{pick["module"]}/{pick["preset"]}/{pick["snapshot"]}', [])
        if models:
            out[f'Core/{m["name"]}/{snap}'] = models

json.dump(out, open(OUT, "w"), indent=1)
print(f"{len(out)} variations with models -> {os.path.relpath(OUT)}")
