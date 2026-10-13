---
title: Tracking Guitars
kind: concept
type: concept
---

# Tracking guitars

A complete pass: blank session to a comped, organized guitar. Every step links to the topic page behind it — follow those when you want the why; stay here for the what, in order.

## 1 · Set the buffer low

You're about to monitor a live player, so latency matters more than plugin headroom.

- `kbd:@40099` — open audio config and drop the buffer to **64–128 samples**.

Anything higher and the guitarist hears themselves late. Full reasoning: [[audio-setup|Audio Setup]].

## 2 · Create the track (named and routed)

Don't insert a blank track. Use the create-track menu so it arrives colored and bused for guitars:

- `kbd:@_FTS_SESSION_CREATE_NEW_ELECTRIC_GUITAR` — an electric-guitar track in the guitar group.
- Two mics on the cab? `kbd:@_FTS_SESSION_TRACK_MANAGER_ADD_MULTI_MIC` groups them. (See [[project-organization|Project Organization]].)

```gif
guitars-create-track
**Start organized.**

- `kbd:<S-n> g` drops an electric-guitar track — named, colored, routed to the guitar bus.
- No renaming or re-routing later; the mix template will land on it correctly.
```

## 3 · Get a sound and monitor

- `kbd:@9` — arm the track.
- `kbd:@_FTS_SESSION_MONITOR_TOGGLE_ON_OFF` — turn on input monitoring so the player hears the amp sim through their headphones.

Dial the tone now, at performance volume. If it feels late even at a low buffer, switch to the interface's direct monitoring — see the [[faq|FAQ]].

## 4 · Pre-roll, then roll

- `kbd:@41819` — turn on pre-roll so there's a lead-in before the punch (set its length with `kbd:@40363`).
- `kbd:@1013` — record. Play the part, `kbd:@40044` to stop.
- Blew the take? `kbd:@_FTS_SESSION_RECORD_RESTART` — deletes it and rolls again in one press.

Stack several passes of the same section — you'll comp the best bits together next. Full detail: [[Recording]].

## 5 · Rank as you listen

Play the takes back and, in Record mode, tap a number to flag the good moments — the marker lands right on the phrase you just heard:

- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_1` — favorite this moment.
- `kbd:@_FTS_SESSION_TAKE_RANK_PLAYPOS_DOWN` — down-rank a fluff.

```gif right
guitars-rank-takes
**Judge while it plays.**

- Tap `1`–`3` to rank the phrase under the play cursor, `0` to bury a bad one.
- The smiley markers become your comp map — no scrubbing back to find keepers.
```

## 6 · Comp the keepers

Switch takes to fixed lanes and stitch the ranked bits into one performance:

- `kbd:@42430` — toggle fixed item lanes so every take is visible at once.
- `kbd:@42482` — audition lane by lane.
- `kbd:@40131` — crop items to the active take once a section is chosen.
- `kbd:@41378` — move the finished comp to the top lane.

The take-ranking markers tell you where the good phrases are, so this is picking, not hunting. Full detail: [[comping-takes|Comping & takes]].

## 7 · Clean up

- Trim breaths and noise between phrases, crossfade the comp seams (`kbd:@40757` to split, then drag edges) — see [[editing|Editing]].
- Confirm the track is named and in the guitar bus (it already is, if you used step 2).

That's a comped guitar, organized and ready to mix. Same shape works for [[tracking-vocals|vocals]] and most overdubs. Back to [[case-studies|Case Studies]].
