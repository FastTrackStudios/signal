# Diagnosing Blitz / dioxus-native bugs

This is the playbook we keep coming back to whenever a dioxus
component looks fine in `wry` / `--platform web` but renders or
behaves wrong on `--platform native` (Blitz). It's distilled from
the SVG `currentColor` bug, the CSS-keyframe rotation bug, and the
Xvfb-DPI parity drift — see [`docs/blitz-fork-workflow.md`](./blitz-fork-workflow.md)
for how the fork is laid out, and the [`Diagnostics/*`
stories](../crates/fts-ui/src/stories.rs) for live reproductions of
each.

## 1. Reproduce in a story before doing anything else

Don't debug against the full app. Pull the broken component into a
single `#[story]` block with one or two knobs and **only the markup
you suspect**. The shell renders it without the surrounding chrome
and the parity harness can run it deterministically. Past examples:

- `Diagnostics / svg-smoke` — twelve labelled probe rows for the
  SVG-vanishes bug.
- `Diagnostics / layout-stack` — pure-div stack to bisect a font
  drift.
- `Diagnostics / dropdown-close` — single dropdown next to a
  click-away target.

Every story you write is permanent and free, so leave them in the
tree as regression markers even after the bug is fixed.

## 2. Build a probe matrix, not a single repro

When you have a "X doesn't work in Blitz" report, write down every
adjacent variant that you can think of and put them in one story
side-by-side. The bug is rarely what the report says it is — what
you actually want is the *boundary* between the cells that work
and the cells that don't.

For the SVG bug the original matrix was:

| size | stroke         | animation     | result |
|------|----------------|---------------|--------|
| 32px | `white`        | none          | ✓      |
| 32px | `currentColor` | none          | ❌      |
| 16px | `white`        | none          | ✓      |
| 16px | `currentColor` | none          | ❌      |
| 16px | `white`        | `animate-spin`| ✓      |
| 16px | `currentColor` | `animate-spin`| ❌      |

That matrix immediately told us the axis was `currentColor`, not
size, not animation. The original bug report blamed size + animation
and we would have spent a week chasing the wrong thing without it.

**Rule of thumb**: at least two cells should disagree on each axis
you're investigating. If you can't disagree on an axis, you don't
need it in the matrix.

## 3. Run the parity harness

`cargo test --release -p architect-ui --features stories --test parity`
gives you a deterministic side-by-side composite of Blitz and wry.
Use it for any visual question. When in doubt, `nix-shell -p
xorg.xorgserver imagemagick --run "<cmd>"` — the harness needs Xvfb,
xdotool, and ImageMagick on `$PATH`.

The composite lives at
`crates/fts-ui/parity_output/<Category>__<name>.composite.png`
(Blitz on the left, wry on the right). Check the per-side images
too — sometimes Blitz crashes silently and "looks identical" only
because both sides are blank.

When the composite looks wrong, generate a diff image:

```sh
nix-shell -p imagemagick --run \
  "convert <story>.blitz.png -alpha off -colorspace gray /tmp/b.png && \
   convert <story>.wry.png -colorspace gray /tmp/w.png && \
   convert /tmp/b.png /tmp/w.png -compose difference -composite \
       -auto-level -modulate 100,200 <story>.diff.png"
```

A solid black diff means pixel-equal. Anything visible there is
your bug. Watch for:

- **Repeated/ghosted text** — vertical drift that accumulates
  per row → see §6 (Xvfb DPI).
- **Outlined shapes only** — colour mismatch (the shapes are at
  the same position but one side is a different colour, so the
  difference has the *outline* of the shape) → see §4
  (`currentColor` / colour-space).
- **Single bright blob in one quadrant** — geometry / transform
  bug.

## 4. Pixel-level inspection: `convert` and `Image`

The parity score alone won't tell you *what* differs. Two cheap
escalation steps:

**Sample specific pixels**:

```sh
nix-shell -p imagemagick --run \
  "convert <story>.blitz.png -format '%[pixel:p{640,400}]' info:"
```

**Find row positions** with a 30-line Python script using PIL:

```sh
nix-shell -p python3Packages.pillow --run "python3 - <<'EOF'
from PIL import Image
img = Image.open('<story>.blitz.png').convert('L')
pix = img.load()
in_band = False; rows = []
for y in range(img.height):
    bright = any(pix[x, y] > 200 for x in range(195, 230))
    if bright and not in_band: rows.append([y, y]); in_band = True
    elif bright: rows[-1][1] = y
    elif not bright: in_band = False
for i, (s, e) in enumerate(rows):
    print(f'row {i}: y={s}..{e}  h={e-s+1}')
EOF"
```

Doing this on `layout-stack` is what proved Blitz was rendering
`width: 32px` divs as 32 device pixels and webkit was rendering
them as 33–34. With that data the fix was one line in
`spawn_xvfb`; without it we'd have spent days hunting font metrics.

## 5. Identify the right repository

Bugs in this stack live in one of three places. Pick the right
one before opening files:

| Symptom                                              | Likely repo |
|------------------------------------------------------|-------------|
| Element paints wrong colour / wrong shape / vanishes | `forks/blitz/packages/blitz-paint` or `blitz-dom` |
| Layout / sizing / flex / grid / box model            | `forks/blitz/packages/blitz-dom/src/layout/` or `stylo_taffy` |
| Animation / transform                                | `forks/blitz/packages/blitz-dom/src/stylo_to_kurbo.rs`, `stylo.rs` |
| Event / focus / keyboard not working                 | `forks/blitz/packages/dioxus-native-dom/src/events.rs` |
| Snapshot / capture / parity infrastructure           | `forks/blitz/`-adjacent: `fts-story` |
| Showcase wrappers / demo glue                        | `architect-ui` |

When the bug is in Blitz (most of them are), branch off
`upstream/main` per [`docs/blitz-fork-workflow.md`](./blitz-fork-workflow.md):
small fixes go on `fix/<slug>` branches that are cleanly PR-able
upstream; integration ships via merge commits onto `fts/integration`.

## 6. Patterns we've seen more than once

### `oklch()` reaching downstream parsers

Stylo formats computed colours in their source space. Anything
downstream that uses `Color::to_css_string()` and feeds the result
to a CSS Color 3 parser (usvg's `svgtypes`, anything using a stale
parser, anything that does substring matching on `rgb(`) silently
drops the colour. **Always** convert via
`Color::to_color_space(Srgb)` and emit `rgba(r, g, b, a)` before
crossing that boundary. See `fix/svg-currentcolor-from-style`.

### CSS animations don't tick in snapshot harnesses

`BaseDocument::resolve(current_time_for_animations: f64)` is what
advances the Stylo animation clock. Snapshot harnesses that always
pass `0.0` will render every keyframe at its initial state.
`architect-story-snapshots` settles at `t = 0` (so animation start times
anchor consistently) and then jumps the final resolve to
`animation_time_secs` (default `0.125 s` = 45° on a 1 s
`animate-spin`). For wry-side parity, inject CSS that pins the
clock to the same point:

```css
*, *::before, *::after {
  animation-delay: -0.125s !important;
  animation-play-state: paused !important;
  transition: none !important;
}
```

### Xvfb default DPI is 75

webkitgtk scales every CSS-pixel value by the X server's DPI
factor (which is 75 on a default Xvfb, against the CSS-px
convention of 96). Blitz's CPU rasteriser uses CSS-px == device-px
at scale 1.0. Without `-dpi 96` on Xvfb, every parity capture
disagrees by ~1 px per row and the dssim score never falls below
~0.08. `architect-story-parity` pins it; if you ever set up a new
headless capture path, set this first.

### Tailwind v4 hover variants and Stylo's media features

Stylo's Servo backend (`stylo/style/servo/media_features.rs`) only
registered six media features upstream: `width`, `scan`,
`resolution`, `device-pixel-ratio`, `-moz-device-pixel-ratio`,
`prefers-color-scheme`. Tailwind v4 wraps every `hover:*` utility in
`@media (hover: hover) { &:hover { … } }` to suppress hover styles
on touch devices — but with no `hover` feature registered, the
matcher treated the query as unsupported and silently dropped the
contained rule. Symptom on the native renderer: every Tailwind
`hover:bg-X` was invisible (buttons, dropdown items, links never
highlighted), even though plain `:hover` worked fine on hand-rolled
CSS.

