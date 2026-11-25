---
title: "Mixing metal drums: ROOMS & OVERHEADS"
speaker: unnamed URM presenter
channel: URM Academy
url: https://www.youtube.com/watch?v=STpPUFXIUH0
video_id: STpPUFXIUH0
kind: video-walkthrough (part of a series: kick, snare, then this, then parallel/bus)
genre: metal
drums: sampled cymbals; overheads, mono room, near room, far room
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/STpPUFXIUH0 - Mixing metal drums： ROOMS & OVERHEADS.txt"
transcript_kind: YouTube auto-captions (timestamped; most settings shown on screen, not spoken)
date: 2017-04-23
duration: "10:55"
summarised: 2026-11-24
tags: [drums, overheads, rooms, mono-room, transient-shaper, compression, saturation, metal]
---

# URM Academy — metal drums: rooms and overheads

A walkthrough with the settings mostly on screen; the transcript has the
**order** and the **intent**, and a few spoken numbers. Weak on numbers,
useful for **giving each room a different job**.

**How to read this note.** What the presenter says. **[I]** = mine.
`[caption?]` = garbled.

## Overheads

1. **EQ:** roll off the low end "to get some of that kick out"; **tame the
   cymbals' high end, judged on a crash-heavy part**; check for an annoying
   ringing tone (01:26–02:17).
2. **Console saturation** (Slate VCC, "Brit N" `[caption?]` — "VCC Channel …
   the BR and setting"), **drive and input at maximum**, "to give a little
   bit more weight to the snare itself within the overheads" (02:17–03:01).
3. **Light compression in parallel mode**, "just … a little bit more
   consistency in volume". With samples the cymbals are already consistent,
   so only a little (03:01–03:42).

## Rooms — one job per mic

- **Mono room:** compressed hard to sound bigger — Kramer PIE, decay 1, limit
  mode, threshold **−14**, output **6.5** (04:23).
- **Near room:** a Kush UBK-1 ("ubk one" `[caption?]`), "squash it a little
  bit" (04:56–05:49).
- **Far room:** **a transient designer, sustain up**, "to lengthen the room"
  (Drumforge "DF trons" `[caption?]`) (05:49).
- **Room EQ (same curve on all three rooms):** because "we're using a lot of
  compression on the rooms I don't want to have a lot of cymbals because
  [they] can get [harsh] very easily when compressing really hard": tame the
  cymbals, **not with a plain low-pass** — "I don't want to cut out all of
  the high end, I still want to save the meat of the cymbals"; also cut
  "some of that ringy tone in the low mids from the snare" (06:49–08:18).
- **Room stereo bus:** a little compression ("adds a nice amount of life"),
  then an EQ to tighten the low end (08:18–08:53).
- Then the rooms were "a little bit too loud" in context: pulled back (09:39).

## Signal mapping (inference)

| Move | Signal phase | Note |
|---|---|---|
| OH HPF, high-end taming on crashes | Rescue / Tone EQ | |
| OH console saturation for snare weight | Tone (`saturate` Transformer/Tube) | |
| OH light parallel comp | Tone (`comp` `mix` ~0.3) | |
| Mono room crushed, near room squashed, far room sustain via transient shaper | Tone | Ours has one `Rooms` lane; the transient shaper (being added) fits the far-room role. |
| Room EQ: cymbal cut (shelf/bell, not LPF), snare ring cut | Tone EQ (`Rooms`) | Apply after compression is set: compression raises the cymbals. |
