# Parity Scenario Contents Spec (Core 10)

This is the current required set. Only these 10 scenarios are in scope for now.

## Global Rules

1. Frame name must be exactly `parity/<category>/<case>`.
2. Frame size defaults to `1920x1080`.
3. PNG export at `1x` only.
4. Keep content deterministic: fixed fonts/assets, no hidden layers, fixed bounds.
5. Export paths per scenario:
- JSON: `export/latest-bridge-export.json`
- Figma PNG: `expected/`
- Renderer PNG: `actual/` (matching filenames)

## Core 10

### `primitives.fractional-geometry`
- What: subpixel coordinate and size behavior.
- Setup:
1. Row A: shapes at `.5` positions and fractional dimensions.
2. Row B: 1px strokes on subpixel boundaries.
3. Row C: mixed integer + fractional neighbors.
- Why: catches aliasing and coordinate rounding drift.

### `paint.gradient-matrix`
- What: gradient fidelity for linear/radial/angular.
- Setup:
1. Row A: linear 2-stop, 4-stop, hard-stop variants.
2. Row B: radial and angular gradients with transforms.
3. Row C: gradients combined with stroke/effect.
- Why: interpolation and transform mapping parity.

### `paint.blend-mode-stack`
- What: blend mode compositing behavior.
- Setup:
1. Row A: normal/multiply/screen/overlay.
2. Row B: darken/lighten/color-burn/soft-light.
3. Row C: 3-layer mixed blend stack.
- Why: blend math/order mismatches are common.

### `effects.layer-background-blur`
- What: layer blur and background blur behavior.
- Setup:
1. Row A: layer blur values `2`, `8`, `24`.
2. Row B: background blur cards over patterned content.
3. Row C: blur + rounded corners + clipping.
- Why: blur is high-risk and often approximated.

### `strokes.caps-joins-dashes`
- What: stroke cap/join/dash semantics.
- Setup:
1. Row A: butt/round/square caps.
2. Row B: miter/round/bevel joins.
3. Row C: dash+gap+offset variants.
- Why: stroke parameter support often diverges.

### `masks.vector-text-image-mask`
- What: mask behavior across vector, text, image masks.
- Setup:
1. Row A: vector mask over color blocks.
2. Row B: text mask over gradient/image.
3. Row C: image mask with transparency regions.
- Why: stresses alpha/mask source handling paths.

### `images.fill-mode-matrix`
- What: image fill mode mapping.
- Setup:
1. Row A: same image in fill/fit/crop/tile modes.
2. Row B: varying frame aspect ratios.
3. Row C: image fill + stroke + corner radius.
- Why: validates placement/sampling correctness.

### `text.mixed-style-runs`
- What: mixed style runs in one text node.
- Setup:
1. Row A: multiple weights/colors in one line.
2. Row B: style run changes with spacing deltas.
3. Row C: mixed runs across wrapped lines.
- Why: catches text model/render integration issues.

### `layout.auto-layout-hug-fill`
- What: auto-layout sizing policy behavior.
- Setup:
1. Row A: row auto-layout with hug children.
2. Row B: column auto-layout with fill children.
3. Row C: nested auto-layout with mixed sizing.
- Why: layout policy drift causes broad UI mismatch.

### `regression.kitchen-sink`
- What: broad end-to-end regression gate.
- Setup:
1. Include at least one block for each major domain: gradient, image, blend, mask, blur, text runs, auto-layout, nested transform.
2. Label each block clearly.
3. Keep content stable between exports.
- Why: final high-coverage parity barrier before sign-off.

## Run Commands

- Core 10 default:

```bash
xtask/scripts/run_parity_scenarios.sh --threshold 0.0
```

- Single scenario focus:

```bash
xtask/scripts/run_parity_scenarios.sh --scenario paint.gradient-matrix --threshold 0.0
```

- Full matrix later (when expanded):

```bash
xtask/scripts/run_parity_scenarios.sh --all --threshold 0.0
```
