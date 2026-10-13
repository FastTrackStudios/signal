# Model-oriented EQ and compressor DSP

This supersedes the compatibility policy in `eq-dsp-refactor.md`. There is no
requirement to preserve erroneous audio behavior or an old application API.
Persisted plugin parameter encodings are boundary adapters, not DSP models.

## Evidence inspected (2026-09-07)

The measurement work is on `worktree/silver-forest-c7df`, ending at `54315910`.
`4f6f46bd` corrects the compressor's square-root gain bug; `a589b291` implements
a reusable optical cell; `54315910` supplies measured LA-2A panel curves.
The current branch has diverged structurally, so port the relevant DSP changes
without replacing newer channel-state work or merging unrelated host changes.

The archive exists at `/run/media/AudioHaven/Plugin Analysis`. LA-2A Gray has
static, level, release and mode captures. Pultec EQP-1A has scans and saturation
captures with engaged controls, frequency modes, harmonics, transfer shapes,
and separability measurements. Pro-Q 4 also has its archived preset/character
measurements. Capture metadata is essential: older gain trajectories encode
-48..+6 dB, newer ones declare their range. Do not reinterpret old bytes using
new constants. This data-format distinction is necessary for measurement
correctness, unrelated to preserving legacy DSP behavior.

The existing `eq-profiles` Pultec mapping and `comp-profiles` LA-2A mapping are
UI approximations. They are not sufficient definitions of these processors.
In particular, the measured LA-2A has no user attack/release controls, and its
measured gain knob is not a 0..24 dB makeup slider.

## Architecture

1. **Controls** are specific to a model, with a fallible preparation boundary.
   A Pultec exposes separate boost and attenuation and bandwidth; an LA-2A
   exposes Peak Reduction and Gain. Generic threshold/ratio EQ-band controls
   remain useful for generic models but are not a universal front panel.
2. **Components** implement independently reusable operations: filter cascades,
   detectors, static gain laws, envelopes/cells, and coloration. Components
   declare their units. Compressor reduction is positive dB throughout.
3. **Models** compose components in their actual order. Knob mapping can change
   multiple coefficients, the detector, or nonlinear stages. A model need not
   reduce to a list of EQ bands or to one compressor style integer.
4. **Processors** own stream history; configuration/preparation owns design.
   Frame APIs advance shared state once. Blocks validate lengths/capacity
   before touching samples. Processing and reset allocate nothing.
5. **Measurement** remains outside DSP. Tests can embed small measured targets
   and provenance; bulk captures remain in the external archive. Linear
   response APIs describe the linear cascade, not saturation at arbitrary
   signal levels. An arctangent stage is an approximation until measured and
   fitted, regardless of the name on the panel.

## Execution

- Remove permissive EQ coefficient installation, silent section truncation,
  and full-chain insertion aliasing; keep invalid designs out of audio.
- Make host encodings explicit instead of advertising a compatibility API.
- Port the gain-domain correction and measured optical cell/LA-2A model.
- Add a composable compressor kernel and typed model configuration; share
  the measured LA-2A gain law and cell with that kernel.
- Add a prepared EQ model API that composes a validated linear cascade and
  explicit coloration, with Pultec as a concrete model.
- Test invalid updates, independent stereo state, reset, block partitioning,
  steady-state gain, and the existing measured LA-2A targets using nextest.
- Document examples and distinguish implementation, fitted evidence, and
  future modeling work.

## Limits of the evidence

The LA-2A steady-state table reports 0.29 dB mean / 0.73 dB worst over 21
operating points. Release is now fitted with depth-dependent stretched
exponentials, accounting for idle makeup and residual compression at -20 dBFS.
The 1 kHz end-to-end regression stays below 0.9 dB worst error at 48 kHz and
includes two held-out knob positions. It also runs at 96 kHz. This is evidence for
these stimuli, not arbitrary programme/frequency accuracy.
Pultec coloration is fitted to normalized 1 kHz transfer residuals; the linear
filter arithmetic remains an approximation pending full frequency-response data. Other named models require their own
capture → fit → verify work; adding an enum variant is not an implementation.

## Implementation and verification

Implemented on this worktree:

