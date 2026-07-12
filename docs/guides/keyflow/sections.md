---
title: Sections
kind: concept
type: concept
order: 2
---

# Sections

Below the [[structure|header]], a chart is a stack of **sections** — the
named parts of the song. A section is a header line naming the part, then the
music underneath it, running until the next header:

```kf-
VS 4
C  F  G  Am

CH 4
F  C  G  Am
```

Two named sections with their bar lengths, engraved:

```kf+
VS 4
C | F | G | Am

CH 4
F | C | G | Am
```

That's a four-bar verse followed by a four-bar chorus. Everything between two
headers belongs to the section above it.

## Naming a section

Use the common short name or spell it out — case doesn't matter:

| Section | Short | | Section | Short |
| ------- | ----- |-| ------- | ----- |
| Intro | `IN` | | Instrumental | `INST` |
| Verse | `VS` | | Interlude | `INT` |
| Chorus | `CH` | | Solo | `SOLO` |
| Pre-Chorus | `PRE-CH` | | Outro | `OUT` |
| Bridge | `BR` | | | |

So `VS`, `Verse`, and `verse` are the same section. (Names are two or more
letters — a lone `C` is the chord C, not a chorus.) `PRE-` and `POST-` go in
front of any name — `Pre-Chorus`, `Post-Verse`.

To tell apart two of the same part — a first and second chorus that differ — drop
a short label between the name and its length:

```kf-
CH 3A 4       chorus variant 3A, four bars
```

## Length in bars

The number after the name is the section's length in **bars**:

```kf-
VS 8          an eight-bar verse
CH 4          a four-bar chorus
```

Leave it off and Keyflow just counts the bars you wrote — `VS` with four bars of
music below it is a four-bar verse. If you write *more* bars than the number
says, Keyflow flags it, so the count doubles as a quick check that the part came
out the length you meant.

## Repeating a section

Write a section's music once, then **replay it by name** — a header with nothing
under it recalls what that section played before:

```kf-
VS 4
1  4  5  1

CH 4
4  1  5  1

VS            replays the verse
CH            replays the chorus
```

So a full song is mostly its section list: lay out `VS`, `CH`, `BR` once, then
order the repeats however the song goes.

## Labels

Add a note to a section in quotes — a dynamic, an instruction, a cue:

```kf-
CH 4 "Big finish"
IN 2 "drums only"
```

The label rides along with the section and shows on the rendered chart.

## Custom sections

For a part that isn't one of the standard names, put your own name in brackets:

```kf-
[Tag] 2
[Sax Solo] 8
```

A custom section behaves like any other — it takes a length and can be replayed
by name (`[Tag]` again).

## A key change at a section

A section can start in a new key: add a key token (see
[[structure|Structure]]) to its header, and it takes effect from the top
of that section.

```kf-
BR 8 #Ab      the bridge moves to A♭
```

Number and Roman-numeral chords in that section resolve against the new key, and
the key signature updates there.

## What's next

- **Lyrics** — writing words under the chords, lined up with the music.

---

Previous: [[structure|Structure]] · Next: [[chords|Chords]] · Up: [[keyflow|Keyflow Guide]]
