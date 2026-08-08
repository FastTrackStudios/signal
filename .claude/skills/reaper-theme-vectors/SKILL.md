---
name: reaper-theme-vectors
description: "Use when replacing a REAPER theme's bitmap art with Dioxus vector components — adding a control to daw-theme-art, measuring source art, wiring it through export.rs, or chasing a component that scores badly, renders blank, or repeats itself across its cell. Covers the measure-draw-audit loop, the two audits and what each is blind to, the sprite-cell rules, and the traps that have each cost an afternoon: premultiplied reads, greyscale-plus-alpha PNGs, vertical state strips, clip-path, and the generate flag that quietly eats 2500 images."
---

# Vectorising REAPER theme art

The theme's PNGs are build output. Each is drawn by a Dioxus component in
`daw-theme-art`, rasterised by `fts-themer generate`, and the *same*
component renders the app's UI as live SVG. One definition, two
renderings — if the app and the exported theme disagree, one is a bug
rather than a divergence.

## The loop

```sh
# 1. Measure. Never draw from a screenshot or a hunch.
python3 - <<'PY'   # or the ad-hoc dumpers under /tmp during a session
# read the PNG, print alpha and channels per pixel
PY

# 2. Draw — a component in vector_controls.rs, wired in export.rs.

# 3. Write the art and audit.
cargo run -p fts-themer --bin fts-themer -- \
    --theme features/reaper/fts-theme generate
cargo run -p daw-theme-art --example align       # geometry + mean colour
cargo run -p daw-theme-art --example compare fx  # 6x tiles, source above ours

# 4. Look at it in REAPER.
nix develop .#reaper-test -c just reaper theme-shot target/theme-shots/x.png
```

`align` is pass/fail on bounding box and alpha-weighted mean colour.
**It passes images that are visibly wrong** — it called `track_fx_norm`
exact while the letters sat two columns left. When a control looks off
but scores well, rank by mean absolute per-pixel error instead; that is
what shows on screen.

`examples/markup` prints the SVG a control renders to. Reach for it the
moment a PNG comes out blank or nonsensical: several failures are in the
markup, not the geometry, and are invisible from the pixels.

## Measuring

**Read coverage, not a silhouette.** Thresholding alpha quantises edges
and invents geometry. The record arm was drawn four different wrong ways
— ellipse, chamfer, clipped circle — because a threshold reported a
straight edge where the source has a circle running into a flared base.
Sub-pixel coverage per row gives radii to a tenth of a pixel.

**A partial pixel keeps its own colour.** A dark rim shows up only in the
edge pixels; if a boundary pixel is much darker than the face, there is a
rim, and its width is roughly that pixel's coverage.

**Beware greyscale-plus-alpha.** Several PNGs are `graya`, not `srgba`. A
reader expecting four channels per pixel matches *none* of it and reports
the image as empty. `track_folder_on` was read as blank twice; it is an
opaque block. Always `magick identify -format '%[channels]'` first, or
force `-alpha on -colorspace sRGB` before parsing.

**Do not `head` a dump.** The record-mode plate looked two rows short of
its cell for an hour. It was not: the dump had been through `head`, which
cut the last two rows off the art rather than off the file.

## Sprite cells

`states()` in `export.rs` is knowledge, not measurement, and it has to be
right or a control renders into a fraction of its own width and repeats.

- Most controls are **three states side by side**: normal, hover,
  pressed.
- Knobs, faders, troughs and **every background** are **one drawing**
  REAPER stretches.
- The FX and send lists are **three states stacked vertically**. The cell
  detector only looks for horizontal periods, so it reports one cell of
  the full width and is right to.
- Folder strips are **three marks side by side in one image**, not three
  pointer states.

`art_x` is the first *drawn* column, which is the cell origin for
everything whose art starts at its own edge — but not for a control that
deliberately leaves one empty. The mixer's FX toggle leaves a seam
column; `leading_gap()` exists for exactly that.

**The cell you pass must be the cell the compositor measured.** A viewBox
one unit wider than the box it lands in squeezes every coordinate inward
— a fraction of a pixel near the origin, a whole one at the far side.
Three controls carried that bug; `examples/cells` prints what the
detector actually found.

## Traps in the drawing

**Strokes.** resvg flattens a gradient on a stroke to its average, and a
stroke straddles its own path so half of it lands outside the shape. Draw
borders as *filled* shapes with the face inset, and gradient bands as
filled annuli. The record ring, the FX pill and the play-rate knob all
needed this.

**clip-path.** A clip-path around a group is the obvious way to round one
end of a plate. resvg dropped the group, plate and all, and the button
rendered as a bare glyph on nothing. Build the radius into each shape.

**`<defs>`.** A stray `<g>` can swallow the closing `</defs>` and put an
entire plate inside the definitions block: valid SVG, renders nothing,
indistinguishable from a component that has stopped working. This is what
`examples/markup` is for.

**rsx attributes** take an identifier or a simple expression. An `if` or
a call with arithmetic in its arguments will either fail to parse or fail
to interpolate. Hoist it into a `let` above the `rsx!`.

## How this theme actually behaves

Things that were guessed wrong and are worth knowing before measuring:

- **Hover often adds rather than scales.** The transport's face goes
  50/47 to 70/67 — twenty levels on both ends, where a proportional lift
  moves the top three times as far as the bottom. `offset` exists beside
  `lift` and `deepen` for this.
- **Buttons darken differently from each other.** Solo holds its red and
  its blue *rises*; blue-defeat holds its blue; mute scales all three by
  0.89. One rule bent to fit gets two of the three wrong.
- **A face starts at the button's own colour.** Mute's top row is
  184,58,78 and `signal.mute` is `#b8394e` — the same value. Lifting it
  further doubles up with the highlight row below.
- **Legends are not one grey.** They brighten when the button lights, and
  the track panel swings much further than the mixer (183→242 against
  204→204).
- **`disabled` on the routing button greys the output lane and nothing
  else** — not the plate, not the other lanes, not the alpha.
- **Backgrounds are flat bands, never gradients.** The ones that look
  like gradients are two bands a few levels apart.
- **A lit ring throws light inside itself**: `#ff5f5f` outside a lit
  record button, `#ff7272` within.

## Generating

```sh
fts-themer generate            # vector components only — the default
fts-themer generate --traced   # ALSO rewrites the 2500 inherited images
```

`--traced` pushes every inherited PNG through the theme's luminance ramp.
That is right when the palette differs from the source and **destructive
when it does not**: mapping the art onto its own colours still rounds
through the ramp's stops. One run lifted `tcp_mainbg` from `#333333` to
`#3d3d3d` and washed out every toolbar icon, with nothing in the output
to say so beyond "2796 images generated".

Recovery, from inside the theme directory:

```sh
rsync -a --existing .source-art/ ./ && fts-themer generate
```

## Keeping the reference honest

`target/theme-shots/pristine.png` must be a shot of the *pristine art
under the current palette*. Build it by copying the theme to a scratch
directory and overlaying `.source-art` before shooting. A stale reference
is worse than none: a shot taken before the palette changed made the
track panel look broken for an hour, and it was fine.
