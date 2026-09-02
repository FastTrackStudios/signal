#!/usr/bin/env python3
"""Turn a param_scan result into a measurement plan.

The scan says what each control does; this decides what to measure. It is a
separate step on purpose — the decisions below are judgement, and keeping
them out of the scan means the plan can be regenerated or hand-edited
without re-measuring anything.

The shape of a plan, and why:

  drive       The top-ranked continuous control. Swept with THD-spaced
              settings so the captures span the unit's quietest to loudest
              distortion rather than its knob positions.
  second      The runner-up, if it moves distortion by more than 3x. Several
              units have two: the Distressor saturates at Input *and* at
              Output, the LA-3A at Peak Reduction and HF Emphasis.
  modes       Every discrete control with more than two states — Ratio,
              Time Const, Detector, Audio. These are enumerated, never
              swept: sampling a Distressor's eight-state Ratio at even steps
              lands twice on the same value and misses NUKE entirely.
  pins        Controls held fixed so they cannot confound the measurement.

Modes are measured one at a time, with everything else at its default,
rather than as a full cartesian product. The product is not affordable —
the Distressor alone would be 8 x 8 x 6 states against a drive axis and a
level axis, some 55,000 renders — and it is not needed: what a model wants
from a mode is how that mode changes the curve, which one axis at a time
answers.

    make-plan.py <scan.json> [--out plan.json]
"""
import json, sys, os

# Controls that win a THD-span ranking without driving anything.
SWITCH_WORDS = ("bypass", "power", "meter", "mix", "blend", "dry", "wet")
# Filters change measured THD by removing harmonics rather than by making
# them, so they are never a drive axis however well they rank.
FILTER_WORDS = ("filter", "cut", "hpf", "lpf", "sc ", "s/c", "sidechain")
# Routing and mode switches. `Path Select` on the Ampex ATR-102 scored
# 776,231x by switching the tape path in and out — the largest span in the
# whole fleet, and not a drive control. Its actual drive, `L Rec Level`,
# ranked below it at 58x.
ROUTING_WORDS = ("select", "enable", "path", "link", "auto", "cal", "phase",
                 "polarity", "midi cc")

# A drive control has to actually make distortion, not merely a large *ratio*
# of it. The Hitsville EQ ranked a frequency-band selector first on a 4.1x
# span between 0.00003% and 0.00014% THD — a ratio of two noise-floor
# readings — and 138 capture jobs were spent measuring silence before this
# floor existed.
MIN_DRIVE_THD_PERCENT = 0.01
# ...and it has to *change* the distortion. The Manley Massive Passive clears
# the floor above (0.061% THD) while every one of its controls has a span of
# exactly 1.0x, because its bands sit disabled at default and nothing it
# offers alters the signal. Distortion that is present but immovable is a
# constant, not an axis.
MIN_DRIVE_SPAN = 1.5
# ...and a drive control moves the *level*, because that is what driving
# something means. This is what separates a drive from a mode, and it does it
# without matching on names: the Studer A800's `IPS` and `Path Select` both
# scored a 991,390x THD span by having one near-silent state, but move the
# gain by 0.5 and 0.3 dB respectively, against `Bias` at 39.1 dB. The
# Fairchild's `Bal` is the same story at 0.2 dB.
MIN_DRIVE_GAIN_SPAN_DB = 3.0

def is_switch(name):
    n = name.lower()
    return any(w in n for w in SWITCH_WORDS)

def is_filter(name):
    n = name.lower()
    return any(w in n for w in FILTER_WORDS)

def is_routing(name):
    n = name.lower()
    return any(w in n for w in ROUTING_WORDS)

def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    scan_path = sys.argv[1]
    scan = json.load(open(scan_path))
    params = scan["parameters"]
    by_name = {p["name"]: p for p in params}

    def usable_drive(p):
        if is_switch(p["name"]) or is_filter(p["name"]) or is_routing(p["name"]):
            return False
        if p["kind"] == "discrete" and len(p["states"]) <= 2:
            return False
        if (p.get("thd_max_percent") or 0.0) < MIN_DRIVE_THD_PERCENT:
            return False
        if (p.get("thd_span_ratio") or 0.0) < MIN_DRIVE_SPAN:
            return False
        gain_span = (p.get("gain_max_db") or 0.0) - (p.get("gain_min_db") or 0.0)
        if gain_span < MIN_DRIVE_GAIN_SPAN_DB:
            return False
        return p.get("thd_span_ratio") is not None

    # Ranked by span — how far a control moves the distortion — which is the
    # right question once the gates above have removed the controls that earn
    # a large span without driving anything.
    ranked = sorted(
        (p for p in params if usable_drive(p)),
        key=lambda p: p["thd_span_ratio"],
        reverse=True,
    )
    drive = ranked[0]["name"] if ranked else None
    second = ranked[1]["name"] if len(ranked) > 1 and ranked[1]["thd_span_ratio"] > 3.0 else None

    modes = [
        {"name": p["name"],
         "states": [{"value": s["value"], "text": s["text"]} for s in p["states"]]}
        for p in params
        if p["kind"] == "discrete" and len(p["states"]) > 2 and not is_switch(p["name"])
    ]

    # Pins: anything that would confound every measurement if left at a
    # surprising default. Mix must be fully wet or the saturation is measured
    # blended with the dry signal.
    pins = []
    for p in params:
        n = p["name"].lower()
        if n == "mix" or n.endswith(" mix"):
            pins.append({"name": p["name"], "value": p["max"], "why": "fully wet"})
        elif "bypass" in n and p.get("default", 0.0) != 0.0:
            pins.append({"name": p["name"], "value": p["min"], "why": "not bypassed"})

    # Units where no control drives the nonlinearity are not un-measurable —
    # their saturation is driven by how hard they are *hit*. A passive EQ has
    # no drive knob at all; its output amplifier saturates on level. So those
    # get a level sweep instead of a drive sweep, which is the measurement
    # that will actually find something.
    kind = "drive" if drive else "level"

    plan = {
        "plugin_name": scan["plugin_name"],
        "plugin_path": scan["plugin_path"],
        "kind": kind,
        "drive": drive,
        "drive_span_ratio": ranked[0]["thd_span_ratio"] if ranked else None,
        "drive_max_thd_percent": ranked[0]["thd_max_percent"] if ranked else None,
        "second_axis": second,
        "modes": modes,
        "pins": pins,
        "note": None,
    }
    if drive is None:
        best = max((p.get("thd_max_percent") or 0.0) for p in params) if params else 0.0
        plan["note"] = (
            f"No control drives this unit's distortion (best any control reached "
            f"was {best:.5f}% THD at the scan's probe level). Measuring a level "
            f"sweep instead: if the saturation is level-driven, as a passive EQ's "
            f"output stage is, that is what finds it."
        )

    out = sys.argv[sys.argv.index("--out") + 1] if "--out" in sys.argv else None
    text = json.dumps(plan, indent=2)
    if out:
        os.makedirs(os.path.dirname(out), exist_ok=True)
        open(out, "w").write(text)
        n_states = sum(len(m["states"]) for m in plan["modes"])
        print(f"{plan['plugin_name']}: drive={drive} second={second} "
              f"modes={len(modes)} ({n_states} states) -> {out}")
    else:
        print(text)
    return 0

if __name__ == "__main__":
    sys.exit(main())
