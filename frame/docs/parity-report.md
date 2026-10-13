# Figma Parity Report

Generate a consolidated parity artifact from:
- feature coverage (`fts-figma-export` payload),
- optional visual acceptance results,
- optional performance snapshot JSON.

## Command

```bash
xtask/scripts/run_parity_pipeline.sh \
  features/render/tests/parity/scenarios/paint.alpha-steps/export/latest-bridge-export.json \
  --out-dir features/render/tests/parity/reports/paint.alpha-steps \
  --actual-png-dir /path/to/actual \
  --expected-png-dir /path/to/expected \
  --threshold 0.0 \
  --require-pass
```

Run the full parity pack (or a single scenario) via:

```bash
xtask/scripts/run_parity_scenarios.sh --threshold 0.0
xtask/scripts/run_parity_scenarios.sh --scenario paint.gradient-matrix --threshold 0.0
```

Headless render + diff flow:

```bash
xtask/scripts/render_parity_actuals.sh --all
xtask/scripts/run_parity_render_and_diff.sh --all --threshold 0.0
```

Manual split flow is still available via:
- `xtask/scripts/capture_render_diagnostics.sh`
- `xtask/scripts/capture_import_diagnostics.sh`
- `xtask/scripts/run_visual_acceptance.sh`
- `xtask/scripts/generate_parity_report.sh`
- `xtask/scripts/run_parity_scenarios.sh`

## Output

- JSON report:
  - `featureCoverage`
  - `visualAcceptance` (if provided)
  - `performance` (if provided)
  - `renderDiagnostics` (if provided)
  - `importDiagnostics` (if provided)
  - `gates` summary
- Markdown report suitable for bead closure evidence.
- `--require-pass` makes pipeline exit non-zero when `gates.parityPass` is `false` (CI-friendly).
