# Can Tailwind carry these components under Blitz?

Research for [#126](https://github.com/FastTrackStudios/FastTrackStudio/issues/126),
under the wayfinding map [#124](https://github.com/FastTrackStudios/FastTrackStudio/issues/124).

Every claim below is cited to source in this tree, to the pinned
dependency source in `~/.cargo`, or to the vendored fork checkout. No
build was run — the parent session holds the target-dir lock — and the two
places where that matters are flagged as **verify empirically**.

## The answer in one paragraph

**Yes for the panel — no for the control.** The boundary is not
"web/desktop versus exporter". It is the `<svg>` element itself. Outside
an `<svg>`, all four targets run a real CSS engine (Stylo in Blitz, the
browser on web) and Tailwind works — including *inside REAPER*, which
this repo already ships. Inside an `<svg>`, Blitz stops using Stylo and
hands the serialised subtree to **usvg** — the same engine the exporter
uses — so the SVG interior is governed by identical rules in the app and
in the exporter: presentation *attributes* only, no classes, no CSS
custom properties, no layout. The split the ticket guessed at is the
right one, and it is enforced by the renderer rather than by convention.

## The four targets, and what each one runs

| # | Target | Stack | CSS engine outside `<svg>` | Inside `<svg>` |
|---|--------|-------|---------------------------|----------------|
| 1 | Browser (dx web) | real browser | full CSS | full CSS |
| 2 | Desktop / standalone | `nice-plug-dioxus` → Blitz | Stylo 0.19 + Taffy 0.12 | **usvg 0.46** |
| 3 | VST3/CLAP editor + REAPER panel | `nice-plug-dioxus` → Blitz | Stylo 0.19 + Taffy 0.12 | **usvg 0.46** |
| 4 | Theme exporter | `dioxus-ssr` → resvg 0.45 | *n/a — the root node is `<svg>`* | **usvg 0.45** |

Targets 2 and 3 are the same code path; `nice-plug-dioxus` differs only
in how it acquires the window (`standalone.rs` vs `embedded.rs` /
`window.rs`), and all three construct `DioxusDocument::new(vdom,
DocumentConfig { viewport, ..Default::default() })`.

So there are really **two** styling regimes, not four:
**DOM/Stylo** and **SVG/usvg**. Target 4 lives entirely in the second.

## How the exporter path actually works today

`features/daw-ui/daw-theme-art/src/render.rs` (on `worktree-reaper-theme`;
the crate does not exist on `main` yet):

- `render_svg(component, props)` = `VirtualDom::new_with_props` →
  `rebuild_in_place` → `dioxus_ssr::render`. That is the whole
  transform. Nothing post-processes the string.
- `rasterise(markup, w, h)` = `usvg::Tree::from_str(markup, &options())`
  → `tiny_skia::Pixmap` → `resvg::render`.
- `options()` is `usvg::Options::default()` plus a system font database.
  **It never sets `Options::style_sheet`** (the field exists —
  `usvg-0.45.1/src/parser/options.rs:100` — it is simply left `None`).

`export.rs::composite_cells` rasterises each sprite cell separately and
`image::imageops::overlay`s them; `render_control` is the only entry
point `fts-themer generate` uses (`fts-themer/src/generate.rs:80–88`).
The HTML-composed `strip.rs::Mixer` is **never** rasterised — it is the
web mixer and a visual reference only, per its own module doc.

**Does anything but inline `style=` survive into the SVG?** Today,
nothing needs to: `grep -c class: features/daw-ui/daw-theme-art/src/*.rs`
returns **0 for every file**, and the components do not even use
`style=` — they emit SVG *presentation attributes* directly
(`fill:`, `fill_opacity:`, `rx:`, `view_box:`), e.g.
`vector_controls.rs:303–314`. That is the most portable form available
and it should stay that way.

**Does the exporter inject a stylesheet or `<style>` element?** No.
`grep -rn "document::\|Stylesheet\|include_str\|asset!"` over
`daw-theme-art/src/` returns nothing.

## What resvg/usvg actually supports for CSS

Read from `usvg-0.45.1` and `usvg-0.46.0`, `src/parser/svgtree/parse.rs`:

1. **`<style>` elements inside the SVG document: supported.**
   `resolve_css()` (line 624) walks `xml.descendants()` for `style` tags
   with `type` absent or `text/css` and feeds them to `simplecss`.
2. **An injected sheet: supported**, via `Options::style_sheet`
   (parsed *before* internal sheets so internal ones win, mimicking
   `rsvg-convert`).
3. **External sheets: not supported.** There is no `<link>` handling and
   no `@import` resolution — `resolve_css` only ever looks at in-document
   `<style>` text and the injected string.
4. **Selectors: a real but small subset.** `simplecss` 0.2 via the
   `simplecss::Element` impl at line 656 — tag, class, id, attribute
   operators, descendant/sibling combinators, and exactly one
   pseudo-class (`:first-child`); every other pseudo-class returns
   `false` unconditionally (line 680: *"since we are querying a static
   SVG we can ignore other pseudo-classes"*). So `:hover`, `:nth-child`,
   `:not()`, `:where()` — all inert.
5. **And here is the load-bearing restriction: only *presentation
   attributes* are applied.** `write_declaration` (line 368) does
   `if aid.is_presentation() { insert_attribute(...) }`. The whitelist is
   `usvg/src/parser/svgtree/mod.rs:673–741` and it is paint and text
   only: fill, stroke and their modifiers, opacity, `display`,
   `visibility`, `clip-path`, `mask`, `filter`, `transform`,
   `transform-origin`, `mix-blend-mode`, font-*, text-*, marker-*,
   `stop-color`, `paint-order`, `shape-rendering`.

   **Not on that list:** `x`, `y`, `width`, `height`, `rx`, `d`, `cx`,
   `viewBox` — geometry is never settable from CSS in usvg — and of
   course `padding`, `margin`, `gap`, `flex`, `grid`, `position`,
   `border-radius`, `box-shadow`, because SVG has no box model and usvg
   has no layout engine at all.
6. **No CSS custom properties.** `grep -rn "var(\|CustomProperty"` over
   `usvg-0.46.0/src/parser/` returns **nothing**. `var()` does not
   resolve; a `fill: var(--foo)` is simply an unparseable paint.

**Consequence for Tailwind specifically.** Even if you injected the
compiled Tailwind sheet into the SVG, essentially none of it would land:
Tailwind v4's output is built on custom properties and `@layer`, and the
utilities that matter for layout (`flex`, `gap-2`, `p-1`, `w-full`,
`rounded`) are all non-presentation properties. What *could* land is a
narrow set of paint utilities (`fill-*`, `stroke-*`, `opacity-*`) — and
only if authored as literal colours, not `var(--color-…)`, and not
`oklch()` (see below).

## Blitz: is the CLAUDE.md prohibition still true?

It is **half true, and the half that is true is the important half.**

### `document::Stylesheet { href }` / `asset!()` — genuinely dead. Not folklore.

Blitz *does* implement `<link rel="stylesheet">`:
`blitz-dom/src/mutator.rs:687` queues `SpecialOp::LoadStylesheet`, and
`load_linked_stylesheet` (line 804) resolves the href against the
document's base URL and calls `self.doc.net_provider.fetch(...)`.

But `blitz-dom/src/document.rs:398–405`:

```rust
let base_url = config.base_url…            // None here
let net_provider = config
    .net_provider
    .unwrap_or_else(|| Arc::new(DummyNetProvider));
```

and every construction site in `nice-plug-dioxus` passes
`DocumentConfig { viewport: Some(viewport), ..Default::default() }` —
`standalone.rs:350`, `window.rs:408`, `window_softbuffer.rs:266`,
`embedded.rs:248` and `:333`. So there is **no net provider and no base
URL** in any FTS Blitz window: the fetch is issued into a dummy and the
stylesheet never arrives. `nice-plug-dioxus/src/assets.rs` says as much
in prose ("mostly a placeholder"). The prohibition on
`document::Stylesheet { href }` and `asset!()` should stay, and it should
stay stated as *"there is no net provider"* rather than *"Blitz is
unreliable"* — the failure is total and deterministic, not flaky.

### `<style>` elements via `document::Style` — fully supported, and already shipping

- `dioxus-native-dom/src/dioxus_document.rs:158` — `create_head_element`
  creates a real element in `<head>` with a text child.
- `blitz-dom/src/mutator.rs:692` — a `"style"` tag is collected into
  `style_nodes`; `flush()` (line 625) calls `process_style_element` for
  each, registering it with the Stylo stylist. Synchronous, no network.
- The engine behind it is **Stylo 0.19** — Firefox's style system. Its
  `stylesheets/` directory carries `layer_rule.rs`, `property_rule.rs`
  (`@property`), `container_rule.rs`, `scope_rule.rs`, `supports_rule.rs`.
  Cascade layers, custom properties and modern colour spaces are all
  there, which is what Tailwind v4 output requires. Layout is Taffy
  0.12 — flex, grid, block, absolute.

**This repo already does exactly this, in REAPER.**
`features/daw-ui/daw-ui/src/test_panels.rs:35–58` `include_str!`s a full
64 KB Tailwind sheet plus the architect-ui theme sheet and mounts them with
`document::Style { … }` inside a Blitz-rendered REAPER panel. Same
pattern in `apps/fasttrackstudio/src/rig_view.rs:29`/`:384` and
`mobile_view.rs:24`/`:59` for the signal UI. `Justfile:31–52` builds the
sheet (`just tailwind` → `apps/fasttrackstudio/assets/tailwind-signal.css`)
and `just tailwind-check` fails CI if the committed sheet drifts from
what the `@source` globs produce — and `apps/fasttrackstudio/input.css`
*already lists* `../../features/daw-ui/daw-ui/src/**/*.rs` as a scanned
source.

So the infrastructure the effort needs is not new work. It exists, it is
CI-gated, and `daw-ui` is already in its scan path.

**Verdict on CLAUDE.md:** the sentence *"inline styles only — Blitz does
not load external CSS files reliably"* is stale as written and should be
re-stated as two separate rules:

> - Never `document::Stylesheet { href }` and never `asset!()` for CSS —
>   FTS Blitz windows run `DummyNetProvider` with no base URL, so the
>   fetch is silently dropped.
> - Embed compiled CSS as a static string via `document::Style {
>   {include_str!(...)} }`. This works, and it is how the signal UI, the
>   session UI and the REAPER test panels already load Tailwind.
> - Inside an `<svg>` element, Stylo does not apply. Use presentation
>   attributes; classes and CSS variables do nothing there.

## The trap: Blitz renders inline `<svg>` through usvg

This is the finding that makes the exporter constraint and the Blitz
constraint the *same* constraint, and it is not documented anywhere in
the tree.

`blitz-dom/src/layout/construct.rs:357–394`:

```rust
if matches!(tag_name, "svg") {
    let mut outer_html = doc.get_node(container_node_id).unwrap().outer_html();
    …
    match crate::util::parse_svg_image(outer_html.as_bytes()) { … }
}
```

Blitz **serialises the `<svg>` subtree back to a string** and parses it
with usvg (`blitz-dom/src/util.rs:170`, `usvg::Options { fontdb, ..Default::default() }`
— `style_sheet: None` again). Three consequences, each verified:

1. **Classes on elements inside an `<svg>` do nothing under Blitz.**
   `write_outer_html_in_style` (`blitz-dom/src/node/node.rs:802`) writes
   *attributes*, not computed styles. A `class="fill-red-500"` is
   serialised verbatim as a class attribute, and the head `<style>` does
   not travel with it, so usvg has no rule to match it against. Stylo may
   compute a `fill` for that node; it is discarded at the boundary.
2. **The only CSS→SVG channel is `currentColor`**, and it is a string
   substitution: `node.rs:840–844` replaces `currentColor` in any
   attribute value with the parent's computed `color.to_css_string()`.
3. **That channel is currently broken for a Tailwind palette.**
   Stylo serialises a computed colour *in its authored colour space*, so
   a Tailwind v4 theme (whose tokens are `oklch(...)`) produces
   `fill="oklch(0.7 0.1 30)"` — which usvg's `svgtypes` cannot parse, so
   the attribute is dropped and the shape paints black or transparent.
   The tree already knows this:
   `libs/ui/ui-snapshot/tests/pixel_probes.rs:105–120` is an
   `#[ignore]`d regression gate saying precisely that, and
   `libs/ui/docs/blitz-diagnosis.md:148–156` generalises it:

   > Stylo formats computed colours in their source space. Anything
   > downstream that uses `Color::to_css_string()` and feeds the result
   > to a CSS Color 3 parser (usvg's `svgtypes`…) silently drops the
   > colour. **Always** convert via `Color::to_color_space(Srgb)`…

   The fix (`color_to_svg_compatible`, on branch
   `fix/svg-currentcolor-from-style`) is **not** in the pinned blitz rev:
   at `c82dd238` `node.rs:808` is still a bare `color.to_css_string()`.
   **Verify empirically** before relying on `currentColor` in any theme-art
   component; the mechanism is confirmed from source but the visible
   symptom under the exact current pin was not rendered.

**Corollary:** the theme-art components' existing discipline — literal
`fill="#rrggbb"` computed in Rust from `daw_theme::Theme`, never
`currentColor`, never a class — is not merely exporter hygiene. It is the
only thing that renders correctly under Blitz *as well*. It should be
written down as a rule, because it currently survives only as habit.

## The decision

**Tailwind carries the panel. It cannot carry the control.**

Draw the line at the `<svg>` boundary and it holds on all four targets
without a per-target branch:

### Above the `<svg>` — Tailwind, freely

The mixer's strip columns, the track panel's rows, the transport bar's
groups, docking, gaps, scrolling, responsive collapse: `div`s with
Tailwind classes. Works on web (browser), desktop and REAPER (Stylo +
Taffy), and is irrelevant to the exporter, which never renders this
layer — REAPER composites its own MCP from ~96 blitted images per the
WALTER layout, so the HTML strip is a reference, not an export artefact.

Mechanism: compile with `just tailwind`, mount with
`document::Style { {include_str!("…/tailwind-signal.css")} }`.
`apps/fasttrackstudio/input.css` already scans `daw-ui`; add any new
crate to its `@source` list and `just tailwind-check` keeps it honest.

### Inside the `<svg>` — presentation attributes, computed in Rust

Every control (`RecordArmButton`, `MuteButton`, `FxButton`, the fader
cap, the pan knob, the meter) stays as it is today: a root `svg` with an
explicit `view_box`, dimensions as fractions of the viewBox, colours
resolved from `daw_theme::Theme` into literal `#rrggbb` strings, and
state (`Interaction::{Normal,Hover,Pressed}`) passed as a **prop, not a
CSS pseudo-class** — `:hover` is inert in usvg, so hover *must* be a
prop for the exporter's three-cell sprite strip to exist at all, and
that same prop is what drives the live hover on web and under Blitz.

The component takes a width and a height and draws correctly at that
size (map decision #124, "Dioxus does not nine-slice"); the exporter's
`composite_cells` handles the sprite strip and stamps the WALTER markers.

### The one grey zone, and how to spend it

A `<style>` element placed *inside* the `<svg>` root **does** work in
both regimes — usvg's `resolve_css` finds it by descendant walk, and
Blitz serialises it along with the rest of the subtree. That gives a
legitimate channel for class-driven **paint** inside a control
(`.face { fill: #b8394e }`), shared by all four targets.

Recommendation: **don't**, at least not yet. It buys deduplication of
fill values while costing a second styling mechanism, and it cannot use
`var()` or `oklch()` — so it could not draw from the Tailwind token set
anyway, which is the only reason you would want it. Rust-computed
literals already give the theme a single source of truth
(`daw_theme::Theme`) that both regimes read. Revisit only if a control's
fill duplication becomes a real maintenance cost.

## Summary table: what survives where

| Mechanism | Browser | Blitz (desktop / VST3 / REAPER) | Exporter (SSR→resvg) |
|---|---|---|---|
| Tailwind class on a `div` | ✅ | ✅ (Stylo+Taffy; shipping today) | n/a — HTML never rasterised |
| `document::Style { include_str! }` | ✅ | ✅ | n/a |
| `document::Stylesheet { href }` / `asset!()` | ✅ | ❌ `DummyNetProvider`, no base URL | ❌ never injected |
| Tailwind class on a shape *inside* `<svg>` | ✅ | ❌ subtree re-parsed by usvg | ❌ no sheet reaches usvg |
| `style="…"` on a shape inside `<svg>` | ✅ | ✅ presentation props only | ✅ presentation props only |
| SVG presentation attribute (`fill=`, `rx=`) | ✅ | ✅ | ✅ |
| Geometry from CSS (`width`, `x`, `rx`) inside `<svg>` | ✅ (SVG2) | ❌ not a presentation attr | ❌ not a presentation attr |
| `var(--token)` inside `<svg>` | ✅ | ❌ usvg has no custom properties | ❌ |
| `oklch()` inside `<svg>` | ✅ | ❌ svgtypes cannot parse | ❌ |
| `fill="currentColor"` inside `<svg>` | ✅ | ⚠️ substituted, but broken under an oklch cascade at the current pin | ❌ nothing to inherit from |
| `:hover` inside `<svg>` | ✅ | ❌ | ❌ simplecss returns false |
| Flex/grid inside `<svg>` | ❌ | ❌ | ❌ SVG has no box model |

## Follow-ups this raises

1. **Rewrite the CLAUDE.md rule** into the three-clause form above. As
   written it forbids a pattern (`document::Style` + `include_str!`) that
   three shipping crates already use, and does not mention the SVG
   boundary, which is the rule that actually matters.
2. **Add `features/daw-ui/daw-theme-art/src/**/*.rs` to
   `apps/fasttrackstudio/input.css`'s `@source` list** when the panels
   move to Tailwind, so `just tailwind-check` covers them.
3. **usvg version skew.** The exporter pins resvg 0.45 → usvg 0.45.1;
   blitz-dom pulls usvg 0.46.0. Same architecture and the same
   `is_presentation` whitelist, but they should be moved in lockstep — a
   component that renders one way in the app and another in the export is
   exactly the "one is a bug, not a divergence" failure the theme-art
   module doc warns about.
4. **The `currentColor`/oklch gate.** `pixel_probes.rs` is `#[ignore]`d
   pending a blitz pin that serialises SVG-compatibly. Until then, no
   theme-art component should use `currentColor`. Worth an explicit test
   asserting no component's markup contains it. **Verify empirically**
   whether `c82dd238` still reproduces.
5. **Not settled here, and it should be:** whether the panels get a
   *second*, smaller compiled sheet scoped to `daw-ui` rather than
   inlining the 64 KB app sheet into every REAPER panel. Parse cost is
   per document and REAPER can host several.

## Sources

In-tree (branch `worktree-reaper-theme` for the `daw-theme-art` paths):

- `features/daw-ui/daw-theme-art/src/render.rs` — SSR→usvg→resvg path, `options()`
- `features/daw-ui/daw-theme-art/src/export.rs` — `cell_markup`, `composite_cells`, `render_control`, `states`
- `features/daw-ui/daw-theme-art/src/vector_controls.rs` — presentation-attribute drawing
- `features/daw-ui/daw-theme-art/src/strip.rs` — the HTML mixer, never rasterised
- `features/daw-ui/daw-ui/src/test_panels.rs` — Tailwind under Blitz in REAPER, today
- `features/daw-ui/daw-ui/src/panels/` — 117 inline `style:`, 2 `class:`
- `apps/fasttrackstudio/src/rig_view.rs`, `mobile_view.rs`, `input.css`, `Justfile`
- `libs/ui/docs/blitz-diagnosis.md`, `libs/ui/ui-snapshot/tests/pixel_probes.rs`
- skill `reaper-theme-vectors` — exporter constraints (strokes, clip-path, `<defs>`, sprite cells, markers)

Pinned dependency source:

- `usvg-0.45.1` / `usvg-0.46.0` — `src/parser/svgtree/parse.rs` (`resolve_css`, `write_declaration`, `XmlNode` selector impl), `src/parser/svgtree/mod.rs` (`is_presentation`), `src/parser/options.rs` (`style_sheet`)
- blitz fork `FastTrackStudios/blitz@c82dd238` — `blitz-dom/src/mutator.rs`, `document.rs`, `config.rs`, `layout/construct.rs`, `util.rs`, `node/node.rs`; `dioxus-native-dom/src/dioxus_document.rs`; `README.md`
- `nice-plug@fts-baseview-03` (`155e28d3`) — `nice-plug-dioxus/src/{standalone,window,window_softbuffer,embedded,assets}.rs`
- `stylo 0.19.0` — `stylesheets/` rule inventory
