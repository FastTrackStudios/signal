# valverig-nam — FTS patch

Upstream: [ValveRig/valverig-nam](https://github.com/ValveRig/valverig-nam),
crates.io `valverig-nam` 0.0.2 (MIT, `LICENSE` kept as published), the
engine `neural-amp-modeler` runs every capture on. Vendored from the
published crate: `src/`, `LICENSE`, `README.md`. `assets/`, `tests/`,
`tools/` and the examples are not copied (4.6 MB of models and reference
vectors), so the library's tests are off here (`[lib] test = false`); they
run in the fork's checkout, see below.

## The patch

Wide registers for the fused layer path (`src/fused.rs`, `src/fused/wide.rs`),
the path every A1 and A2 capture takes, on x86-64 with runtime detection:

- AVX2 + FMA: eight lanes a register where the four-lane path had four.
- AVX-512 (F, VL, DQ): sixteen lanes for sixteen-channel layers, and two
  eight-channel frames a register for eight-channel ones (A2 standard),
  with the 32 registers it brings.

A lane is still one channel of one frame, and every multiply-add is the
same fused operation in the same order, so the output is bit for bit the
four-lane path's (which is in turn bit for bit the generic path's). Machines
without AVX2 + FMA, and aarch64 (NEON), run upstream's code unchanged.

Measured on a Ryzen 9 9950X3D, blocks of 512 at 48 kHz, the rig's own
captures: an eight-channel A2 capture 445 -> 270 ns a frame, a sixteen-
channel standard WaveNet 1043 -> 552.

## Tests

In the fork's checkout (all of upstream's tests, plus): the fused path's
bit-for-bit tests against the generic path run every path the CPU has —
four lanes, AVX2, AVX-512 — and `examples/capture_bench` times real
captures and nulls them against a dump of the unpatched crate (bit for bit).

## Dropping it

Offer the wide path upstream; when a release has it (or something faster
that passes `tests/reference.rs`), take the crates.io release and delete
this directory and its `[patch.crates-io]` line in the root `Cargo.toml`.
