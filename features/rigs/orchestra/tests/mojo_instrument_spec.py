#!/usr/bin/env python3
"""MOJO 2 Horn Section: one instrument's shipped values → `_patches/<instrument>/section.styx`.

usage: mojo_instrument_spec.py <library root> [<NKI name> …]   (default: every full instrument)

Reads `_ksp/<NKI name>/{persistent_1.tsv, groups.json, program.json}`; line
numbers are script_1.ksp (identical in every full instrument). Per
articulation (script order, `%ArtGroupStart` = persistent `%hr4to`):

- vel_gain_db: what the script sets the played groups' ENGINE_PAR_VOLUME to
  after `play_note` in velocity mode (33421-33431):
  `VelGainMin + (knob − VelGainMin)·(0.99^(v−1) − 1)/(0.99^126 − 1)`, with
  knob = the articulation's volume control (`%aal4d[art]`, read from its
  persistent value) and VelGainMin = `$hcxoq`; engine units → dB by the
  script's own law `(18000·log2(ep) − 346768.23)/1000` (25510).
- level_db: `change_vol(−$rvlgn · players)` per note (25480), 1 player;
  `players` carries the rest of the Ensemble (more players add −$rvlgn each).
- script_start_ms: `play_note`'s offset `$wv2n2 + $a0xhd` ms (25232),
  limited by each zone's start_mod (the engine does that).
- key_range: `%ArtRange` (persistent `%yqd5p`): outside it nothing plays (25170).
- rr / rr_random_avoid_last: `$1f1tu` round robins; RR mode `$axg22` = 1 →
  random, never the last pick (20460).
- amp_env: the articulation's Bus1 group volume ENV_AHDSR as flex segments
  — attack to 1 (held while the key is down), then the release to 0
  (`amp_env_release 1`). Hold/decay are 0 and sustain 1 on every shipped
  one-shot group; anything else is reported, not guessed.
"""
import csv, json, math, os, re, shutil, struct, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mojo_zones  # noqa: E402  (the group classifier)

ROOT = sys.argv[1] if len(sys.argv) > 1 else "/run/media/AudioHaven/Sampled/Horns/MOJO 2 Horn Section"

# Script articulation order (185-197) → spec id, its volume knob variable
# (`%aal4d[i] := get_ui_id(<knob>)`, 3275-8852) and whether it is modelled.
ARTS = [
    ("Sus", "$tnc12"), ("Stacc", "$gndi4"), ("Stabs", "$423xn"), ("BendDown", "$chdav"),
    ("RunDown", "$3hz2h"), ("RunUp", "$cekgd"), ("Doits", "$oijh1"), ("Rise", "$hbhgf"),
    ("Shakes", "$jzee2"), ("Trills", "$egrpi"), ("Swells", "$rekbd"), ("Cresc", "$yoa4p"),
    ("Falls", "$5naxv"),
]
MODELLED = {"Sus", "Stacc", "Stabs", "BendDown", "RunDown", "RunUp", "Doits", "Rise", "Shakes", "Trills", "Falls",
            "Swells", "Cresc"}


def persistent(path):
    out = {}
    for row in csv.reader(open(path), delimiter="\t"):
        if len(row) >= 2 and row[0] != "name":
            out[row[0]] = row[1].split()
    return out


def ep_db(ep):
    """Kontakt engine volume units → dB by the script's own conversion: what
    a `change_vol(..., 18000·log2(ep) − 346768)` applies (millidB, exact)."""
    return (18000.0 * math.log2(ep) - 346768.2342478351) / 1000.0


# `$ENGINE_PAR_VOLUME` is cubic in the setting, 0 dB at this value, +12 dB at
# 1_000_000 (KONTRA par_values::UNITY_VOLUME): 18.06 dB an octave of the
# setting, not the script's 18.
UNITY_VOLUME = 629960.5249474361


def volume_db(ep):
    """What Kontakt plays for an `$ENGINE_PAR_VOLUME` setting, dB: a group
    volume the script sets (unlike `ep_db`, a `change_vol`)."""
    return 60.0 * math.log10(ep / UNITY_VOLUME)


def vel_table(knob, vmin):
    den = 0.99 ** 126 - 1.0
    out = []
    for v in range(128):
        vv = max(v, 1)
        ep = vmin + int((knob - vmin) * ((0.99 ** (vv - 1) - 1.0) / den))  # real_to_int truncates
        out.append(round(volume_db(ep), 4))
    return out


RELEASE_TENSION = 0.5 + 2.662588 / 15.0


def ahdsr_env(groups, start):
    """The articulation's Bus1 group volume AHDSR → flex segments, or None."""
    g = next((g for g in groups if g["index"] == start), None)
    env = g and next((e for e in g["envelopes"] if e["kind"] == "ENV_AHDSR" and e["target"] == "volume"
                      and not e["bypassed"]), None)
    if not env:
        return None, f"group {start}: no volume AHDSR"
    if env["hold_ms"] or env["decay_ms"] or abs(env["sustain"] - 1.0) > 1e-6 or env["ahd_only"]:
        return None, (f"group {start}: AHDSR H {env['hold_ms']} D {env['decay_ms']} S {env['sustain']} "
                      f"ahd_only {env['ahd_only']} — not a plain attack/release")
    # curve 0 on every shipped one-shot group: a linear attack (tension 0.5).
    # The release is Kontakt's fall (KONTRA laws.rs FALL, k = ln(3/43)):
    # 1.075·(3/43)^t − 0.075, our flex shape at tension 0.5 + 2.6626/15.
    tension = 0.5 - 0.5 * env["curve"]
    return [(max(env["attack_ms"], 0.0), 1.0, tension), (env["release_ms"], 0.0, RELEASE_TENSION)], \
        env.get("intensity", 1.0)


