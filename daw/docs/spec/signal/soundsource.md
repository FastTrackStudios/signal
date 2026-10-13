# Soundsource

The generator inside a Layer (see [instrument-engine.md](instrument-engine.md)).
This spec defines the `Soundsource` contract and each kind — Oscillator, Sample,
Physical Model, Audio — in full, so one engine covers synths, Keyscape-class pianos, drum kits,
and Cinematic-Studio-Strings-class orchestral libraries. Grounded in
`crates/signal/docs/sampler-trait-design.md` (the dream API) and the CSS work in
`crates/signal/docs/cinematic-studio-strings-progress.md`.

Guiding principle (from the sampler design): **the common 90% is declarative
data; the exotic 10% is a trait you implement.** Multi-mic, round-robin,
dynamics crossfade, articulation switching, and legato are *built-in behaviors
configured by data* — but each is a public trait hook you may replace.

## The trait

r[signal.soundsource.trait]
A `Soundsource` turns note + parameter + timing events into audio for a Layer.
It exposes: `kind()`, `prepare(sample_rate, block_size)`, `note_on(note,
velocity)`, `note_off(note)`, `render(in, out, events)`, `params()`,
`set_param()`. It is the render-tree leaf's generator refinement, so it drops
into the graph unchanged.

r[signal.soundsource.declarative]
A normal library loads with **zero code** — a spec (`.styx`/`.soundpack`) plus a
sample scan. Every exotic behavior (legato, keyswitch, mic mix, round-robin,
dynamics crossfade) is selected + configured by that data, and is a trait a
library MAY override for the rare case the data model can't express.

r[signal.soundsource.params]
A Soundsource's parameters are enumerable (`params()`) and settable by id
(`set_param`), so the Control/Edit UI and the modulation engine drive any of
them generically without knowing the concrete kind.

## Render modes — live vs offline (core value)

r[signal.soundsource.mode]
A Soundsource renders in one of two modes, and MUST behave correctly in both:
- **Live** — driven by an incoming real-time event stream. No knowledge of
  future input. Latency must stay small and bounded; the instrument is playable.
- **Offline** — the full input (a MIDI/document performance) is known in
  advance. The Soundsource MAY look ahead arbitrarily and use future-dependent
  techniques (transition selection, lead-in, time-stretch, alignment).

r[signal.soundsource.mode.live-playable]
In Live mode a fancy engine (legato strings, etc.) MUST remain immediately
playable — minimal, bounded transition latency and no audible stalls. It trades
fidelity for responsiveness (e.g. shorter legato pre-delays, a fast transition
sample) rather than introducing lag.

r[signal.soundsource.mode.offline-lookahead]
In Offline mode the same engine, seeing the input ahead of time, MAY schedule
transitions early and apply "time trickery": pick the ideal (longer, more
expressive) transition sample, place its lead-in *before* the target note so the
transition's arrival lands on the beat, time-stretch/align transitions, and
pre-roll releases. Offline output is the highest-fidelity rendering; Live is the
real-time approximation of it.

r[signal.soundsource.mode.parity]
Live and Offline share one configuration and sample set — they differ only in
scheduling/look-ahead, never in the source data. A note's *sound* is the same;
only its transition timing/quality differs by mode.

## Oscillator

r[signal.soundsource.oscillator]
The Oscillator kind is analog/wavetable synthesis: selectable waveform or
wavetable, with unison (voice count, detune, width), sub-oscillators, FM, ring
modulation, and additive/harmonic voices. It is note-triggered and polyphonic.

## Source modules

A Soundsource runs a chain of optional **source modules** (Omnisphere-inspired)
that shape the raw generator before it reaches the Layer's filter/amp/FX. They
apply to any kind — a Sample soundsource transposes, reverses, unisons, and
granulates just as an Oscillator does. Each module's parameters are ordinary
[Parameters](parameter.md) (modulatable, macro-drivable, learnable). Some modules
(Analog, Drift) are named here and specced in depth later.

