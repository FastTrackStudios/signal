# The Live-Rig API — requirements + the dream Rust design

> Status: design experiment, the counterpart to `sampler-trait-design.md`. Signal
> has two domains: **sample instruments** (that doc) and **live amp/FX rigs** (this
> one). This is the most ergonomic API imaginable for building and performing a
> guitar/amp-modeler rig — GigPerformer rackspaces, Helix blocks/snapshots, Neural
> DSP / Tonex captures — running on daw's audio engine (the rig = one input-armed
> track whose FX chain is the active patch; see `src/rig.rs` `GuitarRig`).

Same principle as the sampler: **the common 90% is declarative data; the exotic
10% is a trait.** A patch is a list of blocks; a profile is a list of patches;
performance (footswitch/MIDI/expression → actions) is built-in behavior you
configure. Each block is a `PluginInstance` (NAM amp, cab IR, hosted CLAP/VST3,
built-in DSP) — the *same* host contract the sampler uses, so daw hosts both with
one engine.

---

## 1. Requirements

### A. Blocks (the chain elements)
1. **Typed FX blocks** with a role: Gate, Drive/OD/Fuzz, **Amp** (NAM model /
   capture), **Cabinet** (IR or NAM), EQ, Compressor, Modulation (chorus/phaser/
   flanger), Delay, Reverb, Pitch, Volume/Expression, Utility, and **any hosted
   CLAP/VST3 plugin** as a generic block.
2. **Amp/cab as captures** — load a `.nam` model (with its loudness + expected-SR
   metadata) or a `.wav` IR; swap the model without rebuilding the chain.
3. **Per-block** input/output trim, bypass, mix, and **named, addressable
   parameters** (gain, bass/mid/treble, …) for automation + control binding.

### B. Chain & routing
4. **Ordered chain** (in → gate → drive → amp → cab → mod → delay → reverb → out).
5. **Parallel paths** — split/merge (e.g. two cabs in parallel, wet/dry, dual amps),
   with per-path level + pan.
6. **Per-block bypass** independent of patch switching.

### C. Patch / snapshot / profile (the organization)
7. **Patch (tone)** — a named chain + input/output trim + level-match loudness.
8. **Snapshots** — instant *parameter/bypass states within a patch* (Helix
   snapshots): switch gain/delay-feedback/which-blocks-on without changing the
   chain — zero rebuild, just param sets.
9. **Profile (rackspace set / setlist)** — ordered patches for a song/gig, with a
   default patch.
10. **Instant, click-free patch switching** — pre-load every patch's chain; switch
    is an atomic swap (daw `insert_plugin_instance` under the renderer lock).
11. **Level-matching** across patches from NAM loudness metadata (Clean ≈ Lead).

### D. Performance (live control)
12. **Footswitch / MIDI** — patch up/down, jump to patch N (program change),
    toggle a block's bypass, select a snapshot, **tap tempo** (sets delay times).
13. **Expression pedal / CC** → any named parameter (wah, volume, morph).
14. **Tuner** + input/output **metering**; a **mute/bypass** (clean DI) switch.

### E. I/O & platform
15. **Live input** — pick input device + DI channel; **output** device + channels.
16. **Sample-rate / buffer** selection (low-latency); JACK/PipeWire host.
17. Runs on daw's engine: rig = an input-armed daw track + an FX chain of
    `PluginInstance`s; **no second engine**.

---

## 2. Primitives (shared with the sampler where they overlap)

```rust
// Reused from the sampler API: Db, Cents, Seconds, Frames, Cc, U7, MidiCh, MicId
// (here: output channel ids), and the interned-id pattern.
pub struct PatchId(Interned);        // "Clean", "Lead"
pub struct BlockId(Interned);        // stable id within a patch (for snapshots/bypass)
pub struct ProfileId(Interned);      // "Worship", "Metal Gig"
pub struct SnapshotId(Interned);     // "Verse", "Chorus"
pub struct ParamRef { block: BlockId, param: Interned }   // amp.gain, delay.feedback
```

