# The Blitz rendering contract

Rules for writing daw-ui components that render through Blitz — the
renderer REAPER embeds, and the one `dioxus-test` rasterizes with. Every
rule here was learned from a real bug; the failure mode is named so you
can recognise it from the symptom. Read this *before* writing a component,
not after the screenshot looks wrong.

Browser engines forgive all of these. Blitz does not, and the panels must
render identically in both — so the browser's forgiveness is the trap.

## 1. A viewBox's min-x/min-y offset is ignored, and nothing clips to it

`<svg viewBox="0 16 23 23">` draws exactly what `viewBox="0 0 23 23"`
draws: the offset is dropped, and shapes outside the box are not clipped.
"Windowing" one drawing through several offset viewBoxes therefore paints
the whole drawing in every window.

**Symptoms seen:** the fader rail painted as three disconnected dashes
(each band painted the full 27-row groove); the FX pill's toggle half
rendered 28 columns left of its stated position, through the `FX` glyphs.

**Rule:** each `<svg>` draws its own slice in its own coordinates —
`viewBox="0 0 w h"` always, with the slice arithmetic done in Rust
(see `VolumeFaderTrack`'s groove intersection). Or draw the composite as
one element (`FxPart::Whole`).

## 2. `height:100%` resolves against auto, to nothing

A `flex:1` child's *box* grows, but `height:100%` on anything inside it
resolves against an auto height and yields the content height. Percentage
chains that work in a browser silently don't here.

**Symptoms seen:** the fader's stretch band stayed 23 rows tall inside a
grown box; the whole strip drew at content height under `h-full`.

**Rule:** pass real pixel heights down. The strip states its height and
hands it to the fader; the fader computes each band's pixels
(`band_px`). If you know the number, say the number.

## 3. A replaced element's attributes beat the layout

An `<svg width="23" height="16">` is sized by its attributes first;
`flex:1` on it loses. An `<svg>` with *no* CSS size in an auto-width
absolute box can also divide space in ways its attributes don't suggest.

**Symptoms seen:** a growing rail band that stated the source band's
height never grew; two absolutely-positioned pill halves divided the pill
between them.

**Rule:** growing bands emit `height="100%"` as the attribute (with the
real height supplied per rule 2 where possible); fixed art states both
the attributes *and* `style="width:..px; height:..px"`.

## 4. Layout-critical values are inline; Tailwind is additive

Every window in this tree embeds the stylesheet as a static string, and a
panel that mounts before it — or a test that mounts without it — must
still lay out. A Tailwind-only `text-[8px]` falls back to the UA's 16px;
a Tailwind-only `flex flex-col` falls back to block layout.

**Symptoms seen (three separate times):** "pan" rendered at twice the
height of its band; the section stack collapsed and the bottom plate
painted over the meter; mute and solo laid out side by side and ran off
the strip.

**Rule:** anything that decides geometry — display, flex direction,
sizes, font sizes, alignment — is stated in `style:`. Tailwind classes
are polish on top. `tests/strip_shot.rs` renders sheet-less, which is the
enforcement.

## 5. SVG ids are document-global

A mixer is a row of strips, and every strip's `defs` land in one
document. Two ids collide silently: everything resolves to the first.
This is invisible while the defs' *content* is identical (every strip's
`#mtr` gradient is the same gradient) and breaks the moment content
varies per instance.

**Symptom seen:** the meter's clip regions — which depend on that strip's
level — clipped every strip's scale to the first strip's meter.

**Rule:** an id whose content varies per instance carries a per-instance
tag (`MeterProps::tag`, fed the track guid). Identical-content ids may
stay shared. Audit: issue #242.

## 6. Assorted, from the reaper-testing skill

- `<select>` renders as an empty box — use cycling buttons.
- `<input type="range">` has no thumb or track — draw sliders in SVG.
- An inline `<svg>` in a flex child needs `flex: 1 1 auto` plus a
  min-height or it collapses or eats siblings.
- `overflow: hidden` does not clip absolutely-positioned children —
  clamp coordinates yourself.
- Clip paths over text cut the glyphs, not the shapes, under the CPU
  rasterizer — the reason the meter's scale is one-tone (issue #246).
- `.focus()` before typing in tests.
