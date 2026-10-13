---
name: reaper-toolbars
description: "Use when managing REAPER toolbars or toolbar icons — adding/updating/removing/reordering toolbar buttons, assigning icons to FTS actions, generating icon strips from Iconify or text, inspecting live or saved toolbar layouts, or keeping a toolbar layout as versioned config. Covers the Toolbar RPC service (daw-proto/daw-control/daw-reaper), the `daw toolbar*` CLI verbs, offline reaper-menu.ini parsing/patching, and the fts-icons generator."
---

# REAPER toolbars + icons

Two halves, one workflow:

- **Buttons** — the `Toolbar` service. Live, over RPC into a running REAPER.
- **Icons** — `fts-icons`. Renders the PNG strips REAPER wants, and can also
  assign them offline by patching `reaper-menu.ini`.

Nothing here needs REAPER's UI. Don't tell the user to "right-click the toolbar
→ Customize" unless the live and offline paths are both unavailable.

## Where everything lives

| Piece | Path |
|---|---|
| Service trait + types | `crates/daw/proto/src/toolbar/{service,types}.rs` |
| Client handle | `crates/daw/control/src/toolbar.rs` |
| REAPER implementation | `features/reaper/daw-reaper/src/toolbar.rs` |
| CLI verbs | `crates/daw/daw/src/cli/{command,ops}.rs` (`Toolbar*`) |
| Offline ini **reader** | `features/dawfile/dawfile-reaper/src/toolbar_config.rs` |
| Offline ini **writer** | `features/reaper/fts-icons/src/menu.rs` (icon assignment only) |
| Icon generator | `features/reaper/fts-icons/` (lib `fts_icons` + bin `fts-icons`) |

## Format primer (get this wrong and REAPER silently shows nothing)

One toolbar icon = **one PNG, three cells side by side**: normal / hover /
clicked. 30×30 per cell at 100%, so 90×30. Three DPI variants, **same filename,
no suffix**:

| scale | strip size | path under the resource dir |
|---|---|---|
| 100% | 90×30 | `Data/toolbar_icons/<name>.png` |
| 150% | 135×45 | `Data/toolbar_icons/150/<name>.png` |
| 200% | 180×60 | `Data/toolbar_icons/200/<name>.png` |

Cell width may exceed 30 (60 = double-wide text button); height is always 30 at
100%. `fts-icons` handles all of this — never hand-render a strip.

Toolbar targets: `Main`, `Floating(1..=32)`, `Midi(1..=8)`. The CLI accepts
`main`, `floating toolbar 3` / `floating-toolbar-3`, `midi toolbar 2`.

## Managing buttons (live)

`daw toolbar …` (or `fts daw toolbar …`) against a running REAPER with the FTS
extension loaded. Read before you write:

```sh
daw toolbar                                  # availability + tracked buttons
daw toolbar-live                             # every non-empty live toolbar
daw toolbar-live --target "floating toolbar 1"
daw toolbar-config ~/.fts-dev/reaper-menu.ini   # saved layout, REAPER not required
```

Then mutate:

```sh
daw toolbar-add _FTS_TEMPO_INSERT_TIMESIG_6_8 "6/8" \
    --target "floating toolbar 1" --workflow fts-tempo \
    --icon fts_timesig_6_8.png --position 4
daw toolbar-update <cmd> "New label" --icon other.png
daw toolbar-move   <cmd> 2 --target main
daw toolbar-icon   <cmd> --icon fts_automation.png     # or --clear
daw toolbar-remove <cmd> --target main
```

Rules that come from the implementation, not from taste:

- **Command names, not ids.** Use `_FTS_…` named commands. `resolve_command_id`
  maps them; numeric ids drift between REAPER installs.
- **Always pass `--workflow`.** `add_button`/`update_button` take a
  `workflow_id`, and `remove_workflow_buttons` is the only clean way to undo a
  batch. A batch added without a shared workflow id has to be removed
  one-by-one.
- **`add_button` is idempotent** — it returns the existing command id rather
  than duplicating. Prefer re-running a build over hand-diffing.
- **Icon by file name** (`ToolbarIconKind::FileName`, the CLI default) means
  REAPER resolves it out of `Data/toolbar_icons` — install the strip *first*,
  then assign. `Path` takes an absolute filesystem path and skips that lookup.
- Operations run on the main thread via `main_thread::query`, so a call that
  returns `ok: false` carries a real REAPER-side `error` — surface it, don't
  retry blindly.