def ahdsr_line(groups, start):
    """`amp_ahdsr { … }`: the Bus1 group's volume ENV_AHDSR as Kontakt runs
    it at its control rate (engine::kontakt_ahdsr), in place of `amp_env`'s
    segments; any hold, decay, sustain and AHD-only mode."""
    g = next(g for g in groups if g["index"] == start)
    env = next(e for e in g["envelopes"] if e["kind"] == "ENV_AHDSR" and e["target"] == "volume"
               and not e["bypassed"])
    return (f"        amp_ahdsr {{ attack_ms {fmt(env['attack_ms'])}, hold_ms {fmt(env['hold_ms'])}, "
            f"decay_ms {fmt(env['decay_ms'])}, sustain {fmt(env['sustain'])}, "
            f"release_ms {fmt(env['release_ms'])}, curve {fmt(env['curve'])}, "
            f"ahd_only {'true' if env['ahd_only'] else 'false'} }}")


def layer_depths(art, start, names, groups):
    """`amp_env_depths ( … )` lines: each layer's Bus1 group volume-AHDSR
    intensity (the layers of one articulation can differ: Alto Sax sustains
    1, 1, 0, 1; Kontakt ends a voice with its envelope whatever the depth),
    or nothing when every layer is at 1."""
    by_index = {g["index"]: g for g in groups}
    out, k = [], 0
    while True:
        gi = start + 4 * k
        cls = mojo_zones.classify(names.get(gi, "")) if gi in names else None
        if cls is None or cls[0] != art:
            break
        env = next((e for e in by_index[gi]["envelopes"] if e["kind"] == "ENV_AHDSR"
                    and e["target"] == "volume" and not e["bypassed"]), None)
        if env is not None:
            out.append((cls[1], env.get("intensity", 1.0)))
        k += 1
    if not out or all(d >= 0.999 for _, d in out):
        return []
    body = " ".join(f"{{dynamic {d}, depth {fmt(v)}}}" for d, v in out)
    return [f"        amp_env_depths ( {body} )  // per layer: the ENV -> volume intensity"]


def key_tables(mods, slot, bands):
    """Key Position routes on an EQ slot's band gains, as `(param, table)`:
    Kontakt reads a shaped Key source centred, frange_bi(table) = 2y − 1
    (BGroup::GenSysModScript 0x4bbd30, ExpFuncFRangeBi 0x237f8c), and a
    depth-1 EQ gain route moves the band 12 dB."""
    out = []
    for m in mods:
        t = m.get("target", "")
        if (m.get("source_kind") != "key" or m.get("slot") != slot or not t.startswith("eqGain")
                or not m.get("curve_enabled") or len(m.get("curve") or []) != 128):
            continue
        n = int(t[len("eqGain"):])
        if not 1 <= n <= bands:
            continue
        sign = -1.0 if m.get("negative") else 1.0
        out.append((f"eq_gain{n}", [sign * m["intensity"] * 12.0 * (2.0 * y - 1.0) for y in m["curve"]]))
    return out


