#!/usr/bin/env python3
"""Shreddage 3.5: one decoded NKI's shipped numbers → `_patches/<instrument>/section.styx`.

usage: shreddage_section.py <library root> <NKI name> <instrument>

The overlay the sampler applies over specs/shreddage-<lib>.styx. Every
number comes from the decode (`_ksp/<NKI>/`: persistent_0.tsv = shipped
values, script_0.ksp, groups.tsv, program.json, buses.json); the model it
follows is `_notes/engine-model.md` (Abyss). Nothing is typed in by hand.

- instrument_gain_db: program volume × the channel bus's volume.
- fretboard: open strings `%str___string_meta` (even entries, string 1
  first); frets = the articulation fret limit (24); a fixed hand
  (engine_pars[4] = n > 0 → fret max(n, capo) − 1, L7493); hand widths 4 on
  string 1, else the gap to the string above − 1 (L7516); placement weights
  `1 << %str___hierarchy[6·mode + 1..5]` for the shipped fretting mode
  engine_pars[11]; RR orders `%s3___rr_seed` (4096 per RR count 2..8);
  borrow chance engine_pars[21] ‰ when engine_pars[20] = 1, the LCG seed
  declared on `$math__rand_seed` and its multiplier from the script;
  same-note retrigger fade 400 ms (`fade_out(400000)`, engine-model §4).
- per articulation (TACT index → our id, ARTICS): velocity-layer edges from
  `%tact___table_matrix` table 0; vel_gain_db from `%vel_db` by the
  articulation's Vel→Vol (`%tact___matrix[128·a + 27]`) and the velocity
  curve, plus the −1 mdB `volume_offset` (L8028); level_db = its group
  volume (groups.tsv); amp_env = its group AHDSR attack (curve → tension
  0.5 − curve/2) and release, held.
"""
import csv, math, os, re, sys

# Abyss TACT articulation index → (our id, RR count from the zones).
# Abyss TACT articulation index → our id (`!abyss_artic_names`), and the
# group each one's envelope and level are read from.
NAMES = {0: "Sustain", 1: "Staccato", 2: "Pop", 3: "Tapping", 4: "Harmonics", 5: "FX",
         6: "HammerPull", 7: "Slide", 8: "PitchedRelease"}
ARTICS = {0: "Sustain", 1: "Staccato", 2: "Pop", 3: "Tapping", 4: "Harmonics", 5: "FX",
          6: "HammerPull", 7: "Slide", 8: "PitchedRelease"}
FIRST_GROUP = {"Sustain": "Sustain", "Staccato": "Staccato", "Pop": "Pops", "Harmonics": "Harmonics",
               "HammerPull": "Hammer-on/Pull-off", "Slide": "Slides", "PitchedRelease": "Pitched Release",
               "Tapping": "Hammer-on/Pull-off", "FX": "FX"}

# Legato (Slide)'s fade bases, `%tact___meta[64·7 + 12..16]` (built at init,
# not shipped; engine-model §1d): source out (fast), transition in,
# transition length, transition out / source out, destination in (µs·10).
SLIDE_META = [500, 500, 4000, 1000, 1000]


def slide_time_us(d, length_us):
    """A slide's transition time (L17640): f = ((clamp(|d|,3,12) − 3)·99/9
    + 1)/100 of the transition length, truncated to µs."""
    f = (((min(max(d, 3), 12) - 3) * 99 / 9) + 1) / 100
    return int(f * length_us)


def ksp_list(script, name):
    """A `declare %name[n] := (a, b, …)` list's entries (−1 = unused)."""
    m = re.search(rf"declare %{name}\[\d+\] := \(([^)]*)\)", script)
    return [int(x) for x in m.group(1).split(",") if int(x) >= 0]


def persistent(path):
    out = {}
    with open(path) as f:
        for line in f:
            k, _, v = line.rstrip("\n").partition("\t")
            out[k] = v.split()
    return out


