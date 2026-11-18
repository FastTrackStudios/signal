#!/usr/bin/env python3
"""Derive a Cinematic Studio library spec from the Kontakt-matched strings spec.

    python3 derive_cs_spec.py woodwinds > specs/cinematic-woodwinds.styx

CSS, CSW and CSB run one KSP engine family over the same group layout, with
each library's recordings in the strings' slots (CSW: Flutter in the
"harmonics" groups, Repetitions in "spiccato", NVsus in "nonvib"). So the
engine model the strings were matched with — performance, dynamics, the
articulations' engine settings, legato_engine, short_note_timing — carries
over, renamed to the library's articulations, minus the ones it has no
recordings of. The library's own identity — name, sections, mics, the
keyswitch layout — comes from its existing spec (`--own`, default
specs/cinematic-<library>.styx as committed, read from git HEAD so a
re-derive never feeds on its own output).

Per-instrument numbers (velocity tables, group curves, legato and marcato
knobs) come from each instrument's decoded KSP values in its
`_patches/<instrument>/section.styx`, not from here.
"""
import os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SPECS = os.path.join(HERE, "..", "specs")
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))

LIBS = {
    "woodwinds": {
        "title": "Cinematic Studio Woodwinds",
        # strings articulation -> this library's (whole word)
        "rename": {
            "Harmrel": "Fluttrel",
            "Harm": "Flutter",
            "Vsusrel": "Vibsusrel",
            "NVrel": "NVsusrel",
            "Nonvib": "NVsus",
            "Spiccato": "Repetitions",
        },
        # strings articulations this library has no recordings of
        "drop": ["Tremolo", "Tremrel", "Clegno", "Port", "Pizzicato", "Bartokpizz"],
        # top-level blocks of the strings spec with no counterpart here
        "drop_blocks": [],
        # library-wide differences from the strings engine (cs_ksp_var_map.tsv):
        # (regex, replacement, why) applied to the derived text
        "subs": [
            (r"(\n        id            Marcato\n(?:.*\n)*?\s+lt_articulation )Leg\b", r"\1MLeg",
             "CSW records its marcato transitions as their own MLeg files"),
            (r"(sustain_noteoff_ms\s+\S+[^\n]*)", r"\1\n    sustain_noteoff_cut_ms 100   // $tukcw — the script's fade_out over the body's key-up release",
         "CSW's script fades a released body to silence over $tukcw = 100 ms", "all"),
            (r"release_delay_ms\s+\S+[^\n]*", "release_delay_ms     0.0   // no wait in the CSW/CSB release callback (CSS waits 2 ms); null-tested: every release tail 96 samples late at 2.0",
             "the release callback plays its sample without CSS's wait(2000)", "all"),
        (r"master_tune_cents\s+\S+[^\n]*", "master_tune_cents    0  // group tunes differ: folded into each zone (fold_group_tune.py)",
         "group tune is per group here, folded into the zones"),
        (r"glide_semitone\s+\([^\n]*", "glide_semitone  ((75 20) (100 20) (500 20))   // $ruv02 = 20 on every CSW patch, flat (no IOI table; $i1kki = 10: no interval scaling; 14679-14693)",
         "CSW bends a connected marcato note by its fixed $ruv02, not the strings' IOI table"),
        (r"glide_tone\s+\([^\n]*", "glide_tone      ((75 20) (100 20) (500 20))", "flat bend (whole tone)"),
        (r"glide_wide\s+\([^\n]*", "glide_wide      ((75 20) (100 20) (500 20))", "flat bend (wider)"),
        (r"marcato_transition \{\n", "marcato_transition {\n            body_delay_ms   40                      // wait($5aowl) = 40 ms on every CSW patch before the body plays (KSP 12301-12814); the transition sample does not wait\n",
         "CSW waits $5aowl before the connected body"),
        (r"busy_tick_ms\s+\S+[^\n]*", "busy_tick_ms    1                       // the crossfade loop ticks $0cqdq = 1 ms: a note inside it waits a tick (KSP 13616-13632)",
         "CSW crossfade-loop tick"),
        (r"(?<![a-z_])xfade_ms\s+\S+[^\n]*", "xfade_ms        70                      // $a3zg3 = 70 on every CSW patch (KSP 14666)",
         "CSW XTime 70 ms"),
        (r"split\s+\(50 60\)[^\n]*", "split           (50 50)                 // $igmiu 50, no hard table in CSW",
         "CSW split 50"),
        (r"node_divisor\s+\([^\n]*", "node_divisor    ((0 65))                // $x444h 65, flat",
         "CSW node divisor flat 65"),
        (r"cc19_soft\s+\([^\n]*", "cc19_soft       ((0 100))               // set_controller(19, $313em = 100) before the wait (KSP 12812)",
         "CSW CC19 = $313em"),
        (r"cc19_other\s+\([^\n]*", "cc19_other      (100 100 100)",
         "CSW CC19 (other)"),
        (r"cc18_soft_step\s+\([^\n]*", "cc18_soft_step  ((0 127))               // CSW never sets CC18: its marcato groups have no CC18 envelope",
         "CSW no CC18 (step)"),
        (r"cc18_soft_leap\s+\([^\n]*", "cc18_soft_leap  ((0 127))",
         "CSW no CC18 (leap)"),
        (r"cc18_other\s+\([^\n]*", "cc18_other      (127 127 127)",
         "CSW no CC18 (other)"),
        (r"body_env_cc19 \([^)]*\)", "body_env_cc19 ( {time_ms 20.0, level -1, curve 0.505}   // the marcato mod groups' one ENV_FLEX: CC19 -> level 1\n                            {time_ms 956.0, level 1.0, curve 0.5} )",
         "CSW marcato group flex"),
        (r"old_sink_level\s+\S+", "old_sink_level    1.0   // no CC19 step mod in CSW: the old body does not sink",
         "CSW old body has no step mod"),
        (r"\n\s+rr_sequence\s[^\n]*", "",
         "shorts draw their RR at random (random(1,n) avoiding the key's last picks), no sequence",
         "all"),
        (r"(\n        id            NVsus\n)",
         "\\1        script_start_ms 333   // $0kimq := 333 on a nonvib sustain; zones without a start-mod range ignore it\n",
         "a nonvib sustain starts 333 ms in where the zone lets it"),
        (r"(\n    lt_offset_low_latency[^\n]*)",
         "\\1\n    overlap_offset_zone1_only true  // CSW: $1fvjk := base + $b0n3s in velocity zone 1 only (9463-9478)",
         "the overlap delay deepens the transition entry in zone 1 only"),
        (r"(\n    vibrato_controller    CC2)\n",
         "\\1\n    // CC2 switches nonvib / vib at 64 (KSP `$takdp<64`, CC `$vkgua` = 2), with a\n"
         "    // timed crossfade (`$2b2p5`, 650 ms; not modelled yet): no CC2 group curves\n"
         "    vibrato_mode          \"on_off\"\n",
         "CSW's vibrato is a CC2 switch, not a group-curve crossfade"),
        (r"\n    first_note_variants[^\n]*", "",
             "no first-note body-variant counter ($2ekvq absent)"),
            (r"\n    first_note_variant_init[^\n]*", "", "(same)"),
            (r"\n    first_note_mute_ms[^\n]*", "",
             "no first-note mute path ($otlef / $yqhyd absent)"),
            (r"velocities \(\(5 7\) \(8 9\)\)", "velocities ((5 7) (5 7))",
             "the vibsus attack zones are 5-7 only (Repetitions); zone 3 UNVERIFIED"),
        ],
    },
}

