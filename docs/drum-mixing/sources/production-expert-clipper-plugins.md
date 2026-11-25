---
title: "What Are Clipper Plugins And Why Use Them?"
speaker: Production Expert (author), quoting Eli Krantzberg, Trond Nedberg, Shane McFee (Kazrog), Rik Simpson
channel: Production Expert
url: https://www.production-expert.com/production-expert-1/what-are-clipper-plugins-and-why-use-them
video_id: null (embeds a Dan Worrall video on aliasing/oversampling; not identified)
kind: article
genre: general (drums one of the main cases)
drums: general
transcript: null (article)
transcript_kind: article
summarised: 2026-11-24
secondhand: true
tags: [drums, drum-bus, clipping, limiting, oversampling, transients]
---

# Production Expert — clipper plugins (article)

A round-up on clipping. Kept for the drum-bus clip/limit stage, where the
other sources (Scheps, Nolly) use a limiter and say little about clippers.

## Points

- A limiter blunts "percussive, transient-heavy sounds like drums"; clipping
  very short drum transients adds distortion that "can preserve the impact
  of the drums while limiting their peak value".
- **Drum bus use (Eli Krantzberg):** KClip **after a bus compressor and EQ**,
  "none of them … working particularly hard": "adds a bit of thickness", helps
  the drums blend.
- **Kick / drum bus (Trond Nedberg):** a clipper on heavy kicks so the 2-bus
  compressors work "more for colour without the kick interfering".
- **A demo:** solo drums raised **12 dB** with Pro-L vs a hard clipper; the
  clipped version "punchier", with "a noticeable change of timbre in the
  snare".
- **Soft vs hard:** "the sharpness of the corners"; soft modes also add
  harmonics below the threshold ("a clipper AND a saturator").
- **Oversampling** is "recommended when using clipping unless a potentially
  harsh sound is desired" (aliasing). Hard clippers differ mainly in their
  oversampling filters (McFee).
- **Order:** clip before the limiter, so the limiter works less (Simpson);
  clip → limit → clip is seen on master buses.
- No dB figure for "how much": "a light touch"; "as long as you don't go
  overboard".

## Signal mapping (inference)

| Point | Signal |
|---|---|
| Drum-bus clipper after comp + EQ, light | Polish on `Drums`: `saturate` shape **Hard**, drive 1–3 dB, `level` −(drive) to stay gain neutral. Check the GR on kick/snare peaks only. |
| Oversampling | Check whether Signal's `saturate` oversamples [I — open]. A hard clip without it aliases on cymbals. |
| Clip, then limit | `saturate` Hard → `comp` 20:1 / 0.1 ms / lookahead as the limiter. |
