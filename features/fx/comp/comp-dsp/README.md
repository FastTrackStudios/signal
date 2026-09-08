# comp-dsp

Build a compressor from model-specific controls, then give its prepared processor
to the audio callback:

```rust
use comp_dsp::{CompressorConfig, La2aControls, Model, ProcessSpec};
let controls = La2aControls { peak_reduction: 0.67, gain: 0.286 };
let prepared = CompressorConfig::new(Model::La2aGray(controls))
    .prepare(ProcessSpec::new(48_000.0, 512)?)?;
let mut processor = prepared.processor();
processor.process_stereo(&mut left, &mut right)?;
let reduction_db = processor.gain_reduction_db();
```

`Model::Generic(GenericControls)` supplies ordinary threshold, ratio, attack,
release and makeup controls. It is a hard-knee feed-forward compressor with a
peak detector. `Model::La2aGray` uses the measured native Peak Reduction/Gain
positions and an optical cell. It does not pretend those knobs are linear
threshold or makeup sliders. Compress mode is implemented; Limit and Emphasis
are not fitted controls in this model.

The reusable `components::Compressor<D, G, E, C>` composes a `LevelDetector`,
`GainComputer`, `Envelope` and `Coloration`. Reduction is **positive dB** at both
gain-computer and envelope boundaries. Audio gains use 20 log10, never a square
root. `La2aGainComputer` and `OptoCell` can be reused independently. Different
topologies, such as feedback detection, can reuse these components in their own
processing order instead of being forced into this feed-forward kernel.

Processors own two independent channel histories. `stereo_link` blends each
rectified sidechain toward the stereo peak; `mix` controls parallel compression.
`process_frame_with_sidechain` accepts an external stereo detector source.
Block lengths and maximum capacity are checked before samples or history change.
`apply(&prepared)` installs controls for the same model without resetting its
history or computing design coefficients on the callback. It rejects model/spec
changes before mutation. Updates step at the block boundary: hosts can ramp
controls or crossfade for smooth automation. Reset preserves configuration and
clears stream state. Construction is outside
the callback; built-in processing and reset allocate nothing. Activating a
new model starts a new stream; a host chooses its model-change crossfade.

The existing Pro-C-style host engine is still available separately. Its amplitude
gain bug is corrected here; it is not the implementation of the new generic
model. Its extra upward/expander/lookahead paths have not been moved into the
new model processor. Existing profile crates describe UI mappings; they are not
measurement-validated implementations of every named unit.

The plugin's LA-2A profile now selects this model with native Peak Reduction/Gain
parameters. The chain retains its sidechain EQ and lookahead, reports the selected
model's reduction, and crossfades to/from the extended processor over 5 ms.
Reserve lookahead storage during activation with `reserve_lookahead` before
changing its length on the callback.

The LA-2A sources were ported from `a589b291`/`54315910`, with the measured static
surface tests retained. The release fit now accounts for captured makeup gain and
compression during the quiet -20 dBFS stimulus. Its depth-dependent stretched
exponential includes accelerating recovery and a shallow long tail. End-to-end
1 kHz trajectory error is below 0.9 dB at 48 kHz, including two knob positions
withheld from fitting; regression tests also cover 96 kHz. The fixture extraction
and fit scripts are in `tools/`, with source hashes embedded in the fixture.
This does not establish accuracy at every frequency or on arbitrary programme.
Pultec and compressor captures are under
`/run/media/AudioHaven/Plugin Analysis`; the architecture/evidence record is in
`docs/spec/model-dsp-refactor.md`. Bulk capture files are not build dependencies.

Verify with `cargo nextest run -p comp-dsp` and `cargo clippy -p comp-dsp --lib`.
