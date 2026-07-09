# eq-dsp

Filter design and biquad-cascade DSP for the FTS-EQ plugin.

## Module map

### Top-level
- `design.rs` — `FilterType` enum and `design_filter(...)` entry point.
  Maps `(filter_type, fc, Q, gain, sr, order)` → `Vec<Coeffs>`.

### Per-filter cascade builders (`design/`)
| Module | Filter | Notes |
|---|---|---|
| `design/allpass.rs` | Allpass, BandPassVariant | Butterworth poles, Q-modulated first section |
| `design/bandpass.rs` | Bandpass | Analog-form path-A pipeline |
| `design/bell.rs` | Peak/Bell | Dispatches to `cascade::compute_cascade_peak_with_slope` |
| `design/hp.rs` + `design/hp/sN.rs` | High-cut | Slope-specific section builders (s=3..9) |
| `design/lp.rs` | Low-cut | Pure Butterworth + odd-N tail |
| `design/notch.rs` | Notch | Cascaded RBJ biquad notches |
| `design/shelf.rs` | LowShelf, HighShelf | Universal-synth shelf with anti-cramping clamps |
| `design/tilt.rs` | TiltShelf | Low-shelf + high-shelf with opposite gains |
| `design/common.rs` | _(helpers)_ | `slope_from_pole_count`, `db_to_linear`, `ui_q_to_bandwidth_q`, `cascade_qs` |

### Lower-level pipeline
- `biquad.rs` — `Coeffs = [f64; 6]`, `PASSTHROUGH`, `Mode0Params { p2..sp6 }.to_biquad()`.
- `cascade.rs` + `cascade/` — section-level kernels (Lagrange-MZT, universal-synth glue,
  per-family builders for bandpass/notch/shelf-alt).
- `proq4_mzt.rs` + `proq4_mzt/` — single-section MZT closed forms per filter family.
- `proq4_per_section_helpers.rs` — universal Lagrange-MZT section synth.
- `proq4_peak.rs` — Vicanek matched-magnitude bell pipeline.

## Conformance

```sh
cargo test -p eq-dsp --test conformance_all_scan --release -- --nocapture
```

The `conformance_all_scan` test compares the magnitude response of each
filter cell `(fc, Q, gain, slope)` against fixed magnitude references in
`tests/reference/`. The 8 filters with universal-synth-backed paths
(allpass, bandpass, bell, flat_tilt, high_cut, low_cut, high_shelf,
low_shelf) currently track those references at 100%. Notch and
tilt_shelf use textbook implementations and do not match — the scan
results for those two are informational, not a regression gate.

## Environment variables

| Var | Effect |
|---|---|
| `FTSEQ_UNIVERSAL_SYNTH=1` | (legacy, no-op — universal-synth is the default) |
| `FTSEQ_BELL_SLOPE=<n>` | Disambiguate slope=5 vs slope=6 in bell dispatcher |
