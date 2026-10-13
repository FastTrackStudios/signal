# fts-icons

Toolbar-icon generator for REAPER — library + CLI. Searches [Iconify](https://iconify.design) (200k+ icons), renders REAPER's 3-state strip (normal / hover / clicked), and installs all three DPI variants straight into a resource path.

It is the icon half of REAPER toolbar management; the button half is the
`Toolbar` service (`daw-proto` → `daw-control` → `daw-reaper`, plus the
`daw toolbar*` CLI verbs). See `.claude/skills/reaper-toolbars/SKILL.md` for the
combined workflow.

## Format primer

A REAPER toolbar icon is one PNG with 3 cells side by side, 30×30 per cell at 100%:

| scale | size | location |
|-------|---------|----------|
| 100% | 90×30 | `Data/toolbar_icons/<name>.png` |
| 150% | 135×45 | `Data/toolbar_icons/150/<name>.png` |
| 200% | 180×60 | `Data/toolbar_icons/200/<name>.png` |

Same filename in all three places — **no** `_150`/`_200` suffix. `fts-icons` handles all of this.

## Usage

```sh
fts-icons search arrow              # list matching iconify ids
fts-icons paths                     # detected REAPER resource paths

# one-off icon, installed into every detected resource path
fts-icons make mdi:eye-outline --file fts_automation \
    --icon '#e6e6e6' --hover-icon '#ffd75e' \
    --clicked-bg '#2e7d32aa' --clicked-border '#69f0ae' \
    --install

# declarative icon set (the real workflow)
fts-icons init                      # writes example icons.toml
fts-icons build icons.toml --install
```

Then in REAPER: right-click toolbar → Customize toolbar → double-click a button → pick the icon.

## Config

```toml
[settings]
# resource_paths = ["~/.fts-dev"]   # else auto-detected (~/.fts-dev, ~/.config/REAPER)

[defaults.normal]
icon = "#e6e6e6"

[defaults.hover]
icon = "#ffd75e"

[defaults.clicked]
icon = "#ffffff"
bg = "#2e7d32aa"
border = "#69f0ae"

[[icon]]
file = "fts_automation"        # output filename
source = "mdi:eye-outline"     # iconify id
assign = "40252"               # optional: point a toolbar button at this icon
  [icon.hover]                 # optional per-state override
  icon = "#00e676"

[[icon]]
file = "fts_timesig_6_8"
source = "text:6/8"            # generated text — `num/den` renders stacked
assign = "_FTS_TEMPO_INSERT_TIMESIG_6_8"
```

- Colors: `#rgb` / `#rrggbb` / `#rrggbbaa` (alpha), `"none"` clears an inherited value.
- Per-state fields: `icon`, `bg`, `border`, `border_width`, `icon_size`, `bg_size`, `corner_radius`. Sizes are px at 100% and scale automatically.
- `width` (per icon, or `[settings]` default): cell width at 100%. 30 = square, 60 = double-wide text button. REAPER accepts any width; height stays 30.
- Layering (later wins): builtin → `defaults.all` → `defaults.normal` → `defaults.<state>` → `icon.all` → `icon.normal` → `icon.<state>`. Unspecified hover/clicked inherit the normal look; per-icon settings beat global defaults.

Fetched SVGs are cached under `~/.cache/fts-icons/`.

### Sources

- `prefix:name` — Iconify icon
- `text:6/8` — generated stacked digits (time-signature style)
- `text:ABC` — generated centered text
- `text:+ MULTI-/MIC` — leading `+ ` renders a plus at vertical center beside the (stackable) label
- `a + b` (spaces required) — composite: parts side by side, each prefixed (`mdi:chevron-double-up + mdi:map-marker`)

### Toolbar wiring (`assign`)

`assign = "<command id>"` patches `reaper-menu.ini` on `--install`: finds every
toolbar button whose `item_N` runs that command (numeric or `_NAMED`) and sets
its `icon_N` — matching by command survives button reordering. A backup is
written next to the ini. Restart REAPER to pick it up; see
`examples/timesigs.toml` for a full toolbar.

Assignment happens offline, on the ini. To assign in a **running** REAPER
instead, build with the `toolbar` feature and hand `BuiltIcon::toolbar_icon()`
(a `daw_proto::ToolbarIcon` resolved by file name) to the `Toolbar` service's
`set_button_icon` / `add_button` — no restart needed.

## Build

```sh
cargo build -p fts-icons --release          # → target/release/fts-icons
cargo build -p fts-icons --features toolbar # + daw-proto ToolbarIcon interop
```

Library entry point: `fts_icons::build(defaults, icons, settings, &Output)` →
`Report { icons: Vec<BuiltIcon>, assignments, .. }`. The CLI is a thin shell
over it.
