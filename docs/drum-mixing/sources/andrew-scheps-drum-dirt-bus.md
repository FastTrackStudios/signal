---
title: '(Drums) Parallel Processing "Drum Dirt" Bus | One Track Or Entire Kit'
speaker: Andrew Scheps
channel: Puremix
url: https://www.youtube.com/watch?v=AcfeMxUs-xo
video_id: AcfeMxUs-xo
kind: video-technique (excerpt from a Puremix mix session)
genre: rock / pop-rock
drums: real kit, clean and well recorded
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/AcfeMxUs-xo - (Drums) Parallel Processing ＂Drum Dirt＂ Bus ｜ One Track Or Entire Kit.txt"
transcript_kind: YouTube auto-captions (timestamped)
date: 2019-01-29
duration: "5:18"
summarised: 2026-11-24
tags: [drums, parallel-distortion, parallel-processing, drum-bus, automation, cymbals]
---

# Andrew Scheps — the "Drum Dirt" bus

A parallel **distortion** send for the whole kit, kept in his template, used
mostly in choruses. See also [his mix-bus and drum processing
note](andrew-scheps-mix-buss-drum-processing.md), where the same bus appears
on another song.

**How to read this note.** His words and numbers. **[I]** = mine.
`[caption?]` = garbled.

## The chain

- He'd normally use **SansAmp** for distortion, but it "is not phase
  coherent" in parallel: "if you put it parallel and bring it up everything
  starts to sound phasey" (00:02). **[I]** a parallel lane must not shift
  phase against the dry kit, or the blend combs.
- **Lo-Fi (subtle), then iZotope Trash 2**, "smashing the hell out of
  everything" ("lowii" / "trash too" `[caption?]`). He then removes the Lo-Fi:
  "not adding anything".
- **"An unbelievably broad dip in the mid-range"** after the distortion,
  "because it gets rid of all of the cymbals that have just been made
  unlistenable by all the distortion". The bus is "all about **mid-range and
  low end** … below 1 k", with "whatever air … for a little bit of extra
  crispiness" let through (01:09–01:45).
- **Return at −14.5 dB** "because of how much level will come out".
- On its own "it sounds terrible"; blended, "my drums are badass … gives them
  power that you don't get without this distortion" (01:09).

## How he uses it

- **Mute automation on the return**, not the send: "it doesn't have any
  sustain to it so it doesn't matter where I actually do the mute". Usually
  **in for choruses only** (01:45–02:18), on a one-bar grid.
- Sends per track: on this song he **turned the kick's send down** because he
  liked what it did "to the snare and the overheads more than the kick
  drum", which also removed "ultra low end" (04:04).
- What it adds: "a little bit of ringiness, a little bit of length", "a little
  bit of weight".

## Signal mapping (inference)

| His move | Signal phase | Note |
|---|---|---|
| Parallel distortion bus, mids/lows only, cymbals EQ'd out after | Tone (saturation, parallel) | `Process/Compress/Crunch` (or a `Process/FX` lane): `saturate` Hard/Diode high drive, then `eq` with a wide cut ~2–8 kHz and LPF, return low. |
| Phase-coherent distortion only | — | Signal's `saturate` is memoryless (no crossover) [I — check]: safe in parallel. |
| Kick send lower than snare/OH sends | Tone | Per-track send levels into the lane. |
| Chorus-only via return mute | Balance (automation) | Mute the lane per section. |
