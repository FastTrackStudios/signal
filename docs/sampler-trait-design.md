# The Sampler API — requirements + the dream Rust design

> Status: design experiment. Goal — define the *traits* (the contract) for a
> KODA-class sampling platform with the most ergonomic Rust API imaginable, so
> drum kits (Modern, Massive 2), orchestral libraries (Cinematic Studio Strings),
> and piano instruments (Keyscape-style) are all *implementations against one
> beautiful surface*. The runtime sits on daw's audio engine (each instrument is
> a `PluginInstance`; a multi is a daw project) — see `DOMAIN.md` and
> `[[facet-monorepo-migration]]`.

The guiding principle: **the common 90% is declarative data; the exotic 10% is a
trait you implement.** Loading a normal library is a builder + a folder scan.
Legato, keyswitches, mic mixing, round-robin, dynamics crossfade are *built-in
behaviors you configure*, not code you write — but every one of them is a public
trait you can replace.

---

## 1. Requirements

### A. Mapping & zones (developer)
1. **Multi-layer zones** — one zone carries *all* its round-robins, mic positions,
   and dynamic layers, instead of exploding into N groups (KODA's headline).
2. **Auto-map from filename tokens** — `Vln_Sus_mf_RR2_Close_C3.wav` →
   `{section, articulation, dynamic, rr, mic, note}` without hand-mapping.
3. **Zone metadata** — key range, velocity range, root key, fine-tune, gain, pan,
   loop points, sample start/end, trigger mode (note-on / release / legato),
   choke group, exclusive group/polyphony.
4. **Two mapping worlds** — *convention* (filenames encode the keymap, e.g. CSS)
   and *zone/metadata* (keymap is data, filenames arbitrary, e.g. Spectrasonics).

### B. Hierarchy & organization (developer)
5. **3 group kinds — Articulation, Variation, Group** — nested, giving both
   organizational and functional meaning (an Articulation switches by keyswitch;
   a Variation is an alternate take set; a Group shares polyphony/choke/trigger).
6. **Templates** — instrument + script templates to start from.
7. **Developer gates** — control exactly what the performer can see/change.

### C. Voicing & musicality (engine)
8. **Velocity layers** + **dynamics crossfade** (e.g. CC1 morphs between sustain
   layers continuously — CSS, orchestral).
9. **Round-robin** — cycle, random, with reset-on-silence; per-group RR state.
10. **True legato** — recorded note→note transition samples, monophonic legato
    zones, up/down direction, **with every timing parameter a curve over velocity
    (and interval), not a constant**: different velocities select different
    transition recordings *and* different portamento / pre-delay / crossfade times
    (CSS: soft+slow = long expressive delay, hard+fast = tight short delay; bigger
    leaps glide longer).
11. **Release triggers** — release samples scaled by held duration (piano, plucks).
12. **Sustain pedal (CC64)** + half-pedal, pedal-noise samples, **string/sympathetic
    resonance** (piano).
13. **Choke groups** (hi-hat open→closed), exclusive groups, per-group polyphony.
14. **Keyswitches** + **CC mapping**, both **re-assignable** by the performer.

### D. Mics, mixing, multi (performer)
15. **Mic positions** — each renders its own stereo stream; load/unload mics to
    trade RAM for flexibility.
16. **Dedicated mic mix** — per-mic gain/pan/mute/solo, mic→bus routing, bus FX.
17. **Multi-timbral rack** — many instruments at once, per-slot MIDI filter + quick
    controls + mix strip.
18. **Synchronised delay compensation** across all instruments in a multi.
19. **Performance scripts for multis** — layering/splits/round-robin-across-patches.

### E. Modulation, DSP, scripting (developer)
20. **Global modulators** (LFO / envelope / CC-follower / random) with drag-and-drop
    assignment to any parameter.
21. **Built-in effects + custom DSP** — insert on zone / group / mic / instrument /
    bus, with a scripting hook for novel processors.
22. **Performance scripting framework** — transform incoming MIDI before voicing
    (the legato/keyswitch/humanize logic), "vibe-coder friendly."

### F. Platform
23. **Account-based authorization** + library packaging/licensing.
24. **Runs on daw's audio engine** — instrument = `PluginInstance`, mics = tracks,
    multi = project; **zero second engine**.
25. **GUI is data** — a WYSIWYG-designed view binds to named parameters (out of
    scope for this trait doc, but parameters must be *addressable by name*).

---

## 2. Primitives — make illegal states unrepresentable

```rust
/// Domain newtypes. Cheap, Copy, and they make every signature self-documenting.
/// `U7`/`U14` enforce MIDI ranges at the type level; constructors clamp.
pub struct Note(pub u8);            // MIDI key 0..=127
pub struct Velocity(pub U7);
pub struct Cc(pub U7);              // controller number
pub struct U7(u8);                  // 0..=127, clamping ctor
pub struct U14(u16);                // pitch bend
pub struct MidiCh(u8);              // 0..=15
pub struct Db(pub f32);            // gain in decibels (0.0 = unity)
pub struct Cents(pub f32);
pub struct Seconds(pub f64);
pub struct Frames(pub u64);

/// Interned, Copy ids — compare/hash by integer, print by name. No String in the
/// hot path; authoring uses `&str` and interns once.
pub struct MicId(Interned);         // "Close", "Room", "Decca Tree"
pub struct ArticulationId(Interned);
pub struct GroupId(Interned);
pub struct ZoneId(u32);
pub struct InstrumentId(Interned);

/// A continuous performance axis (CC1 dynamics, velocity, mod wheel, custom).
/// Zones declare which axis selects/crossfades them — not hardcoded to "CC1".
pub enum Axis { Velocity, Cc(Cc), ChannelPressure, Custom(Interned) }
```

Everything below is generic over these. A function that takes `Note` and
`Velocity` can never be called with them swapped.

---

## 3. The model — a declarative instrument tree

The library *is* data. Build it with a fluent builder or scan it from disk; the
engine never needs bespoke code for a normal instrument.

```rust
/// The whole virtual instrument: a tree of articulations, an output set of mics,
/// the axes that drive it, and its performance config. Pure data — `Send + Sync`,
/// cheaply cloneable handles to the (Arc'd) sample pool.
pub struct InstrumentModel {
    pub id: InstrumentId,
    pub mics: Vec<Mic>,                 // each mic = its own output stream
    pub dynamics: Option<Axis>,         // e.g. Cc(1) — crossfades dynamic layers
    pub articulations: Vec<Articulation>,
    pub keyswitches: KeyswitchMap,      // re-assignable
    pub controls: ControlMap,           // named, performer-facing params
    pub sample_rate: u32,
}

/// A playing technique, selected by keyswitch / CC / program. KODA "Articulation".
pub struct Articulation {
    pub id: ArticulationId,
    pub select: Select,                 // Keyswitch(Note) | Cc(Cc, range) | Always
    pub variations: Vec<Variation>,     // RR-sets, sul tasto vs normale, …
    pub legato: Option<Legato>,         // None = polyphonic; Some = mono+transitions
    pub release: Option<ReleaseTrigger>,
}

/// An alternate take-set within an articulation. KODA "Variation".
pub struct Variation { pub id: GroupId, pub select: Select, pub groups: Vec<Group> }

/// The functional leaf bundle: zones that share polyphony / choke / trigger /
/// round-robin behavior. KODA "Group".
pub struct Group {
    pub id: GroupId,
    pub trigger: Trigger,               // NoteOn | Release | Legato
    pub polyphony: Polyphony,           // Unlimited | Voices(n) | Mono
    pub choke: Option<GroupId>,         // hi-hat open choked by closed
    pub round_robin: RoundRobin,        // Cycle | Random | Off, + reset policy
    pub zones: Vec<Zone>,
}

/// One multi-layer zone: a key×velocity rectangle that already contains its RRs,
/// mics, and dynamic layers (the headline). The engine fans a hit into one voice
/// per (active mic × selected dynamic-layer × chosen RR).
pub struct Zone {
    pub keys: RangeInclusive<Note>,
    pub vel: RangeInclusive<Velocity>,
    pub root: Note,
    pub tune: Cents,
    pub gain: Db,
    pub pan: f32,
    /// Layers indexed by (mic, dynamic, rr) → a sample slice. This is what makes
    /// it "multi-layer": one struct, every render of this region.
    pub layers: ZoneLayers,
}

/// Resolve a sample for a coordinate. The convention vs zone-mode distinction
/// lives behind this — both implement the same `get`.
pub trait ZoneLayers: Send + Sync {
    fn sample(&self, mic: MicId, dynamic: DynLayer, rr: u32) -> Option<&SampleSlice>;
    fn dynamics(&self) -> &[DynLayer];   // declared crossfade layers, low→high
    fn round_robins(&self) -> u32;
}
```

### Authoring — auto-map then refine

```rust
// Convention library (CSS): tokens parse the keymap straight off the filenames.
let css = InstrumentModel::scan("…/CSS/Violins")
    .tokens("{section}_{artic}_{dyn}_RR{rr}_{mic}_{note}")
    .mics(["Close" => "C", "Main" => "M", "Room" => "R"])
    .dynamics_on(Axis::Cc(Cc(1)))                 // CC1 morphs p→mf→f
    .articulation("Sustain",  Select::keyswitch(C(-1)))
    .articulation("Staccato", Select::keyswitch(Db(-1)))
    .articulation("Legato",   Select::keyswitch(D(-1)).legato(Legato::true_(/*…*/)))
    .build()?;

// Zone-mode library (Spectrasonics-style): filenames are arbitrary; hand the
// keymap as data, the builder skips filename parsing.
let omni = InstrumentModel::from_zones(zones).mics(["Main"]).build()?;
```

The builder is *total*: every method has a sane default, so the minimal instrument
is `InstrumentModel::scan(dir).build()?`.

---

## 4. The playable trait — what the user called "the SamplerPlayer trait"

This is the contract a *runtime* instrument satisfies. One built-in engine
(`SampleEngine`) implements it for any `InstrumentModel`; you only write your own
if you're doing something the data model can't express.

```rust
/// A live, voiced instrument. `Send` so it rides daw's audio thread inside a
/// `PluginInstance`. Everything is realtime-safe: no alloc, no lock, no IO in
/// `note_on`/`render` (samples are pre-streamed; see `Loader`).
pub trait Instrument: Send {
    // ── Performance input ──────────────────────────────────────────────
    fn note_on(&mut self, note: Note, vel: Velocity);
    fn note_off(&mut self, note: Note);
    fn note_off_vel(&mut self, note: Note, vel: Velocity) { self.note_off(note) }
    fn control(&mut self, cc: Cc, value: U7);
    fn pitch_bend(&mut self, bend: U14) {}
    fn aftertouch(&mut self, note: Note, pressure: U7) {}
    fn channel_pressure(&mut self, pressure: U7) {}
    fn all_notes_off(&mut self);
    fn panic(&mut self) { self.all_notes_off() }

    // ── Articulation / keyswitch ───────────────────────────────────────
    fn articulations(&self) -> &[ArticulationId];
    fn select_articulation(&mut self, id: ArticulationId);
    fn active_articulation(&self) -> ArticulationId;

    // ── Render (multi-mic) ─────────────────────────────────────────────
    /// Fill one block. Writes a stereo buffer *per active mic* — the host mixes
    /// or routes each mic to its own daw track. Realtime-safe.
    fn render(&mut self, block: &mut MicBlock);
    fn mics(&self) -> &[MicId];
    fn set_mic_active(&mut self, mic: MicId, on: bool);   // purge/load to trade RAM

    // ── State the host needs ───────────────────────────────────────────
    fn voices_active(&self) -> usize;
    fn latency(&self) -> Frames;                          // for delay comp
    fn sample_rate(&self) -> u32;
}
```

That trait is *the* business-logic seam. Note how small it is: notes in, mics out,
articulation control, plus the metadata a host needs. Everything rich
(round-robin, dynamics crossfade, legato, release, choke) happens *inside*
`render`, driven by the model and the two engine traits below.

### The render buffer — mics are first-class

```rust
/// One block of per-mic stereo output. The host (a daw track group) maps each
/// mic to a track; the mic mixer (§6) sums them when a single output is wanted.
pub struct MicBlock<'a> { frames: usize, mics: &'a mut [(MicId, StereoBuf<'a>)] }
impl MicBlock<'_> { pub fn mic_mut(&mut self, id: MicId) -> Option<&mut StereoBuf>; }
```

---

## 5. The two engine extension traits — where the 10% lives

The default `SampleEngine` composes these with built-in impls. Swap one to get a
custom instrument *without* rewriting the host or the model.

```rust
/// Turn one note event (after performance scripting) into voices, selecting zones
/// across mics/dynamics/RR. The default impl handles velocity layers, CC-axis
/// dynamics crossfade, round-robin cycling, and per-group polyphony/choke. You
/// override this only for genuinely novel selection (e.g. a granular re-pitcher).
pub trait Voicer: Send {
    fn voices_for(&self, ev: &VoiceRequest, perf: &PerfState, alloc: &mut VoicePool);
}

/// Transform incoming MIDI *before* voicing. THIS is the scripting framework:
/// keyswitch routing, CC→articulation, true-legato detection, humanize, velocity
/// curves, round-robin-reset, chord splits. Built-ins are values; custom is a
/// type. Pure (no audio), so it's trivial to test and hot-swap.
pub trait PerformanceScript: Send {
    /// Return zero or more `Action`s (play/stop/articulate) for one raw message.
    fn on_message(&mut self, msg: MidiMessage, perf: &mut Performance);
    /// Optional per-block tick for time-based behavior (arps, ramps).
    fn tick(&mut self, _frames: usize, _perf: &mut Performance) {}
}

/// `Performance` is the script's command surface + shared state (held notes, last
/// note for legato, current articulation, RR counters). Scripts compose: a
/// pipeline runs them in order, each seeing the prior's edits.
pub struct Performance<'a> { /* held notes, legato target, articulation, … */ }
impl Performance<'_> {
    pub fn play(&mut self, note: Note, vel: Velocity);
    pub fn legato_to(&mut self, from: Note, to: Note, vel: Velocity);
    pub fn stop(&mut self, note: Note);
    pub fn set_articulation(&mut self, id: ArticulationId);
    pub fn held(&self) -> &[Note];
}
```

### Built-in scripts (configured, not coded)

```rust
// These ship in the box; the model wires them from declarative config.
KeyswitchRouter::from(&model.keyswitches)   // C-1→Sustain, C#-1→Staccato, …
CcArticulation::new(Cc(58), ranges)         // CC58 picks articulation
TrueLegato::new(legato_cfg)                 // reads the per-velocity Legato (§ below)
RoundRobinReset::on_silence(Seconds(0.5))
SustainPedal::new(Cc(64))                   // hold + half-pedal + pedal-noise group
VelocityCurve::cubic(0.7)
Humanize { timing_ms: 4.0, vel: 6 }

// Compose into a pipeline. Custom user logic is just another element.
let script = ScriptPipeline::new()
    .then(KeyswitchRouter::from(&model.keyswitches))
    .then(TrueLegato::new(legato_cfg))
    .then(SustainPedal::new(Cc(64)))
    .then(MyCustomArp { /* impl PerformanceScript */ });
```

### Legato, in depth — the marquee behavior (velocity-/interval-parameterized)

CSS-style legato is the proof the model has enough depth. A fast move and a slow
move use **different transition recordings AND different timings**; a wide leap
glides longer than a step. None of that is a constant — every timing is a *curve*.
The data captures it; the built-in `TrueLegato` script reads it; you never write
legato code for a normal library.

```rust
/// Everything about an articulation's legato. Note every timing is a `VelCurve`,
/// not an `f32` — that's what makes per-velocity portamento/transition timing
/// possible (your CSS requirement).
pub struct Legato {
    /// Recorded note→note transition samples, keyed by direction + interval +
    /// velocity-layer + mic. `None` = synthetic pitch-glide only (no recorded
    /// bow/breath transitions). Different velocities pick different recordings.
    pub transitions: Option<Box<dyn TransitionLayers>>,
    /// Delay before the *target* note speaks — CSS "delayed legato". A curve:
    /// soft/slow → longer delay (expressive), hard/fast → short (responsive).
    pub pre_delay: VelCurve<Seconds>,
    /// Pitch-glide time, by velocity, optionally scaled by leap size.
    pub portamento: VelCurve<Seconds>,
    /// Crossfade from the previous note's tail into the transition/target.
    pub crossfade: VelCurve<Seconds>,
    pub mode: LegatoMode,                 // Mono | PolyLegato
}

/// Pick the transition sample for one legato move — the role `ZoneLayers` plays
/// for sustains, but over the *move* (from→to) rather than the held note. The
/// velocity layer here is independent of the sustain dynamic layer.
pub trait TransitionLayers: Send + Sync {
    fn sample(&self, from: Note, to: Note, vel: Velocity, dir: Direction, mic: MicId)
        -> Option<&SampleSlice>;
    fn velocity_layers(&self) -> &[VelLayer];   // distinct transition recordings
}

/// A velocity → value curve, O(1) to sample on the audio thread. THIS is the type
/// that lets "different velocities have different portamento / transition times."
/// Build it from breakpoints, a closure, or a constant; optionally scale by the
/// legato interval (leap size).
pub struct VelCurve<T> { /* breakpoints + interpolation + optional interval scale */ }
impl<T: Lerp + Copy> VelCurve<T> {
    pub fn constant(v: T) -> Self;
    pub fn breakpoints(points: impl IntoIterator<Item = (Velocity, T)>) -> Self;
    pub fn from_fn(f: impl Fn(Velocity) -> T) -> Self;
    pub fn scaled_by_interval(self, f: impl Fn(Interval) -> f32) -> Self;
    pub fn at(&self, vel: Velocity, interval: Interval) -> T;     // realtime sample
}
```

`TrueLegato` (built-in `PerformanceScript`): on a legato move it picks the
transition recording via `transitions.sample(from,to,vel,dir,mic)`, waits
`pre_delay.at(vel, interval)`, glides pitch over `portamento.at(vel, interval)`,
and crossfades over `crossfade.at(vel, …)`. All three timings vary per note — no
custom code.

---

## 6. Mics, mixing, delay compensation

```rust
/// A mic position: its id, a default mix strip, and whether it's resident in RAM.
pub struct Mic { pub id: MicId, pub default: MixStrip, pub loaded: bool }

/// Per-mic / per-bus channel strip — the dedicated mic-mix view binds to these.
pub struct MixStrip { pub gain: Db, pub pan: f32, pub mute: bool, pub solo: bool,
                      pub send: Vec<(BusId, Db)>, pub fx: FxChain }

/// Combine an instrument's per-mic output into a routing graph. The *default* impl
/// = sum mics with their strips; the daw-backed impl makes each mic a track and
/// each bus a bus-track (so the mic mix IS the daw mixer — no second mixer).
pub trait MicMixer: Send {
    fn route(&mut self, mics: &MicBlock, out: &mut BusGraph);
}
```

**Delay compensation** is intrinsic: every `Instrument` reports `latency()`
(stream pre-buffer + any look-ahead legato), and the `Rack` (§7) delays every
slot to the max so a multi stays phase-aligned — one number, set once per block.

---

## 7. Modulation & effects — uniform, addressable, hot

```rust
/// Anything that produces a control value over time. LFO/Env/CcFollower/Random/
/// MacroKnob all impl this; drag-and-drop assignment = pushing a `ModRoute`.
pub trait Modulator: Send { fn value(&mut self, ctx: &ModContext) -> f32; }

/// A DSP block. Built-ins (EQ, comp, convolver, delay) + your custom DSP impl the
/// same trait; insert anywhere (zone/group/mic/instrument/bus). Realtime-safe.
pub trait Effect: Send {
    fn prepare(&mut self, sr: f64, max_block: usize);
    fn process(&mut self, buf: &mut StereoBuf);
    fn params(&self) -> &[Param];                 // named, for GUI binding + mod
}

/// The assignment matrix — `source → target × amount`. Targets are *named*
/// `ParamRef`s (instrument.cutoff, mic[Room].gain, fx[2].mix), so the WYSIWYG GUI
/// and the mod matrix address the same parameters by name.
pub struct ModMatrix { pub routes: Vec<ModRoute> }
pub struct ModRoute { pub source: ModulatorId, pub target: ParamRef, pub amount: f32, pub curve: Curve }
```

---

## 8. The multi-timbral Rack

```rust
/// A multi: many instruments, MIDI routing, a mix, delay comp, and multi-level
/// performance scripts (splits/layers across slots).
pub struct Rack {
    pub slots: Vec<RackSlot>,
    pub script: ScriptPipeline,     // rack-level (split/layer/round-robin-patches)
    pub master: MixStrip,
}
pub struct RackSlot {
    pub instrument: Box<dyn Instrument>,
    pub midi: MidiFilter,           // channel, key range, vel range, transpose
    pub strip: MixStrip,            // quick controls
}
impl Rack {
    pub fn route(&mut self, ch: MidiCh, msg: MidiMessage);  // → matching slots
    pub fn render(&mut self, out: &mut BusGraph);           // delay-comp aligned
    pub fn latency(&self) -> Frames { self.slots.iter().map(|s| s.instrument.latency()).max()… }
}
```

---

## 9. Loading & licensing — IO off the hot path

```rust
/// Streams samples so `note_on` never touches disk. Backed by daw's `AudioSource`
/// (mmap WAV / decoded memory) + prefetch — signal owns the *library modeling*,
/// daw owns the *bytes*. One decoder, one mmap path.
pub trait Loader: Send + Sync {
    fn preload(&self, profile: PreloadProfile) -> PreloadStats;   // warm the cache
    fn slice(&self, id: SampleRef) -> SampleSlice;                // realtime handle
}

/// Authorization gate — account-based unlock + per-library license. Checked at
/// load, never in render.
pub trait Authorization: Send + Sync {
    fn authorize(&self, library: &LibraryId, account: &Account) -> Result<License>;
}
```

---

## 10. daw integration — one engine, no duplication

```rust
/// An `Instrument` becomes a daw `PluginInstance` (MIDI in → audio out). Each mic
/// is a daw track; a `Rack` is a daw project (slot tracks + mic buses + master).
/// This is the existing `SamplerInstrument` generalized — see `src/instrument.rs`.
impl daw::plugin::PluginInstance for InstrumentHost {
    fn process_block(&mut self, _in_l, _in_r, out_l, out_r, ev: &PluginEvents) {
        for m in ev.midi { self.instrument.apply(m) }      // notes/cc/keyswitch
        self.instrument.render(&mut self.mic_block);
        self.mic_mixer.route(&self.mic_block, …);          // or one track per mic
    }
}
// Rack → daw project: `SamplerRig::from_rack(rack, prefs)` builds tracks/buses/
// sends exactly like the drum-mixer mapping already does (§ sampler_rig.rs).
```

---

## 11. The three libraries, written against the API

The test of "most ergonomic": each real library is a short, readable build.

### Drum kit (Modern / Massive 2) — one-shots, multi-mic, RR, choke

```rust
let kit = InstrumentModel::scan("…/Massive2")
    .tokens("{piece}_{mic}_V{vel}_RR{rr}")
    .mics(["Close", "OH", "Room"])
    .piece("Kick",   |p| p.trigger(NoteOn).key(C1))
    .piece("Snare",  |p| p.trigger(NoteOn).key(D1).round_robin(Cycle))
    .piece("HatOpen",   |p| p.key(Fs1).choke(group("hat")))
    .piece("HatClosed", |p| p.key(Gs1).choke(group("hat")))   // chokes the open
    .build()?;
// Mix: close mics direct, OH/Room → bus → master (daw sends; see sampler_rig.rs).
```

### Cinematic Studio Strings — articulations, CC1 dynamics, true legato

```rust
let css = InstrumentModel::scan("…/CSS/Violins")
    .tokens("Vln_{artic}_{dyn}_RR{rr}_{mic}_{note}")
    .mics(["Close", "Main", "Mix"])
    .dynamics_on(Axis::Cc(Cc(1)))                       // p↔mf↔f morph
    .articulation("Sustain",  ks(C0))
    .articulation("Spiccato", ks(Cs0).round_robin(Cycle))
    .articulation("Legato",   ks(D0).legato(Legato {
        transitions: Some(css_bow_transitions),                     // recorded, per vel layer
        pre_delay:  VelCurve::breakpoints([(20, ms(120)), (110, ms(20))]),  // delayed-legato
        portamento: VelCurve::breakpoints([(20, ms(90)),  (100, ms(25))])   // slow glides, fast snaps
                       .scaled_by_interval(|i| 1.0 + i.semitones() as f32 * 0.012),  // leaps glide longer
        crossfade:  VelCurve::constant(ms(40)),
        mode: LegatoMode::Mono,
    }))
    .build()?;
// CSS's whole expressive legato — per-velocity transition recordings + per-velocity,
// interval-scaled portamento/pre-delay — is data; `TrueLegato` reads it, no code.
```

### Piano (Keyscape-style) — many vel layers, release, pedal, resonance

```rust
let piano = InstrumentModel::scan("…/Keyscape/Grand")
    .tokens("{note}_V{vel}_{mic}")
    .mics(["Player", "Audience"])
    .velocity_layers(16)
    .release(ReleaseTrigger::scaled_by_hold())          // release samples
    .group("Pedal", |g| g.trigger(Cc(64)).resonance(Resonance::sympathetic()))
    .group("PedalNoise", |g| g.trigger(Cc(64)))         // mechanical noise
    .build()?;
// SustainPedal + Resonance scripts ship built-in; this is pure config.
```

---

## 12. Why this is the right shape

- **One small playable trait** (`Instrument`) is the whole host contract — daw,
  the rack, tests, and the UI all speak it. That's the "SamplerPlayer trait" done
  right: notes in, mics out, articulation control, latency for delay comp.
- **Two extension traits** (`Voicer`, `PerformanceScript`) capture *all* exotic
  behavior; legato/keyswitch/RR/dynamics/pedal are built-in *values* of those.
- **Data-first**: every real library (drums, strings, piano) is a builder + a
  folder scan — no code — yet each behavior is a replaceable trait.
- **Mics, modulation, effects, parameters are all named + addressable**, so the
  WYSIWYG GUI, the mod matrix, and automation bind to the same surface.
- **No second engine**: `Instrument` → `PluginInstance`, `Rack` → daw project.
  daw owns audio I/O, mixing, routing, FX hosting, sample bytes; signal owns the
  *library modeling* and the *voicing/performance intelligence*.

### Trait inventory (the contract to implement against)
| Trait | Responsibility | Built-ins |
|---|---|---|
| `Instrument` | the playable unit (notes→mics) | `SampleEngine` |
| `Voicer` | note→voices (zone/RR/dynamics/poly) | `DefaultVoicer` |
| `PerformanceScript` | pre-voicing MIDI transform | `Keyswitch`, `TrueLegato`, `SustainPedal`, `CcArticulation`, `RoundRobinReset`, `VelocityCurve`, `Humanize` |
| `ZoneLayers` | sample lookup per (mic,dyn,rr) | convention + zone-mode |
| `MicMixer` | per-mic output → routing | sum / daw-tracks |
| `Modulator` | control value over time | LFO/Env/CcFollower/Random/Macro |
| `Effect` | DSP block | EQ/Comp/Convolver/Delay + custom |
| `Loader` | stream bytes (off hot path) | daw `AudioSource` |
| `Authorization` | account/library unlock | account-based |

Next: ratify these signatures, then re-cast the existing `SampleEngine` /
`SamplerInstrument` / `SamplerRig` as the *default implementations* of
`Instrument` / `Voicer` / `MicMixer`, and the drum/CSS/piano specs as data against
the builder.
