# eq-dsp

Typed configuration, validated filter designs and real-time equalization.

```rust
use eq_dsp::{BandConfig, CutSlope, EqConfig, EqProcessor, Placement, ProcessSpec};

let mut config = EqConfig::new();
let presence = config.add_band(
    BandConfig::bell(3_000.0, 2.5, 0.8).placement(Placement::Mid),
)?;
config.add_band(BandConfig::high_pass(80.0, CutSlope::DbPerOctave(24.0)))?;

let spec = ProcessSpec::new(48_000.0, 512)?;
let prepared = config.prepare(spec)?;
let mut processor = EqProcessor::new(&prepared)?;
processor.process_stereo(&mut left, &mut right)?;

// Control side: configuration is ordinary editable data.
config.band_mut(presence)?.enabled = false;
let update = config.prepare(spec)?;

// Audio side: installation borrows data; its owner reclaims it off-thread.
processor.apply(&update)?;
```

Runnable version: `cargo run -p eq-dsp --example equalize`.

## Ownership and real-time contract

`EqConfig` owns intent, `PreparedEq` owns validated designs, and `EqProcessor`
owns history and preallocated storage. Construct and drop the processor outside
an audio callback. Processing, compatible prepared updates and reset perform
no heap allocation, reallocation or deallocation, including the first block.
The library creates no threads and performs no platform I/O.

Processing rejects unequal channel lengths and oversized blocks before touching
audio or history. Empty blocks do nothing. Mono input is treated as a centered
signal; its output is the average of the resulting left and right channels.
Use the stereo API when independent left/right output matters.

Prepared updates require the same `ProcessSpec`, sufficient existing band
capacity and unchanged latency. Spectral processing currently uses 4096-point
analysis and adds 4095 samples. Inspect `PreparedEq::latency_samples()` before
activating a newly constructed processor and coordinate compensation with the
host. Configuration and processor ownership remain with the application; there
is no hidden Arc swap, deferred-drop queue or background worker.

Band IDs survive reordering; removal does not reuse an ID in that configuration.
IDs are not global identities across unrelated configurations. History is retained
for identities that stay in their processing position; moved/new slots reset their
history. Prepared static coefficient changes crossfade over 5 ms. Changing routing,
dynamics topology, output gain or latency is a separate host transition decision;
these controls do not promise a full-engine crossfade.

For sample-positioned automation, split a block at the event position and apply
an already prepared update between sub-blocks. Dynamic SVF gain is sample-based.
Shapes that modulate a static cascade retain the established block-rate detector
redesign and 0.1 dB redesign threshold; their behavior depends on block partitioning.
Their supported design scratch fits inline storage. A failed live cascade design
keeps the previous stable coefficients; inspect `last_design_error()` for diagnosis.

## Filters and dynamics

`Filter` has shape-specific fields. `CutSlope` supports continuous 0..=36 dB/oct,
48/72/96 dB/oct steps and Brickwall. `Steepness` describes bounded transitions.
Bell and notch reject first-order steepness. The full engine retains its supported
10..=30000 Hz, Q 0.025..=40 and base gain -30..=30 dB domain; frequency must also be
below Nyquist. The single-filter API validates its design directly.

`Dynamics` retains its settings when switched to `DynamicsMode::Static`.
`Threshold` distinguishes fixed dB from automatic learning. `DetectorSource`
selects the band's own region or a custom frequency range. `Ballistics` offers
milliseconds for dynamic bands and explicitly named Pro-Q percentage behavior.
Spectral attack/release retain the spectral engine's own constants; the band's
ballistics control applies to whole-band dynamics. Density is normalized 0..=1.

The validated full-engine API rejects spectral placement other than Stereo.
In transient mode, dynamics currently support Bell/LowShelf/HighShelf on Both
streams (applied after recombination); static bands can select either stream.
Unsupported routing returns an error instead of silently ignoring a request.
`host` contains the plugin/preset encoding adapters; it does not opt out of
coefficient validation.

## Single filters and hardware