---

## 3. Blocks — typed FX that are `PluginInstance`s

A block is a DSP unit with a *role* + named params. The DSP is one of: a NAM
model, a cab IR convolver, built-in native DSP, or a hosted plugin — all already
`PluginInstance` (the sampler/daw host contract). The role gives the UI + the
chain semantic meaning; it never changes how daw runs it.

```rust
/// One FX block. `Send` (rides the audio thread). The audio is `PluginInstance`;
/// `Block` adds the rig-domain role + addressable params + trims/bypass.
pub trait Block: Send {
    fn role(&self) -> BlockRole;
    fn id(&self) -> BlockId;
    fn params(&self) -> &[Param];                 // named, for control/automation
    fn set_param(&mut self, p: &str, value: f32);
    fn bypassed(&self) -> bool;
    fn set_bypassed(&mut self, on: bool);
    /// The audio engine drives this — a block IS a plugin instance.
    fn as_plugin(&mut self) -> &mut dyn daw::plugin::PluginInstance;
}

pub enum BlockRole {
    Gate, Drive, Amp, Cabinet, Eq, Compressor,
    Modulation, Delay, Reverb, Pitch, Volume, Utility, Plugin,
}

/// Built-in block constructors — the ergonomic surface. Each returns `impl Block`.
pub mod block {
    pub fn amp_nam(path: &Path) -> Result<Amp>;        // NAM model, carries loudness/SR meta
    pub fn cab_ir(path: &Path) -> Result<Cabinet>;     // .wav impulse response
    pub fn cab_nam(path: &Path) -> Result<Cabinet>;    // neural cab
    pub fn drive(kind: DriveKind) -> Drive;            // built-in OD/fuzz/boost
    pub fn delay() -> Delay;  pub fn reverb() -> Reverb;  pub fn eq() -> Eq;
    pub fn plugin(path: &Path, role: BlockRole) -> Result<Plugin>;  // any CLAP/VST3
}
```

`Amp` exposes `loudness() -> Option<Db>` + `expected_sample_rate()` (NAM metadata)
so the rig can level-match; `swap_model(path)` reloads the capture in place.

---

## 4. Chain — series with parallel paths

```rust
/// The signal graph of a patch. v1: a series spine with optional parallel
/// sections. A `Parallel` node fans the signal to lanes (each a sub-chain) and
/// sums them with per-lane level/pan — dual cabs, wet/dry, two amps.
pub enum Node {
    Block(Box<dyn Block>),
    Parallel { lanes: Vec<Lane>, mix: ParallelMix },   // split → lanes → sum
}
pub struct Lane { pub chain: Vec<Node>, pub level: Db, pub pan: f32 }
pub struct Chain { pub nodes: Vec<Node> }

impl Chain {
    pub fn builder() -> ChainBuilder;                  // fluent: .drive(..).amp(..).cab(..)
    pub fn block(&self, id: BlockId) -> Option<&dyn Block>;
    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut dyn Block>;
}
```

On daw this lowers to the track's FX chain; a `Parallel` lowers to daw sends +
bus tracks summed back (the same primitive the drum-mixer mapping uses).

---

## 5. Patch / Snapshot / Profile

```rust
/// A tone: a chain + I/O trim + level-match metadata + its snapshots.
pub struct Patch {
    pub id: PatchId,
    pub chain: Chain,
    pub input_trim: Db,
    pub output_trim: Db,
    pub snapshots: Vec<Snapshot>,
}

/// An instant state WITHIN a patch — param values + per-block bypass. Switching a
/// snapshot is a handful of `set_param`/`set_bypassed` calls, no chain rebuild.
pub struct Snapshot { pub id: SnapshotId, pub params: Vec<(ParamRef, f32)>, pub bypass: Vec<(BlockId, bool)> }

/// A set of patches (GigPerformer rackspace set / a setlist). Loads as a unit so
/// switching is instant.
pub struct Profile { pub id: ProfileId, pub patches: Vec<Patch>, pub default: usize }

impl Profile {
    pub fn from_styx(path: &Path) -> Result<Self>;     // the existing rig .styx
    pub fn builder(name: &str) -> ProfileBuilder;
}
```