LIBS["brass"] = {
    "title": "Cinematic Studio Brass",
    # strings slot -> brass recordings (csb_spec_requirements.md §1)
    "rename": {
        "Vsusrel": "Susrel",
        "Vibsus": "Sus",
        "Tremrel": "Mutesusrel",
        "Tremolo": "Mutesus",
        "Pizzicato": "Mutereps",
        "Clegno": "Rips",
        "Harmrel": "Fluttrel",
        "Harm": "Flutter",
        "Spiccato": "Repetitions",
    },
    # no vibrato dimension, no portamento, no bartok slot
    "drop": ["Nonvib", "NVrel", "NVLeg", "NVLegzero", "Port", "Bartokpizz"],
    "drop_blocks": [],
    # brass articulations with no strings slot, kept from the brass spec
    "own_articulations": ["Stoppedsus", "Stoppedreps", "Stoppedsusrel", "Taka"],
    "subs": [
        (r"(sustain_noteoff_ms\s+\S+[^\n]*)", r"\1\n    sustain_noteoff_cut_ms 200   // $qxeb4 — the script's fade_out over the body's key-up release",
         "CSB's script fades a released body to silence over $qxeb4 = 200 ms", "all"),
        (r"release_delay_ms\s+\S+[^\n]*", "release_delay_ms     0.0   // no wait in the CSW/CSB release callback (CSS waits 2 ms); null-tested: every release tail 96 samples late at 2.0",
         "the release callback plays its sample without CSS's wait(2000)", "all"),
        (r"master_tune_cents\s+\S+[^\n]*", "master_tune_cents    0  // group tunes differ: folded into each zone (fold_group_tune.py)",
         "group tune is per group here, folded into the zones"),
        (r"glide_semitone\s+\([^\n]*", "glide_semitone  ((75 0) (100 0) (500 0))   // CSB ships its bend switches off ($upjkh = $ma0b1 = 0 on every patch, 16615): no scoop",
         "CSB connected notes do not bend (null: Solo Trumpet MC v120 ioi300 +0.2 -> -11.5 dB)"),
        (r"glide_tone\s+\([^\n]*", "glide_tone      ((75 0) (100 0) (500 0))", "no bend (whole tone)"),
        (r"glide_wide\s+\([^\n]*", "glide_wide      ((75 0) (100 0) (500 0))", "no bend (wider)"),
        (r"(\n        id            Marcato\n)", "\\1        note_on_cc    ((19 127))   // set_controller(19, 127) on a fresh marcato note (KSP 11018)\n",
         "CSB sets CC19 := 127 on a fresh marcato note"),
        (r"marcato_transition \{\n", "marcato_transition {\n            cc19_controller 19                      // set_controller(19, …): the marcato filter envelope reads it\n",
         "CSB: a note inside the previous crossfade loop waits for its next tick"),
        (r"busy_tick_ms\s+\S+[^\n]*", "busy_tick_ms    1                       // the crossfade loop ticks $0cqdq = 1 ms: a note inside it waits a tick (KSP 15338-15355)",
         "CSB crossfade-loop tick"),
        (r"split\s+\(50 60\)[^\n]*", "split           (50 50)                 // $igmiu 50 on every CSB patch",
         "CSB split 50"),
        (r"node_divisor\s+\([^\n]*", "node_divisor    ((0 90))                // $x444h 90, flat, every CSB patch",
         "CSB node divisor flat 90"),
        (r"cc19_soft\s+\([^\n]*", "cc19_soft       ((0 0))                 // set_controller(19, $313em = 0) for a soft connected note (KSP 14063-14070)",
         "CSB CC19 soft = $313em"),
        (r"cc19_other\s+\([^\n]*", "cc19_other      (90 127 127)            // hard $s3dc1 = 90; beyond an octave 127",
         "CSB CC19 other"),
        (r"cc18_soft_step\s+\([^\n]*", "cc18_soft_step  ((0 127))               // CSB never sets CC18: its marcato groups have no CC18 envelope",
         "CSB no CC18 (step)"),
        (r"cc18_soft_leap\s+\([^\n]*", "cc18_soft_leap  ((0 127))",
         "CSB no CC18 (leap)"),
        (r"cc18_other\s+\([^\n]*", "cc18_other      (127 127 127)",
         "CSB no CC18 (other)"),
        (r"body_env_cc19 \([^)]*\)", "body_env_cc19 ( {time_ms 20.0, level -1, curve 0.505}   // the marcato mod groups ENV_FLEX: CC19 -> levels 1 and 2\n                            {time_ms 280.0, level -1, curve 0.5}\n                            {time_ms 1200.0, level 1.0, curve 0.5} )",
         "CSB marcato group flex"),
        (r"old_sink_level\s+\S+", "old_sink_level    1.0   // no CC19 step mod in CSB: the old body does not sink",
         "CSB old body has no step mod"),
        (r"\n\s+rr_sequence\s[^\n]*", "",
         "shorts draw their RR at random (random(1,n) avoiding the key's last picks), no sequence",
         "all"),
        (r"(\n        id            Marcato\n(?:.*\n)*?\s+rr_per_note\s+)true[^\n]*",
         "\\1false  // one RR toggle for every note ($d3x3j := 1 - $d3x3j, 11021 / 14060)",
         "CSB marcato alternates its two RRs globally, not per note-class"),
        (r"random_tune_cents\s+\d+[^\n]*",
         "random_tune_cents   0     // CSB detunes no short ($5xze0 = 0 on every patch; KSP 12794)",
         "no random detune on brass shorts", "all"),
        (r"\n    vibrato_controller[^\n]*",
         "\n    // The script ignores MIDI CC2 (it is in %vn532) and sets it to its saved\n"
         "    // vibrato knob ($takdp, 126 on every CSB patch): the release groups' CC2\n"
         "    // curves read that, so brass has no vibrato crossfade and a fixed CC2.\n"
         "    fixed_ccs ({cc 2, value 126})",
         "no vibrato crossfade; CC2 held at $takdp"),
        (r"\n(\s*//[^\n]*\n)*\s*short_note_cc1_map \{[^}]*\}", "",
         "the KSP never reads CC1 for the short type (CC58 / KS velocity) — verify in Kontakt"),
        (r"\n    first_note_variants[^\n]*", "", "no first-note body variants (Sus zones vel 1-4, one start)"),
        (r"\n    first_note_variant_init[^\n]*", "", "(same)"),
        (r"\n    first_note_attack \{[^\n]*", "", "no attack-voice zones in the vibsus groups"),
        (r"first_note_mute_ms[^\n]*", "first_note_mute_ms      (0 0)", "no mute wait; zones 2-3 fade in"),
        (r'(\n\s+)0-10  (?:Sustain|"Sus")', r'\g<1>0-10  "Sus"', "CC58 0-10 = sustain"),
        (r"overlay \{ articulation Repetitions, cc1_scale \([^)]*\) \}",
         "overlay { articulation Repetitions, cc1_scale (" + " ".join(["100"] * 128)
         + "), skip_after_hard_transition false, connected_from_velocity 64 }",
         "CSB overlay: no Velocomp, every first note, loud connected notes ($evk4m 64)"),
    ],
}

