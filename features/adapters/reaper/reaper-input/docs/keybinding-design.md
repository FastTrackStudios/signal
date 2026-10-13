# FTS Keybinding Design

Modifier key philosophy and assignment rules for the FTS input system.

## Mouse Modifier Hierarchy

| Priority | Modifier | Semantic Role | Example |
|----------|----------|---------------|---------|
| 1 | None | Primary action | Move edge, Edit loop point |
| 2 | **Shift** | Ignore snap (or extended variant) | Move edge ignoring snap |
| 3 | **Alt/Option** | Alternate action | Copy item (instead of move) |
| 4 | **Alt+Shift** | Alternate action ignoring snap **OR** fallback when Shift is occupied | Copy ignoring snap |
| 5 | **Cmd/Ctrl** | Add-to / secondary behavior | Add to razor edit area |
| 6 | **Shift+Cmd** | Secondary ignoring snap | Add to razor ignoring snap |
| 7 | **Cmd+Alt** | Special / rare combos | Add stretch marker |

## Core Rules

1. **Shift = ignore snap** -- This is the primary "freedom from grid" modifier across all contexts.
2. **When Shift is already occupied** (e.g., ruler: Shift = MoveLoopPoints), the ignore-snap variant moves to **Shift+Alt**.
3. **No duplicate actions** -- The same action must never appear under two different modifier branches.
4. **Alt = alternate action** -- A fundamentally different operation (copy vs move, zoom vs scroll).
5. **Cmd = add/toggle/secondary** -- Additive behavior or secondary system commands.
6. **NoAction over confusion** -- Unused modifier slots should be explicitly NoAction rather than guessed.

## Keyboard Modifier Conventions

| Modifier | Semantic Role | Examples |
|----------|---------------|---------|
| None (bare key) | Primary vim-style command | `h/j/k/l`, `d`, `s`, `space` |
| Shift | Extended/edit variant | `Shift+n` = duplicate, `Shift+m` = insert+edit marker |
| Ctrl | Big jump / alternate behavior | `Ctrl+h` = prev item, `Ctrl+space` = play/pause |
| Alt/Option | Skip constraints | `Alt+space` = play skip time selection |
| Cmd | System/platform commands | `Cmd+,` = preferences, `Cmd+1-4` = window sets |
| Prefix (which-key) | Complex feature trees | `v` = visibility, `f` = FX, `a` = automation |

## Which-Key Rule

Complex features use **prefix trees** (bare key sequences), not modifier combos. Modifiers are reserved for immediate single-action bindings.

## Reference: Current Mouse Modifier Assignments

### Media Item Edge (`MM_CTX_ITEMEDGE`)

| Modifier | Action |
|----------|--------|
| Default | MoveEdge |
| Shift | MoveEdgeIgnoringSnap |

### Media Item Body (`MM_CTX_ITEM`)

| Modifier | Action |
|----------|--------|
| Default | MoveItemIgnoringTimeSelection |
| Shift | MoveItemIgnoringSnapAndTimeSelection |
| Alt | CopyItem |
| Alt+Shift | CopyItemIgnoringSnap |

### Media Item Lower (`MM_CTX_ITEMLOWER`)

| Modifier | Action |
|----------|--------|
| Default | SelectRazorEditArea |
| Shift | SelectRazorEditAreaIgnoringSnap |
| Cmd | AddToRazorEditArea |
| Shift+Cmd | AddToRazorEditAreaIgnoringSnap |

### Media Item Click (`MM_CTX_ITEM_CLK`)

| Modifier | Action |
|----------|--------|
| Default | SelectItemAndMoveEditCursor |
| Shift | AddRangeOfItemsToSelection / ExtendTimeSelection |
| Cmd | ToggleItemSelection |
| Shift+Cmd | SelectItemAndMoveEditCursorIgnoringSnap |
| Alt | SelectItemIgnoringGrouping |
| Alt+Shift | ExtendRazorEditArea |
| Cmd+Alt | AddStretchMarker |

### Ruler (`MM_CTX_RULER`)

| Modifier | Action |
|----------|--------|
| Default | EditLoopPoint |
| Shift | MoveLoopPoints |
| Alt | NoAction |
| Alt+Shift | MoveLoopPointsIgnoringSnap |
| Cmd | NoAction |
| Shift+Cmd | NoAction |
| Cmd+Alt | NoAction |
| Shift+Cmd+Alt | NoAction |

### Track Left Drag (`MM_CTX_TRACK`)

| Modifier | Action |
|----------|--------|
| Default | MarqueeSelectItems |
| Shift | MarqueeAddToItemSelection |
| Alt | MarqueeZoom |
| Cmd | Draw straight line (25 m) |
| Shift+Cmd | Draw freehand line (27 m) |

### Track Click (`MM_CTX_TRACK_CLK`)

| Modifier | Action |
|----------|--------|
| Default | DeselectAllItems |
| Shift | DeselectAllItemsAndMoveEditCursor |
| Alt+Shift | DeselectAllItemsAndMoveEditCursorIgnoringSnap |
