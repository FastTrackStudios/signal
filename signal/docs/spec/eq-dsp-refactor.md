# EQ DSP API and runtime refactor

Follow-up policy and model work: [model-dsp-refactor.md](model-dsp-refactor.md).

Status: complete and validated 2026-09-07. Requested 2026-09-06.

## Objective

Provide an idiomatic Rust EQ library with a small ergonomic facade, typed
configuration, immutable prepared filter designs, and an explicit real-time
processing contract. Preserve the existing measured filter arithmetic while
migrating the plugin, rig, trigger DSP, imports, and response display.

## Architecture

Application/preset adapter -> EqConfig -> PreparedEq -> EqProcessor -> audio.
Prepared filters also feed response evaluation. Configuration owns user intent;
prepared designs own validated coefficients and sample-rate-dependent settings;
processors own history, envelopes, delay lines, smoothing and scratch storage.
Keep one crate initially and retain a low-level single-filter API.

## Milestones

- [x] Establish existing EQ DSP test baseline.
- [x] Typed configuration, errors, stable band IDs, capacity handling, and
  compatibility conversion for canonical/Pro-Q settings.
- [x] Named normalized coefficients, validated prepared filters, separate
  processor history, and shared response evaluation.
- [x] Prepared EQ lifecycle, mono/stereo buffer contracts, allocation-free
  processing and update installation, explicit reset and latency behavior.
- [x] Fast automation and detector-driven modulation with bounded work;
  smooth frame-level bypass and documented topology transitions.
- [x] Separate engine routing, dynamics, spectral, listening and output
  responsibilities; contain compatibility codes and numerical implementation.
- [x] Migrate production callers, hardware cascade evaluation and UI response;
  retire unsafe mutation entry points from the normal facade.
- [x] Regression, contract, allocation, routing and performance checks;
  runnable examples and public API documentation.

## API decisions

- Shape-specific builders with familiar Hz/dB/Q methods; validate at preparation.
- Explicit cut slope including fractional dB/oct and Brickwall, independent
  shape steepness, typed stream/threshold/detector/dynamics modes.
- Retain inactive mode settings in editable configuration.
- Millisecond dynamics controls for ordinary use; explicitly named
  frequency-dependent percentage ballistics for compatibility.
- Fallible insertion returns a stable BandId; removal/reordering cannot retarget
  an old ID. Capacity is configured at setup, with 24 as the compatibility default.
- Processing state has private fields. Configuration edits cannot leave live
  coefficients stale. Invalid updates leave the installed state unchanged.
- Preserve f64 arithmetic and DF1/TDF2 choices. Add mono and caller-buffer APIs.
- Separate preparation from installation. Installation must not free retired
  heap allocations in the callback; the caller owns reclamation.
- Equal stereo lengths and a prepared maximum block size are checked before
  audio/state mutation. Reset clears history without filter design.
- Surface spectral latency changes before activation.
- Shared coefficient response is exact for static filters. Dynamic snapshots
  and nonlinear response limitations are explicit. Stereo routing requires
  a complex 2x2 transfer, not a sum of every band's dB values.
- Keep numerical internals accessible only where real callers need them; avoid
  a speculative trait/plugin framework.

## Preservation and validation

Baseline command: `cargo test -p eq-dsp --lib --tests --offline`.
Baseline: 212 unit tests passed, 5 ignored; integration suites: 2 bandpass,
16 engine, 14 golden, 1 shelf probe passed. Several golden tests deliberately
pin existing defects (ShelfAlt instability, low-Q BandShelf instability,
slope mapping, ineffective BandPassVariant). These are characterization tests,
not evidence that those designs are valid. The new validated API must reject
unstable designs; repairs to filter math require explicit behavioral tests.

Acceptance:

1. Existing valid impulse/magnitude/dynamics/placement references retained.
2. Invalid numbers, unstable coefficients, capacity and stale IDs are errors.
3. Failed updates and invalid buffers preserve the previous state and audio.
4. No allocations/deallocations on first prepared block, steady processing,
   supported modulation, reset, or prepared-update installation.
5. Exercise static/dynamic/spectral/transient/listen paths and mono/stereo.
6. Verify response against rendered signals and mixed stereo routing.
7. Check automation discontinuities and partition invariance where the algorithm
   is sample-based; document block-rate detector redesign semantics.
8. Measure processing cost with representative and worst supported settings;
   report the machine/load and avoid claiming portable real-time deadlines.
9. Build migrated consumers and run targeted tests and Rustdoc examples.

## Execution notes