LIBS["solo-strings"] = {
    "title": "Cinematic Studio Solo Strings",
    # the same articulations and slots as the strings (CSSS NKIs, groups.json)
    "rename": {},
    "drop": [],
    "drop_blocks": [],
    # library-wide differences (tests/csss_ksp_var_map.tsv; CSSS Violin I
    # script_1.ksp): an older build of the engine
    "subs": [
        (r"(\n    vibrato_controller\s+CC2)\n",
         "\\1\n    // CC2 switches nonvib / vib (CSSS vibsus groups carry no CC2 volume mod;\n"
         "    // $vkgua = 2): no CC2 group curves, the vibsus attack only on the vib side\n"
         "    vibrato_mode          \"on_off\"\n",
         "CSSS's vibrato is a CC2 switch, not a group-curve crossfade"),
        (r"(first_note_variant_init\s+\S+[^\n]*)", "\\1\n    first_note_variant_random true   // CSSS draws the body variant random(2,4) (CSSS 8637-8642)",
         "CSSS random first-note variant"),
        (r"first_note_mute_ms[^\n]*", "first_note_mute_ms      (0 0)   // no \"F. wait\": the body fades in at once over $2krfe (CSSS 9047-9092)",
         "CSSS no first-note wait"),
        (r"master_tune_cents\s+\S+[^\n]*", "master_tune_cents    0  // every CSSS group has tune 1.0 (CSS: 1.0052122); null: Kontakt plays the zone tune alone",
         "CSSS groups are untuned"),
        (r"cc19_soft\s+\([^\n]*", "cc19_soft       ((0 0))                 // set_controller(19, $313em = 0) on every marcato note (CSSS 11951-11954)",
         "CSSS CC19 = $313em"),
        (r"cc19_other\s+\([^\n]*", "cc19_other      (0 0 0)",
         "CSSS CC19 (other)"),
        (r"cc18_soft_step\s+\([^\n]*", "cc18_soft_step  ((0 127))               // CSSS never sets CC18: its marcato groups have no CC18 route",
         "CSSS no CC18 (step)"),
        (r"cc18_soft_leap\s+\([^\n]*", "cc18_soft_leap  ((0 127))",
         "CSSS no CC18 (leap)"),
        (r"cc18_other\s+\([^\n]*", "cc18_other      (127 127 127)",
         "CSSS no CC18 (other)"),
        (r"body_env_cc19 \([^)]*\)", "body_env_cc19 ( {time_ms 4.0, level -1, curve 0.505}   // the marcato mod groups' one ENV_FLEX: CC19 -> level 1\n                            {time_ms 916.0, level 1.0, curve 0.7} )",
         "CSSS marcato group flex"),
        (r"old_sink_level\s+\S+", "old_sink_level    1.0   // no CC19 step mod in CSSS: the old body does not sink",
         "CSSS old body has no step mod"),
        (r"overlay \{ articulation Spiccato, cc1_scale \([^)]*\)",
         "overlay { articulation Spiccato, cc1_scale (" + " ".join(["100"] * 128) + ")",
         "CSSS overlay: no Velocomp, the raw velocity (CSSS 12053-12131)"),
    ],
}

