---
title: "How to Easily Mix Cymbals"
speaker: Joey Sturgis
channel: Joey Sturgis Tones
url: https://www.youtube.com/watch?v=e3jwLM8lC80
video_id: e3jwLM8lC80
kind: video-technique
genre: metal / metalcore (clean modern metal vs "vibey" rock)
drums: two cases — sampled cymbals from a drum sampler, and a live kit in a small room
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/e3jwLM8lC80 - How to Easily Mix Cymbals.txt"
transcript_kind: YouTube auto-captions (timestamped; "symbols" = cymbals throughout)
date: 2021-10-20
duration: "7:56"
summarised: 2026-11-24
tags: [drums, cymbals, overheads, spot-mics, bleed, resonance-suppression, compression, drum-bus, sampled-cymbals]
---

# Joey Sturgis — how to easily mix cymbals

A short JST tutorial. Few numbers, but a clear **order of operations** for
cymbals and the distinction between sampled and live cymbals. Partly a
product video (the "softener" he names is a JST plug-in).

**How to read this note.** His words. **[I]** = mine.

## Sampled cymbals (00:32–03:03)

1. **EQ:** "filter out any excess low end and then find the harsh
   frequencies and reduce them until you're left with a pleasant high end".
   Samples need less EQ: "already processed to sound pretty good".
2. **A little compression**: the sample track "has bleed from the drum
   shells, this will even out the transients to keep the focus on the
   cymbals".
3. **An "algorithmic softener"** (DF Clarify; resonance/harshness suppression
   against a noise profile): "cuts problem areas which makes the whole thing
   feel brighter".
4. **Glue to the kit with bus compression:** "each time a kick, snare or tom
   transient hits the compressor the entire drum performance is ducked …
   when the snare hits the threshold the cymbals come down a little bit
   too". The point: dry, bleed-free cymbal samples otherwise sound pasted
   on.
5. **A drum reverb** after the bus comp (or on a send) "makes different
   samples sound like they're playing into the same room". For "vibey rock"
   rather than "super clean metal", try other orders.

## Live cymbals (03:36–06:58)

- Sources: **overheads** (12–18 in above the cymbals), **spot mics** (6–8 in,
  condenser) on any cymbal that "needs a lot of detail or is very important"
  (hat, ride, china, splash), **rooms** (ribbons warmer/mid-range,
  condensers brighter/more transient and cymbal).
- **Edit bleed out of the spot and tom mics first**, because compression
  "will bring up everything on the track".
- Each cymbal mic: a little EQ and compression, then **all to one cymbal
  bus**:
  - **HPF** so "my cymbal processing [doesn't] smear the kick";
  - cut harsh, constant build-ups: **"most tracks have a build-up around
    500 Hz and 3 k"**;
  - **compression to even out the shell transients** that build up when the
    mics sum;
  - **the softener** again for the resonances;
  - then the drum bus compressor.
- **Warning:** "be careful not to cut too many notches … or you're just going
  to end up with something that sounds like white noise which will get lost
  in the mix."

## Signal mapping (inference)

| His move | Signal phase | Note |
|---|---|---|
| HPF, cut 500 Hz / 3 kHz build-ups | Rescue EQ on cymbal mics; Tone EQ on `Cymbals` | Ours: OH 450 Hz −3 done; 3 kHz not yet. |
| Compression to even shell transients on the cymbal bus | Tone (on `Cymbals`) | Ours: cymbal comps idle. |
| Softener / resonance suppression | Polish (on `Cymbals`) | Signal has no soothe-style block; the keyed dynamic EQ is the nearest. |
| Bus compression glueing samples | Tone (`Drums` glue) | Our kit is sample-based: this matters. |
| Shared drum reverb | Depth | A short room verb on the kit send, cymbals included at a low send. |
