---
title: Recording
kind: input
type: input
mode: mode-record
---

# Recording

Record mode turns the number row into a take-ranking pad for comping while you track. Ranks drop a marker on the take so the comp pass later is just picking the smiley faces.

Activate the Record mode workflow to use these bindings — they layer over the base profile (see [[Input System|modes]]) and step aside when you leave the mode. (Latency while monitoring? That's [[audio-setup|Audio Setup]], not recording.)

## Getting a take down

The whole loop is four steps:

1. **Arm** the track — `kbd:@9` toggles record-arm on the selection.
2. **Monitor** so the performer hears themselves — `kbd:@_FTS_SESSION_MONITOR_TOGGLE_ON_OFF` (keep the buffer small; see [[audio-setup|Audio Setup]]).
3. **Roll** — `kbd:@1013` starts recording; play, then `kbd:@40044` to stop.
4. **Keep or retry** — happy? Move on. Not? `kbd:@_FTS_SESSION_RECORD_RESTART` deletes the bad take and rolls again in one press.

To punch in and out at exact points, make a time selection over the phrase first — recording only writes inside it (auto-punch).

## Pre-roll and count-in

Give the player a lead-in so they're in the pocket by bar one:

- `kbd:@41819` — Toggle pre-roll on record (REAPER rolls a bit of lead-in before the punch, then records).
- `kbd:@41818` — Toggle pre-roll on play, to audition with the same lead-in.
- `kbd:@40363` — Open the metronome / pre-roll settings to set the lead-in length and click.
- `kbd:@40364` — Toggle the metronome.

For a bars-and-beats count-in the band can see, drop a count-in region: `kbd:@_FTS_SESSION_INSERT_COUNT_IN_REGION` (part of the [[markers-regions|section regions]]).

```gif right
recording-preroll
**Lead-in, not a cold start.**

- `kbd:@41819` gives a pre-roll before every record pass.
- Set its length in the metronome / pre-roll settings (`kbd:@40363`).
- A count-in region shows the band the bars before the downbeat.
```

## Rank takes as they happen

While a take plays back, tap a number to rank it — the marker lands two seconds behind the play position, right where the phrase you just heard lives:

- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_1` — Rank :) at the play position.
- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_2` — Rank :)) at the play position.
- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_3` — Rank :))) at the play position.
- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_DOWN` — Down-rank at the play position.

```gif
recording-take-ranking
**Rank as you listen.**

- While a take plays, tap a number to rank the moment you just heard.
- The marker lands ~two seconds behind the play cursor, on the phrase itself.
- Comping later is just picking the smiley faces — no scrubbing to find the keeper.
```

Hold Shift to rank the whole take instead of a moment — the marker sits at the item start:

- `kbd:@_FTS_SESSION_TAKE_RANK_ITEM_1` — Rank :) item-wide (likewise `kbd:<S-2>`, `kbd:<S-3>`, `kbd:<S-0>`).

Point, don't select — rank the take under the mouse cursor:

- `kbd:@_FTS_SESSION_TAKE_RANK_MOUSE_1` — Favorite the take at the mouse.
- `kbd:@_FTS_SESSION_TAKE_RANK_MOUSE_DOWN` — Down-rank the take at the mouse.

## Tracking controls

- `kbd:@1013` — Record (the base transport binding, unchanged).
- `kbd:@9` — Toggle record-arm on the selected tracks.
- `kbd:@_FTS_SESSION_MONITOR_TOGGLE_ON_OFF` — Toggle input monitoring on/off.
- `kbd:@_FTS_SESSION_MONITOR_TOGGLE_TAPE_OFF` — Switch monitoring auto/tape and off.
- `kbd:@41819` — Toggle pre-roll on record.
- `kbd:@_FTS_SESSION_RECORD_RESTART` — Restart recording: delete the bad take and roll again in one press.

The base [[Transport]] keys keep working underneath — `kbd:@40044` still stops, `kbd:@40172` and `kbd:@40173` still hop markers.
