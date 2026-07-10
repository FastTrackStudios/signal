# openwurli-dsp (vendored)

Physically-modeled Wurlitzer 200A electric-piano DSP — modal reed synthesis,
preamp circuit simulation, tremolo, power amp, and the framework-agnostic
`WurliEngine` voice manager.

## Provenance & license — GPL-3.0

This crate is **vendored from [openwurli](https://github.com/hal0zer0/openwurli)**
(author: hal0zer0) and is licensed **GPL-3.0-or-later**. See `LICENSE`.

It is vendored into this monorepo **for personal use only**. Because it is GPL,
it is kept as a self-contained leaf crate: only `openwurli-dsp` is imported (not
the plugin/xtask crates), and it is consumed by the signal engine as the
"City Wurli" native instrument voice.

## Local modifications from upstream

- **`src/filters.rs`** — upstream's `Biquad` wraps `melange-primitives`, an
  external **git** dependency. The monorepo forbids git deps, so the biquad is
  reimplemented locally using the same RBJ Audio EQ Cookbook coefficients and
  Direct Form II Transposed structure. This removes the crate's only external
  (non-crates.io) dependency; the crate now has **no runtime dependencies**.
- **`Cargo.toml`** — hardcoded package metadata (edition/license/repo/authors)
  instead of `.workspace = true` inheritance, so the GPL license label is not
  overwritten by the workspace's MIT/Apache default. The `melange-primitives`
  dependency line is dropped.

Everything else (engine, voice, reed, pickup, hammer, tables, preamp, power amp,
speaker, tremolo, oversampler, MLP corrections) is upstream-faithful.
