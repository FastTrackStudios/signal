---
title: Modes
kind: concept
type: concept
---

# Modes

A mode is a workflow state — one production phase at a time. Activating a mode layers its keybind overlays over the base profile: new keys appear, and where a mode rebinds a key the base already uses, the mode wins until you leave. Some modes also flip REAPER settings while active (snapping, pre-roll, toolbars) and restore them on exit. See [[Input System|the input layer]] for how overlays stack.

## The modes

They're numbered in the order a song moves through production. Each one re-tools the keyboard for that phase:

- **Organize** — `kbd:@_FTS_SESSION_MODE_ORGANIZE` — planning, song structure, setlists. The `o` prefix becomes the Organize menu.
- **Write** — `kbd:@_FTS_SESSION_MODE_WRITE` — lyric, melody, and idea capture.
- **Produce** — `kbd:@_FTS_SESSION_MODE_PRODUCE` — arrangement, sound design, instrument selection.
- **Record** — `kbd:@_FTS_SESSION_MODE_RECORD` — tracking, takes, monitoring. Turns the number row into a take-ranking pad ([[Recording]]).
- **Edit** — `kbd:@_FTS_SESSION_MODE_EDIT` — comping, timing, cleanup ([[comping-takes|Comping]] and [[editing|Editing]]).
- **Mix** — `kbd:@_FTS_SESSION_MODE_MIX` — mixer focus, processing, automation ([[mixing|Mixing]]).
- **Master** — `kbd:@_FTS_SESSION_MODE_MASTER` — master bus, metering, export prep.
- **Live** — `kbd:@_FTS_SESSION_MODE_LIVE` — performance / setlist playback view.
- **Scoring** — `kbd:@_FTS_SESSION_MODE_SCORING` — multi-agent orchestration layout, toolbars stripped away.
- **Video** — `kbd:@_FTS_SESSION_MODE_VIDEO` — sync to picture / video editing.

```gif
modes-switch
`<A-m>` opens the mode menu; hold Alt and tap a letter to jump straight to that workflow.
```

## Two ways to switch

- **Direct chord** — `<A-1>` … `<A-9>` (and `<A-0>`) jump straight to a mode, ordered by production phase. Fastest when both hands are free.
- **Letter prefix** — `kbd:<A-m>` opens a menu where each leaf is a mnemonic letter. Keep Alt held and rapid-fire: `Alt+m`, then `r` for Record.

## Why modes

The same key can do the right thing in every phase because only one phase is live at a time. `p` might rank a take in Record and open a menu elsewhere — you never run out of keys, and your muscle memory for the base profile is never overwritten, only *layered*. A note marked with a mode chip — like [[Recording]] — needs that mode active; everything else in this guide runs on the base profile, live in every mode.
