---
name: reaper-theme-colors
description: "Use when changing how REAPER looks — editing a .ReaperTheme palette, working out which key paints a given part of the UI, generating a theme from the FTS palette, or chasing a stubborn grey area that theming did not fix. Covers the three separate files REAPER themes from, the COLORREF encoding and its flag byte, every palette key family and what it paints, the non-colour keys that must never be written, and the generate/apply/screenshot loop."
---

# REAPER theme colours

## Three files, not one

The single most expensive misunderstanding: **a REAPER theme is three
independent things**, and a "finished" theme that still has grey patches
is almost always missing one of them.

| what you see | comes from | reachable from our palette |
|---|---|---|
| arrange, track panels, meters, MIDI editor, lists | `.ReaperTheme` `[color theme]` — ~400 int keys | **yes** — `Theme::reaper_palette()` |
| toolbar art, mixer strip chrome, buttons, fader caps | PNGs in the image folder + WALTER in `rtconfig.txt` | only by recolouring the art |
| **menu bar, every dialog, combo boxes, list views, scrollbars** | **`libSwell.colortheme`** in the *resource* dir | **yes** — `Theme::swell_colortheme()` |

That third row is the one nobody finds. On Linux, SWELL (REAPER's Win32
compatibility layer) draws the menu bar and all dialogs from a completely
separate file that ships **light** (`_3dface #B3B3B3`). It is not part of
the theme, is not mentioned by the theme, and is not shipped with it — so
a fully dark `.ReaperTheme` still has a grey bar across the top and grey
dialogs everywhere, and no amount of palette work will fix it.

```sh
fts-themer apply --swell ~/fts-dev     # writes libSwell.colortheme there
```

Restart REAPER after writing it; unlike the palette it is not reloadable.

## How a colour is stored

`.ReaperTheme` is an ini. `[color theme]` values are **signed decimal
`COLORREF`s**, `0x00BBGGRR` — *red in the low byte*, not RGB order:

```
col_arrangebg=4342338     →  0x424242  →  #424242
0x00102030                →  b=0x10 g=0x20 r=0x30  →  #302010
```

**The top byte is not spare.** REAPER packs flags there, and a key it
considers unset reads back as a large negative number
(`col_main_bg=-2144193998`). Rewriting the whole word flips a key from
"unset" to "set", changing behaviour rather than just colour — so a writer
must preserve the high byte. `fts_themer::ThemeIni::set_color` does; a
naive `format!("{key}={int}")` does not.

## Keys that are not colours

Some ints in `[color theme]` are blend words or flags. Writing a colour
over one silently changes how a layer composites — the symptom is a
region that renders at the wrong opacity or inverts, with nothing obviously
wrong in the file.

- `*_drawmode`, `*dm` — blend mode + alpha. Low byte is the mode
  (0 normal, 1 additive, 2 dodge, 3 multiply, 4 overlay, 254 HSV), alpha is
  `(((n >> 8) & 0x3ff) - 0x200) / 256`.
- `*_mode`, `*_flags` — flag words.
- `activetake_tag`, `autogroup`, `selitem_tag`, `peaksedges`,
  `col_nodarkmodemiscwnd` — plain ints with non-colour meanings.

`fts_themer::groups::is_color()` knows these, and the export has a test
asserting it never emits one.

## What each family paints

Roughly 400 keys, but they cluster. **Anything not listed here that stays
grey is probably in the PNG art or SWELL, not the palette.**

### Window chrome

- `col_main_bg` — the window behind docked panes. **Forgetting this is the
  classic cause of grey gutters** around an otherwise dark theme.
- `col_main_bg2` — the app surface (what `reaper_import` reads as "surface").
- `col_main_text` / `_text2` — primary / secondary text.
- `col_main_3dsh` / `_3dhl` — border shadow / highlight.
- `col_main_editbk` — entry-field background.
- `col_main_resize` / `_resize2` — splitter bars between panes.
- `col_toolbar_frame`, `col_toolbar_text`, `col_toolbar_text_on` — toolbar
  frame and text (the *icons* are PNGs).
- `docker_bg`, `docker_selface`, `docker_unselface`, `docker_text`,
  `docker_text_sel`, `docker_shadow` — the tab strip around every docked
  pane. Large, and usually forgotten.

### Arrange and ruler

- `col_arrangebg` — arrange background.
- `col_tracklistbg`, `col_mixerbg` — empty area below the TCP / MCP.
- `col_gridlines`, `col_gridlines2`, `col_gridlines3` — beat, subdivision,
  bar lines. Each has a `*dm` sibling that is a blend word.