def group_eq(art, start, names, fx_slots, notes, lp_floor, envs=None, mods=None):
    """`(dynamic, stages)` per layer of `art`: the active inserts of each
    layer's Bus1 group (the group the shipped Mix mic plays), as spec
    stages `(kind, params)` in slot order. Layers are the articulation's
    4-group blocks from `start` (dynamic named by mojo_zones.classify).

    - kind 7, filter type 22-24: the EQ;
    - kind 7, filter type 50: the one-layer velocity lowpass. The script
      sets its cutoff to min(1, v/127 + $SimDynMinLP/1e6) (33439-33452):
      a 0 cutoff plus a velocity route with that table;
    - kind 9: a stereo width (its parameter, the float at byte 9: side ×
      (1 + p), measured on Trumpet Stabs). Its output gain is on the zones
      (mojo_zones.py).
    Anything else is noted, not run."""
    out, k = [], 0
    envs = envs or {}
    mods = mods or {}
    while True:
        gi = start + 4 * k
        cls = mojo_zones.classify(names.get(gi, "")) if gi in names else None
        if cls is None or cls[0] != art:
            break
        dyn = cls[1]
        stages = []
        for s in sorted(fx_slots.get(gi, []), key=lambda s: s["slot"]):
            if s.get("bypass", True):
                continue
            if s["kind"] == 6:
                # Saturation: shape (float, byte 9) and mode (int, byte 13);
                # Enhanced (2) is decoded (KONTRA b6e0526), Drums (1) not yet
                ch = bytes.fromhex(s["children_hex"])
                shape = struct.unpack("<f", ch[9:13])[0] if len(ch) >= 13 else 0.0
                mode = ch[13] if len(ch) > 13 else 0
                out_db = 20 * math.log10(s.get("output_gain") or 1.0)
                if mode == 2:
                    stages.append(("saturation", {"shape": shape, "output_db": out_db}))
                else:
                    notes.append(f"{art} {dyn}: saturation mode {mode} (slot {s['slot']}) not run")
            elif s["kind"] == 3:
                # Kontakt's classic Limiter: children bytes 9..13 the input
                # gain (dB), 13..17 the release (ms) as KONTRA reads them
                # (BEffectLimiter); the slot's output gain after it
                ch = bytes.fromhex(s["children_hex"])
                inp = struct.unpack("<f", ch[9:13])[0] if len(ch) >= 13 else 0.0
                rel = struct.unpack("<f", ch[13:17])[0] if len(ch) >= 17 else 20.0
                stages.append(("limiter", {"input_db": inp, "release_ms": rel,
                                           "output_db": 20 * math.log10(s.get("output_gain") or 1.0)}))
            elif s["kind"] == 9:
                _, width = mojo_zones.group_stage({0: [s]}, 0)
                if width:
                    stages.append(("width", {"width": width}))
            elif s["kind"] == 7 and s.get("filter_type") in (22, 23, 24) and s.get("eq_bands"):
                stages.append(("eq", {"bands": [v for b in s["eq_bands"] for v in b],
                                      "key_tables": key_tables(mods.get(gi, []), s["slot"], len(s["eq_bands"]))}))
            elif s["kind"] == 7 and s.get("filter_type") == 50 and art in ("LegUp", "LegDown", "LegTail"):
                # the transition groups' lowpass: the script leaves it (only
                # octave legato sets it, 26112-26121) at its saved cutoff,
                # swept open by the group's ENV_FLEX on that slot
                flex = next((e for e in envs.get(gi, []) if e["kind"] == "ENV_FLEX"
                             and e["target"] == "filterCutoff" and e.get("slot") == s["slot"]
                             and not e["bypassed"]), None)
                stages.append(("sv_lp1", {"cutoff": float(s.get("cutoff") or 0.0),
                                          "resonance": float(s.get("resonance") or 0.0),
                                          "env": flex and [tuple(x) for x in flex["segments"][:3]],
                                          "intensity": flex and flex["intensity"]}))
            elif s["kind"] == 7 and s.get("filter_type") == 50:
                table = [min(1.0, v / 127.0 + lp_floor) for v in range(128)]
                stages.append(("sv_lp1", {"cutoff": 0.0, "resonance": float(s.get("resonance") or 0.0),
                                          "velocity_table": table}))
            else:
                notes.append(f"{art} {dyn}: insert kind {s['kind']} type {s.get('filter_type')} "
                             f"(slot {s['slot']}) not run")
        # controller → volume modulators (Kontakt's controller volume law,
        # run at the voice's amplifier: an insert route, KONTRA ModKind::Volume)
        for m in mods.get(gi, []):
            if (m.get("source_kind") != "cc" or m.get("target") != "volume" or m.get("slot") is not None
                    or not m.get("intensity")):
                continue
            if m.get("negative") or m.get("curve_enabled"):
                notes.append(f"{art} {dyn}: CC{m['cc']} volume (negative/shaped) not run")
                continue
            stages.append(("cc_volume", {"cc": m["cc"], "intensity": m["intensity"],
                                         "lag_ms": m.get("lag_ms") or 0}))
        if stages:
            out.append((dyn, stages))
        k += 1
    return out


def cc_starts(groups):
    """The controllers the modelled groups' volume routes read, at the start
    value Kontakt saves with the route (`cc_start`: CC113 at 100)."""
    out = {}
    for g in groups:
        for m in g.get("mods", []):
            if (m.get("source_kind") == "cc" and m.get("target") == "volume" and m.get("intensity")
                    and m.get("cc_start") is not None and mojo_zones.classify(g["name"])):
                out.setdefault(m["cc"], m["cc_start"])
    return sorted(out.items())


def cc_mirrors(ksp, pers):
    """`set_controller(N, %CC[$x])` in the script's `on controller` under
    `if ($CC_NUM=$x)`: controller $x (its saved value) is copied to N on
    every event (MOJO 2's CC1 → CC113, 37620-37626)."""
    src = open(os.path.join(ksp, "script_1.ksp"), errors="replace").read()
    out = []
    for to, var in re.findall(r"set_controller\((\d+),%CC\[(\$\w+)\]\)", src):
        if var in pers:
            out.append((int(pers[var][0]), int(to)))
    return sorted(set(out))


def release_numbers(ksp, pers):
    """Normal release (offset ms, fade ms): script_0 hands them over at load
    (persistent_0 `$RelNormOffset` / `$RelNormFade`); persistent_1's copies
    (`%ej4i2` / `%eg214`) can be cut short (Alto)."""
    p0 = {}
    path = os.path.join(ksp, "persistent_0.tsv")
    if os.path.exists(path):
        p0 = persistent(path)
    off = p0.get("$RelNormOffset", [pers["%ej4i2"][0]])[0]
    fade = p0.get("$RelNormFade", [pers["%eg214"][0]])[0]
    return float(off), float(fade)


def release_gain_table(rel_knob, sus_knob, vmin, trim_db):
    """The normal release sample's level by the note-on's raw velocity, dB
    (release callback 36892-36970): the release groups at 630000 (0 dB),
    + dB(release volume knob) + dB(sustain volume knob)
    + dB(VelGainMin + ⌊v·(630000 − VelGainMin)/127⌋) − the per-note trim."""
    out = []
    for v in range(128):
        ep = vmin + (v * (630000 - vmin)) // 127
        out.append(round(volume_db(630000) + ep_db(rel_knob) + ep_db(sus_knob) + ep_db(max(ep, 1)) + trim_db, 4))
    return out


