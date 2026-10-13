# text.mixed-style-runs

## What it tests
Mixed style runs inside a single text node.

## Setup
1. Create a Figma frame named `parity/text/mixed-style-runs`.
2. Add these contents:
- Row A: one line with multiple weights/colors.
- Row B: style run changes with letter spacing deltas.
- Row C: mixed runs across wrapped lines.
3. Export bridge JSON to `latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Style-run support catches gaps between text model and render implementation.

## Artifacts

- Expected image: [expected.png](./expected.png)
- Actual image: [actual.png](./actual.png)
- Export JSON: [latest-bridge-export.json](./latest-bridge-export.json)