TAKE_FROM_STRINGS = ["performance", "dynamics", "articulations", "legato_engine", "short_note_timing"]
TAKE_FROM_OWN = ["sections", "mics", "keyswitch"]


def top_blocks(text):
    """{name: (start, end)} line spans of the top-level `name {`/`name (` blocks."""
    lines = text.split("\n")
    out = {}
    i = 0
    while i < len(lines):
        m = re.match(r"^([a-z_]+) [({]$", lines[i])
        if m:
            close = ")" if lines[i].endswith("(") else "}"
            j = i + 1
            while lines[j] != close:
                j += 1
            out[m.group(1)] = (i, j)
            i = j
        i += 1
    return out


def block(text, name):
    lines = text.split("\n")
    a, b = top_blocks(text)[name]
    # the comment lines directly above belong to the block
    while a > 0 and lines[a - 1].startswith("//"):
        a -= 1
    return "\n".join(lines[a : b + 1])


def scalar(text, key):
    m = re.search(rf"^{key}\s+(.*)$", text, re.M)
    return m.group(1) if m else None


def drop_articulations(arts, drop):
    """Remove the `    {` … `    }` entries whose id is in `drop`."""
    lines = arts.split("\n")
    out, i = [], 0
    while i < len(lines):
        if lines[i] == "    {":
            j = i
            while lines[j] != "    }":
                j += 1
            entry = lines[i : j + 1]
            ids = [re.match(r"^\s+id\s+(\S+)", l) for l in entry]
            aid = next((m.group(1) for m in ids if m), None)
            if aid not in drop:
                out.extend(entry)
            i = j + 1
            continue
        out.append(lines[i])
        i += 1
    return "\n".join(out)