def curved(vel):
    """The shipped LINEAR velocity curve: v′ = 1 + ⌊vel·126/127⌋."""
    return 1 + (vel * 126) // 127


# The articulations whose notes roll the key noise (sustain, staccato, stab,
# crescendo; not the FX one-shots: KONTRA, oneshots probe, 30 tries for 30 notes).
KEYNOISE_ARTS = ("Sus", "Stacc", "Stabs", "Swells", "Cresc")

# The legato's knobs per articulation (persistent_1, names from the
# script's own rename table, readable-script_1/ren.txt): on, mode CC,
# mode velocity, transition volume, transition length, its CC (37617+).
LEGATO_VARS = {
    "Sus": ("Sus", "$ecwxx", "$nf4pv", "$mbwiu", "$a5z5e", "$trtal", 28),
    "Stabs": ("Stab", "$eptsj", "$5nxil", "$tqwmj", "$zgicz", "$pscdl", 34),
    "Cresc": ("Cresc", "$5aghd", "$yg3qb", "$32bfr", "$vkcb3", "$gvbqa", 37),
}
LEGATO_SWITCH_CC = {"Sus": 26, "Stabs": 32, "Cresc": 35}

# Tempo-synced lengths (decode-swells-crescendos.md §1): persistent_0 prefix,
# the beats controller, the shipped beats (persistent_1).
TIMED = {"Swells": ("Swell", 20, "$mrp0d"), "Cresc": ("Cresc", 19, "$szm1z")}