r[signal.soundsource.module]
A Soundsource exposes an ordered set of source modules, each independently
enabled. A module operates on the source signal/playback in place; disabled
modules are bypassed with no cost. Every module parameter is enumerable via
`params()` (`signal.soundsource.params`).

r[signal.soundsource.module.wave]
**Wave** is the pitch/playback module: **Transpose** (semitones), **Coarse** and
**Fine** tune, a **Key Tracking** on/off (off = fixed pitch across the keyboard,
for percussion/FX), and **Reverse** (play the source backwards). Wave is the base
tuning every soundsource has.

r[signal.soundsource.module.analog]
**Analog** models analog-oscillator character (drift/instability, waveform
warmth). Named here; parameters specced later.

r[signal.soundsource.module.drift]
**Drift** applies slow, per-voice random pitch/timbre movement for
analog-style liveliness. Named here; parameters specced later.

r[signal.soundsource.module.unison]
**Unison** stacks detuned copies of the source for width/thickness: a voice
**Amount** (count) with **Detune** spread, expressed as **Fine** and **Coarse**
detune. Unison is per-note (each played note gets its own stack).

r[signal.soundsource.module.harmony]
**Harmony** adds pitched copies of the source at musical intervals (a built-in
harmonizer): a set of interval voices with per-voice level/pan, optionally
scale-aware. Distinct from Unison (musical intervals, not detune).

r[signal.soundsource.module.fm]
**FM** frequency-modulates the source by a modulator (ratio + depth), for
metallic/bell/aggressive timbres.

r[signal.soundsource.module.ring]
**Ring Modulator** multiplies the source by a carrier (ratio + mix) for
inharmonic/metallic tones.

r[signal.soundsource.module.waveshaper]
**Waveshaper** applies a nonlinear transfer curve (drive + curve selection) for
harmonic saturation/distortion of the source.

r[signal.soundsource.module.granular]
**Granular** resynthesizes the source as a cloud of grains: grain size, density,
position (with spray), pitch, and window — for time-stretch, textures, and clouds.
It is a first-class source module, not an FX.

## Physical Modeling

r[signal.soundsource.physical]
The Physical Modeling kind generates sound by simulating a physical instrument:
an **excitation** (hammer, bow, pluck, breath) driving a **resonant model**
(strings, tube, membrane, body/soundboard). It is note-triggered and polyphonic
like the Oscillator kind, but its timbre emerges from the model and the playing
dynamics rather than stored waveforms — giving true continuous sustain,
sympathetic resonance, and note-to-note interaction a fixed sample cannot. A
physically-modeled piano (hammer → string → soundboard) is the reference case.

r[signal.soundsource.physical.hybrid]
A Physical Model MAY be **sample-excited / hybrid**: real recordings (e.g.
Keyscape samples) excite or calibrate the resonant model, combining sampled
realism with modeled continuity (release/pedal resonance, duplex-scale
sympathetics, un-looped infinite sustain). The model and a Sample soundsource
can coexist in one Layer.

## Sample

r[signal.soundsource.sample]
The Sample kind plays a multisample library: a keymap of **zones**, each a
region of the (key × velocity) space bound to an audio file with root key,
tune, gain, pan, loop, sample window, and trigger mode. Two mapping worlds are
supported: **zone mode** (keymap is explicit data; filenames arbitrary — e.g.
Spectrasonics) and **convention mode** (filenames encode the keymap — e.g. CSS).

r[signal.soundsource.sample.zone-carries-all]
One logical zone carries *all* of its round-robins, mic positions, and dynamic
layers — the keymap does not explode into N parallel copies. A note selects a
zone, then the engine resolves the round-robin, mic set, and dynamic within it.

r[signal.soundsource.sample.roundrobin]
Zones sharing a (key, velocity) window but differing by round-robin index form a
round-robin set. Cycling is configurable: sequential `cycle`, `random`, or
`no-repeat-random`. Round-robin state is per-note.

