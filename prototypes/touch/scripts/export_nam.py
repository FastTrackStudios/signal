#!/usr/bin/env python3
"""What each variation loads, from the rig's own config
(~/.config/signal/rig/modules.styx, drive-presets.styx, blocks.styx), for
the prototype's browser:

  src/data/nam.json    Amp / Drive variations -> the NAM models they load
                       (names only, no paths)
  src/data/algos.json  Delay / Reverb / Time variations -> each block's
                       algorithm; block presets -> their algorithm (delay
                       style, reverb algorithm, modulation engine)

Algorithm names follow the Signal app's tables (features/rigs/guitar/ui/
src/control.rs DELAY_ALGOS, VERB_ALGOS, MOD_ENGINES)."""

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
        continue  # Core variations show no models: their Amp and Drive do
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

json.dump(out, open(OUT, "w"), indent=1)
print(f"{len(out)} variations with models -> {os.path.relpath(OUT)}")

# ── Algorithms ─────────────────────────────────────────────────────────
DELAY = ["Tape", "Digital", "Analog", "Lo-Fi", "Shimmer", "Reverse", "Ice", "Rhythm", "Drum", "Oil Can", "MultiTap", "Spectral", "Filter"]
VERB = ["Room", "Hall", "Plate", "Spring", "Cloud", "Bloom", "Shimmer", "Chorale", "Magneto", "NonLinear", "Swell", "Reflections", "Velvet", "FreeVerb", "Convolution"]
MOD = ["Cubic", "BBD", "Tape", "Orbit", "Juno", "CE-2", "Dimension", "Clone", "Tri-Chorus", "SCF", "Julia"]

bl = open(os.path.join(RIG, "blocks.styx")).read()
blocks = {}
for m in re.finditer(r'\{block_type (\w+), name "([^"]+)", params \(([^)]*)\), bypass (\w+)', bl):
    btype, name, params, bypass = m.groups()
    p = {k: float(v) for k, v in re.findall(r"\{param (\w+), value ([-0-9.e]+)\}", params)}
    algo = None
    if bypass == "true":
        algo = "off"
    elif btype == "delay" and "style" in p:
        algo = DELAY[int(p["style"])] if int(p["style"]) < len(DELAY) else None
    elif btype == "reverb" and "algorithm" in p:
        algo = VERB[int(p["algorithm"])] if int(p["algorithm"]) < len(VERB) else None
    elif btype in ("chorus", "flanger", "vibrato") and "engine" in p:
        algo = MOD[int(p["engine"])] if int(p["engine"]) < len(MOD) else None
    blocks[name] = {"type": btype, "algo": algo}

algos = {f"block:{v['type']}/{k}": v["algo"] for k, v in blocks.items() if v["algo"]}
snaps = {}
for m in re.finditer(r"\{module (\w+), name ([^,]+), snapshots \(", mods):
    kind, name = m.group(1), unq(m.group(2))
    if kind not in ("Delay", "Reverb", "Time"):
        continue
    body = block(mods, m.end())
    for sm in re.finditer(r"\{name ([^,]+), nam [^}]*?blocks \(([^)]*)\), modules \(([^)]*)\)", body):
        snap, bls, subs = sm.groups()
        snaps[f"{kind}/{name}/{unq(snap)}"] = {
            "blocks": [{"block": b, "preset": pr, "algo": blocks.get(pr, {}).get("algo")} for b, pr in re.findall(r'\{block "([^"]+)", preset "([^"]+)"\}', bls)],
            "modules": re.findall(r"\{module (\w+), preset ([^,]+), snapshot ([^}]+)\}", subs),
        }
for key, v in snaps.items():
    lines = [{"block": b["block"], "preset": b["preset"], "algo": b["algo"]} for b in v["blocks"] if b["algo"] and b["algo"] != "off"]
    # A Time variation: its Delay's and Reverb's.
    for mod, pr, sn in v["modules"]:
        sub = snaps.get(f"{mod}/{unq(pr)}/{unq(sn)}")
        if sub:
            lines += [{"block": b["block"], "preset": b["preset"], "algo": b["algo"]} for b in sub["blocks"] if b["algo"] and b["algo"] != "off"]
    algos[key] = lines
ALGOS = os.path.join(HERE, "..", "src", "data", "algos.json")
json.dump(algos, open(ALGOS, "w"), indent=1)
print(f"{len(algos)} algorithms -> {os.path.relpath(ALGOS)}")
