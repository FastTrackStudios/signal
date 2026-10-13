# masks.vector-text-image-mask

## What it tests
Mask semantics across vector, text, and image mask sources.

## Setup
1. Create a Figma frame named `parity/masks/vector-text-image-mask`.
2. Add these contents:
- Row A: vector mask over color blocks.
- Row B: text mask over gradient and image.
- Row C: image mask with transparent regions.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Different mask sources stress different sampling and alpha paths.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
