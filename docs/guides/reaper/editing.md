---
title: Editing
kind: input
type: input
category: editing
---

# Editing

The core edit verbs are single keys — split, delete, duplicate — with the standard clipboard chords underneath for anyone migrating from another DAW. (See [[Input System|the input layer]] for how single keys, chords, and prefixes fit together.)

## Split, delete, duplicate

- `kbd:@40757` — Split the selected items at the cursor.
- `kbd:@40006` — Delete the selected items.
- `kbd:@_FTS_SMART_DUPLICATE` — Smart duplicate: copy the selection forward by its own measure span, group- and color-preserving. Repeated hits build a section on the grid.

```gif
editing-split-duplicate
`s` splits at the cursor; smart-duplicate drops the copy a full measure-span later, so repeats land on the grid.
```

## Glue and clipboard

- `kbd:@42432` — Glue the items within the time selection into one.
- `kbd:@40059` — Cut the selected items.
- `kbd:@40058` — Paste items at the cursor.
- `kbd:@40153` — Open the selected item in the MIDI editor (same key closes it from inside).

## Snapping and the undo stack

- `kbd:@1157` — Toggle snapping.
- `kbd:@40029` — Undo.
- `kbd:@40030` — Redo.

Once an arrangement takes shape, mark its sections in [[markers-regions|Markers & regions]] and comp your takes in [[comping-takes|Comping & takes]].
