---
title: "Keys to the Castle – Cymbal Mixing Secrets with Eyal Levi, Joey Sturgis and Josh Newell"
speaker: Eyal Levi (author); Josh Newell (one tip)
channel: GearGods
url: https://geargods.net/features/keys-to-the-castle-cymbal-mixing-secrets-with-eyal-levi-joey-sturgis-and-josh-newell/
video_id: eMPON6zRnyg (embedded; no captions available — the article text is summarised)
kind: article
genre: metal
drums: general (live overheads)
transcript: null (the embedded video has no captions; summarised from the article)
transcript_kind: article
summarised: 2026-11-24
tags: [drums, cymbals, overheads, harmonics, sidechain, limiter, parallel-compression, snare-in-overheads]
---

# GearGods — cymbal mixing secrets (Levi, Newell)

An article by Eyal Levi (URM) with one tip from Josh Newell. Sturgis is in the
title and the embedded video, but no technique in the text is his.

## Why cymbals matter (Levi)

- Bad cymbals make a mix sound "basement", cause fatigue and make you keep
  turning up. Cymbals "are very broadband and can swallow other instruments
  whole" — guitars, vocals, snare.
- **Burying them "should be a last resort."**

## 1. EQ: cut what hurts (Levi)

- HPF the overheads (stand rumble, room resonance, unwanted drum bleed).
- **Boost-and-sweep with a narrow Q**, find what hurts, cut it.
- **Check the harmonics of a problem frequency:** a resonance at **3 kHz** →
  also check **1.5, 6 and 12 kHz**.
- Don't "completely neuter your cymbals".

## 2. The snare in the overheads (Levi)

- Don't take all the drums out of the overheads or rooms: you get only "the
  sound of a stick hitting a head".
- **If the snare is too loud in the OHs, any compressor on them reacts to the
  snare, not the cymbals.** Prepare the OHs first:
  - **A: duck the OHs from the snare** (sidechain compressor keyed by the
    snare send), light: "a little goes a long way"; inaudible.
  - **B: a limiter on the cymbal track** (Waves L2), threshold low enough to
    catch the snare hits and bring them level with the cymbals, so later
    compression doesn't react to the snare ("a weird pumpy imbalance").

## 3. Parallel compression for sustain (Newell)

- When cymbals decay too quickly: mult the cymbal track, compress the copy
  with a **very slow release**, blend it under. You should hear only "longer
  cymbals".

## Signal mapping (inference)

| Move | Signal phase | Note |
|---|---|---|
| Harmonic check of a ring (×½, ×2, ×4) | Rescue EQ | Cheap to do from a spectrum: our OH ring at f → look at f/2, 2f, 4f. |
| Snare ducking in the OHs | Relational (keyed dynamics) | The keyed dynamic EQ keyed by snare hits, one wide band. |
| Limiter on OHs to level the snare peaks | Tone | `comp` 20:1, attack 0.1 ms, lookahead 2–5 ms, release 50 ms. |
| Slow-release parallel comp for cymbal length | Tone | `comp` `mix` ~0.3, release 500–1000 ms. |