r[signal.soundsource.sample.multimic]
A Sample library MAY ship multiple **microphone** positions (e.g. Close / Room /
Stereo / Spot). Each is `{ id, label, kind (blended|separate), default }`. The
Layer provides a per-mic mixer (level, pan, mute, solo); mics load independently
so unused ones cost no memory. Blended mics sum to one output; separate mics can
route to their own buses.

r[signal.soundsource.sample.articulation]
An **articulation** groups zones by playing technique (Sustain, Short/Staccato,
Legato, Release, Trill, Marcato, Pizzicato, …). Articulations are selectable at
play time by keyswitch, a controller (e.g. CC58), velocity threshold, or a fixed
default. Switching an articulation changes which zones a note selects, not the
routing.

r[signal.soundsource.sample.groups]
Three organizational kinds nest and carry meaning: **Articulation** (switched),
**Variation** (an alternate take set / processed copy, e.g. "Mixed"/"Unmixed"),
and **Group** (shares polyphony, choke group, and trigger mode). A zone names its
articulation, variant, group, mic, and dynamic.

r[signal.soundsource.sample.dynamics]
Loudness/timbre dynamics are expressed as either **velocity layers** (a note's
velocity picks the layer) or a **CC crossfade** (a controller — CC1 by default —
morphs continuously between dynamic layers ppp…fff). A library declares which,
per articulation.

r[signal.soundsource.sample.trigger]
A zone's trigger mode selects when it fires: note-on (attack), one-shot
(ignores note-off), release/key-up, pedal-down/up (CC64 threshold), CC/aftertouch
threshold. Release samples fire on note-off at a trimmed length.

r[signal.soundsource.sample.keyswitch]
Keyswitch notes (below/above the playable range) and controller ranges select
articulations without sounding. The mapping is data; a UI surfaces the current
articulation.

## Legato engine

r[signal.soundsource.legato]
A legato articulation connects consecutive monophonic notes with a **transition**
sample rather than a fresh attack. Transitions are recorded per interval and
direction (up/down) and are **source-labelled** (e.g. "up_C#" is the C#→D#
transition); the engine picks the transition for the played interval and offsets
its pitch so the transition's END lands on the target note.

r[signal.soundsource.legato.velocity-zones]
Legato has velocity-zoned variants trading speed for expression: an
**Expressive** mode (more velocity zones, longer pre-delays — richer transitions)
and a **Low-Latency** mode (fewer zones, shorter pre-delays — tighter response).
Concrete CSS reference: Expressive pre-delays 333/250/100 ms across velocity
0–64/65–100/101–127; Low-Latency 150/100 ms.

r[signal.soundsource.legato.live]
In Live mode legato defaults to a bounded-latency behavior: choose the
Low-Latency transition zone (or trim the pre-delay) so the target note speaks
promptly. The engine never waits on unknown future input; it commits to the
transition as soon as the next note-on arrives.

r[signal.soundsource.legato.offline]
In Offline mode legato uses look-ahead: seeing the next note before it sounds,
the engine schedules the Expressive transition's lead-in *ahead* of the target
so the transition arrives on time, may time-stretch/align the transition to the
inter-onset interval, and picks the most expressive available transition
regardless of pre-delay. This is the "time trickery" that a live stream can't do.

r[signal.soundsource.legato.mono]
Legato is monophonic per voice-line: a new note within the legato window
transitions the held voice (no separate attack, no doubled/detuned body). Same-
note retrigger (a legato re-articulation) and portamento (a pitch slide below a
velocity threshold) are configurable sub-behaviors.

## Audio

r[signal.soundsource.audio]
The Audio kind emits the Layer's **input** as its source — the live guitar DI is
the guitar rig's Layer source, feeding straight into the Layer's filter/amp/FX.
It applies a level; note events are ignored (it is continuous, not note-
triggered). A file variant streams an audio file as the source (cinematic beds,
one-shots, granular fodder).

r[signal.soundsource.audio.unifies-guitar]
Because the guitar DI is just an Audio Soundsource, the guitar rig is a Layer on
the same engine as synths and orchestras — processing (filter/amp/FX) is shared;
only the source differs.
