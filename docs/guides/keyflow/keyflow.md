---
title: Keyflow Guide
kind: concept
type: concept
order: 0
---

# Keyflow Guide

A hands-on tour of the `.kf` format. Each page introduces one concept and builds
on the last, so by the end you can read and write a complete chart.

Keyflow is **plain text**. You can type a chart in any editor, paste it into a
chat, commit it to git, or generate it from MIDI — and it renders to the same
lead sheet either way. The format is designed to be playable *as-is*, without
tooling.

A taste of what a finished chart looks like:

```kf-src
Vienna (Live) - Billy Joel
4/4 140bpm #Gm

VS
Gm Bb F Ab Eb Bb C D11

CH
Bb F Ab Eb
```

That's two ideas: a **header** describing the song, then **sections** of music.
Notice there are no bar lines — `Gm Bb F Ab` is simply four bars, one chord each.
Keyflow uses rhythm modifiers (not `|`) to say how long a chord lasts; a bare
chord just fills its bar.

## The guide

1. [[structure|Structure]] — the document: title, artist, time signature, tempo, key.
2. [[sections|Sections]] — organizing the song: section names, lengths in bars, repeating a part, labels, and custom sections.
3. [[chords|Chords]] — writing a single chord: root, quality, seventh family, extensions, alterations, slash bass.
4. [[notation-systems|Notation Systems]] — the three interchangeable ways to name roots: letter names, Nashville numbers, Roman numerals.
5. [[rhythm|Rhythm]] — how long each chord lasts: the one-chord-per-bar default, slashes, `()` groups, and note-value durations.
6. [[melody|Melody]] — writing the tune line: notes as letters or numbers, octaves, durations, stacked notes, and pairing it with the chords.
7. [[lyrics|Lyrics]] — words under the chords: a `[lyrics]` line, `{Chord}` markers on syllables, and hyphen splits for melisma.
8. [[key-meter-changes|Key & Meter Changes]] — moving to a new key or time signature mid-song, and the `!T` one-bar meter change.
9. [[annotations|Annotations & Expression]] — staff text, instrument cues, dynamics, and crescendo/decrescendo hairpins.
10. [[repeats|Repeats & Endings]] — repeat a bar, a line, or a span, and write first/second endings.

## Two things to know up front

- **One chord per bar by default.** Space-separated chords each take a whole
  measure. You only reach for rhythm modifiers (slashes, durations) when a bar
  holds more than one chord or an off-beat feel — that's a later page.
- **Three ways to name a chord, everywhere.** Letter names (`G`, `Cmaj7`),
  Nashville numbers (`1`, `4`), and Roman numerals (`I`, `IV`) are all
  first-class, for both chords and melody. Pick the one that fits the chart.

## Charts in these notes

Keyflow snippets in a note are ordinary fenced code blocks; the fence's info
string says how the snippet renders:

- `kf` — the engraved chart only (the everyday choice).
- `kf+` — the source text and the engraved chart together, side by side.
  The chapters here use it because the source is the lesson.
- `kf-src` — the source only, as plain code (no engraving).

Every engraved chart also has a small `</>` button (top-right, on hover) that
flips its source open or closed while you read.

Back to all [[guides|Guides]].
