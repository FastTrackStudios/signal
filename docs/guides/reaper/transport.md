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
Space plays; press it again to stop and the edit cursor snaps back to where playback began.
```

## Getting around

- `kbd:@40042` — Go to the start of the project.
- `kbd:@40172` — Go to the previous marker.
- `kbd:@40173` — Go to the next marker. Both work while the transport runs.

```gif
transport-markers
`,` and `.` hop the edit cursor between markers — the keycaps carry < and >, pointing the way.
```

## Rolling

- `kbd:@1013` — Toggle recording. Combined with `kbd:@40044`-to-stop, a take is just record, perform, stop.

For deeper tracking workflows — take ranking, monitoring, pre-roll — switch to Record mode and read [[Recording]]. Track setup lives in [[Tracks]].
