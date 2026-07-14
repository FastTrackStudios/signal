# Signal Instrument Engine

The sound-**generating** side of the signal domain: one engine that powers
Synths, Orchestral instruments, Percussion, and Cinematic sounds. They differ
only in what *generates* the audio — the rest (layering, filtering, amp,
envelopes, modulation, FX, multi-mic) is shared. This is the target API; see
`crates/signal/docs/DESIGN.md` for background and
`features/sampler/signal-sampler/docs/SOUNDSOURCE.md` for the generator design.

Sibling of [domain-model.md](domain-model.md): that spec covers the FX/preset
*hierarchy* (Block → … → Setlist); this one covers the *instrument that makes
sound*. They meet at the **Layer**.

## The unified engine

r[signal.instrument]
A single instrument engine hosts every sound-generating rig (Synth, Keys,
Orchestral, Percussion, Cinematic). A concrete instrument is a **Part**: a set
of parallel **Layers** summed to the Part output, plus Part-level modulators and
FX. Two instruments differ only in their Layers' **Soundsources** and parameter
values — never in the routing/rendering machinery.

r[signal.instrument.realtime]
The engine renders on the audio callback with no heap allocation on the hot
path (allocate in `prepare`), no locks in `render`, and no threads spawned from
`render`. Every node is `Send`. Preset/param changes must not require a re-host
of the running graph (see `signal.instrument.control.live`).

r[signal.instrument.platforms]
The engine core (generators, filters, amp, envelopes) MUST build for native,
WASM/AudioWorklet, and embedded `no_std` + `alloc`; platform I/O (audio drivers,
MIDI, file streaming) lives only in adapter crates.

## Layers

r[signal.instrument.layer]
A Layer is a self-contained sub-instance — its own source, filters, amp, FX, and
modulators — of the form: **Soundsource → Filter(s) → Amp → Layer FX**. Layers
are labelled A, B, C, D, … and are extensible beyond four. A Part sums its active
Layers.

r[signal.instrument.layer.zone]
Each Layer has a keyboard **Zone**: a key-range split and a velocity window with
crossfade gains. Incoming notes are filtered + velocity-scaled per Layer before
reaching its source (key splits + velocity layering are a Layer property, not a
Soundsource one).

r[signal.instrument.layer.independent]
Layers are independently editable: a change to one Layer's source, filter,
envelopes, or modulation MUST NOT affect another Layer.

## Soundsources (the generator)

r[signal.soundsource]
The pluggable **generator** of a Layer is a `Soundsource`: it turns note and
parameter events into audio and ignores audio input, except the Audio kind. The
`Soundsource` trait exposes `kind()`, `prepare(sample_rate, block_size)`,
`note_on(note, velocity)`, `note_off(note)`, `render(in, out, events)`, and
`params()`/`set_param()`. It is a refinement of the render-tree leaf so a
`Soundsource` drops into the graph with no new machinery.

r[signal.soundsource.kinds]
Four Soundsource kinds exist:
- **Oscillator** — analog/wavetable synthesis (unison, FM, ring, harmonia).
- **Sample** — multisample playback (zone maps, round-robins, mics, loops):
  Keyscape, Omnisphere soundsources, drum kits, orchestral libraries.
- **Physical Model** — simulated instruments (excitation → resonant model): a
  physically-modeled piano, bowed/plucked strings; MAY be sample-excited/hybrid.
- **Audio** — live input or a file as the source: the **guitar rig** (the DI
  feeds straight into the Layer's filter/amp/FX), plus cinematic beds and
  one-shots.

r[signal.soundsource.audio-input]
`render` receives the Layer's input buffers. Oscillator and Sample soundsources
ignore them; the Audio soundsource emits them (applying its level), so the same
engine hosts a synthesized Layer and a live-DI Layer identically.

r[signal.soundsource.sample.loops]
A Sample soundsource honors per-zone sustain loops. When a source library omits
explicit loop points but carries them in its audio metadata (e.g. Spectrasonics
`STINFO`), the loader recovers them; sustained content MUST loop rather than
decay.

## Multi-mic

r[signal.instrument.multimic]
A Sample soundsource MAY expose multiple microphone positions. Each Layer that
uses a multi-mic source provides a per-mic mixer: level, pan, mute, and solo per
mic. Multi-mic is first-class, not bolted on.

## Filter, amp, FX

r[signal.instrument.filter]
Each Layer has a filter section (up to two stages, series or parallel) with
cutoff, resonance, mode (lowpass/highpass/bandpass/notch), pole count, and a
filter-envelope amount. Cutoff is expressed in Hz.

r[signal.instrument.amp]
Each Layer has an amp stage driven by its amp envelope; the Layer's level sets
its contribution to the Part sum.

r[signal.instrument.fx]
Each Layer, and the Part, has FX racks of fixed slots; each slot realizes a
built-in DSP unit or a hosted plugin. A rack MAY be bypassed as a whole.

## Modulation

r[signal.instrument.modulation]
Modulation is a set of routes: any **source** (Envelope, LFO, Mod Envelope, or a
performance control such as mod wheel, velocity, or aftertouch) drives any
**target** parameter with a signed depth. The engine ticks sources per block and
writes parameter offsets to targets.

r[signal.instrument.modulation.envelope]
An envelope is DAHDSR: delay, attack, hold, decay, sustain, release. Attack,
decay, and release each carry a **power/curve** (segment bend); sustain is a
level. Time knobs span 0–20 s on a logarithmic response. Each Layer has at least
an amp envelope and a filter envelope.

r[signal.instrument.modulation.velocity]
Each envelope has a velocity-sensitivity amount; 50% is the neutral default,
higher increases velocity influence and lower decreases it.

r[signal.instrument.modulation.lfo]
LFOs have a rate (free or tempo-synced), a shape, and retrigger behavior, and are
selectable as modulation sources.

## Control surface (the API the UI edits)

r[signal.instrument.control]
The instrument exposes its state for editing at two altitudes: **global** Live
macros over the whole Part, and **per-layer** parameters. Both are readable and
settable over the rig's service so a detached GUI edits them.

r[signal.instrument.control.globals]
Live global macros are performance controls applied over the loaded patch:
vibrato (rate/depth), filter (cutoff/resonance/envelope), unison (detune/amount),
amp and filter envelopes (ADSR + velocity), ambience (amount/length), tone
(low/mid/high), and effects (on/off + limiter). Bipolar macros are neutral at
their center; unipolar at zero. They offset the patch, never replace it.

r[signal.instrument.control.layer]
Per-layer editing exposes the Layer's source, filter (with a response curve),
amp and filter envelopes (with interactive graphs), LFOs, and modulation routes.
The envelope graph and its knob row edit one shared parameter set (two-way sync).

r[signal.instrument.control.live]
Global and per-layer parameter changes take effect on the next render block via
a live-parameter path — they MUST NOT retrigger held voices or re-host the graph.

r[signal.instrument.control.source]
The Layer's source slot is editable generically: the UI can show the
`SoundsourceKind` and its per-kind parameters, and (later) swap the Soundsource,
without knowing whether it is an Oscillator, Sample, or Audio source.
