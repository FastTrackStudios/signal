# primitives.fractional-geometry

## What it tests
Subpixel positioning and sizing behavior for primitive rasterization.

## Setup
1. Create a Figma frame named `parity/primitives/fractional-geometry`.
2. Add these contents:
- Row A: shapes placed at .5 coordinates and fractional dimensions.
- Row B: 1px strokes on subpixel boundaries.
- Row C: mixed integer and fractional neighbors for aliasing comparison.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Subpixel handling is a common source of parity drift and aliasing mismatches.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