From Rust, the same surface is `daw_control::Daw::toolbar()` (async:
`add_button`, `update_button`, `remove_button`, `move_button`,
`set_button_icon`, `remove_workflow_buttons`, `get_tracked_buttons`,
`get_live_toolbar_json`). In-extension code implements/calls the
`#[architect::rpc] trait Toolbar` directly.

## Generating icons

`icons.toml` is the unit of config — treat one file as one toolbar and check it
into the repo. Config-management is the point: regenerate, don't hand-edit PNGs.

```sh
fts-icons search compass              # find an Iconify id
fts-icons paths                       # detected resource paths (reaper.ini probe)
fts-icons init                        # example icons.toml
fts-icons build icons.toml --install  # render + install + assign
fts-icons build icons.toml --out ./preview   # dry-ish run: no install, no assign
```

```toml
[settings]
resource_paths = ["~/.fts-dev"]   # else auto-detect ~/.fts-dev, ~/.config/REAPER
# width = 60                      # default cell width for this file

[defaults.all]
icon_size = 27
corner_radius = 6
[defaults.normal]
icon = "#e6e6e6"
[defaults.hover]
icon = "#ffd75e"
[defaults.clicked]
icon = "#ffffff"
bg = "#2e7d32aa"
border = "#69f0ae"

[[icon]]
file = "fts_timesig_6_8"                        # → fts_timesig_6_8.png
source = "text:6/8"                             # stacked digits
assign = "_FTS_TEMPO_INSERT_TIMESIG_6_8"        # applied on --install
  [icon.hover]
  icon = "#00e676"
```

Sources: `prefix:name` (Iconify, cached in `~/.cache/fts-icons/`), `text:ABC`,
`text:6/8` (stacked), `text:+ MULTI-/MIC` (leading plus glyph), and
`a + b` with literal spaces for a side-by-side composite (each part needs its
own prefix).

Style layering, later wins: builtin → `defaults.all` → `defaults.normal` →
`defaults.<state>` → `icon.all` → `icon.normal` → `icon.<state>`. So an
unspecified hover/clicked **inherits the normal look** — only override what
actually differs. Colors are `#rgb`/`#rrggbb`/`#rrggbbaa`, or `"none"` to clear
an inherited value. Sizes (`icon_size`, `bg_size`, `border_width`,
`corner_radius`) are px at 100% and scale automatically.

Worked examples live in `features/reaper/fts-icons/examples/` —
`timesigs.toml` is the fullest one (a whole toolbar, square stacked-digit
cells); `tracks.toml` colors per-instrument glyphs; `mix.toml` and
`record1.toml` show `width = 60` wide text buttons (record1 mixes both widths).

## Assigning: live vs offline

Two paths, pick deliberately:

- **`assign =` in icons.toml** (`--install` only) patches `reaper-menu.ini`:
  matches buttons by the command in `item_N`, sets/inserts `icon_N`, backs the
  file up as `reaper-menu.ini.fts-icons.bak`. Matching by command means the
  assignment survives reordering. **Requires a REAPER restart / menu-set
  reload**, and it will be clobbered if REAPER is running and later rewrites
  the ini — so do this with REAPER closed.
- **`daw toolbar-icon` / `toolbar-add --icon`** assigns in a *running* REAPER,
  no restart. From Rust with `fts-icons`'s `toolbar` feature,
  `BuiltIcon::toolbar_icon()` hands you the `daw_proto::ToolbarIcon` directly.

Typical config-managed loop:

1. Edit `icons.toml`; `fts-icons build icons.toml --out ./preview` and look at
   the PNGs (they're strips — left cell normal, right cell clicked).
2. `fts-icons build icons.toml --install` to write into the resource path(s).
3. Assign live via `daw toolbar-*` if REAPER is up, else rely on `assign =`
   plus a restart.
4. `daw toolbar-live --target …` to confirm what actually landed.

## Gotchas

- Installing an icon does **not** put a button on a toolbar, and adding a
  button does **not** create an icon. Two steps, always.
- A resource path is a directory containing `reaper.ini`. `~/.fts-dev` is the
  dev instance; `~/.config/REAPER` the normal one. Auto-detect picks up both —
  pass `--resource-path` when you mean only one.
- `fts-icons` reaches the network for Iconify ids (cached after first fetch);
  `text:` sources are fully offline and need system fonts (DejaVu Sans).
- The 150/200 variants are separate files with the *same* name. A stray
  `name_150.png` is REAPER-invisible.
- Never edit `reaper-menu.ini` by hand while REAPER is open.