def own_articulation_entries(own, ids):
    """The `    {` … `    }` articulation entries of the library's own spec
    with these ids, verbatim."""
    if not ids:
        return ""
    arts = block(own, "articulations").split("\n")
    out, i = [], 0
    while i < len(arts):
        if arts[i] == "    {":
            j = i
            while arts[j] != "    }":
                j += 1
            entry = arts[i : j + 1]
            m = next((re.match(r"^\s+id\s+(\S+)", l) for l in entry if re.match(r"^\s+id\s+", l)), None)
            if m and m.group(1) in ids:
                out.extend(entry)
            i = j + 1
            continue
        i += 1
    return "\n".join(out) + "\n" if out else ""


def rename(text, mapping):
    for old in sorted(mapping, key=len, reverse=True):
        text = re.sub(rf"(?<![A-Za-z]){re.escape(old)}(?![A-Za-z])", mapping[old], text)
    return text


def drop_refs(text, drop):
    """Blank fields that point at a dropped articulation (release_artic Tremrel)."""
    for d in drop:
        text = re.sub(rf"^(\s*)(release_artic|vibrato_pair|sordino_pair)\s+{d}\s*$", r'\1\2 ""', text, flags=re.M)
    return text


def main():
    lib = sys.argv[1]
    cfg = LIBS[lib]
    strings = open(os.path.join(SPECS, "cinematic-strings.styx")).read()
    own_rel = f"features/rigs/orchestra/specs/cinematic-{lib}.styx"
    own = subprocess.run(
        ["git", "-C", REPO, "show", f"HEAD:{own_rel}"], check=True, capture_output=True, text=True
    ).stdout

    parts = [
        f"// {cfg['title']} — library spec, DERIVED by tests/derive_cs_spec.py from the",
        "// Kontakt-matched cinematic-strings.styx (one KSP engine family, the same group",
        "// layout) — do not edit the engine-model blocks here; change the strings spec or",
        "// the derivation. Identity blocks (sections, mics, keyswitch) are this library's own.",
        "// Per-instrument numbers live in each instrument's _patches/<instrument>/section.styx.",
        "",
        f"name    {scalar(own, 'name')}",
        f"vendor  {scalar(own, 'vendor')}",
    ]
    for k in ("live_legato_interval_max", "live_chord_window_ms"):
        v = scalar(strings, k)
        if v is not None:
            parts.append(f"{k} {v}")
    parts.append("")
    for name in ["sections", "mics"]:
        parts += [block(own, name), ""]
    for name in TAKE_FROM_STRINGS:
        if name in cfg["drop_blocks"]:
            continue
        b = block(strings, name)
        if name == "articulations":
            b = drop_articulations(b, set(cfg["drop"]))
            extra = own_articulation_entries(own, cfg.get("own_articulations", []))
            if extra:
                b = b.rstrip()
                assert b.endswith(")")
                b = b[:-1] + extra + ")"
        b = drop_refs(rename(b, cfg["rename"]), cfg["drop"])
        if name == "dynamics":
            # the pizzicato type map has no pizzicato to pick
            b = re.sub(r"\n(\s*//[^\n]*\n)*\s*pizzicato_cc1_map \{[^}]*\}\n", "\n", b)
        parts += [b, ""]
    parts += [block(own, "keyswitch"), ""]
    text = "\n".join(parts)
    for sub in cfg.get("subs", []):
        pat, rep, why = sub[:3]
        # a 4th element "all" applies the rule everywhere, not just once
        text, n = re.subn(pat, rep, text, count=0 if sub[3:] == ("all",) else 1)
        if n == 0:
            raise SystemExit(f"derive: substitution not applied ({why}): {pat}")
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