### Building a profile — the ergonomic test

```rust
let worship = Profile::builder("Worship")
    .patch("Clean", |p| p.chain(|c| c
        .amp_nam("amps/AC30.nam")?.cab_ir("cabs/V30.wav")?
        .delay(|d| d.time_ms(380).feedback(0.3).mix(0.18))
        .reverb(|r| r.hall().mix(0.2)))
        .level_match())                                // auto from NAM loudness
    .patch("Lead", |p| p.chain(|c| c
        .drive(DriveKind::Tube).gain(0.7)
        .amp_nam("amps/Soldano.nam")?.cab_ir("cabs/V30.wav")?
        .delay(|d| d.time_ms(440).feedback(0.45).mix(0.25)))
        .snapshot("Solo", |s| s.set("delay.mix", 0.4).set("amp.gain", 0.85)))
    .default("Clean")
    .build()?;
```

---

## 6. The `Rig` trait — the playable live rig (counterpart to `Instrument`)

```rust
/// A live amp/FX rig: DI in → active patch chain → out, with instant switching.
/// `Send`; the chain blocks run inside daw's renderer on an input-armed track.
pub trait Rig: Send {
    // ── Patch / snapshot ───────────────────────────────────────────────
    fn patches(&self) -> &[PatchId];
    fn active_patch(&self) -> usize;
    fn select_patch(&mut self, idx: usize);            // INSTANT, click-free
    fn next_patch(&mut self);  fn prev_patch(&mut self);
    fn select_snapshot(&mut self, id: SnapshotId);

    // ── Live control ───────────────────────────────────────────────────
    fn set_block_bypass(&mut self, block: BlockId, on: bool);
    fn set_param(&mut self, target: ParamRef, value: f32);   // expression/CC bind
    fn tap_tempo(&mut self);                            // → time-block delays
    fn set_bypass(&mut self, on: bool);                 // whole-rig clean DI

    // ── Monitoring ─────────────────────────────────────────────────────
    fn input_peak(&self) -> f32;
    fn output_peak(&self) -> f32;
    fn tuner(&self) -> TunerReading;                    // detected pitch + cents
    fn set_input_trim(&mut self, db: Db);
    fn set_output_trim(&mut self, db: Db);
}
```

Tiny, like `Instrument`. The richness (level-match on switch, the parallel-cab
routing, the per-block params) lives in the patch/chain data + the built-in
behaviors below — not in the trait.

---

## 7. Performance control — footswitch / MIDI / expression

```rust
/// Maps physical/MIDI control onto `Rig` actions. The live-rig analog of the
/// sampler's `PerformanceScript`. Built-ins cover the standard pedalboard; custom
/// is a `impl Controller`.
pub trait Controller: Send {
    fn on_event(&mut self, ev: ControlEvent, rig: &mut dyn Rig);
}
pub enum ControlEvent { Footswitch(u8, bool), Cc(Cc, U7), ProgramChange(u8), Note(Note, Velocity) }

// Built-ins, configured not coded:
PatchStepper::footswitches(up, down)            // 2 switches cycle patches
ProgramChangeMap::patches()                     // PC# → patch
ExpressionBind::new(Cc(11), ParamRef::of("volume.level"))
BlockToggle::footswitch(sw, BlockId::of("delay"))
TapTempo::footswitch(sw)
SnapshotSwitcher::ccs(...)
```

---

## 8. Level-matching, tuner, metering (built-in behaviors)

- **Level-match**: on `select_patch`, the rig reads the active patch's primary
  `Amp::loudness()` and nudges `output_trim` toward a target so patches land at an
  even perceived level (the existing `ProfileRig::set_level_match`).
- **Tuner**: a `Utility` block (or a tap on the input) runs pitch detection; the
  rig exposes the reading; muting the output = silent tuning.