Fork: [`FastTrackStudios/stylo`](https://github.com/FastTrackStudios/stylo),
branch `fts/integration` adds `hover` / `any-hover` / `pointer` /
`any-pointer` to the Servo `MEDIA_FEATURES` array (and registers the
four atom names in `stylo_atoms/static_atoms.txt` so `atom!()` can
resolve them at compile time). Hard-codes "fine pointer that can
hover" because no Servo embedder targets touch yet.

When extending: keep `[patch.crates-io]` entries for `stylo`,
`stylo_traits`, `stylo_atoms`, `stylo_static_prefs`, `stylo_dom`,
`selectors`, `servo_arc`, `to_shmem` — they all live in the same
workspace and a partial patch produces "multiple versions of crate
X in dependency graph" errors that show up as 75+ trait-bound
failures in `blitz-dom`.

### Focus-driven UI primitives need real blur events

`dioxus-primitives::dropdown_menu`, `select`, `popover`, etc. all
close themselves by watching `focus.any_focused()` flipping false
— there is **no** "click outside" handler. The trigger needs a
real blur event (which Blitz emits via `generate_focus_events`
when a click hits an unfocusable sibling), AND the
`mounted.set_focus(true).await` the trigger calls in its `onclick`
must not panic. The latter was the actual bug behind "dropdowns
aren't closing on native": Dioxus polls the spawned focus future
synchronously inside the same call stack as Blitz's event-dispatch
`borrow_mut`, so any in-future `doc.borrow_mut()` panics with
`RefCell already borrowed`. Symptom looked like "doesn't close"
because the renderer was dead after the first click. Fix:
`NodeHandle::set_focus` queues into
`Rc<RefCell<Vec<(NodeId, bool)>>>` that `DioxusDocument` drains
after every UI event and after every `poll`, so the focus
mutation lands on a clean borrow. See
`fix/dom-handle-focus-defer-queue` and the
`Diagnostics / dropdown-close` story.

When you encounter a `RefCell already borrowed` panic in any
`NodeHandle::*` future, the same queue-defer pattern almost
certainly applies — never call `self.doc_mut()` synchronously
from a future returned from a `RenderedElementBacking` method.

## 7. Tracing what Blitz is actually doing

When the visual output isn't enough:

```sh
RUST_LOG=blitz_dom=debug,blitz_paint=debug,anyrender_vello_cpu=debug \
  cargo test --release -p architect-ui --features stories \
  --test parity <story>_parity -- --ignored --nocapture
```

For one-off questions, `eprintln!` inside the suspect Blitz code
path is the fastest signal. Add a `[patch]` to architect-ui's
`Cargo.toml` so the workspace picks up the local fork checkout
without a push round-trip:

```toml
[patch."https://github.com/FastTrackStudios/blitz"]
blitz-dom = { path = "/home/cody/Development/FastTrackStudio/forks/blitz/packages/blitz-dom" }
# …repeat for every blitz-* / dioxus-native-* / stylo_taffy / debug_timer crate
```

Always remove this before committing — it makes the repo unbuildable
on any other machine. The diagnostic-iteration cycle is:

1. Add `eprintln!` in the fork checkout.
2. `cargo test --release …` from architect-ui (rebuilds your fork crate
   incrementally).
3. Read the printed values.
4. Iterate.
5. Once the fix is identified: commit on a fresh `fix/<slug>`
   branch off `upstream/main`, restack `fts/integration`, push
   both, drop the `[patch]`, `cargo update -p dioxus-native`,
   verify, commit architect-ui.

## 8. When to stop and what to escalate

Three exit conditions, in priority order:

1. **You found a one-line fix and the parity composite agrees.**
   Ship it. Open a PR upstream when the diff is small and clean
   (the `currentColor` and Xvfb-DPI fixes were both a handful of
   lines and went straight to upstream-shaped branches).

2. **You found a real bug but the fix is invasive (touches the
   Stylo bridge, the layout pipeline, or the event system).**
   File an issue on `FastTrackStudios/blitz` with the diag story
   linked, and a hypothesis. Don't merge a workaround into
   `fts/integration` unless it's strictly additive (e.g. injecting
   a defensive attribute) — the fork rebases on every upstream
   sync.

3. **The "bug" is renderer-engine inherent (font hinting, sub-pixel
   AA between vello-cpu and webkit, GPU-vs-CPU rasterisation).**
   Document it in this file, loosen the dssim threshold for that
   story if the diff is genuinely noise, and move on.

If you spent more than two hours and haven't moved between those
three states, you're probably missing a probe — go back to §2 and
add another row to the matrix.
