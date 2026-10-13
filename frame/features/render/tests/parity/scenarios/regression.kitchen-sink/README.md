# regression.kitchen-sink

## What it tests
A combined broad-spectrum regression gate across all major features.

## Setup
1. Create a Figma frame named `parity/regression/kitchen-sink`.
2. Add these contents:
- Include at least one block for each major category: gradients, images, blends, masks, blur, text runs, auto-layout, nested transforms.
- Organize blocks with clear labels.
- Keep content stable so this can be a release gate.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
This serves as the final comprehensive parity barrier before sign-off.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