Use `Filter::prepare()` and `FilterProcessor` without constructing the full EQ.
`FilterProcessor::configure()` validates a bounded single-filter redesign without
allocation, retains the current design on error, and crossfades updates. Stereo
frame processing advances smoothing once per frame, independently of channel
routing. All processing uses f64 and preserves the established DF1/TDF2 choices.

`BiquadCoefficients` exposes named normalized numerator/denominator coefficients
and validates finite values, nonzero a0 and strict pole stability.
`PreparedFilter::from_coefficients()` accepts those coefficients. The expert
`from_sections()` adapter accepts raw `[a0, a1, a2, b0, b1, b2]` arrays.
Hardware models expose `prepared_filter()` for the same validated linear cascade.
No section is silently truncated by the validated API.

## Response evaluation

Prepared filters provide complex response, magnitude, phase, group delay and
caller-buffer magnitude evaluation. `PreparedEq::base_response()` returns a
complex 2x2 matrix so mixed left/right/mid/side routing composes correctly.

The base response describes settled configured filters. It excludes dynamic gain,
transient splitting, output controls, listening and nonlinear character. Those
systems do not have one universal static transfer function. The graph uses equal
power uncorrelated stereo input to reduce the matrix to a single display curve.
Native graph settings include the actual slope, DSP Q and stereo placement.

## Models and host encodings

A model owns its panel semantics and processing topology. For a Pultec-style
EQ, separate boost and attenuation controls produce interacting filter sections:

```rust
use eq_dsp::{ProcessSpec, hardware::hardware_eq::PultecEqp1aSettings};
let controls = PultecEqp1aSettings {
    low_boost_db: 8.0,
    low_atten_db: 5.0,
    drive_percent: 25.0,
    ..Default::default()
};
let prepared = controls.prepare(ProcessSpec::new(48_000.0, 512)?)?;
let mut processor = prepared.processor();
processor.process_stereo(&mut left, &mut right)?;
```

`model::PreparedModel<C>` accepts custom `Coloration` components before or after
its validated cascade. Each channel has separate coloration history.
`apply(&prepared)` preserves that history, crossfades the filter, and ramps trim;
incompatible stream specs, placement or latency return an error before mutation.
Custom coloration implements `update` to copy controls without replacing history.
Built-in preparation, updates, processing and reset allocate nothing.

Pultec uses `PultecColoration`, fitted to normalized 1 kHz transfer captures at
-12/0 dBFS and checked at held-out -24/-6 dBFS. A DC blocker removes the fitted
even-order offset. Beyond the measured domain, a bounded extension is a modeling
choice; elevated drive has not been validated for aliasing. The filter response
remains an approximation pending full frequency-response measurements.
`linear_filter()` excludes coloration and trim. `AnalogColoration::Arctangent`
remains available as an explicitly approximate component for custom models.

The plugin hardware adapter uses this same Pultec processor. API/SSL adapters
share the prepared-model path with approximate coloration; Neve uses the shared
validated filter processor. Hardware coefficient storage is inline, and compatible
filter edits preserve history through a short crossfade.

`host::CanonicalBandConfig` and `host::CanonicalEq` adapt persisted plugin parameter
encodings. There is no `compat` module or permissive coefficient-installation mode.
Unsupported ShelfAlt/BandPassVariant processing is rejected, unstable designs
retain the previous stable cascade and expose `last_design_error`, excessive host
slope indices saturate at Brickwall, and full-chain insertion returns an error.
New application code should use typed `EqConfig` or a model's own controls.

## Verification

- `cargo nextest run -p eq-dsp`
- `cargo test -p eq-dsp --doc`
- `cargo clippy -p eq-dsp --lib`
- `cargo nextest run -p trigger-dsp`
- `cargo nextest run -p eq-ui --lib`
- `cargo run -p eq-dsp --example callback_cost`

The callback benchmark reports observed median/max times for its machine and
load. It is not a scheduling or worst-case deadline guarantee. The existing
std/realfft dependency stack remains; this API refactor does not claim no_std
support for the full spectral engine.
