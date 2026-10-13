# Frame Visual Diff Harness

This harness provides a deterministic pixel diff check between two PNG files.

## Command

```bash
cargo run -p frame-cli --features anyrender --bin visual_diff -- \
  /path/to/actual.png /path/to/expected.png 2.0
```

Arguments:
- `actual.png`: rendered output from Frame preview
- `expected.png`: reference export (Figma or trusted baseline)
- `threshold_percent` (optional): allowed differing pixel percentage (default `2.0`)

## Output

The tool prints one JSON line:
- `diffPixels`
- `diffPercent`
- `meanAbsDeltaPerChannel`
- `maxChannelDelta`
- `passed`

Exit code:
- `0` when `passed=true`
- `1` when diff exceeds threshold or input errors

## Notes

- Images must have the same dimensions.
- This is the acceptance harness foundation for bead `roam-test-5g6.12`.

