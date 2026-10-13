# FTS Figma Export Plugin

Exports selected Figma layers/frames as high-fidelity JSON payloads for Frame/AnyRender ingestion.

## What it exports

- Full selected node hierarchy (children preserved)
- Layout + geometry + style metadata (fills, strokes, effects, auto-layout, transforms)
- Text content and text layout metadata
- Optional per-node SVG (base64) for vector-faithful rendering
- Optional per-node PNG (base64) as fallback raster

Schema tags:
- `fts.figma.export/v1` (legacy)
- `fts.figma.export/v2` (current, deduped assets table + node asset refs)

## Setup (pnpm)

```bash
cd /Users/codywright/Documents/Development/FastTrackStudio/frame/packages/fts-figma-import
pnpm install
pnpm build
```

During development:

```bash
pnpm watch
```

## Use in Figma

1. In Figma desktop: `Plugins -> Development -> Import plugin from manifest...`
2. Select `/Users/codywright/Documents/Development/FastTrackStudio/frame/packages/fts-figma-import/manifest.json`
3. Select one or more frame/layer roots.
4. Click `Export Now`.
5. Use `Copy JSON` to paste directly into your app/import UI, or `Download JSON` for file-based import.

## Bridge mode

Figma plugins cannot write local files directly. Use the local bridge process:

```bash
cd /Users/codywright/Documents/Development/FastTrackStudio/frame/packages/fts-figma-import
pnpm bridge
# or auto-restart on script edits:
pnpm bridge:watch
```

### Single export to bridge

1. Keep `Bridge URL` as `http://localhost:43123/figma-export`
2. Click `Send to /tmp Bridge`

This writes:

- `/tmp/fts-figma-export.json`

### Core 10 parity pack export

Use the plugin buttons:

- `Validate Core 10 Frames`
- `Export Core 10 to Bridge`
- `Export All Parity to Bridge` (exports every frame named `parity/...`)

Required frame names on the page:

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

Bridge writes per scenario:

- `export/latest-bridge-export.json`
- `expected/frame.png` (direct Figma PNG)

Default parity root on bridge host:

- `/Users/codywright/Documents/Development/FastTrackStudio/frame/features/render/tests/parity/scenarios`

## Live sync mode

The plugin supports a lightweight session-level live mode:

- `Enable Live Sync` re-runs export when selection or document changes.
- The latest payload remains in the text area and can be copied any time.

This gives a real-time authoring loop while you continue iterating in Figma.
