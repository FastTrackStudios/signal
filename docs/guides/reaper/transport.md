---
title: Transport
kind: input
type: input
category: transport
---

# Transport

The transport is the first thing to get under your fingers — everything else in a session happens around play, stop, and record. These bindings live in the base fasttrackstudio profile (see [[Input System|the input layer]]).

## Play and stop

- `kbd:@40044` — Play / Stop. Press it again to stop; the edit cursor returns to where playback started, so repeated takes audition the same spot.
- `kbd:@40073` — Play / Pause. Halts playback and keeps the play cursor where it is, when you don't want to lose your place.
- `kbd:@40317` — Play, skipping the time selection. Perfect for checking an edit by hearing straight through the splice.

```gif
transport-play-stop
**Play / stop** is the key you'll press most.

- `kbd:@40044` toggles playback — hit it again to stop.
- Stopping returns the edit cursor to where playback started, so repeated takes audition the same spot.
- Need to keep your place instead? `kbd:@40073` pauses without snapping back.
```

## Getting around

- `kbd:@40042` — Go to the start of the project.
- `kbd:@40172` — Go to the previous marker.
- `kbd:@40173` — Go to the next marker. Both work while the transport runs.

```gif
transport-markers
**Marker hopping** keeps you moving without the mouse.

- `kbd:@40172` / `kbd:@40173` jump to the previous / next marker.
- The keycaps carry `<` and `>` — the arrows point the way.
- Both work while the transport is rolling.
```

## Rolling

- `kbd:@1013` — Toggle recording. Combined with `kbd:@40044`-to-stop, a take is just record, perform, stop.

For deeper tracking workflows — take ranking, monitoring, pre-roll — switch to Record mode and read [[Recording]]. Track setup lives in [[Tracks]].
