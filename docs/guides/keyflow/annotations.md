---
title: Annotations & Expression
kind: concept
type: concept
order: 9
---

# Annotations & Expression

Beyond the notes themselves, a chart carries **markings** — a word of direction,
a dynamic, a swell. In Keyflow these go on their own lines inside a section,
mixed in with the chord and melody lines.

## Staff text

A quoted string is **free text** placed on the staff. By default it sits below;
`^` puts it above, `_` keeps it below:

```kf-
"straight feel"        below the staff (the default)
^"BIG"                 above the staff
_"rit. ...."           below the staff
```

Staff text and a dynamic alongside the chords, engraved:

```kf+
^"Swell"
dyn mf
1 | 4 | 5 | 1
```

Put the line wherever the text belongs in the music — before the bar it
describes, between two chord lines, and so on. (To pin text to a *single chord*
instead of a whole spot, attach it to the chord: `Cmaj7"as written"` — see
[[chords|Chords]].)

## Instrument cues

A cue aimed at one player starts with `@` and the instrument name, then the
text:

```kf-
@Drums "full groove now"
@Bass "walk it down"
@Keys "pad only"
```

It reads like staff text but is tagged for that instrument, so a part can show
just its own cues.

## Dynamics

Classical dynamics use the `dyn` keyword, so a lone `f` or `p` is never mistaken
for a chord:

```kf-
dyn mp
dyn ff
dyn fp
```

The levels run `ppp pp p mp mf f ff fff`, plus the accents `sf`, `sfz`, and `fp`.

A dynamic sits **below** the staff by default; add `above` to lift it:

```kf-
dyn mf above
```

To place it on a particular beat of the bar rather than the downbeat, add `@`
and the beat number:

```kf-
dyn fff@4        forte-fortissimo on beat 4
```

## Hairpins

A hairpin is a crescendo or decrescendo wedge, written with `hairpin` and a
direction — `<` to swell, `>` to fade — over a **beat range** `start..end`:

```kf-
hairpin < 1..4        crescendo across the bar
hairpin > 2..4        decrescendo from beat 2 to 4
```

Like dynamics, hairpins default below the staff; add `above` to move them up.

## What's next

- **Repeats & Endings** — the last piece: saying a part once and looping it,
  with first and second endings.

---

Previous: [[key-meter-changes|Key & Meter Changes]] · Next: [[repeats|Repeats & Endings]] · Up: [[keyflow|Keyflow Guide]]
