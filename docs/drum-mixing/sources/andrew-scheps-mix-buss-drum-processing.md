---
title: "Mix buss & drum processing with Andrew Scheps"
speaker: Andrew Scheps
channel: Mix with the Masters
url: https://www.youtube.com/watch?v=8_J9DP4EsFk
video_id: 8_J9DP4EsFk
kind: video-walkthrough (preview from "Inside the Track")
genre: rock/pop with stylised "machine drums played by humans"
drums: real kit (kick in/out, snare top/bottom, toms, hat, OHs, ride, room) + a re-amped ambience track + two kick samples
transcript: "/run/media/AudioHaven/fts-corpus/mix-references/research/transcripts/8_J9DP4EsFk - Mix buss & drum processing with Andrew Scheps.txt"
transcript_kind: YouTube captions (human-edited, timestamped)
date: 2020-09-02
duration: "8:33"
summarised: 2026-11-24
tags: [drums, mix-bus, limiter, parallel-compression, drum-bus, rooms, overheads, hi-hat, automation, reverb]
---

# Andrew Scheps — mix bus and drum processing

Scheps' mix bus, then the drum kit. Useful for three things: **a limiter that
lengthens the kick and snare** (4–5 dB on peaks), **several parallel
compressors fed by the drum aux**, and **parallel/room layers that come and
go by section** so the kit grows with the song.

**How to read this note.** His words and numbers. **[I]** = mine.

## Mix bus (00:02–03:25)

- A broad "happy face" Pultec-style EQ, "the sound of the console": **+2.5 dB
  at 100 Hz, +4 dB very broad at 10 kHz** ("almost like a shelf").
- **Oxford Limiter last**, ceiling **−0.1 dB**, slowest attack/fastest release,
  auto gain off, a little "enhance" (inflator).
- **The limiter shapes the drums:** "this is actually a big part of how I'm
  lengthening the kick and the snare. The kick and snare are very, very loud
  in the mix without this … they just come poking out and they're too loud,
  and they're too short." With it, "probably getting up to **four or five dB
  of gain reduction** on those peaks". **[I]** drums mixed loud *into* a
  final limiter: the limiter is part of the drum sound (Nolly also mixes into
  his master limiter).

## The drum kit (04:10–20:25)

- **Hi-hat mic not used; ride and hat spots not used**; the room "just in
  places" (04:43). "There's so much cymbal bleed into the closer mics and
  especially the room mics … the overheads aren't a big part of the cymbal
  sound" (17:32).
- **Overheads:** a little top-end EQ boost, then **a high shelf** "to get a
  little more top end"; in dry sections "they are the thing that just gives a
  little bit of air to the kit".
- **Drums Dirt** (see [the Drum Dirt note](andrew-scheps-drum-dirt-bus.md)):
  HPF → Lo-Fi (light clipping) → Trash (heavy clipping) → EQ "sucking out all
  of the nasty mid-range"; only in "the pushes" and choruses. "The cymbals
  don't push as much … that hat really jumps out".
- **Rooms brought in only for the pushes**: "you don't notice those coming and
  going … it just sounds like … the drummer is beating the crap out of the
  drums" (07:31–08:12).
- **Why parallel layers instead of a fader:** "it's not the same as just
  turning up the drum kit, if I did that, what you would have is a loud dry
  drum kit. But instead, drum kit stays the same level but it just gets thick
  underneath everything else" (07:01).
- **Kick + snare crush:** a shared parallel **dbx 160 at ~4–4.5:1**, "each
  transient just triggers the compressor", "not too much compression … adds a
  little bit of length and it changes the shape of the attack" (12:11).
- **Two stereo parallel drum compressors from the drum aux:** a **Fatso**
  ("quite a bit of peak limiting because the attack's so hard"; without it,
  "a little more boring, a little dryer") and a **"drum crush": a Fairchild
  670 multi-mono (L/R unlinked)** "working pretty hard". "Everything just
  sounds like more fun with them" (19:13–20:25).
- **Snare reverb, choruses only** (mute automation): a short ambience "almost
  like gated reverb", to keep the snare's *relative* length when the dense
  arrangement "suck[s it] dry".
- Toms: an aux adding **+100 Hz and +5 kHz** "attack and boom", into a short
  ambience that is pitch-shifted (an 80s/90s AMS trick).
- Snare top: a notch on a ring that "is also the harsh part of the hi-hat
  bleed"; a clipper (Lo-Fi) for "a little bit of level".

## Signal mapping (inference)

| His move | Signal phase | Note |
|---|---|---|
| Mix-bus limiter doing 4–5 dB on kick/snare peaks | Polish (limiting) at the top | Our drums are locked at −23 LUFS; a master limiter is a later step. On the drum bus: a clipper/limiter catching only kick/snare peaks. |
| Kick+snare parallel crush at 4:1 | Tone (parallel, `Process/Compress/Punch`) | Send kick and snare only. |
| Fatso + unlinked Fairchild on the drum aux | Tone (parallel, `Tight`/`Smash`) | Unlinked L/R: Signal's comp links stereo [I — check]. |
| Rooms and dirt only in pushes/choruses | Balance (automation of Process lanes) | Mute/unmute lanes per section. |
| Snare reverb in choruses only | Depth (automation) | |
| OH high shelf, OHs as air | Tone EQ | |
