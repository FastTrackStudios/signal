# images.fill-mode-matrix

## What it tests
Image fill mode behavior for fill/fit/crop/tile-style mappings.

## Setup
1. Create a Figma frame named `parity/images/fill-mode-matrix`.
2. Add these contents:
- Row A: identical image in each fill mode.
- Row B: different frame aspect ratios per mode.
- Row C: image fill plus stroke and corner radius.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Image sampling and placement are core parity requirements for UI assets.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
