---
name: blitz-design
description: "Write and refactor Dioxus UI that renders correctly in Blitz (the Vello/wgpu renderer behind the Signal desktop app, plugins and rig surfaces) — which CSS, HTML elements and events Blitz supports, what to write instead of the unsupported ones, and the layout rules that keep a surface inside its window. Use when building or fixing any signal UI (rig control surfaces, panels, footswitch grids, the app chrome), when something renders wrong, overflows, gets cut off or does not scroll, or before reaching for a CSS feature you have not seen used in this repo."
---

# Designing for Blitz

The Signal app does not run in a browser. `dioxus_native` hands the DOM to
**Blitz**: Stylo for style, **Taffy** for layout, Parley for text, **Vello**
on wgpu (Metal on macOS) for paint. Most of CSS works; a specific list does
not, and it fails *silently* — the property is dropped and the element lays
out as if you never wrote it. When a surface looks wrong, first check the
property against the tables below.

Source of truth: <https://blitz.is/status/css>, `/status/elements`,
`/status/events` (snapshot below taken 2026-09-21). Re-check when bumping the
blitz rev in the root `Cargo.toml`.

## The rules that bite, and what to write instead

| don't | why | write instead |
|---|---|---|
| `position: fixed` / `sticky` / `static` | not supported; **absolute is always relative to the immediate parent** | overlays: mount at the root, `position: absolute; inset: 0` there. Give every absolute child a `position: relative` parent you chose on purpose. |
| `overflow: auto` (`overflow-y-auto`) | not supported — dropped | `overflow: scroll` **is** supported (scrollbars + wheel + `scroll` event). Use `overflow-y: scroll` on a pane that must scroll, `overflow: hidden` on one that must clip. |
| `text-overflow: ellipsis`, Tailwind `truncate`, `line-clamp` | not supported | `overflow: hidden; white-space: nowrap` clips; shorten the label in Rust if an ellipsis matters. |
| `vertical-align` | not supported | flex: `display: flex; align-items: center`. |
| SVG `fill` / `stroke` / `stroke-width` in CSS | SVG styling not supported | SVG **presentation attributes**: `fill: "none"`, `stroke: "#a1a1aa"`, `stroke_width: "2"` on the element. |
| an icon sized only by `width`/`height` attributes | a stylesheet rule (Tailwind preflight's `svg { height: auto }`) beats attributes and collapses it | size it in `style` too (`fts_chrome::Glyph` does). |
| `text-shadow`, `mix-blend-mode`, `background-blend-mode`, `isolation` | not supported | `box-shadow` (supported) or paint it in a custom widget. |
| `filter` beyond `blur`/`drop-shadow`, `backdrop-filter` | Vello supports only blur + drop-shadow; backdrop-filter is Skia-only | pre-darkened colours; a translucent `background-color`. |
| 3D transforms, `perspective`, `backface-visibility` | not supported | 2D `transform`, `translate`, `rotate`, `scale` all work. |
| container queries, `container-type`, `zoom`, `resize` | not supported | size from the parent with flex/grid; pass a size down as a prop when a component must adapt. |
| multi-column (`column-count`) | not supported | grid. |
| `scroll-snap-*`, `overscroll-behavior` | not supported | snap in Rust on `scroll`/`wheel`. |
| `<select>`, `<meter>`, `<progress>`, `<output>`, `<dialog>`, `<video>`, `<audio>`, `<picture>`, `<script>` | not supported | build them from `div`s (see `architect-ui`'s pickers); `<canvas>` for anything drawn. |
| a `<button>` made `flex` expecting its content at the start | Blitz's UA sheet gives buttons `justify-content: center`, which survives an author `display: flex` (a browser would start-align) | the app's global sheet sets `@layer base{:where(button.flex,button.inline-flex){justify-content:flex-start}}` (layered: Tailwind v4 utilities live in `@layer utilities`, and an unlayered rule would beat them all) (apps/desktop `main.rs`); in a crate rendered elsewhere, set `justify-content` on the button or use a `div` row. |
| `onchange` on inputs | `change` event not supported | `oninput`, and commit on `onblur` / Enter (`onkeydown`). |
| drag & drop events (`ondragstart`, `ondrop`…) | not supported | pointer events: `onpointerdown` → track `onpointermove` → `onpointerup` (single primary pointer only). |
| clipboard events, `resize`/`load` window events, Resize/Intersection/Mutation observers | not supported | read sizes from props/state; do not wait on load. |
| `@import` of a stylesheet, `document::Stylesheet { href }`, Tailwind `asset!()` | not loaded reliably in plugin/embedded contexts | inline `style: "..."` or `document::Style { {CSS_STR} }` with `include_str!` (see CLAUDE.md "GUI rendering"). |
| `cursor: url(...)` | custom cursors not supported | keyword cursors (`pointer`, `grab`, `ew-resize`…) work. |
| `text-transform: capitalize` | not supported | `uppercase`/`lowercase` work; capitalise in Rust. |
| `opacity` on a container expecting overflow to show | **opacity clips its node** regardless of `overflow` | put the opacity on the inner element, not the one whose children overhang. |

### Safe to use

`display` block/inline/inline-block/**flex**/**grid**/contents/none ·
`position: relative|absolute` · `z-index` · `box-sizing` · `inset` ·
width/height/min/max (and flow-relative) · `aspect-ratio` · `gap` ·
padding/margin/border (all styles), `border-radius` · `box-shadow` · `outline`
· flexbox (direction, wrap, grow, shrink, basis, order) · grid
(template rows/columns with px/%/fr/min-content/max-content/auto/`fit-content()`,
named lines, areas, auto-flow, auto rows/cols — **no subgrid**) · box
alignment (`align-*`, `justify-*`) · 2D transforms + `transform-origin` ·
**transitions and animations** · `clip-path`, `mask-*` · backgrounds (colour,
image incl. gradients, size, position, repeat, clip) · fonts (`@font-face`,
weight, style, stretch, features, variations) · text (`color`, `text-align`,
`line-height`, `letter-spacing`, decoration, `word-break`, `overflow-wrap`,
`white-space` wrap modes, `text-indent`) · `opacity`, `visibility` ·
`pointer-events`, `user-select` (auto/text; `none` only stops a selection
starting there), `touch-action`, `caret-color` · `scrollbar-width`,
`scrollbar-color`, `scroll-behavior` · `::before`/`::after` with string
`content` · tables (emulated with grid; no `<tfoot>`, `<col>`, `<caption>`).

Events that work: mouse (down/up/click/dblclick/auxclick/contextmenu/move/
enter/leave/over/out), pointer (single primary pointer), touch, `wheel`,
`scroll`, keyboard, `input`, focus/blur/focusin/focusout, form `submit`.

## Layout rules that keep a surface inside its window

The rig surfaces are walls of panels. Almost every "cut off on the right"
bug is one of these:

1. **Flex children default to `min-width: auto`** — a child never shrinks
   below its content, so one long label or fixed-width descendant pushes the
   whole row past the window. Every flex item that should shrink gets
   `min-width: 0` (`min-height: 0` in a column). Same for grid tracks: use
   `minmax(0, 1fr)`, not bare `1fr` (whose minimum is `auto`).
2. **No fixed pixel widths on columns of a surface.** A row of N equal
   footswitches is `display: grid; grid-template-columns: repeat(N, minmax(0, 1fr))`
   or flex items with `flex: 1 1 0; min-width: 0`. Pixel sizes belong on
   leaf controls (a knob's diameter), never on the containers that must
   add up to the window.
3. **Widths must sum to the parent.** Sidebar `px` + content `1fr` is fine;
   sidebar `px` + content `px` is a guess about the window size.
4. **Size from the top down.** The root is `width: 100%; height: 100%;
   display: flex` and each level passes the space on with `flex: 1;
   min-width: 0; min-height: 0`. A level that forgets breaks everything
   below it.
5. **Clip at the edges you mean.** The surface root gets `overflow: hidden`
   so an overflow is visible as a clean cut in review rather than a
   window-sized scroll. A pane that really holds more than fits uses
   `overflow-y: scroll`.
6. **Text in tight cells**: `white-space: nowrap; overflow: hidden` plus
   `min-width: 0` on the cell, and a label short enough for the smallest
   window we support (the laptop: ~1760×1100 logical).
7. **Absolute children need a chosen parent** (rule above): without an
   explicit `position: relative` ancestor at the right level they position
   against whatever their immediate parent happens to be.

## Inline SVG and `currentColor`

Blitz draws an inline `<svg>` by serialising it and handing it to usvg, with
`currentColor` replaced by the element's computed colour. Upstream wrote that
colour in its own colour space — `oklch(...)` under Signal's Tailwind v4
theme — which usvg cannot parse, so class-coloured icons drew **black**. The
FTS blitz fork (`../blitz`, `blitz-dom/src/node/node.rs`) now converts to
legacy sRGB first. If icons go black after a blitz bump, that fix was lost.

## Custom GPU widgets

Visualisers (EQ curve, spectrograms, meters) are Blitz **custom widgets**
painting straight into the Vello scene (`signal_guitar_ui::eq_vello` and
friends; `<canvas>` with the proprietary raw-wgpu-texture API). Give their
host element an explicit size from the layout (`flex: 1; min-width: 0;
min-height: 0` inside a sized parent) — a zero-sized host paints nothing,
and a host sized by its content has no content to size it.

## Checking your work

- **Design mode** — the real UI over a synthesised rig, no audio/MIDI/DSP:
  `open "/Volumes/dev-drive/bin/Signal Rig.app" --args --guitar --design`
  (macOS dev bundle; keeps config on the dev drive) or `just guitar --design`.
- **Headless PNG** — `just guitar-shot OUT W H` renders the rig UI with no
  window, no compositor and no device (`signal-guitar-ui` example
  `rig_shot`). Render at the **smallest** supported size (1760×1100) as well
  as the desk size (2560×1440): an overflow only shows at the small one.
- **The window must fit the screen.** On macOS a window larger than its
  display renders every frame but never presents one — the app looks frozen
  on its first frame. It opens maximized on macOS by
  default; `FTS_WINDOW_SIZE` / the `window-size` pref override that (see
  `window_placement` in `apps/desktop/src/main.rs`).
- A property that "does nothing" is almost always in the unsupported table.
  Grep this skill before debugging the layout engine.