- **Metering**: input peak = the input-armed track's pre-FX tap; output peak = the
  track's post-fader `Meters` cell (daw provides both).

---

## 9. daw integration — one engine, no duplication

```rust
/// A `Rig` is realized as ONE input-armed daw track whose FX chain is the active
/// patch's blocks. This is the existing `GuitarRig` generalized to the `Rig`
/// trait. Patch switch = swap the pre-built block `PluginInstance`s into the
/// track's fixed FX-slot guids (glitch-free under the renderer's per-block lock).
/// A `Parallel` node lowers to daw sends + summed bus tracks.
impl Rig for DawRig {
    fn select_patch(&mut self, idx: usize) {
        // for each slot: daw.insert_plugin_instance(slot_guid, patch.block(i).as_plugin_box())
    }
    // input_peak via the input probe; output_peak via Meters; params via set_param on the live block.
}
// Build: `DawRig::open(&AudioIoPrefs, &Profile)` — armed track + engine, every
// patch's chain pre-instantiated, default patch active. (= today's GuitarRig::open.)
```

---

## 10. The libraries / rigs, written against the API

```rust
// A simple combo: amp + cab, one patch.
let combo = Profile::builder("Combo")
    .patch("Tone", |p| p.chain(|c| c.amp_nam("AC30.nam")?.cab_ir("Greenback.wav")?))
    .build()?;

// A modern metal rig: gate → screamer → high-gain amp → DUAL parallel cabs → delay.
let metal = Profile::builder("Metal")
    .patch("Rhythm", |p| p.chain(|c| c
        .gate(|g| g.threshold_db(-58.0))
        .drive(DriveKind::Screamer).gain(0.4)
        .amp_nam("amps/5153.nam")?
        .parallel(|par| par                         // two cabs, panned
            .lane(|l| l.cab_ir("cabs/V30_L.wav")?.pan(-0.4))
            .lane(|l| l.cab_ir("cabs/Greenback_R.wav")?.pan(0.4)))
        .delay(|d| d.time_ms(300).mix(0.12)))
        .snapshot("Lead", |s| s.set("amp.gain", 0.9).set("delay.mix", 0.3)))
    .build()?;
```

---

## 11. Why this shape

- **One tiny `Rig` trait** is the whole live-performance contract (patches,
  snapshots, params, tap, metering) — the amp-modeler counterpart to `Instrument`.
- **Blocks are `PluginInstance`s** — the exact host contract the sampler + daw
  already use, so a NAM amp, a cab IR, a built-in delay, and a hosted CLAP plugin
  are all just blocks; daw runs them with one engine.
- **Patches/profiles are data** (the existing `.styx`), snapshots are param sets,
  performance is configured controllers — every standard rig is no-code, yet each
  behavior is a replaceable trait.
- **No second engine**: `Rig` → an input-armed daw track + FX chain; parallel
  paths → daw sends/buses. Reuses `GuitarRig`/`AudioEngine`/`insert_plugin_instance`.

### Trait inventory
| Trait | Responsibility | Built-ins |
|---|---|---|
| `Rig` | the playable live rig (patches/snapshots/params/tap/meter) | `DawRig` (= GuitarRig) |
| `Block` | one typed FX in the chain (role + params + audio) | `Amp`(NAM), `Cabinet`(IR/NAM), `Drive`, `Delay`, `Reverb`, `Eq`, `Gate`, `Compressor`, `Plugin`(CLAP/VST3) |
| `Controller` | physical/MIDI control → rig actions | `PatchStepper`, `ProgramChangeMap`, `ExpressionBind`, `BlockToggle`, `TapTempo`, `SnapshotSwitcher` |

Shared with the sampler: `daw::plugin::PluginInstance` (block audio), `Effect`
(built-in DSP blocks), the device-I/O layer (`AudioIoPrefs`), and the daw-project
realization. Next: recast `RigBlock`/`RigPatch`/`RigProfile`/`GuitarRig` as the
default `Block`/`Patch`/`Profile`/`Rig` impls — same way the sampler recasts
`SampleEngine`.
