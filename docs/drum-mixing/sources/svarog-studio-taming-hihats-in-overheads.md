---
title: "Taming hi-hats in the overhead tracks"
speaker: unnamed engineer (Svarog Studio)
channel: Svarog Studio
url: https://www.youtube.com/watch?v=-TxqiBSu6eI
video_id: -TxqiBSu6eI
kind: video-technique
genre: metal (a single by the band Eradicator)
drums: real kit; hi-hat close mic routed to no output, used only as a key
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/-TxqiBSu6eI - Taming hi-hats in the overhead tracks.txt"
transcript_kind: YouTube auto-captions ("high heads" = hi-hats)
date: 2024-08-28
duration: "2:53"
summarised: 2026-11-24
tags: [drums, hi-hat, overheads, sidechain, multiband, ducker, keyed-dynamics]
---

# Svarog Studio — taming hi-hats in the overheads

A short metal-mix trick for **too much hi-hat in the overheads**, when the
hi-hat close mic isn't even in the mix. The same idea appears in forum advice
(Gearspace "Hi hat is killing my overheads": key a dynamic EQ on the OHs from
the hat mic).

**How to read this note.** His steps. **[I]** = mine.

## The problem

- "A little bit too much hi-hat in the overhead track"; the hat close mic is
  "set to no output". A static EQ cut "is going to mess up the overhead
  sound"; a multiband compressor on the hat's range self-triggers on "the
  snare … the rest of the cymbals".

## The fix (two stages)

1. **A high-band multiband compressor on the overheads, keyed by the hi-hat
   close mic.** Each hat hit ducks only the hat's band in the OHs.
2. **Keep the snare out of the key:** the hat mic has lots of snare bleed, so
   every snare hit also ducked the OHs' top and "makes our snare sound a bit
   smaller and weaker". He puts a **gate in ducker mode on the hat key track,
   keyed by the snare bus**: each snare hit *closes* the hat key, so only hat
   hits reach the OH compressor.
- He watches the compressor stop reacting in a snare-heavy, hat-free part.

## Signal mapping (inference)

| His move | Signal phase | Note |
|---|---|---|
| OH high band ducked by hat hits | Relational (keyed dynamic EQ on `Cymbals/OHs`) | Signal's keyed band can be keyed **by the hat's hits** directly (trigger or hit markers), which removes his second stage: no snare in the key at all. |
| Ducker gate on the key, keyed by the snare | — | Not needed if keyed from hat hit markers. |
| Range | — | He sets it by ear; no numbers. Start 2–4 dB at ~8–12 kHz [I]. |
