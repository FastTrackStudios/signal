# Gain compensation — nothing gets louder by accident

Every FTS processor is **loudness-neutral by default**: turning a knob
changes the *character* of the sound, not its level, so the engineer judges
the processing rather than the loudness change, and the rest of the mix is
not disturbed. This is a strict contract for every production plugin and
every stage in a stack ([stack.md](stack.md)). The only exceptions are the
processors whose *purpose* is level (utility gain/trim, the limiter's
ceiling, the `level` rider) and an explicit per-instance opt-out.

Reference implementations: `saturate-dsp` (`Makeup::Matched`, the preamp
makeup calibrated at −6 dBFS), `reverb` primitive calibration. The comp
currently has **manual** makeup only; that is the first thing this spec
changes.

## The contract

r[fx.gain-comp.default-on]
Every FTS processor that can change level as a side effect of its
character controls — compressors (all styles), saturators/preamps, EQ
(parametric and hardware models), delays/reverbs (wet/dry balance),
multiband, transient, gate/expander — exposes an `auto_gain` parameter
(stepped on/off) that is **ON by default** in every factory preset and on
instantiation. With it on, the processor's output level tracks its input
level as its character controls move.

r[fx.gain-comp.opt-out]
`auto_gain` off reveals the processor's manual output/makeup control at its
current value (the compensation is *folded into* the manual control when
switched off, so switching never jumps the level — what you hear is what
the knob now reads). Switching on again re-derives compensation from the
parameters and resets the manual control to 0 dB.

r[fx.gain-comp.deterministic]
Compensation is **deterministic from the parameters**, never measured from
the programme: the same settings always produce the same gain, the gain is
smooth and monotonic in each parameter, and there is no signal-dependent
gain riding (no pumping, no level that depends on how long the plugin has
been running). (A measured "learn/match" trim may be offered later as an
explicit action, never as the default path.)

r[fx.gain-comp.reference]
The calibration reference is pink noise at **−18 dBFS RMS** (K-20 / line
level): with `auto_gain` on, for every reachable setting of a processor's
character controls, the RMS of the processed reference is within **±1 dB**
of the dry reference, and within **±0.5 dB** across the *typical* range of
each control (the middle 80 % of its travel). Each processor's calibration
model (how it derives gain from its settings) is documented in its crate
and locked by a test (`r[verify fx.gain-comp.reference]`) that sweeps each
character control and asserts the bound.

r[fx.gain-comp.continuity]
Compensation never jumps: a change of compensation gain is smoothed with
the same smoother as the processor's own gain (≥ 5 ms, click-free), and
switching style/model with the same user-facing settings keeps the level
within the reference bound (the new model's compensation applies, the old
one's is released — both smoothed).

r[fx.gain-comp.bypass]
Bypass and A/B (`mix` at 0 vs 100, stage enable/disable, stack
lane mute) compare at equal loudness by construction — because the
processed path is compensated, bypassing it does not change level. Any
per-plugin "delta/listen" mode is exempt (it is diagnostic).

## Per-kind models

r[fx.gain-comp.comp]
Compressor compensation = the estimated steady-state gain reduction of the
reference at the current threshold/ratio/knee, plus the style's known
level offset (FET/opto/VCA/vari-mu characters have a measured static
offset at unity). Attack/release/lookahead do not enter the estimate
(they change transients, not the steady level). Parallel `mix` blends the
compensated wet with the dry so the mix knob is loudness-flat.

r[fx.gain-comp.saturate]
Saturator compensation = inverse of the stage's small-signal gain
(drive/input) times the stage's measured RMS ratio for the reference at
that drive (the "matched" makeup), per circuit; a circuit whose character
is "gets louder" (a clean preamp with a level knob) is still matched — the
*level* knob is the exception control, the *drive* knob is compensated.

r[fx.gain-comp.eq]
EQ compensation = the negative of the curve's pink-weighted mean gain
over 20 Hz–20 kHz (the Pro-Q "Auto Gain" model): boosts are pulled down,
cuts are pushed up. Cut/notch bands and shelves count by their energy
footprint, so a low-cut at 80 Hz is roughly neutral while a +6 dB 1 kHz
bell costs about −2 dB. Hardware models include their unit's insertion
offset. `gain_scale` is inside the compensated path.

r[fx.gain-comp.time]
Delay and reverb: the wet path is compensated for tail energy so that
raising feedback/decay/size with `auto_gain` on does not raise the summed
level (an equal-power wet/dry law with tail-energy normalization); the
`mix` knob stays loudness-flat end to end.

r[fx.gain-comp.exempt]
Exempt processors (no `auto_gain`, level is the point): utility
gain/trim, the limiter (ceiling and its own make-up are the controls), the
`level` rider, meters, and any "output"/"trim" control on a compensated
processor (which is applied *after* compensation and is the engineer's
deliberate level move).

## Stacks

r[fx.gain-comp.stack]
In a stack each stage is compensated individually, so adding, removing,
reordering, or disabling a stage does not change the level. A parallel
sum is normalized for coherent signals (sum / N active lanes) by default,
with per-stack `sum_mode` (coherent 1/N, power 1/√N, raw) and per-lane
gain that defaults to 0 dB, so stacking five parallel compressors at equal
lane gain is as loud as one.

## Verification

r[fx.gain-comp.verify-harness]
One shared test harness (in `features/fx/verify` or equivalent) renders the
reference through any `Stage`, sweeps its character controls, and reports
the worst-case level deviation; every production plugin has a test that
uses it and fails outside the bound. The harness is the definition of
"compensated" — a plugin that passes it is compliant, one that does not is
not, regardless of intent.
