---
title: Modes
kind: concept
type: concept
---

# Modes

A mode is a workflow state — one production phase at a time. Activating a mode layers its keybind overlays over the base profile: new keys appear, and where a mode rebinds a key the base already uses, the mode wins until you leave. Some modes also flip REAPER settings while active (snapping, pre-roll) and restore them on exit. See [[Input System|the input layer]] for how overlays stack.

## The production flow

The modes are numbered in the order a song moves through production:

- `kbd:@_FTS_SESSION_MODE_ORGANIZE` — Organize.
- `kbd:@_FTS_SESSION_MODE_WRITE` — Write.
- `kbd:@_FTS_SESSION_MODE_PRODUCE` — Produce.
- `kbd:@_FTS_SESSION_MODE_RECORD` — Record (its take-ranking layer is [[Recording]]).
- `kbd:@_FTS_SESSION_MODE_EDIT` — Edit.
- `kbd:@_FTS_SESSION_MODE_MIX` — Mix (see [[mixing|Mixing]]).
- `kbd:@_FTS_SESSION_MODE_MASTER` — Master.

```gif
modes-switch
`<A-m>` opens the mode menu; hold Alt and tap a letter to jump straight to that workflow.
```

## Two ways to switch

- **Direct chord** — `<A-1>` … `<A-9>` (and `<A-0>`) jump straight to a mode, ordered by production phase. Fastest when both hands are free.
- **Letter prefix** — `kbd:<A-m>` opens a menu where each leaf is a mnemonic letter. Keep Alt held and rapid-fire: `Alt+m`, then `r` for Record.

A note marked with a mode chip — like [[Recording]] — needs that mode active. Everything else in this guide runs on the base profile, live in every mode.