def ints(p, k, n=None):
    v = [int(x) for x in p[k]]
    if n is not None:
        v += [0] * (n - len(v))
    return v


# Kontakt's AHDSR, as KONTRA (Apache-2.0) measured it on Kontakt 8.13:
# an attack is silent its first 33 frames at 48 kHz and runs its curve in
# the time left; segments follow expm1(k·t)/expm1(k), with the attack's k
# from its stored curve c (law/src/formula.rs attack_shape, SPAN 5e5,
# FLOOR 2e4) and decay/release k = −2.662588 (format/kontakt/src/laws.rs
# FALL). Our flex segments use the same law with a = (0.5 − tension)·15.
LAG_MS = 33 / 48.0
FALL_K = -2.662588


def attack_k(c):
    c = max(-1.0, min(1.0, c))
    b = math.exp((1 - abs(c)) * math.log(5e5) - math.log(2e4))
    return math.log(b / (1 + b)) if c > 0 else math.log((1 + b) / b)


def slide_line(matrix, ep, script):
    x = matrix[128 * 7 + 6: 128 * 7 + 11]
    us = [x[i] * SLIDE_META[i] // 10 for i in range(5)]
    times = " ".join(str(slide_time_us(d, us[2])) for d in range(1, 25))
    srcs = " ".join(NAMES[a] for a in ksp_list(script, "s3__slidable_artics"))
    return (f"    slide {{ key {matrix[128 * 7 + 11]}, articulation Slide, sources ({srcs}), "
            f"from (Sustain Tapping), max_interval 24, time_us ({times}), "
            f"source_fade_ms {us[3] / 1000}, transition_fade_in_ms {us[1] / 1000}, "
            f"transition_fade_out_ms {us[3] / 1000}, body_offset_ms 99.999, "
            f"body_fade_in_ms {us[4] / 1000}, age_db_per_s {-1 * ep[29]} }}")


def kontakt_env(attack_ms, curve, release_ms):
    """[(time_ms, level, tension)]: lag, attack, release."""
    tension = lambda k: 0.5 - k / 15.0
    segs = []
    if attack_ms > 0:
        lag = min(LAG_MS, attack_ms)
        segs.append((lag, 0.0, 0.5))
        segs.append((attack_ms - lag, 1.0, tension(attack_k(curve))))
    else:
        segs.append((0.0, 1.0, 0.5))
    # the release is the voice's own note-off fade (release_ms_by_held on
    # Kontakt's fall law, the spec's release_curve_k): not an envelope segment
    return segs


def fmt(x):
    s = f"{x:.4f}".rstrip("0").rstrip(".")
    return s if s not in ("-0", "") else "0"


def main():
    root, nki, inst = sys.argv[1:4]
    k = os.path.join(root, "_ksp", nki)
    from shreddage_zones import load_meta
    m = load_meta(k)
    if m:
        import shreddage_section_meta
        return shreddage_section_meta.main(root, nki, inst, m[0])
    p = persistent(os.path.join(k, "persistent_0.tsv"))
    script = open(os.path.join(k, "script_0.ksp")).read()
    groups = {r["name"]: r for r in csv.DictReader(open(os.path.join(k, "groups.tsv")), delimiter="\t")}

    import json
    program = json.load(open(os.path.join(k, "program.json")))
    buses = json.load(open(os.path.join(k, "buses.json")))
    bus = next(b for b in buses if b["index"] == 0)
    gain_db = 20 * math.log10(program["volume"]) + 20 * math.log10(bus["pub"][0])

    ep = ints(p, "%s3__engine_pars", 90)
    meta = ints(p, "%str___string_meta")
    opens = meta[0::2]
    capo = ep[77]
    hand = max(ep[4], capo) - 1 if ep[4] > 0 else None
    if hand is None:
        sys.exit("automatic hand position: not modelled yet")
    mode = ep[11]
    m = re.search(r"declare %str___hierarchy\[24\] := \(([^)]*)\)", script)
    hier = [int(x) for x in m.group(1).split(",")][6 * mode: 6 * mode + 6]
    widths = [4] + [opens[s - 1] - opens[s] - 1 for s in range(1, len(opens))]
    if mode == 2:
        sys.exit("Moving Lead's sliding hand window: not modelled yet")
    timer_ms = int(re.search(r"set_listener\(\$NI_SIGNAL_TIMER_MS,(\d+)\)", script).group(1)) / 1000
    seed = int(re.search(r"declare \$math__rand_seed := (\d+)", script).group(1))
    mult = int(re.search(r"\$math__rand_seed := (\d+)\*\$math__rand_seed\+1", script).group(1))
    rr_seed = ints(p, "%s3___rr_seed", 4096 * 7)
    table = ints(p, "%tact___table_matrix", 512 * 9)
    matrix = ints(p, "%tact___matrix", 128 * 9)
    matrix_pop_min = matrix[128 * 2 + 13]
    # the articulations with a pitched release: %tact___meta[64·a + 40] is
    # built at init, not shipped; the decode lists them (engine-model §1d)
    rel_sources = [0, 3, 6, 7]
    vel_db = ints(p, "%vel_db", 1408)
    curve = ints(p, "%s3__engine_par_vel_curve", 128)


    L = [f"// Generated by features/rigs/fretted/tests/shreddage_section.py from {nki} — do not edit.",
         f"// program volume {program['volume']} × channel bus {bus['pub'][0]}",
         f"instrument_gain_db {fmt(gain_db)}",
         "fretboard {",
         f"    strings ({' '.join(map(str, opens))})",
         "    frets 24",
         f"    hand_fret {hand}",
         f"    hand_widths ({' '.join(map(str, widths))})",
         f"    // fretting mode {mode}: 1 << %str___hierarchy[{6 * mode + 1}..{6 * mode + 5}]",
         f"    weights {{ in_hand {1 << hier[1]}, free {1 << hier[2]}, adjacent {1 << hier[3]}, "
         f"same_string {1 << hier[4]}, same_fret {1 << hier[5]} }}",
         "    fresh_last_fret 0",
         # %s3___string_rr_matrix[144·artic + 24·string + fret] (L8440):
         # frets run 0..24, so fret 24 shares the next string's fret 0
         "    rr_slot_stride 24",
         # each note: a 1 µs phantom (`play_note(126,1,0,1)`, L11326) and the
         # poly window `wait(50)` µs at latency 0 (L7452)
         f"    onset_delay_us {51 + 1000 * ep[26]}",
         "    retrigger_fade_ms 400",
         # s3__engine__reset_hand from the timer listener once nothing is held
         # for engine_pars[59] ms (on release L18127, on listener L25632)
         f"    idle_reset {{ after_ms {ep[59]}, tick_ms {timer_ms} }}",
         # Pop opens a Sustain whose velocity passes Pop's VEL rule
         # (%tact___matrix[128·2 + 13]); the body follows after
         # wait(engine_pars[54]·100 + 10 µs), the pop fading over 50 ms
         # and the Sustain entering 15 ms in (script_0.ksp L18075-18240).
         f"    lead_ins ({{ articulation Sustain, min_velocity {matrix_pop_min}, lead Pop, "
         f"body_delay_ms {fmt((ep[54] * 100 + 10) / 1000)}, lead_fade_ms 50, body_offset_ms 15 }})",
         # release_event (L6946-6990): articulations with %tact___meta[64·a + 40]
         # leave a pitched release when held engine_pars[50]..[51] ms; the
         # counter steps only on a fretted string (L17301 `pitched_rel_fret > 0`)
         f"    release_layer {{ articulation PitchedRelease, sources ({' '.join(NAMES[a] for a in rel_sources)}), "
         f"min_held_ms {ep[50]}, max_held_ms {ep[51]} }}",
         # hammer (L17065-17144, L17839): %s3__hammer_sources rings, the new
         # note's articulation in %s3__hammerable_artics, velocity within
         # Legato (Hammer/Pull)'s VEL rule, |interval| 1..engine_pars[8];
         # volume offset engine_pars[27]·−5000 mdB
         f"    hammer {{ articulation HammerPull, sources ({' '.join(NAMES[a] for a in ksp_list(script, 's3__hammer_sources'))}), "
         f"from ({' '.join(NAMES[a] for a in ksp_list(script, 's3__hammerable_artics'))}), "
         f"max_velocity {matrix[128 * 6 + 14]}, range {ep[8]}, gain_db {-5 * ep[27]} }}",
         # Legato (Slide), L17156-17210, L17524-17660: its KEY rule (held),
         # %s3__slidable_artics ring, Sustain/Tapping slide (meta[9] and
         # matrix[5] set), up to 24 semitones (L7472); fades = the shipped
         # X-fade knobs (matrix[128·7 + 6..10]) × SLIDE_META / 10; the
         # destination enters 99 999 µs in (100 000 clamped below meta[6]·100);
         # level −1 dB per second of the origin's age × engine_pars[29]
         slide_line(matrix, ep, script),
         # Tapping (L17426, trigger type 8): a hammer from the ringing note or
         # the fret; an idle open string plays Sustain (L17138)
         "    tap { articulation Tapping, open_as Sustain }",
         # FX plays straight from `on note` (L10434-10474)
         "    free_articulations (FX)",
         ]
    if ep[20] == 1:
        L.append(f"    borrow {{ chance_permille {ep[21]}, seed {seed}, multiplier {mult}, frets 2 }}")
    L.append("    rr_orders (")
    for i in range(7):
        L.append(f"        {{ rr {i + 2}, order ({' '.join(map(str, rr_seed[4096 * i: 4096 * (i + 1)]))}) }}")
    L += ["    )", "}", "articulations ("]

    for a, aid in ARTICS.items():
        tab = table[512 * a: 512 * a + 128]
        edges = [v for v in range(2, 128) if tab[v] != tab[v - 1]]
        vv = matrix[128 * a + 27]
        law = []
        for v in range(128):
            c = curve[max(v, 1)]
            lo, fr = vv // 100, vv % 100
            hi = lo + (0 if lo == 10 else 1)
            mdb = int(((100 - fr) * vel_db[c * 11 + lo] + fr * vel_db[c * 11 + hi]) / 100) - 1
            law.append(fmt(mdb / 1000))
        g = groups.get(f"Blend {FIRST_GROUP[aid]} Str 1") or groups[f"Blend {FIRST_GROUP[aid]}"]
        att, _, _, _, rel, crv = g["ahdsr_AHDSRc"].split("/")
        env = kontakt_env(float(att), float(crv), float(rel))
        L += ["    {",
              f"        id {aid}",
              f"        // %tact___table_matrix[{512 * a}..]: layer edges",
              f"        vel_thresholds ({' '.join(map(str, edges))})",
              f"        // %vel_db by Vel->Vol {vv}, −1 mdB volume_offset",
              f"        vel_gain_db ({' '.join(law)})",
              f"        // group volume {g['vol_lin']}",
              f"        level_db {fmt(float(g['vol_db']))}",
              "        // Kontakt AHDSR: 33-frame silent lag, attack curve and release fall as",
              "        // KONTRA measured them (format/kontakt/src/laws.rs, law/src/formula.rs)",
              f"        amp_env ( {' '.join(f'{{time_ms {fmt(a)}, level {fmt(b)}, curve {c:.4f}}}' for a, b, c in env)} )",
              "        amp_env_hold true",
              f"        release_ms_by_held ((0 {fmt(float(rel))}))",
              "    }"]
    L.append(")")
    path = os.path.join(root, "_patches", inst, "section.styx")
    if os.path.exists(path):
        import shutil, time
        shutil.copy(path, f"{path}.pre-{time.strftime('%Y-%m-%d-%H%M%S')}")
    open(path, "w").write("\n".join(L) + "\n")
    print(path)


if __name__ == "__main__":
    main()
