# Modulator

The universal **modulator** system: any **source** drives any **target**
[Parameter](parameter.md) with a signed amount and a curve. Named "Modulator"
(not "Modulation") to keep it distinct from modulation *audio effects* — chorus,
tremolo, phaser — which are ordinary FX blocks. One mechanism serves every
domain — synth layers, sampler instruments, guitar FX, master effects — so
"modulate anything with anything" is a single, consistent feature. Reference:
`features/fx/macromod/` (`ModulationRoute`, `ModulationSource`, `ParamTarget`).

## Routes

r[signal.modulator.route]
A modulator route is `{ id, source, target: ParamTarget, amount ∈ [-1,1]
(negative = inverted), curve, enabled }`. Routes are additive: a target's value
is its base plus the sum of its enabled routes' contributions, clamped to range.

r[signal.modulator.tick]
The engine evaluates each source once per render block and writes each route's
contribution (`source_value × amount`, shaped by the route curve) to its target
parameter, before the target node renders. Per-voice sources produce per-voice
offsets.

r[signal.modulator.curve]
Each route may shape the source→target mapping with a curve (linear, exp/log, or
a multi-point custom curve, plus uni/bi-polar). Bipolar routes are centered at
the source's neutral value.

r[signal.modulator.depth-ui]
Modulators are visual: a source can be dragged onto any parameter to create a
route; the target shows a modulation ring/arc whose extent is the amount, draggable
to set depth; a matrix view lists all routes. (See
[instrument-engine.md](instrument-engine.md) `signal.instrument.control`.)

## Sources

r[signal.modulator.sources]
Sources fall into three families, all usable as a route's `source`:
**generators** (Envelope, LFO), **performance/MIDI** (velocity, key tracking,
sustain, expression, breath, aftertouch, mod wheel, MPE X/Y/Z, …), and
**parameters** (any Parameter as a source — param-modulates-param, incl. macros).

### Generators

r[signal.modulator.envelope]
An envelope source is DAHDSR: delay, attack, hold, decay, sustain, release. Each
of attack/decay/release carries a **power/curve** (segment bend); sustain is a
level; times span 0–20 s on a logarithmic response. An envelope is note-gated
(per voice) and MAY loop. It exposes a velocity-sensitivity amount (neutral 50%,
higher = more velocity influence). Envelopes are editable as interactive graphs
(draggable nodes + a synced knob row).

r[signal.modulator.lfo]
An LFO source has a rate (free-running in Hz or tempo-synced to note values), a
shape (sine/tri/saw/square/random/sample-hold or a custom drawn shape), phase,
and retrigger behavior (free / note-retrigger). LFOs may be uni- or bi-polar.

### Performance / MIDI

r[signal.modulator.perf]
Performance sources expose the player's real-time gestures as modulators:
- **Bias** — a constant offset (a fixed hand-set modulation amount).
- **Velocity** — note-on velocity (per voice).
- **Key tracking** — note pitch across the keyboard, with a settable center + slope.
- **Sustain** — the sustain pedal (CC64).
- **Expression** — expression (CC11).
- **Breath** — breath controller (CC2).
- **Aftertouch** — channel and polyphonic aftertouch.
- **Mod wheel** — CC1.
- Any other CC — assignable.

r[signal.modulator.mpe]
Full **MPE** is a first-class source set: per-note **X** (pitch bend / glide),
**Y** (timbre, CC74 by default), and **Z** (pressure). MPE dimensions are
per-voice sources, so a route from MPE-Y to filter cutoff modulates each held
note independently. The engine allocates a channel per note in MPE mode.

r[signal.modulator.morph]
**Morph** is a source that blends between two or more snapshots/states of a set
of parameters — a single control that sweeps a whole timbre. A morph target set
is a group of parameters whose values interpolate along the morph position.

### Parameters as sources

r[signal.modulator.param-source]
Any [Parameter](parameter.md) can be a modulator source, so parameters link
and modulate each other. A Macro knob (see [macro.md](macro.md)) is the
canonical param-source: one control driving many targets.

## Scope & polyphony

r[signal.modulator.scope]
A route is either **per-voice** (its source is per-voice — envelope, velocity,
MPE, key track — and it modulates a per-voice target) or **global** (its source
is global — a free LFO, mod wheel, a global macro — and it modulates a shared
parameter). The engine resolves scope from the source/target and never mixes a
per-voice source into a global-only target incorrectly.
