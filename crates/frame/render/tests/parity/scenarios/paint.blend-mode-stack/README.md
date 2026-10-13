# paint.blend-mode-stack

## What it tests
Blend mode behavior across common and non-default modes.

## Setup
1. Create a Figma frame named `parity/paint/blend-mode-stack`.
2. Add these contents:
- Row A: normal, multiply, screen, overlay over colored backdrops.
- Row B: darken, lighten, color-burn, soft-light examples.
- Row C: 3-layer blend stack with mixed modes.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Blend compositing differences can be subtle but produce large pixel-diff failures in real designs.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
