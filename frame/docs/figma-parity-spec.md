# Figma Parity Spec (v1 execution)

This document is the implementation contract for `roam-test-5g6.1` and downstream beads.

## Corpus Baseline

Primary corpus source: `/tmp/fts-figma-export.json`

Quick coverage summary command:

```bash
xtask/scripts/feature_coverage_report.sh \
  features/render/tests/parity/scenarios/paint.alpha-steps/export/latest-bridge-export.json \
  --out-json features/render/tests/parity/reports/feature-coverage.json
```

Observed baseline (captured with `cells/frame/tools/analyze_fts_export.sh`):
- schema: `fts.figma.export/v1`
- nodes: `461`
- svg exports: `396`
- total svg chars: `58,637,156`
- max single svg chars: `49,828,604`
- node types: `BOOLEAN_OPERATION`, `ELLIPSE`, `FRAME`, `GROUP`, `INSTANCE`, `LINE`, `RECTANGLE`, `STAR`, `TEXT`, `VECTOR`
- fills: `SOLID`, `GRADIENT_LINEAR`, `GRADIENT_RADIAL`, `GRADIENT_ANGULAR`, `IMAGE`
- effects: `DROP_SHADOW`, `INNER_SHADOW`, `LAYER_BLUR`

## Feature Matrix (Execution Order)

1. Import schema + compatibility
- v1 bridge input accepted
- v2 bridge input accepted with asset table hydration
- REST-compatible GetFile payload accepted

2. Paint stack fidelity
- Fill order preserved
- Solid fills exact
- Gradient fills (linear/radial/angular) with transform mapping
- Stroke style parity baseline (weight/cap/join)
- Image fill rendering

3. Compositing/effects
- Clip content parity
- Drop shadow parity
- Inner shadow parity
- Layer blur parity
- Blend mode parity for common modes

4. Geometry fidelity
- Vector path fit/aspect correctness
- Boolean operation parity
- Group transform and child transform correctness

5. Text fidelity
- Multi-run style rendering
- Line height + letter spacing
- Horizontal/vertical alignment
- Resize mode behavior

6. Editor parity
- Hierarchical hit-testing
- Drill-down selection behavior
- Bounding box + spacing overlays
- Inspector live edits with undo/redo

7. Performance
- Retained scene update path
- Dirty region/layout/paint recompute only
- 120fps interaction target on Apple Silicon

## Acceptance Rubric

1. Visual parity
- P0 fixtures (complex plugin UIs) have no critical geometry/paint mismatches.
- Pixel diff threshold target: <= 2.0% differing pixels for non-text fixtures at reference zoom.

2. Behavior parity
- Selection drill-down matches Figma expectations.
- Inspector edits produce immediate document mutation and can be undone/redone.

3. Performance
- Active pan/zoom/drag avoids catch-up rendering.
- Target frame budget: 8.33ms/frame at 120Hz in interaction mode.

## Current Gap Inventory (from baseline run)

Open high impact gaps:
- IMAGE fill draw path is not yet connected in the `render` crate.
- Effects pipeline is partially implemented (drop shadow draw, inner/layer blur placeholder).
- Stroke semantics are simplified (no cap/join/dash parity yet).
- Boolean/vector fidelity still relies heavily on SVG fallback.
