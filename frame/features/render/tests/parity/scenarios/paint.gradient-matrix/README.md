# paint.gradient-matrix

## What it tests
Gradient fidelity for linear, radial, and angular paint types.

## Setup
1. Create a Figma frame named `parity/paint/gradient-matrix`.
2. Add these contents:
- Row A: linear gradients with 2-stop, 4-stop, and hard-stop variants.
- Row B: radial and angular gradients with non-default transforms.
- Row C: gradient + stroke/effect combinations.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Gradient interpolation and transform mapping are frequent parity breakpoints.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