- EQ `compat` is removed. `host` now names the encoding boundary, with no
  permissive coefficient mode. Full chains return a capacity error; unsupported
  shapes are rejected; unstable redesigns preserve the running stable cascade.
  Excessive host slope indices no longer wrap to a shallow filter. EQ graph Q
  no longer doubles as a hidden slope control; embedded graph callers supply
  slope explicitly or use the second-order default.
- `eq_dsp::model::PreparedModel<C>` composes validated cascades with per-channel
  coloration before or after filtering. `PultecEqp1aSettings::prepare` is the
  concrete front-panel example; linear response excludes coloration and trim.
- `comp_dsp::components` provides detector, gain-computer, envelope and coloration
  traits and a feed-forward kernel. The measured LA-2A now uses that kernel.
  Generic controls and native LA-2A controls have distinct typed configurations.
- `PreparedCompressor` stores designed channel templates. Compatible borrowed
  updates preserve history and allocate nothing. Model/spec changes are rejected
  transactionally and require explicit activation. Parameter updates step at the
  boundary; hosts choose smoothing or processor crossfades.
- The older Pro-C-style host processor is isolated in `pro_c.rs`; its existing
  upward/expander/lookahead paths remain separate from the new model processor.
  Its square-root gain error is fixed and regression-tested across -1..-40 dB.
  Its soft knee now stays continuous and never boosts below threshold; hard-knee
  threshold no longer divides by zero. Style factors scale time constants rather
  than poles, preventing Optical instability. Analytic gain-law regressions pass
  before regenerating the affected golden vectors. EQ golden updates are limited
  to excessive host slopes; fake bandpass-variant fixtures were removed.
- Runnable examples: `eq-dsp/examples/pultec.rs` and
  `comp-dsp/examples/models.rs`; public guides in each crate's README.

## Integration and capture regression follow-through

- The compressor plugin's LA-2A profile now selects the measured model and writes
  native normalized Peak Reduction/Gain parameters. The face no longer advertises
  an unfitted Limit mode. Metering reads the selected model. Sidechain EQ and
  lookahead remain shared; switching to/from the extended host core crossfades.
- Sidechain low-pass filtering now operates before rectification, so it actually
  rejects high-frequency carriers. Lookahead storage is reserved at activation.
- Optical release extraction retains provenance and the original byte range.
  Previous fitting confused applied gain with gain reduction and treated the
  quiet portion as silence. `extract_la2a_release.py`, `fit_la2a_release.py` and
  `release_verify` make the corrected capture → fit → verify loop reproducible.
- `PultecColoration` fits a fifth-order transfer residual with DC-centered even
  terms. Training uses -12/0 dBFS; validation uses -24/-6 dBFS. Maximum normalized
  sample error is below 7e-6 across all four levels. A bounded C1 extension outside
  ±1 and a 5 Hz DC blocker are explicit model choices, not measured circuit facts.
  Elevated-drive aliasing and full frequency response remain unverified.
- The hardware host uses the shared prepared-model pipeline for Pultec/API/SSL,
  and the validated filter processor for Neve. Coefficient storage is inline.
  Compatible filter updates preserve history and crossfade; Pultec trim/drive
  ramps. Invalid designs retain the previous running configuration.
- Prepared model updates reject incompatible stream specs, placement and latency
  transactionally. Tests cover native host parity, allocation-free preparation
  and edits, independent channels, block partitioning, DC rejection, extension
  continuity and reset behavior. Fixtures include capture source hashes and need
  no external plugin or bulk archive to run.

Additional named profiles still require their own capture → fit → verify work.
The extended Pro-C host retains its upward/expander paths. This implementation
does not label every profile mapping as a measurement-validated analog model.

Validation commands: nextest for eq-dsp, comp-dsp, comp-profiles, trigger-dsp and
native EQ/compressor UI; strict library Clippy and rustdoc for both DSP crates;
doctests/examples; compilation of both native plugins and downstream consumers.

Verified on 2026-09-07: 555 distinct nextest tests pass (412 DSP/profile/trigger,
143 native UI), with 11 existing DSP/trigger skips. The final added hardware
history/invalid-update regression passes in the focused model suite. Both DSP
crate doctests, strict library Clippy, strict rustdoc and examples pass. EQ,
compressor and trigger plugins plus signal-fx, signal-import, reverb-ui,
saturate-ui and signal-guitar-ui compile; downstream pre-existing warnings remain.
