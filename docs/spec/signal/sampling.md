# Sampling

The data model of a **Sample** [Soundsource](soundsource.md) — how a sampled
instrument (Keyscape, Omnisphere soundsources, drum kits, CSS-class orchestral
libraries) is described. This spec defines the three axes a real library needs:
**Multi-Mics**, **Articulations**, and **SoundLayers**. It refines the sample
requirements in [soundsource.md](soundsource.md) (`signal.soundsource.sample.*`).
Reference types: `features/rigs/synth/proto/` (`SynthZone`, `SynthMic`,
`SynthArticulation`).

## Zones — the atom

r[signal.sampling.zone]
The atom of a sampled instrument is a **zone**: one sample body plus everything
needed to place and play it — key range + root key, velocity window (with
crossfade edges), tuning, gain/pan, sample start, and its sustain loop
(`signal.soundsource.sample.loops`). A zone belongs to exactly one articulation,
one round-robin slot, and — when multi-mic — one mic. Everything else here is a
grouping of zones.

r[signal.sampling.roundrobin]
Zones covering the same key/velocity/articulation MAY form a **round-robin** set,
cycled per repeated note to avoid the machine-gun effect. Cycling is per
(articulation, key) and MAY be random, sequential, or with repeat-avoidance.

## Multi-Mics

r[signal.sampling.multimic]
A library MAY capture each note through several **microphone positions** (e.g.
Close, Room, Hall, Spot). A mic is a named channel with a full parallel set of
zones. It carries a per-mic mixer strip: level, pan, mute, solo, and phase. All
mics of a played note trigger together and sum to the layer output.

r[signal.sampling.multimic.stream]
Mics are independently loadable: a mic MAY be disabled/unloaded to save memory
and streaming bandwidth without disturbing the others (a purge/load state per
mic). A disabled mic contributes nothing and costs no I/O.

## Articulations

r[signal.sampling.articulation]
An **articulation** is a named playing style of the instrument (sustain,
staccato, spiccato, pizzicato, marcato, tremolo, legato, …) — a labelled set of
zones covering the keyboard for that style. An instrument is a set of
articulations sharing one key/velocity space.

r[signal.sampling.articulation.select]
The active articulation is chosen at play time by a **selector**: keyswitches
(notes outside the playable range), a MIDI CC/program, velocity, or a UI/host
control. Selection is a Parameter (`signal.parameter`), so it is modulatable,
macro-drivable, and automatable like any other.

r[signal.sampling.articulation.legato]
A **legato** articulation is special: overlapping notes trigger modelled
transitions rather than independent voices. Per `signal.soundsource.legato`, it is
**live-playable** (bounded latency, real transitions) and, when the engine can see
input ahead (offline render), MAY apply look-ahead time-trickery for smoother
joins — same articulation data, two render modes.

## SoundLayers

r[signal.sampling.soundlayer]
A Sample soundsource MAY stack **SoundLayers** — parallel sampled elements summed
to make one composite sound (Omnisphere-style: an attack transient layered under a
body, or blended timbres). Each SoundLayer is a full zone map (its own
articulations, round-robins, mics, tuning, and gain) and mixes into the source
output with its own level/pan.

r[signal.sampling.soundlayer.vs-layer]
SoundLayers are **inside** one Soundsource (they make a single generator's sound);
instrument **Layers** (`signal.instrument.layer`, A/B/C/D) are outside, each with
its own soundsource, filter, amp, and FX. A SoundLayer does not have its own
filter/amp chain — it feeds the one soundsource's output.

r[signal.sampling.dynamics]
Sustained multi-dynamic content (pp→ff) MAY be a continuous **dynamics** control
that crossfades velocity/dynamic layers under an expression controller (e.g. CC1
or CC11) rather than hard velocity switches, so a held note swells through its
recorded dynamics. The dynamics control is a Parameter.

## Memory & streaming

r[signal.sampling.streaming]
A sampled instrument MAY exceed RAM: zones stream from disk with a preloaded
head, and the loader honors header-only reads (`read_pack_header`) so a library
maps its zones/loops/mics without decoding all audio. Triggering a not-yet-loaded
zone MUST degrade gracefully (the engine preloads on patch load; see the sampler
warmup note).
