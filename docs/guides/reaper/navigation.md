---
title: Navigation
kind: input
type: input
category: navigation
---

# Navigation

Moving around a session should never take your hands off the home row. The navigation layer maps cursor and track movement onto vim keys, with the arrow keys doing the same jobs for anyone who prefers them. (New to prefix menus and layers? Read [[Input System|the input layer]] first.)

## Move the edit cursor

- `kbd:@40838` — Move the edit cursor to the current / previous measure.
- `kbd:@40837` — Move the edit cursor to the next measure.
- `kbd:@40646` — Nudge the cursor left to the previous grid division.
- `kbd:@40647` — Nudge the cursor right to the next grid division.

```gif
navigation-cursor-move
`h` / `l` step the edit cursor by measure; `<C-h>` / `<C-l>` step by grid division.
```

## Move track selection

- `kbd:@40285` — Select the next track down.
- `kbd:@40286` — Select the previous track up.
- `kbd:@40421` — Extend the track selection to the next track.
- `kbd:@40420` — Extend the track selection to the previous track.

## Jump

- `kbd:@40042` — Go to the start of the project.
- `kbd:@40157` — Insert a marker at the cursor (then hop between them from [[Transport]]).

Vim muscle memory carries over: `h` `j` `k` `l` are left / down / up / right, Shift extends a selection, and Ctrl takes bigger steps. Regions and section jumps live in [[markers-regions|Markers & regions]].
