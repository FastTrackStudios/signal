---
title: Lyrics
kind: concept
type: concept
order: 7
---

# Lyrics

Words go under the music as a **`[lyrics]` line** inside a section — right below
the chords they're sung against:

```kf-
VS 4
C  C  F  C
[lyrics] Twinkle twinkle little star
```

Words under the chords, engraved:

```kf+
VS 4
C | C | F | C
[lyrics] Twinkle twinkle little star
```

`[lyrics]` is a reserved marker (the words for the section above it), not a
section of its own — unlike the custom `[Name]` headers from
[[sections|Sections]].

## Lining chords up with words

To show exactly where a chord changes mid-line, put it in `{curly braces}` right
before the syllable it lands on:

```kf-
[lyrics] {C}Hello {G}world {Am}how {F}are you
```

The chord sits with the word, so a singer reads the change at the moment it
happens. Words without a brace just carry on under the chord before them:

```kf-
[lyrics] {Gm}Slow down you {A#}crazy child
```

Here `Slow down you` are all under Gm, and `crazy child` under A♯.

## Splitting a word across notes

When one word is sung over several notes or chords, break it with **hyphens** —
each piece is its own syllable:

```kf-
[lyrics] A-ma-zing grace how sweet
```

That's six syllables — `A`, `ma`, `zing`, `grace`, `how`, `sweet` — with the
first word stretched across three. Hyphens and `{chords}` combine, so a chord can
land on any syllable:

```kf-
[lyrics] {Cmaj7}A-{Dm7}ma-{G}zing grace
```

## More than one verse

Stack a `[lyrics]` line for each set of words that shares the same chords:

```kf-
CH 4
F  C  G  Am
[lyrics] first time round we sing this line
[lyrics] second time the words are new
```

You now have every layer of a Keyflow chart: the [[structure|header]], the
[[sections|sections]] that organize the song, [[chords|chords]] in
any of the [[notation-systems|three systems]], their
[[rhythm|rhythm]], a [[melody|melody]], and the lyrics underneath.

## What's next

- **Key & Meter Changes** — the one thing left: moving to a new key or time
  signature partway through a song.

---

Previous: [[melody|Melody]] · Next: [[key-meter-changes|Key & Meter Changes]] · Up: [[keyflow|Keyflow Guide]]
