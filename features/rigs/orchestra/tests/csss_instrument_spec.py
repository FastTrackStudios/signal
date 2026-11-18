#!/usr/bin/env python3
"""Write a CS Solo Strings instrument's decoded numbers as a `section.styx` overlay.

    python3 csss_instrument_spec.py <_ksp/CSSS <instrument> dir> > "_patches/Solo <instrument>/section.styx"

CSSS runs an older build of the strings' engine (tests/csss_ksp_var_map.tsv):
the strings' articulations and group layout, but with the woodwinds' engine
traits, so this is the woodwinds generator (csw_instrument_spec.py) over the
strings' tables:

- shorts: random round robins avoiding the key's last two (no fixed
  sequences); the short families' fall-time knobs at 75 (CSS 127);
- marcato: no hard-attack path, one transition start offset `$5ibvn` and
  volume `$b23jt`, the body at `$yp2sq` on every connection;
- legato: no overlap delay; the transition starts at a flat offset per
  velocity zone — Standard `$n4tyf/$5vo4f/$3mu55`, the mode the patch ships
  (`$tuu20 = 0`), whichever legato mode CC58 asks for;
- the body handoff `%k1aqh`, as CSW / CSB.
"""
import os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import css_section_spec as css  # noqa: E402
import csw_instrument_spec as csw  # noqa: E402

csw.BODY_GROUPS = dict(css.BODY_GROUPS)
csw.RELEASE_GROUPS = dict(css.RELEASE_GROUPS)
# short articulation -> (zone file articulation, group family, threshold var, dB var)
csw.SHORTS = {
    "Spiccato": ("Spiccato", "spiccato", "%g1qri", "%bcez1"),
    "Staccatissimo": ("Staccatissimo", "staccatissimo", "%aoywv", "%ak5pm"),
    "Staccato": ("Staccato", "staccato", "%nfxdy", "%opj4y"),
    "Sfz": ("Sfz", "sfz", "%cjzwd", "%5u3ic"),
    "Pizzicato": ("Pizzicato", "pizzicato", "%mjhzg", "%hu3zk"),
    "Bartokpizz": ("Bartokpizz", "bartok", "%kgcee", "%eehua"),
    "Clegno": ("Clegno", "col legno", "%fuiq3", "%a3xuw"),
}
csw.TWO_PLAYER = set()
# Violin II, Viola and Cello name their non-vibrato recordings NVsus (Violin I:
# Nonvib); they fill the nonvib slot either way (css_section.sh relabels the zones)
csw.SLOT_ALSO = dict(csw.SLOT_ALSO, Nonvib={"Nonvib", "NVsus"})


def legato(pers, zones=None, group_names=None, two_player=False):
    """CSSS's legato is CSB's (the same knobs: old-body fades $mhmq3 / $qkgxg /
    $yn5xm, the LT retire table $w4sb3 / $elc3z / $h3l3o / $4wrso, the body
    handoffs %k1aqh / %wu5b4), with the transition entry by velocity zone in
    Standard's $n4tyf / $5vo4f / $3mu55 (CSB: $k2afl / $xem4t / $yds0m) and
    no overlap delay; plus the first note's fade after the mute ($2krfe)."""
    import contextlib, io  # noqa: PLC0415

    import csb_instrument_spec as csb  # noqa: PLC0415

    aliased = dict(pers)
    for csb_name, csss_name in (("$k2afl", "$n4tyf"), ("$xem4t", "$5vo4f"), ("$yds0m", "$3mu55")):
        aliased[csb_name] = pers[csss_name]
    # CSB-only knobs CSSS lacks, at their off values: "HighRange" $vv4qt 127
    aliased.setdefault("$vv4qt", [127])
    groups = {g["name"][4:]: g for g in ALL_GROUPS if g["name"].startswith("Mix ")}
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        csb.legato(aliased, groups, zones, group_names)
    text = out.getvalue()
    # CC58 6-10 (expressive) selects CSSS's Advanced offsets $lttts / $pdeco /
    # $ezee1 (measured: Kontakt's EX transitions lead Standard's by 83 ms)
    adv = f"({css.one(pers, '$lttts')} {css.one(pers, '$pdeco')} {css.one(pers, '$ezee1')})"
    text = re.sub(r"^(    lt_offset_expressive ).*$", r"\g<1>" + adv, text, count=1, flags=re.M)
    text = text.rstrip()
    assert text.endswith("}")
    print(text[:-1] + f"    first_note_fade_ms ({css.one(pers, '$2krfe')} {css.one(pers, '$2krfe')})\n}}")


csw.legato = legato
_marcato = csw.marcato


def marcato(pers, zones=None, group_names=None):
    _marcato(pers, zones, group_names)
    # a fresh marcato note swells in on the marcato group's flex (CSSS sends
    # CC19 = 0, leaving the stored level 1): CSS plays it at full level
    groups = {g["name"][4:]: g for g in ALL_GROUPS if g["name"].startswith("Mix ")}
    flex = css.flex(groups, "marcato mod")
    # ... and a fresh one is levelled by its velocity, in the script's
    # integer millidB: %lwgt4[50] := -$5n14h * -100000 / 127,
    # change_vol((127 - vel) * %lwgt4[50] / 100) (CSSS 6140, 12036-12037);
    # a connected body takes none (its transition does: lt_vel_db)
    k = css.one(pers, "$5n14h")
    lw = kdiv(-k * -100000, 127)
    table = " ".join(css.fmt(kdiv((127 - min(v, 127)) * lw, 100) / 1000) for v in range(128))
    print("    {\n        id Marcato")
    if flex:
        css.emit_amp_env([tuple(seg) for seg in flex])
    print(f"        vel_gain_db ({table})")
    print("    }")
    # the pizz family's release knob ($3kbfg), as the strings
    groups = {g["name"][4:]: g for g in ALL_GROUPS if g["name"].startswith("Mix ")}
    css.pizz_release(pers, groups)


csw.marcato = marcato


def kdiv(a, b):
    """KSP integer division (truncates toward zero)."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


def main():
    global ALL_GROUPS
    import json  # noqa: PLC0415

    import contextlib, io  # noqa: PLC0415

    ALL_GROUPS = json.load(open(os.path.join(sys.argv[1], "groups.json")))
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        csw.main()
    text = out.getvalue().replace("Generated by csw_instrument_spec.py", "Generated by csss_instrument_spec.py", 1)
    # every CSSS short draws its RR at random per key (CSSS 7910-7916): no
    # fixed sequence, no per-note alternation (the strings spec has both)
    # the marcato body enters $yp2sq + random(0, $a4bsp) deep (CSSS 11357)
    a4bsp = css.one(css.persistent(sys.argv[1]), "$a4bsp")
    # the LT's level: + (127 - vel) * %lwgt4[51] from $0kd5m, + the phrase
    # ramp ($4lqhx $ee3a4 over $xu41m / $c2hkn) (CSSS 11757-11773)
    pers = css.persistent(sys.argv[1])
    ramp = " ".join(str(css.one(pers, v)) for v in ("$4lqhx", "$ee3a4", "$xu41m", "$c2hkn"))
    text = re.sub(r"^(            body_offset_big_ms [^\n]*\n)",
                  r"\1            body_offset_random_ms " + str(a4bsp) + "\n"
                  + "            lt_vel_db " + str(css.one(pers, "$0kd5m")) + "\n"
                  + "            lt_ramp (" + ramp + ")\n", text, count=1, flags=re.M)
    text = re.sub(r"^(        rr_random_avoid_last \d+\n)", r"\1        rr_sequence ()\n        rr_per_note false\n",
                  text, flags=re.M)
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
