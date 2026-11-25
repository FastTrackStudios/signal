---
title: "The Greatest MIX TRICK I've found for Overheads"
speaker: Colt Capperrune
channel: Colt Capperrune
url: https://www.youtube.com/watch?v=3o4GW-QCpx4
video_id: 3o4GW-QCpx4
kind: video-technique
genre: live-feeling band recording (all tracked together in a room); he says it also works on full-sample kits
drums: real kit, overhead mics
plugin: FabFilter Pro-Q 4 (spectral dynamics + dynamic EQ), then saturation
transcript: /run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/3o4GW-QCpx4 - The Greatest MIX TRICK I've found for Overheads.txt
transcript_kind: YouTube auto-captions (no timestamps, mishearings)
summarised: 2026-11-24
tags: [drums, overheads, cymbals, snare, dynamic-eq, spectral-dynamics, resonance-suppression, saturation]
---

# Colt Capperrune — the overhead trick

One technique for the **overheads**, which he calls the best he's found: fix
three recurring problems *with dynamics instead of static EQ*, so the
overheads can go **much higher in the mix** and still sound natural. It's
built from several plugins in the past; since **FabFilter Pro-Q 4** it all
fits in one.

**How to read this note.** What he says he does, with his numbers. **[I]** and
**[Signal]** items are my mapping, not his words. `[caption?]` marks garbled
auto-captions. The captions have no timestamps, so I can't tie steps to the
`t=278s` in the link.

## The problem it solves

The same three things on nearly every kit's overheads, to varying degrees:

| Problem | Where | Why static EQ doesn't fix it |
|---|---|---|
| **Cymbal harshness**: crashes, china | ~2–2.5 kHz up to ~5 kHz | Cutting it makes the overheads stop sounding **open**: the openness lives in ~2.5–5 kHz, and the snare starts to sound congested and closed off. |
| **Snare boxiness** in the overheads | ~400 Hz (he sets it at ~450 Hz in the demo) | Cutting it makes the overheads **harsher**: a lot of their warmth lives there. |
| **Thin snare** | its fundamental, "usually somewhere between 180 and 250 Hz" | He likes thick snares and wants more thump from the overheads. |

**The goal:** soften the cymbals, take the box off the snare and thicken it,
so he can push the overhead fader a lot higher and it still sounds natural,
**even with full samples on the kit**, because more overhead in the mix
doesn't bring harshness, boxiness or thinness with it.

## The setup (Pro-Q 4), in order

### 1. Spectral dynamics on the cymbals: two bands
- Two **spectral dynamics** bands pulled down (he uses the modifier-drag
  gesture in Pro-Q 4): one around **~2.5 kHz** and one up around **~6 kHz**,
  both at **24 dB/octave**.
- **Why two:** crashes usually live higher than the china, and a small and a
  large crash sit in different ranges. 24 dB/oct "feels like it puts the two
  filters together".
- On the **Expert** tab of each band:
  - **Density a little finer**, so it adjusts more specific frequencies.
  - **Attack a little slower**, so it **doesn't catch the snare** and only
    catches the crashes.
- **Set the thresholds by ear,** playing a section with crashes. In the demo:
  - the lower band ends up **centred around the china/crash at ~3.5 kHz**;
  - the upper one a bit higher still;
  - he checks it's "barely catching any snare drum" (a little when the drummer
    hits harder) while catching the hi-hat and all the crashes.
- **A high shelf** added to open the top back up.

### 2. High-pass the overheads
- Remove the kick's low end from the overheads: **high-pass fairly steeply,
  right under the snare's thump** (visible on the analyser).

### 3. Dynamic cut of the snare box
- A **regular dynamic band** (not spectral) at **~450 Hz**, **24 dB/oct, very
  narrow**, pulled well down.
- **Attack all the way fast**, because this one should catch the snare.
  (He found the default was already catching just the snare.)
- Because the cut only happens when the snare hits, the overheads keep their
  warmth between hits.

### 4. Dynamic *boost* of the snare thump (expansion)
- On the snare's thump, he **pushes the dynamic band up** instead of down, so
  it **expands**: the boost only happens when the snare hits.
- Widened to **24 dB/oct** ("don't ask me why I like 24 dB, it just feels
  good to me for any sort of stuff like this").
- He doesn't state the frequency on screen in the transcript; his earlier
  range is **~180–250 Hz**, the snare's fundamental.

**Result of 3 + 4:** the snare gets "so much better" and "all of that change in
the tone of the snare drum is with dynamics": it **keeps the warmth in the
cymbals while enhancing the thud and removing the box**. He says there's "a
little bit of a high shelf" (EQ) and that's all.

### 5. Saturation, to roll off the top
- He likes **rolling off the top end** with it. Use "whatever you like":
  recently a tube/tape plugin (name garbled in the captions, "T three"
  `[caption?]`) and **UAD Oxide tape**, which he is "absolutely in love with".
- Then **back the overhead fader down** to blend, since it's now much fuller.
  Blended in: "thick thick as all get out".

## Context he gives

- The demo song was **tracked as a full band in a room**, so guitar, bass and
  vocal bleed is in the overheads, which he loves ("a very live feeling
  record").
- He stresses the recording was **very good**: great mic technique,
  "everything's in perfect phase", good room and kit. The boxiness and
  harshness are "just something that exists in like every drum kit almost",
  not a knock on the drummer, the engineer or the mics.
- He demos on a quiet intro (kick, snare, hat only) for the snare box, and a
  busy crash-heavy ending for the cymbals.

## Signal mapping (inference)

**[I]** throughout.

| His move | Signal phase | Note |
|---|---|---|
| Spectral-dynamics bands on the crashes/china | Polish (resonance suppression) | Same job as soothe in the Nolly notes. But see the question below. |
| Steep high-pass under the snare thump | Rescue EQ | Corrective. |
| Narrow dynamic cut at ~450 Hz, triggered by the snare | Rescue EQ (corrective, dynamic) | Cuts only while the snare sounds. |
| Dynamic boost (expansion) on the snare fundamental | Tone (tonal EQ, dynamic) | Compare the one-note track, `Snare/SUM/Fund`, which reinforces the same note from the snare lane. Here it's done inside the overheads. |
| High shelf | Tone | |
| Saturation, rolling off the top | Tone (saturation) | |
| Fader back down after | Balance | The fader sets the level, not a block. |

**Questions this raises for the Signal method:**
- `Cymbals/OHs` is a **mic**, and the roles table gives mics **Rescue and Tone
  only**. Spectral/resonance suppression is a **Polish** block, so either
  this is a Rescue EQ on the overheads or the role rule has an exception here.
- The dynamic bands are **sidechain-like**: the snare box cut and thump boost
  react to the snare's hits. In Signal the snare already has a
  trigger. A trigger-keyed band on the overheads, instead of the overhead
  detecting the snare itself, would avoid the cymbals triggering it. His fix
  is a slow attack on the cymbal bands and a fast one on the snare band.
- **Gain neutral:** both the dynamic and the spectral stages change level
  depending on the material, so each phase's make-up would need measuring on
  the song, as with the compressors.

## Open questions

- The exact **frequency, Q and range** of the thump boost are not said aloud.
- The **threshold and range** of each band are set by ear and not given.
- The saturation plugin name (not UAD Oxide) is garbled.
- The video is **sponsored (Sweetwater) with affiliate links** (Pro-Q 4 and the
  others). I left that out of the summary.