def legato_numbers(p0, pers, knob, vmin, num_vels, art="Sus"):
    """The sustain legato (decode-legato.md): its switch and speed mode
    (persistent_1), the timing set (persistent_0 `$LegSus*`), the
    transition's level by velocity and its layer edges.

    Level, dB: the transition groups' VOLUME `VelGainMin + (750000 −
    VelGainMin)·(0.99^v′ − 1)/(0.99^127 − 1)` (26102-26109) + TransVol +
    the sustain's volume knob (change_vol, 26445-26458). Layer: `v′/(⌊127/
    nVel⌋ + 1) − (nVel − NumLegVels)`, ≥ 0 (26048-26052), as MIDI velocity
    edges."""
    g = lambda k: float(p0[k][0])
    one = lambda k: int(pers[k][0])
    pre, on_var, cc_var, vel_var, vol_var, len_var, len_cc = LEGATO_VARS[art]
    den = 0.99 ** 127 - 1.0
    trans_vol = one(vol_var)
    gains = []
    for v in range(128):
        vc = curved(max(v, 1))
        ep = vmin + int((750000 - vmin) * ((0.99 ** vc - 1.0) / den))
        gains.append(round(volume_db(ep) + ep_db(trans_vol) + ep_db(knob), 4))
    n_leg = int(p0["$MiscNumLegVels"][0])
    step = 127 // num_vels + 1
    layer = lambda vel: max(0, curved(max(vel, 1)) // step - (num_vels - n_leg))
    edges = [next(v for v in range(128) if layer(v) >= k) for k in range(1, n_leg)]
    mode = "cc" if one(cc_var) else "velocity" if one(vel_var) else "speed"

    def timing(L):
        t = {
            "fade_in_ms": L("FadeIn"), "trans_offset_ms": L("TransOffset"),
            "hold_small_ms": L("TransHoldSmall"), "hold_large_ms": L("TransHoldLarge"),
            "fade_out_ms": L("FadeOut"),
            "target_offset_ms": g("$MiscInstAttackOffsetMS") + L("TargetOffset"),
            "slow_small_pct": int(L("SlowSpeedSmall")), "fast_small_pct": int(L("FastSpeedSmall")),
            "slow_large_pct": int(L("SlowSpeedLarge")), "fast_large_pct": int(L("FastSpeedLarge")),
            "slowest_ms": int(L("SlowPlayingSpeed")), "fastest_ms": int(L("FastPlayingSpeed")),
        }
        if mode == "cc":
            t["speed_cc"] = len_cc
            t["speed_cc_default"] = one(len_var)
        elif mode == "velocity":
            t["speed_by_velocity"] = "true"
        return t

    # natural legato (velocity dynamics): NatLegGain instead of TransVol
    nat_gains = [round(x - ep_db(trans_vol) + float(p0["$MiscNaturalLegatoVol"][0]) / 1000.0, 4)
                 for x in gains]
    n_tail = int(p0["$MiscNumLegUpDownUnisTailsIntervals"][0])
    tail_step = 127 // n_tail + 1
    tail_edges = [next(v for v in range(128) if curved(max(v, 1)) // tail_step >= k) for k in range(1, n_tail)]
    octave = timing(lambda k: g(f"$OctLeg{pre}{k}"))
    natural = {"key": one("$qnlt3"), "on": one("$uzpex") == 1,
               "length_ms": g("$MiscNaturalLegatoLength"), "gains": nat_gains,
               "tail_edges": tail_edges, "tail_ms": g("$MiscNatOctLegTailLength")}
    L = lambda k: g(f"$Leg{pre}{k}")
    t = {
        "fade_in_ms": L("FadeIn"), "trans_offset_ms": L("TransOffset"),
        "hold_small_ms": L("TransHoldSmall"), "hold_large_ms": L("TransHoldLarge"),
        "fade_out_ms": L("FadeOut"),
        "target_offset_ms": g("$MiscInstAttackOffsetMS") + L("TargetOffset"),
        "slow_small_pct": int(L("SlowSpeedSmall")), "fast_small_pct": int(L("FastSpeedSmall")),
        "slow_large_pct": int(L("SlowSpeedLarge")), "fast_large_pct": int(L("FastSpeedLarge")),
        "slowest_ms": int(L("SlowPlayingSpeed")), "fastest_ms": int(L("FastPlayingSpeed")),
    }
    if mode == "cc":
        t["speed_cc"] = len_cc
        t["speed_cc_default"] = one(len_var)
    elif mode == "velocity":
        t["speed_by_velocity"] = "true"
    return {"on": one(on_var) == 1, "gains": gains, "edges": edges, "timing": t, "mode": mode,
            "octave": octave, "natural": natural,
            "trans_vol": trans_vol, "max_interval": int(p0["$MiscNumLegIntervals"][0]) - 1}


def release_types_lines(p0, pers, sus_knob, vmin, trim_db, ranges, one):
    """Sus's `release_types` (decode-release-types.md): the five types by
    the script's number (0 Normal, 1 Doit, 2 Fall, 3 Trill, 4 Shake), the
    purple keys (%nuppg), the orange keyswitches at velocity >= 120, the
    articulation cutoff ($hrmlz). Offsets/fades from persistent_0 (the
    persistent_1 copies can be cut short); gain = 630000 + type knob +
    SusVol + VelGainMin law − ensemble (release_gain_table)."""
    g = lambda k: float(p0[k][0])
    tail = 2 ** (one("$q1stw") / 73477 + 1) - 2
    rng = lambda i: f"key_range ({ranges[2 * i]} {ranges[2 * i + 1]}), "
    gains = lambda knob: " ".join(fmt(x) for x in release_gain_table(knob, sus_knob, vmin, trim_db))
    radio = lambda names: next((i for i, v in enumerate(names) if one(v) == 1), 0)
    fall_len = radio(("$pf4p2", "$3c0io", "$230ly"))
    trill_i = radio(("$qnlfz", "$nrvwv", "$nzt41", "$tw4id"))
    types = [
        # Normal: the note-on's layer; its range is the sustain's
        f"{{ artic SusRel, layered true, start_ms {fmt(g('$RelNormOffset'))}, fade_ms {fmt(g('$RelNormFade'))}, "
        f"tail_ms {fmt(tail)},\n                gain_db ({gains(one('$ues0b'))}) }}",
        f"{{ artic DoitRel, {rng(6)}start_ms {fmt(g('$RelDoitOffset'))}, fade_ms {fmt(g('$RelDoitFade'))}, "
        f"tail_ms {fmt(tail)},\n                gain_db ({gains(one('$5gpp0'))}) }}",
        # Fall: the length by CC18 (0-42 / 43-84 / 85-127), each its own offset and fade
        f"{{ artic FallRel, {rng(12)}start_ms {fmt(g('$RelFallOffset1'))}, fade_ms {fmt(g('$RelFallFade1'))}, "
        f"tail_ms {fmt(tail)}, variant_cc 18, variant_edges (43 85), variants (L2 L3 L4), "
        f"variant_default {fall_len},\n                variant_start_ms ({fmt(g('$RelFallOffset1'))} "
        f"{fmt(g('$RelFallOffset2'))} {fmt(g('$RelFallOffset3'))}), variant_fade_ms ({fmt(g('$RelFallFade1'))} "
        f"{fmt(g('$RelFallFade2'))} {fmt(g('$RelFallFade3'))}),\n                gain_db ({gains(one('$tawxh'))}) }}",
        # Trill: the interval by CC42 (0-31 / 32-63 / 64-95 / 96-127)
        f"{{ artic TrillRel, {rng(9)}start_ms {fmt(g('$RelTrillOffset'))}, fade_ms {fmt(g('$RelTrillFade'))}, "
        f"tail_ms {fmt(tail)}, variant_cc 42, variant_edges (32 64 96), variants (ht wt min3 maj3), "
        f"variant_default {trill_i},\n                gain_db ({gains(one('$oh1rm'))}) }}",
        f"{{ artic ShakeRel, {rng(8)}start_ms {fmt(g('$RelShakeOffset'))}, fade_ms {fmt(g('$RelShakeFade'))}, "
        f"tail_ms {fmt(tail)},\n                gain_db ({gains(one('$4oqce'))}) }}",
    ]
    keys = " ".join(str(int(x)) for x in pers["%nuppg"]) if "%nuppg" in pers else "96 97 100 99 98"
    return [
        "        // the release types (36445-37013, decode-release-types.md): latched by the",
        "        // purple keys, one-shot by Doits/Falls/Trills/Shakes keyswitches at >= 120",
        f"        release_types {{ latched {one('$x2fxp')}, keys ({keys}), override_velocity 120,",
        "            overrides (33 1 36 2 35 3 34 4), cutoff " + ("true" if one("$hrmlz") == 1 else "false") + ",",
        "            types (",
    ] + [f"                {t}" for t in types] + ["            ) }"]


def fmt(v):
    s = f"{v:.4f}".rstrip("0").rstrip(".")
    return s if s not in ("-0", "") else "0"


def fx_lines(fx):
    """A `group_fx` block for `group_eq`'s layers."""
    if not fx:
        return []
    out = ["        group_fx ("]
    for dyn, stages in fx:
        st, rt = [], []
        for n, (kind, p) in enumerate(stages):
            if kind == "eq":
                st.append(f"{{ kind eq, bands ({' '.join(fmt(v) for v in p['bands'])}) }}")
                for param, table in p.get("key_tables", []):
                    rt.append(f"{{ stage {n}, param {param}, cc 0, source key, "
                              f"table ({' '.join(fmt(v) for v in table)}) }}")
            elif kind == "width":
                st.append(f"{{ kind width, width {fmt(p['width'])} }}")
            elif kind == "saturation":
                st.append(f"{{ kind saturation, shape {p['shape']:.7f}, output_db {fmt(p['output_db'])} }}")
            elif kind == "cc_volume":
                rt.append(f"{{ stage 0, param volume, cc {p['cc']}, intensity {fmt(p['intensity'])}, "
                          f"lag_ms {fmt(p['lag_ms'])} }}")
                continue
            elif kind == "limiter":
                st.append(f"{{ kind limiter, input_db {fmt(p['input_db'])}, release_ms {fmt(p['release_ms'])}, "
                          f"output_db {fmt(p['output_db'])} }}")
            else:
                st.append(f"{{ kind {kind}, cutoff {fmt(p['cutoff'])}, resonance {fmt(p['resonance'])} }}")
                if p.get("env"):
                    segs = " ".join(f"{{time_ms {fmt(t)}, level {fmt(lv)}, curve {fmt(c)}}}" for t, lv, c in p["env"])
                    rt.append(f"{{ stage {n}, param cutoff, cc 0, intensity {fmt(p['intensity'])}, env ({segs}) }}")
                if p.get("velocity_table"):
                    rt.append(f"{{ stage {n}, param cutoff, cc 0, source velocity, "
                              f"table ({' '.join(fmt(v) for v in p['velocity_table'])}) }}")
        out.append(f"            {{ dynamic {dyn}, stages ({' '.join(st)}), routes ({' '.join(rt)}) }}")
    out.append("        )")
    return out


def build(nki):
    ksp = os.path.join(ROOT, "_ksp", nki)
    inst = re.sub(r"\s+2\.0$", "", nki)
    pers = persistent(os.path.join(ksp, "persistent_1.tsv"))
    p0 = persistent(os.path.join(ksp, "persistent_0.tsv"))
    g0 = lambda k: float(p0[k][0])
    groups = json.load(open(os.path.join(ksp, "groups.json")))
    program = json.load(open(os.path.join(ksp, "program.json")))
    one = lambda k: int(pers[k][0])
    starts = [int(x) for x in pers["%hr4to"]]
    ranges = [int(x) for x in pers["%yqd5p"]]
    num_rr, vmin = one("$1f1tu"), one("$hcxoq")
    if one("$xhoj1") != 2:
        raise SystemExit(f"{nki}: velocity curve {one('$xhoj1')} is not LINEAR — thresholds would move")
    rr_avoid = 1 if one("$axg22") == 1 and one("$mrsqg") == 0 else 0
    start_ms = one("$wv2n2") + one("$a0xhd")
    level_db = -one("$rvlgn") / 1000.0  # × 1 player

    buses = json.load(open(os.path.join(ksp, "buses.json")))
    bus1 = next(b for b in buses if b["index"] == 0)
    bus1_vol = bus1["pub"][0]
    gain_db = 20 * math.log10(program["volume"]) + 20 * math.log10(bus1_vol)
    fx_slots = {x["group"]: x["slots"] for x in json.load(open(os.path.join(ksp, "group_fx.json")))}
    names = {g["index"]: g["name"] for g in groups}
    gmods = {g["index"]: g.get("mods", []) for g in groups}

    lines = [
        f"// Generated by mojo_instrument_spec.py from MOJO 2 {nki} — do not edit.",
        f"// Instrument gain = program volume {program['volume']} × Bus 1 (the Mix mic's bus)",
        f"// fader {bus1_vol}. Not modelled yet: the active instrument insert (slot 7,",
        "// kind 11), the one-layer articulations' velocity lowpass (slot 7, type 50).",
        f"instrument_gain_db {fmt(gain_db)}",
        # the Ensemble (decode-players-divisi.md): players CC $m2uxz in bands
        # of 12, Auto Divisi CC $045sg, the shipped count $gl1sy and divisi
        # $jlrfr, Humanize $5l4xp, Stereo Spread $nbpgd, the humanize maxima
        # (ReadPGSValues 20838-20840), EnsAttenPerPlayer $rvlgn (the
        # articulations' level_db carries one player's), DivisiBufferMs
        # $gyc05; steal / ResetDivisi fades 200 / 250 ms (25340, 22801)
        f"players {{ count_cc {one('$m2uxz')}, divisi_cc {one('$045sg')}, count_band 12, "
        f"count {one('$gl1sy')}, divisi {'true' if one('$jlrfr') else 'false'}, "
        f"humanize {one('$5l4xp')}, spread {one('$nbpgd')}, detune_max_mcents {one('$tiuuq')}, "
        f"volume_max_mdb {one('$fa4bb')}, offset_max_ms {one('$vjiqj')}, "
        f"attenuation_mdb {one('$rvlgn')}, buffer_ms {one('$gyc05')}, steal_fade_ms 200, reset_fade_ms 250 }}",
        # controllers read at their saved start value until MIDI moves them,
        # and the ones the script copies (its `on controller`)
        "start_ccs (" + " ".join(f"{{ cc {c}, value {v} }}" for c, v in cc_starts(groups)) + ")",
        "mirror_ccs (" + " ".join(f"{{ from {a}, to {b} }}" for a, b in cc_mirrors(ksp, pers)) + ")",
        "articulations (",
    ]
    notes = []
    for i, (art, knob_var) in enumerate(ARTS):
        if art not in MODELLED:
            continue
        knob = one(knob_var)
        env, why = ahdsr_env(groups, starts[i])
        depth = None
        if isinstance(why, str):
            notes.append(f"{art}: {why}")
        elif why is not None and why < 0.999:
            depth = why
        fx = group_eq(art, starts[i], names, fx_slots, notes, one("$lrshn") / 1e6, None, gmods)
        table = vel_table(knob, vmin)
        lines += [
            "    {",
            f"        id {art}",
            f"        rr {num_rr}",
            f"        rr_random_avoid_last {rr_avoid}",
            f"        script_start_ms {start_ms}",
            f"        level_db {fmt(level_db)}",
            f"        key_range ({ranges[2 * i]} {ranges[2 * i + 1]})",
            f"        // knob {knob} → {fmt(ep_db(knob))} dB at velocity 127; VelGainMin {vmin} → {fmt(ep_db(vmin))} dB",
            f"        vel_gain_db ({' '.join(fmt(x) for x in table)})",
        ]
        if env:
            body = " ".join(f"{{time_ms {fmt(t)}, level {fmt(lv)}, curve {fmt(c)}}}" for t, lv, c in env)
            lines += [f"        amp_env ( {body} )", "        amp_env_hold true", "        amp_env_release 1"]
            lines.append(ahdsr_line(groups, starts[i]))
            if depth is not None:
                lines.append(f"        amp_env_depth {fmt(depth)}  // the ENV -> volume route's intensity")
            lines += layer_depths(art, starts[i], names, groups)
        if art in TIMED:
            pre, cc, ship = TIMED[art]
            bpm = int(p0[f"${pre}BPM"][0])
            beats = [int(p0[f"${pre}{i}NumBeats"][0]) for i in range(1, 6)]
            beats = [b for b in beats if b > 0]
            rec = [int((60000000 // bpm) * b / 1000.0) for b in beats]
            lines.append(f"        // tempo-synced (20249-20328): recorded at {bpm} BPM, {beats} beats")
            lines.append(f"        timed_length {{ takes ({' '.join(f'L{i + 1}' for i in range(len(rec)))}), "
                         f"rec_ms ({' '.join(map(str, rec))}), min_speed_pct {int(p0[f'${pre}MinSpeed'][0])}, "
                         f"beats_cc {cc}, beats {one(ship)} }}")
        if art in KEYNOISE_ARTS:
            # the key noise (25520-25557): chance $1stdn, level $zlhlf (0 =
            # silent, still drawn), the art's knob, the click groups' volume
            # by curved velocity, ClickFadeMs
            zl = one("$zlhlf")
            den = 0.99 ** 127 - 1.0
            kg = []
            for v in range(128):
                ep = 500000 + int(500000 * ((0.99 ** curved(max(v, 1)) - 1.0) / den))
                kg.append(round(volume_db(ep) + ep_db(zl) + ep_db(knob), 4) if zl > 0 else -144.0)
            lines.append(f"        keynoise {{ artic Keyclick, key 64, chance_pct {one('$1stdn')}, "
                         f"fade_ms {fmt(g0('$MiscClickFade'))}, force_key {one('$erctx')}, "
                         f"gain_db ({' '.join(fmt(x) for x in kg)}) }}")
        if art in LEGATO_VARS:
            leg = legato_numbers(p0, pers, knob, vmin, 4, art)
            timing = ", ".join(f"{k} {fmt(v) if isinstance(v, float) else v}" for k, v in leg["timing"].items())
            # the switch: CC26 / CC32 / CC35 above 63 (37912-37918, 38259, 38304)
            extra = ""
            if art == "Cresc":
                # no bypass; the transition layer fixed; key-up fades the
                # target over Exp2(cresc release knob) (30965, 37596-37612)
                rel = 2 ** (one("$ukiha") / 73477 + 1) - 2
                extra = (f", fixed_layer {int(p0['$MiscCrescLegTransitionDynLayer'][0])}, "
                         f"release_fade_ms {fmt(rel)}")
            if art == "Stabs":
                # stab legato only within the bypass of the last connection
                # (28665); its key-up fades the target over Exp2(release knob)
                rel = 2 ** (one("$oumez") / 73477 + 1) - 2
                extra = f", bypass_ms {one('$cbfyl')}, release_fade_ms {fmt(rel)}"
            fmt_t = lambda tt: ", ".join(f"{k} {fmt(v) if isinstance(v, float) else v}" for k, v in tt.items())
            # octave legato: pedal (CC64) and a leap past 3 keys (25595), the
            # Octave_Legato groups at layer 0, the $OctLeg* set
            extra += (f",\n            octave {{ up OctLegUp, down OctLegDown, pedal_cc 64, min_leap 3, "
                      f"timing {{ {fmt_t(leg['octave'])} }} }}")
            if art == "Sus":
                # natural legato: latched key $qnlt3, $uzpex, NatLegLen,
                # NatLegGain, the unison tails (decode-legato.md §2-§4)
                n = leg["natural"]
                extra += (f",\n            natural {{ key {n['key']}, on {'true' if n['on'] else 'false'}, "
                          f"length_ms {fmt(n['length_ms'])}, tail LegTail, "
                          f"tail_layer_thresholds ({' '.join(map(str, n['tail_edges']))}), tail_ms {fmt(n['tail_ms'])},\n"
                          f"                gain_db ({' '.join(fmt(x) for x in n['gains'])}) }}")
            lines += [
                f"        // the {art} legato (decode-legato.md); speed by {leg['mode']}; TransVol {leg['trans_vol']}",
                f"        script_legato {{ enable_cc {LEGATO_SWITCH_CC[art]}, on {'true' if leg['on'] else 'false'}, "
                f"up LegUp, down LegDown{extra},",
                f"            max_interval {leg['max_interval']}, layer_thresholds ({' '.join(map(str, leg['edges']))}),",
                f"            gain_db ({' '.join(fmt(x) for x in leg['gains'])}),",
                f"            timing {{ {timing} }} }}",
            ]
        if art == "Sus":
            lines += ["        // each new sustain note fades sounding releases (25218)",
                      "        fade_releases_ms 75"]
            lines += release_types_lines(p0, pers, knob, vmin, level_db, ranges, one)
        lines += fx_lines(fx)
        lines.append("    }")
    # The normal release articulation (`$MiscLongNVNormalRelGroupStart`): its
    # zones' RR, the release groups' inserts, and its envelope — fade_in over
    # RelFade from the note-off, then fade_out over 2^(SusRel/73477+1) − 2 ms.
    off, fade = release_numbers(ksp, pers)
    tail_ms = 2 ** (one("$q1stw") / 73477 + 1) - 2
    # The legato transition groups: their envelope, inserts and the
    # ensemble trim (the transition's level is its `script_legato` table).
    for art, var in (("LegUp", "$MiscLegUpGroupStart"), ("LegDown", "$MiscLegDownGroupStart"),
                     ("OctLegUp", "$MiscOctLegUpGroupStart"), ("OctLegDown", "$MiscOctLegDownGroupStart"),
                     ("LegTail", "$MisLegUpDownUnisonTailsGroupStart"),
                     ("Keyclick", "$KeyClickGroupStart")):
        gstart = int(p0[var][0])
        env, why = ahdsr_env(groups, gstart)
        if isinstance(why, str):
            notes.append(f"{art}: {why}")
        envs = {g["index"]: g.get("envelopes", []) for g in groups}
        fx = group_eq(art, gstart, names, fx_slots, notes, one("$lrshn") / 1e6, envs, gmods)
        lines += ["    {", f"        id {art}", f"        level_db {fmt(level_db)}"]
        if env:
            body = " ".join(f"{{time_ms {fmt(t)}, level {fmt(lv)}, curve {fmt(c)}}}" for t, lv, c in env)
            lines += [f"        amp_env ( {body} )", "        amp_env_hold true", "        amp_env_release 1"]
            lines.append(ahdsr_line(groups, gstart))
            if why is not None and why < 0.999:
                lines.append(f"        amp_env_depth {fmt(why)}  // the ENV -> volume route's intensity")
        elif isinstance(why, str) and "no volume AHDSR" in why:
            # no volume envelope in the group (the key clicks): full level
            # from the first frame, not the engine's family default
            lines += ["        amp_env ( {time_ms 0, level 1, curve 0.5} )", "        amp_env_hold true"]
        lines += fx_lines(fx)
        lines.append("    }")
    # The release articulations (their groups' RR, inserts and AHDSR; the
    # fades and the 9950 ms tail are the release type's own, release_types)
    for art, var in (("SusRel", "$MiscLongNVNormalRelGroupStart"), ("DoitRel", "$MiscLongDoitRelGroupStart"),
                     ("FallRel", "$MiscLongFallRelGroupStart"), ("TrillRel", "$MiscLongTrillRelGroupStart"),
                     ("ShakeRel", "$MiscLongShakeRelGroupStart")):
        gstart = int(p0[var][0])
        env, why = ahdsr_env(groups, gstart)
        fx = group_eq(art, gstart, names, fx_slots, notes, one("$lrshn") / 1e6, None, gmods)
        lines += ["    {", f"        id {art}", f"        rr {num_rr}", f"        rr_random_avoid_last {rr_avoid}"]
        if env:
            body = " ".join(f"{{time_ms {fmt(t)}, level {fmt(lv)}, curve {fmt(c)}}}" for t, lv, c in env)
            lines += [f"        amp_env ( {body} )", "        amp_env_hold true", "        amp_env_release 1"]
            lines.append(ahdsr_line(groups, gstart))
        lines += fx_lines(fx)
        lines.append("    }")
    lines.append(")")

    patch = os.path.join(ROOT, "_patches", inst)
    os.makedirs(patch, exist_ok=True)
    dst = os.path.join(patch, "section.styx")
    new = "\n".join(lines) + "\n"
    if os.path.exists(dst):
        if open(dst).read() == new:
            print(f"{inst}: unchanged")
            return
        shutil.copy2(dst, dst + time.strftime(".pre-%Y-%m-%d-%H%M%S"))
    open(dst, "w").write(new)
    print(f"{inst}: RR {num_rr}, start {start_ms} ms, trim {fmt(level_db)} dB"
          + ("".join(f"\n    note: {n}" for n in notes)))


def main():
    names = sys.argv[2:] or sorted(
        d for d in os.listdir(os.path.join(ROOT, "_ksp")) if " - " not in d and d.endswith("2.0")
    )
    for n in names:
        build(n)


if __name__ == "__main__":
    main()
