---
title: Structure
kind: concept
type: concept
order: 1
---

# Structure

Every Keyflow file has the same two-part shape:

```kf-
Vienna (Live) - Billy Joel    ← header: what the song is
4/4 140bpm #Gm

VS                            ← sections: the music
Gm Bb F Ab
```

Engraved, a small chart with a header and one section looks like this:

```kf+
Vienna - Billy Joel
4/4 140bpm #Gm

VS
Gm | Bb | F | Ab
```

1. A **header** at the top describing the song — its title, who wrote it, and
   the musical defaults (time signature, tempo, key).
2. One or more **sections** of music below it.

This page covers the header. Sections and the music itself come in later pages.

## The title line

The first line of text is the title. It may also carry an artist and a subtitle:

```kf-
Vienna
```
```kf-
Vienna - Billy Joel
```
```kf-
Vienna (Live) - Billy Joel
```

- Text before ` - ` is the **title**; text after it is the **artist**.
- Text in `(parentheses)` becomes the **subtitle**.

So `Vienna (Live) - Billy Joel` reads as title *Vienna*, subtitle *Live*, artist
*Billy Joel*.

The title line is **optional** — a chart can start straight at the metadata line
or even at the first section. But naming your songs is a good habit, and the
title is what shows up at the top of the rendered chart.

## The metadata line

The next line sets the song's musical defaults. It holds up to three tokens,
**space-separated, in any order**:

```kf-
4/4 140bpm #Gm
```
```kf-
68bpm 4/4 #G
```

| Token        | Means              | Examples                       |
| ------------ | ------------------ | ------------------------------ |
| `N/D`        | Time signature     | `4/4`, `6/8`, `3/4`, `12/8`    |
| `Nbpm`       | Tempo, in BPM      | `120bpm`, `68bpm`              |
| `#Key`       | Key                | `#C`, `#Gm`, `#Eb`, `#F#`      |

Every token is optional. `4/4 #C` (no tempo) and `120bpm` (just a tempo) are both
valid. Anything you omit falls back to a default — `4/4`, no fixed tempo, and key
of C.

### Reading the key

The key token starts with a `#` (or `b`) **marker** — its only job is to tell the
parser "this token is the key," so it isn't mistaken for a chord. The marker does
*not* mean the key is sharp or flat. The key's own accidental and quality are
written into the name itself:

| Written | Key             |
| ------- | --------------- |
| `#C`    | C major         |
| `#Gm`   | G minor         |
| `#Eb`   | E♭ major        |
| `#F#`   | F♯ major        |
| `#Am`   | A minor         |

A trailing `m` makes it minor; no `m` means major. (`#Eb` and `bEb` mean the same
thing — pick whichever marker reads better to you.)

The key matters beyond display: it's what lets you write chords and melody as
**Nashville numbers** (`1 4 5`) or **Roman numerals** (`I IV V`) instead of letter
names, since those are relative to the key. More on that in the Chords pages.

## Comments

A semicolon starts a comment. Everything after it on the line is ignored:

```kf-
4/4 120bpm #C    ; mid-tempo, straight feel
```

## Putting it together

A complete header, with the music that follows it:

```kf-
Build My Life - Housefires
68bpm 4/4 #G

Intro
1 4 1/3 4

VS
1 4 1/3 4
```

Here the header names the song and sets G major at 68 BPM in 4/4. Because the key
is set, the section can be written in Nashville numbers (`1 4 1/3 4`) — four bars,
one chord per bar.

## What's next

That's the whole header. From here the guide moves into the music:

- **Sections** — naming and ordering the parts of a song (`VS`, `CH`, `BR`…).
- **Chords** — letter names, Nashville numbers, and Roman numerals.
- **Rhythm** — how a bar holds more than one chord, and why you rarely need `|`.

---

Next: [[sections|Sections]] · Up: [[keyflow|Keyflow Guide]]
