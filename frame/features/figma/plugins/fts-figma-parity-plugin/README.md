# FTS Parity Scenario Generator (Plugma)

Plugma-based Figma plugin that generates the Core 10 parity scenario frames used by the renderer parity workflow.

## What it does

Running the plugin will:

1. Create or update Core 10 frames:
- `parity/primitives/fractional-geometry`
- `parity/paint/gradient-matrix`
- `parity/paint/blend-mode-stack`
- `parity/effects/layer-background-blur`
- `parity/strokes/caps-joins-dashes`
- `parity/masks/vector-text-image-mask`
- `parity/images/fill-mode-matrix`
- `parity/text/mixed-style-runs`
- `parity/layout/auto-layout-hug-fill`
- `parity/regression/kitchen-sink`
2. Create additional isolated frames for high-risk features:
- `parity/text/font-weight-style`
- `parity/text/alignment-truncation`
- `parity/effects/shadow-matrix`
- `parity/effects/drop-shadow-matrix`
- `parity/effects/inner-shadow-matrix`
- `parity/effects/effect-ordering`
- `parity/effects/background-blur-stress`
- `parity/paint/blend-mode-combos`
- `parity/masks/mask-clip-interactions`
- `parity/text/text-advanced`
- `parity/images/image-sampling`
- `parity/transform/transform-precision`
- `parity/masks/clip-content-nesting`
- `parity/transform/nested-transform-stack`
3. Set each frame to `1920x1080`.
4. Add starter content for each scenario category.
5. Position frames in a grid and select them.

## Dev setup

```bash
cd /Users/codywright/Documents/Development/FastTrackStudio/frame/packages/fts-figma-parity-plugin
npm install
npm run dev
```

## Import in Figma

1. Run `npm run dev` (or `npm run build`).
2. In Figma desktop, open Actions (`Cmd+/`) and run `Import plugin from manifest...`.
3. Choose:
- `/Users/codywright/Documents/Development/FastTrackStudio/frame/packages/fts-figma-parity-plugin/dist/manifest.json`
4. Run `FTS Parity Scenario Generator`.

## Build for non-dev import

```bash
npm run build
```

Then import from `dist/manifest.json`.