- `col_cursor` (edit cursor — REAPER's de-facto accent), `col_cursor2`.
- `col_tl_bg`, `col_tl_bgsel`, `col_tl_fg`, `col_tl_fg2` — ruler.
- `marker`, `marker_edge`, `marker_lane_bg`, `marker_lane_text`;
  `region`, `region_lane_bg`, `region_lane_text`.
- `marquee_*`, `marqueezoom_*`, `areasel_*`, `linkedlane_*` — selection
  rectangles: each has `fill`, `outline`, and a blend word.

### Track and mixer panels

- `col_tr1_bg` / `col_tr2_bg` — alternating track-strip rows. **If these are
  equal the track list reads as one flat slab.**
- `col_tr1_divline` / `col_tr2_divline`, `col_tr1_peaks` / `col_tr2_peaks`.
- `col_seltrack`, `col_seltrack2` — selected track in TCP / MCP.
- `mcp_fx_normal` / `_bypassed` / `_offlined`, `mcp_fxparm_*` — mixer FX
  list text states.
- `mcp_sends_normal` / `_muted` / `_levels`, `mcp_send_midihw`.
- `mcp_list_scrollbar*`, `tcp_list_scrollbar*`.
- `io_text`, `io_3dhl`, `io_3dsh` — routing dialogs.

Most *visible* mixer chrome — the strip background, fader caps, buttons —
is PNG art, not palette. See the fader-accent note below.

### Media items

- `col_mi_bg` / `col_mi_bg2` — item background, alternating.
- `col_mi_label`, `col_mi_label_sel`, `col_mi_label_float*` — take names.
- `col_mi_fades`, `col_mi_fade2`; `col_fadearm*` — fade handles.
- `col_peaksedge*`, `col_peaksfade*` — waveform edges.
- `col_stretchmarker*` — stretch markers (a handle family: `_h0`–`_h2` are
  hover states).

### Meters

- `col_vubot` / `col_vumid` / `col_vutop` / `col_vuclip` — the ramp,
  bottom to clip. `reaper_import` reads these as safe/warn/danger.
- `col_vuintcol` — meter background.

### Envelopes

- `col_env1`–`col_env16` — the sixteen envelope lane colours.
- `env_item_vol`, `env_item_pan`, `env_item_mute`, `env_item_pitch`,
  `env_trim_vol`, `env_track_mute`, `env_sends_mute` — per-type envelopes.
  Give these *meanings* (volume = peak colour, mute = mute colour) rather
  than arbitrary hues.
- `col_envlane1_divline` / `2`.

### MIDI editor

The one place REAPER and our expression editor draw the same thing, so
these must match the editor's palette exactly or the two surfaces disagree.

- `midi_trackbg1` / `2` — white-key and black-key rows.
- `midi_trackbg_outer1` / `2` — rows outside the active take.
- `midi_pkey1` / `2` / `3` — piano keys.
- `midi_grid1` / `2` / `3`, `midi_griddi`, `midi_gridhc`.
- `midi_editcurs`, `midi_selbg`, `midi_selpitch1` / `2`.
- `midifont_col_light` / `_dark` (+ `_unsel`) — note text, picked by
  contrast against the note.
- `midieditorlist_*` — the MIDI editor's track list (11 keys).

### Lists, notation, wiring

- `genlist_*` — every generic list: media explorer, FX browser, managers.
  Nine keys, large surfaces, routinely forgotten.
- `score_*` — notation editor.
- `wiring_*` — the routing/wiring view, 25 keys.
- `group_0`–`group_31` — track-group tints. Only need to be mutually
  distinguishable.

## Generating a theme

Don't author 400 colours — that's data entry, and it guarantees the parts
nobody thought about drift grey. Author ~20 and derive:

```
daw_theme::defaults  (the only literals)
   │
   ├── Theme::reaper_palette()   → ~200 palette keys
   ├── Theme::swell_colortheme() → menu bar + dialogs
   └── expression-editor theme.rs
```

```sh
fts-themer apply --dry-run          # what would change
fts-themer apply --swell ~/fts-dev  # palette + SWELL
just reaper theme-contact           # photograph both surfaces
```

`apply` is a **merge**, not a replacement: only keys the palette determines
are written, so hand-tuned values for things nobody has modelled survive.

## Checking the result

Colours are not reviewable by reading hex. Take the picture:

```sh
nix develop .#reaper-test -c just reaper theme-contact
```

Cheap invariants worth encoding as tests, because they catch the errors
that are invisible in a diff and obvious in a screenshot:

- every surface key is dark in a dark theme (catches one forgotten key
  showing as a bright gutter);
- alternating rows differ (`col_tr1_bg` ≠ `col_tr2_bg`);
- surface steps form an ordered ladder, so "sunken" and "raised" mean
  something;
- series stay mutually distinct (pitch classes, groups, tool zones);
- text has contrast against the surface it sits on.

## Traps

- **`vst_noscan=1`, not blank paths.** REAPER treats an empty `vstpath` as
  "unset" and scans its defaults anyway. A *warm* profile skips the scan
  regardless, so blanking the paths looks like it works until you use a
  fresh profile and sit through a full scan behind a modal dialog.
- **No "reload theme" action exists.** REAPER 7 ships only the element
  finder and the tweak window. The `OpenColorThemeFile` API re-reads the
  theme, and re-opening the *active* theme is the reload —
  `daw theme-reload`.
- **`libSwell.colortheme` needs a restart**, unlike the palette.
- **A licence file makes screenshots clean.** A fresh profile shows "Still
  Evaluating" over the arrange view; copy `reaper-license.rk` in. Copy,
  never symlink — REAPER rewrites it.
- **The fader-accent colour is images, not palette.** The mixer thumb
  colour comes from `<accent>/mcp_volthumb.png` plus `A_Fader_*` WALTER
  layouts; `fts-themer add-accent` generates both.
- **WALTER marker pixels are geometry.** Magenta `#FF00FF` and yellow
  `#FFFF00` in theme art are nine-slice guides. Recolouring them breaks how
  the image stretches, and being fully saturated they also win any
  "dominant colour" heuristic.
