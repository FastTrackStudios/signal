# effects.layer-background-blur

## What it tests
Layer blur and background blur behavior and clipping interaction.

## Setup
1. Create a Figma frame named `parity/effects/layer-background-blur`.
2. Add these contents:
- Row A: layer blur values 2, 8, 24.
- Row B: background blur cards over patterned content.
- Row C: blur with rounded corners and clipped content.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Blur support is a known parity risk and often implemented as approximation.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
