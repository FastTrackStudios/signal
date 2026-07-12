---
title: Rhythm
kind: concept
type: concept
order: 5
---

# Rhythm

Most lead sheets change chords no more than once or twice a bar, so Keyflow
makes the common case free: **a bare chord fills its whole bar.** You write the
progression and nothing else, and only reach for rhythm when a bar holds more
than one chord. That's why you almost never type a bar line.

```kf-src
C  F  G  Am
```

Slashes mark the beats — engraved, a mix of held and strummed bars:

```kf+
C //// | F //// | G / / / | C
```

Four chords, four bars — one chord each. No `|`, no durations.

## One chord, one bar

A chord with no rhythm attached lasts exactly one measure, whatever the time
signature. Space-separated chords simply march one per bar:

```kf-src
C  F  G  Am        four bars
Gm Bb F Ab Eb      five bars
```

This is the *measure-fill default*, and everything below is how you override it
when a bar needs more than one chord.

## Splitting a bar: slashes

A slash `/` is **one beat**. Follow a chord with slashes to give it an exact
number of beats, so several chords can share a bar:

```kf-src
C // G //           two chords, two beats each   (a 4/4 bar)
C / G / Em / A /     four chords, one beat each   (a 4/4 bar)
```

In 4/4:

| Write | The chord lasts |
| ----- | --------------- |
| `C /` | 1 beat |
| `C //` | 2 beats |
| `C ///` | 3 beats |
| `C ////` | 4 beats (a full bar) |

So a chord lasts as many beats as it has slashes, and its symbol sits on the
first of those beats. (A bare `C` and `C ////` both fill a 4/4 bar — bare is
just the shorthand.) The slashes can also be written attached: `C///` means the
same as `C / / /`.

When the beats in a bar add up past the time signature, the next chord starts a
new bar automatically — you never close a bar by hand.

### Dotted slashes and compound meter

A dot makes a slash a **dotted** beat: `/.` is 1½ beats in 4/4. This matters most
in compound meters like 6/8, where the natural pulse is a dotted quarter — two of
them fill the bar:

```kf-src
6/8
Am /. /.            one bar, two dotted-quarter beats
C /. G /.            two chords, half a bar each
```

## Grouping with ( )

Parentheses bind chords into one **group** that splits a span of time evenly.
The plainest use is two chords in a bar:

```kf-src
G  C  (Em D)  G
```

That's four bars — `G`, `C`, then a bar shared by `Em` and `D` (half each), then
`G`. Compare the group to bare chords: `Em D` on its own would be *two* bars, but
`(Em D)` keeps them inside *one*.

A group divides its time equally by however many chords it holds, so odd splits
fall out naturally:

```kf-src
(D Em G)            a triplet — three chords across one bar
(C D E F)           four chords, a beat each
```

By default a group fills one bar. To make it shorter, give it a target duration
the same way you'd time a chord — with slashes or a note value:

```kf-src
(D Em)//            the pair spans two beats (one each)
(D Em G)_4          a triplet across a single quarter note
```

So `()` is a little bar-within-a-bar: whatever length you give it is shared out
evenly among the chords inside.

## Exact note values

When you want a specific note value rather than a count of beats, write an
underscore and the value — `1` whole, `2` half, `4` quarter, `8` eighth, `16`
sixteenth:

```kf-src
C_2 G_2             two half notes (a 4/4 bar)
C_4 F_4 G_4 Am_4     four quarter notes
```

Add `.` for a dotted value and `t` for a triplet:

```kf-src
C_4.                dotted quarter
C_8t D_8t E_8t       an eighth-note triplet
```

### Durations stick

A duration carries forward to the chords after it, so you only write it once:

```kf-src
C_2 G F D            every chord is a half note — two bars
```

The length sticks until another duration changes it. For a default that covers a
whole section (or the whole chart, if placed before any section), use
`/Duration`:

```kf-src
/Duration 4          every following chord is a quarter note by default
C F G Am
```

A section's own `/Duration` overrides a chart-wide one for that section only. To
give a single chord a different length *without* disturbing the sticky default,
prefix it with `!`:

```kf-src
C_2 !G_4 F           G is a one-off quarter; F is still a half note
```

## Repeating a bar

`%` repeats the previous bar exactly:

```kf-src
C  %  %  %           four bars of C
Am F C G  %          the four-bar phrase, then a copy of its last bar
```

## Bar lines, when you want them

You rarely *need* `|` — the measure-fill default and the rhythms above already
say where bars fall. But some people like to draw the bars in, and Keyflow reads
them two ways.

Put a `|` before each chord and you get the familiar one-chord-per-bar layout,
now with visible bar lines (spaces optional):

```kf-src
|G |C |Em |D         four bars, one chord each
|G|C|Em|D            the same
```

Or fence several chords between a pair of `|`, and they split that bar evenly —
just like a group:

```kf-src
| G C Em D | F |     bar of four (one beat each, in 4/4), then a whole bar of F
| G C | Em D | F |   two half-bar pairs, then a whole-bar F
```

Each fenced bar divides on its own, so you can mix densities freely down a line.

## What's next

- **Melody** — notes use the same letter-or-number naming as chords, plus these
  same durations.

---

Previous: [[notation-systems|Notation Systems]] · Next: [[melody|Melody]] · Up: [[keyflow|Keyflow Guide]]
