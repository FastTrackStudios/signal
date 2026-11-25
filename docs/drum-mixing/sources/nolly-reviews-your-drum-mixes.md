---
title: "What you Need to Hear! - NOLLY Reviews YOUR MIXES!"
speaker: Adam "Nolly" Getgood
channel: Kohle Audio Kult
url: https://www.youtube.com/watch?v=Y-8ndr_t7VA
video_id: Y-8ndr_t7VA
kind: mix-critique
genre: metal (thrash/blast sections; multitracks from "The Nolly Drum Experience" course)
drums: real kit recorded by Nolly in a big room; students' mixes of the same multitracks
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/Y-8ndr_t7VA - What you Need to Hear! - NOLLY Reviews YOUR MIXES!.txt"
transcript_kind: YouTube auto-captions (timestamped)
date: 2024-07-27
duration: "19:05"
summarised: 2026-11-24
tags: [drums, critique, rooms, cymbals, hi-hat, ride, gates, linear-phase, bus-compression, samples]
---

# Nolly reviews students' drum mixes

A "best of" cut from a 45-minute critique: Nolly listens to students' mixes of
the same multitracks (his course) and names what works and what doesn't.
Useful because it is a list of **failure modes**, judged against one kit.
Part of it is a course advert; left out here.

**How to read this note.** Nolly's opinions, with his numbers. **[I]** = mine.

## What he flags

**Rooms (his summary of the common shortcoming, 16:31)**
- "People either **washing out their cymbals** a bit too much and making
  things sound a bit distant, or … **leaving too much low end** in them and
  it's kind of swamping the mix without bringing much energy despite the
  level of the room mic."
- A big room and a small room need opposite moves: in a **snappy small room**
  you may **reduce presence** or **boost body**.
- One mix was "quite pumpy … that kick drum is ducking things a lot,
  especially the reverb tail of the snare": the room sound became "almost
  gated … around the kick drum" (02:41). **[I]** a room compressor (or bus
  compressor) driven by the kick; Signal's `sc_hpf` on the room comp fixes
  the cause.
- "Cymbals sound very roomy" → refine the rooms out a little (04:20).

**Cymbal spot mics (hi-hat, ride)**
- In fast thrash and blast sections the **cymbals get lost** with the band in:
  the hat is "just a little shimmery sound … rather than something which feels
  energetic"; the ride's attack is there but not "a big clangy ride" (11:06).
  Fix: **bring up the close mics** — "that would do quite a lot".
- **Don't pan a mono cymbal spot hard** (03:49): "it becomes evident that it's
  just only in one speaker … not a natural stereo image". Pull it in to where
  the cymbal sits in the overheads.
- A splash with little close mic "sounds really well integrated with the
  crashes": too much close mic makes it "an effect popping out" (12:10).
- **Too much cymbal bleed in a tom** ("the top end of the cymbal is almost
  louder than the top end of the tom", 11:06).

**Linear-phase EQ: avoid on drums (04:55)**
- Pre-ringing on the kick low end: "a slight inrush of low end for every
  kick", "the attack … getting eaten up". "In general I'd just say stay away
  from linear phase EQ … I basically never use [it]."

**Gates (09:20)**
- An aggressive snare gate (to −30 dB or more) dropped soft hits and let
  "choppy" bleed in on fills. **He aims for ~10–12 dB of reduction**, so soft
  hits under threshold are still heard and the open/closed contrast is small.
- Toms: strip silence and fade by hand instead of gating, so soft floor-tom
  hits survive (15:27).

**Bus compression (07:40)**
- A mix felt "uncompressed … a little bit stiff". Putting **his usual bus
  compressor (fast attack to clamp down the snare)** on it made it better:
  "an upfrontness and … rigidity to the snare". **[I]** the FG-Grey setting in
  the [Wildfire note](nolly-periphery-wildfire-masterclass.md): 3 ms, 100 ms,
  4:1, SC HPF 60 Hz, ~3–4 dB GR on the snare.
- Snare "could use a bit more beef": **low shelf ~150 Hz, boosted** (06:32).

**Samples**
- Uneven triggering makes a snare *less* consistent: integrate samples with
  the real snare and make sure every hit triggers (13:47).

## Signal mapping (inference)

| His point | Signal phase / check |
|---|---|
| Rooms washing out cymbals / too much low end | Tone EQ on `Rooms`: high shelf cut, HPF. Check: Rooms' > 6 kHz energy vs the OHs' in a crash section; Rooms' < 120 Hz vs the kick's. |
| Kick pumping the rooms | Tone: room comp `sc_hpf` 100–150 Hz. |
| Hat/ride lost in fast sections | Balance (spot faders), automation per section. |
| Mono spots not hard-panned | Depth (pan to the OH image). |
| No linear-phase EQ on drums | Signal's EQ is minimum-phase [I — check]. |
| Gate range 10–12 dB | Rescue (gate `reduction` −10 to −12 on snare). |
| Fast-attack bus comp clamping the snare | Tone/Relational on the mix bus or drum bus. |

## Open questions

- The bus-compressor settings he "pulls up" aren't read out here.
