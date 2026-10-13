# layout.auto-layout-hug-fill

## What it tests
Auto-layout behavior for hug, fill, and fixed sizing.

## Setup
1. Create a Figma frame named `parity/layout/auto-layout-hug-fill`.
2. Add these contents:
- Row A: row auto-layout with hug content children.
- Row B: column auto-layout with fill children.
- Row C: nested auto-layout with mixed sizing policies.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Layout policy mismatches produce large downstream visual drift.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