- Working tree was clean at start. No implementation changes were made during
  the preceding source review.
- Existing DSP uses std/realfft; this refactor must not introduce platform I/O
  or threads. A full no_std conversion of those dependencies is a separate
  dependency concern; keep the configuration/coefficient boundary portable.

## Implemented design decisions

- `config.rs`, `filter.rs`, and `prepared.rs` implement the public facade;
  `host.rs` owns persisted encodings and resolves canonical slopes before
  storing typed shape, placement, stream and resolved-order state in the engine.
- Runtime installation borrows immutable preparation. It copies bounded data
  into already allocated storage, avoiding deferred ownership/reclamation work.
- The plugin and rig deliberately use the canonical adapter for their persisted
  parameter IDs. They send static/dynamic settings atomically, and unchanged
  parameter polling does not redesign a band. Importers use the same mappings.
- Single-filter designs and their scratch use inline storage for supported
  orders. Expert math with unbounded orders may spill and is not a callback API.
- Spectral FFTs use prepared scratch and consume spectrum buffers in place;
  both hidden FFT scratch allocation and a per-channel spectrum clone were
  found by the new first-block allocation test and removed.
- Prepared coefficient changes crossfade over 5 ms. Full-engine routing, output
  and latency transitions remain explicit host decisions; incompatible latency
  changes require construction/activation of another processor. Reordering
  retains identity but resets moved slots' history.
- The validated API rejects unsupported spectral placement and transient/dynamics
  routing, rather than pretending those combinations have been implemented.
  Whole-band dynamics offer millisecond ballistics; spectral processing retains
  its existing ballistics. Host parameter encoding remains a boundary concern.
- Reset is allocation-free and matches fresh typed processing, including learned
  spectral state and dynamic initialization at the configured base gain.
- Runtime modulated cascades retain their original formulas and block cadence.
  A rejected unstable redesign retains stable coefficients and exposes an error.
- UI response uses prepared DSP designs once per graph update. Native graph data
  now includes slope, physical DSP Q and placement. The single combined line
  represents equal-power uncorrelated stereo input, evaluated through a complex
  transfer matrix.
- Superseded by `model-dsp-refactor.md`: permissive runtime behavior is removed,
  unsupported shapes are rejected, and host encoding adapters live in `host`.

## Observed callback cost

Command: `cargo run -p eq-dsp --example callback_cost --offline` (optimized dev
profile). AMD Ryzen 9 9950X3D; substantial concurrent build load (load averages
around 80/117/119 during validation). 48 kHz, 512-frame stereo blocks, 24 bands,
32 warm-up blocks and 256 measured blocks per scenario. The frame budget is
10.667 ms. These are observations, not an execution-time bound or an idle-machine
before/after performance comparison.

| Scenario | Median | Maximum observed |
| --- | ---: | ---: |
| Static | 0.236 ms | 2.124 ms |
| Dynamic SVF | 1.260 ms | 6.015 ms |
| Spectral | 0.426 ms | 3.703 ms |
| Transient | 0.469 ms | 0.981 ms |
| Modulated static cascade | 0.970 ms | 2.152 ms |

Public usage and migration details: `features/fx/eq/eq-dsp/README.md`.
Runnable examples: `equalize` and `callback_cost`.

## Validation results

| Check | Result |
| --- | --- |
| EQ DSP unit/integration tests | 259 passed, 5 existing ignored |
| Trigger DSP unit/integration tests | 58 passed, 6 existing ignored |
| EQ UI core and response tests | 21 passed |
| Public API Rustdoc example | 1 passed |
| Rustdoc build with `-D warnings` | Passed |
| EQ DSP library Clippy with `-D warnings` | Passed |
| All EQ DSP examples compile | Passed |
| Combined EQ UI / rig / EQ plugin / trigger plugin / import build | Passed |
| Final EQ plugin build after adapter lifecycle changes | Passed |
| Formatting and whitespace checks | Passed |

The 14 API tests include first-block allocation/deallocation checks, prepared
installation, reset, every supported bell order, detector-driven cascade design,
static/dynamic/spectral/transient/listen paths, invalid-buffer atomicity, stable
IDs, capacity, pole rejection, coefficient transitions and stereo response versus
rendered audio. Existing audible reference tests remain unchanged.

Final adapter review also corrected the EQ plugin reset hook to reset history
while remaining prepared, initialized analyzer scratch/feed outside processing,
avoided unchanged Neve parameter rebuilds, and removed response-display vectors
from the callback by evaluating the exported prepared hardware cascade directly.
