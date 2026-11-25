---
title: "NOLLY MIXING MASTERCLASS - Cymbal Processing"
speaker: Adam "Nolly" Getgood
channel: GGD
url: https://www.youtube.com/watch?v=c4jHCyfIQtU
video_id: c4jHCyfIQtU
kind: video-walkthrough
genre: rock (a GGD library demo song; the methods are the ones he uses on metal)
drums: GGD library recorded at Middle Farm (real room, overheads, spot mics, stereo + mono room)
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/c4jHCyfIQtU - NOLLY MIXING MASTERCLASS - Cymbal Processing.txt"
transcript_kind: YouTube auto-captions (timestamped, mishearings)
date: 2019-02-08
duration: "15:55"
summarised: 2026-11-24
tags: [drums, overheads, cymbals, hi-hat, ride, rooms, upward-expansion, sidechain, automation]
---

# Nolly — cymbal processing (GGD masterclass)

One chapter of a GGD masterclass: the cymbal bus (overheads and spot mics
treated together), then the room bus, then section-by-section automation.
Older than his *Wildfire* masterclass (2019 vs 2025): compare the two before
taking either as his current method. Timestamps are the transcript's.

**How to read this note.** What he says he does, with his numbers. **[I]** /
**[Signal]** = my mapping. `[caption?]` = garbled captions.

## The idea

- **Overheads are cymbal mics, not kit mics.** He doesn't use them for beef;
  the **rooms** carry the shells' body (01:03). Some engineers mix "overheads
  first, fill in with close mics"; he doesn't.
- **Treat overheads and cymbal spots as one group**, find a general blend
  (00:00). In this mix the **hi-hat spot is "turned way down"**, barely used;
  the **ride spot is more prominent**, because a ride, loud in the room,
  "tend[s] not to cut through into the overheads anywhere near as much as the
  crash cymbals and hi-hats" (00:31).

## Overheads / cymbals chain

1. **High-pass at 280 Hz**, "huge" (01:03), plus a low-pass (~15 kHz, he
   says later, 04:55).
2. **A small cut around 500 Hz** that "sounded a bit boxy".
3. **The big one: a bell cut of −8 dB at ~6 kHz** (02:11). "There's always an
   issue somewhere between about 5 to 7 k with crash cymbals in particular …
   a huge great big spike there that sounds very abrasive." He calls this the
   move that makes or breaks cymbals: "if you don't find where the crash
   cymbals in particular [are] ringing out most harshly and cut there it's
   gonna be really difficult to make [a] cymbal sound good in the mix".
4. **One or two notches on ringing frequencies**, not more (02:42–03:50): a
   crash "whistle" at **2,750 Hz** (08:13). "You could drive yourself crazy
   finding every single frequency that needs notching … and end up with … a
   slightly neutered sounding cymbal." Focus "on the one or two most
   abrasive."
5. **Compression, GGD Smash & Grab in "smash" mode, overhead preset**
   (03:50; "jeugd smash-and-grab" `[caption?]`). Two jobs:
   - **tame the snare (and tom) transients in the overheads**, so moving the
     overhead fader doesn't change the snare sound;
   - **lengthen the cymbals** ("a bit of extra length").
   - **Air control +1.5 dB** to give back the sheen lost to the low-pass.
   - **Mix below 100 %**: "100% mix … can be a bit too aggressive". The
     audible effect: the crash's initial attack "sounds a lot smoother … more
     of a kind of wash that will sit into the mix" (05:29).
6. **A small send to the parallel drum compressor** — "I don't leave
   anything out of it" — but **far less cymbal than snare** (06:02).

## Rooms

- On each room channel: high-pass, **a presence cut on the mono room** ("quite
  trashy sounding"), a little smash compression (06:34).
- **Room bus EQ** (06:34–08:13):
  - **High-pass 120 Hz with a low Q**, a soft roll-off reaching high up:
    "a ton of kick drum coming through in the rooms … just not really
    necessary".
  - **Boost ~1.5 kHz** for aggression in the shells "without making the
    cymbals sound too crazy".
  - **A cut where the cymbals are harsh**, then **a glassy sheen on top**.
  - **The same 2,750 Hz notch as the overheads**: "generally any notch that I
    apply to the overheads I'll apply as well [to] the room channels".
- **Priority in the rooms: the snare.** "What I want to hear above all else
  is the snare drum … explosion expanding into the room." "I'm not afraid to
  shelve away frequencies that are perhaps making the cymbals dominate."
- **Room bus compression, GGD comp "rooms" preset, smash mode:** compress the
  rooms "quite a lot" for "that really thick, glued together room sound"
  (08:46). Problem: it **reduces the snare peaks**, the thing he most wants.
- **The trick — snare-keyed upward expansion on the room bus** (09:19): the
  same GGD gate in **upward-expansion mode**, its **sidechain fed by the
  snare top**. Each snare hit makes the rooms "jump up in volume for a
  certain length of time", restoring the snare's headroom over the
  compressed rooms. It raises the cymbals in the rooms too, but "tricks the
  ear into feeling like there's just a lot more snare". "Such a crucial part
  … there's just no other way I've found of getting that same clarity of the
  snare in the room mics while also getting that really aggressively
  compressed vibe" (10:56). He hears it as the snare's "mid-range-y stereo
  spread".

## Automation by section (11:29–15:24)

- **Verses:** overheads unchanged; kick −2 dB, snare −1.5 dB, snare's
  parallel-comp send about −5 dB `[caption?]` ("about 5 DB for a half DB"),
  less snare reverb, **rooms down "quite significantly"**, and the
  **mono room favoured** (it was muted till then): "keeps them more pinned
  down the middle".
- **The fill into the chorus:** **rooms up** for the fill, and the **drum bus
  +1 dB-ish** `[caption?]` ("about DB in a bit") lasting into the downbeat.
- "Having the room mics cranked in the chorus … sounds great but would
  probably be a bit too much in the verse."

## Signal mapping (inference)

**[I]** throughout.

| His move | Signal phase | Note |
|---|---|---|
| OH HPF 280 Hz, LPF ~15 kHz, 500 Hz cut | Rescue EQ (`Cymbals/OHs`) | Corrective. Ours: HPF 150 Hz, 450 Hz −3. |
| −8 dB bell at ~6 kHz on crashes | Tone EQ, or Polish (keyed dynamic band) | Static in his chain. On a hard-rock chorus with crashes this is the single biggest move. |
| 1–2 notches on cymbal ring (2,750 Hz), mirrored on rooms | Rescue EQ, on OHs *and* Rooms | Measure the ring once (spectrum over a crash), use the same Hz in both. |
| Smash compression on OHs, mix < 100 %, +1.5 dB air | Tone (parallel compression, in-block `mix`) | Signal's comp `mix` does this in one block. |
| Room bus EQ: soft HPF 120 Hz, +1.5 kHz, cymbal cut, sheen | Tone EQ (`Rooms`) | |
| Room comp, smash | Tone (thickness compression) | Ours: 4:1, 3/120 ms, ~5 dB. |
| Snare-keyed upward expansion on rooms | Relational (keyed dynamics) | Needs a hit-keyed **upward** expander/gain lift. The keyed dynamic EQ (one wide band, boost on snare hits) could do it. |
| Verse/fill automation | Balance (automation) | Not a block. |

## Open questions

- The Smash & Grab's settings (attack, release, GR) aren't stated; only the
  mode and preset name.
- "About 5 dB" on the verse parallel send and "about a dB" on the fill are
  garbled.
- The 6 kHz bell's Q isn't given (he says "big bell").
