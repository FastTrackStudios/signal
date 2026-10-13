# Signal Hierarchy

The composition model of the signal domain: every rig — guitar FX, synth,
sampler, orchestral — is built from the **same** nested primitives. This is the
canonical, backend-agnostic hierarchy; [domain-model.md](domain-model.md) is the
overview map, and the REAPER track/container mapping is one backend of it.

The performance layer that sits above this (Profiles/Stacks, Songs/Setlists) is
specced in [profile.md](profile.md) and [song-setlist.md](song-setlist.md); the
saved-artifact form of any level is a **Preset** (below).

## The chain

r[signal.hierarchy]
Each level composes the level below it, up to a **Preset** — one complete,
recallable instrument sound — then a performance layer (Rig, Profile, Song,
Setlist) organizes Presets for playing:

```
sound build-up      Block → Module → Layer → Engine → Preset     one sound
loaded together     Rig      = many Presets at once              combi / multi
go-to library       Profile  = Presets as patches, in Scenes     + Stacks
song structure      Song     = sections → patches
performance         Setlist  = ordered Songs
```

- **Block** — a single FX/plugin/DSP unit with parameter state.
- **Module** — an ordered chain of blocks (a processing stage).
- **Layer** — one complete signal path: a source + its modules/blocks;
  **nestable** (may contain child layers — an imported preset *is* a Layer).
- **Engine** — groups layers with scene/variant switching.
- **Preset** — a complete, recallable sound: one or more Engines with their
  Layers, sources, and FX. The unit a musician browses, loads, and plays.
- **Rig** — the runtime host that loads several Presets simultaneously and
  combines them (a combi/multi: key splits, velocity layers, stacks) plus shared
  FX sends. Not a fixed sound tier — a container of Presets.
- **Profile** — a curated collection of go-to Presets as **patches**, organized
  into **Scenes** (recallable combinations) and **Stacks** (footswitch
  rotations). See [profile.md](profile.md).
- **Song** / **Setlist** — sections mapping to patches, and an ordered run of
  songs. See [song-setlist.md](song-setlist.md).

r[signal.hierarchy.performance]
Above a single Preset, three artifacts organize Presets for performance and are
themselves [Presets](#presets) (browsable, saveable): a **Rig** (many Presets
loaded and combined at once), a **Profile** (a library of go-to Presets as
patches, arranged in **Scenes** and **Stacks**), and **Songs**/**Setlists** (the
section-driven structure that recalls patches during a performance). A **Scene**
is a recallable snapshot of which Presets/variants are active across the rig; a
**Stack** is a footswitch rotation through patches (see [profile.md](profile.md)).

r[signal.hierarchy.uniform]
This is the **only** routing model. Synth, sampler, and every instrument engine
build their signal chains exclusively from Blocks and Modules — there is no
parallel bespoke routing. An instrument's internal path (Soundsource → Filter →
Amp → FX, see [instrument-engine.md](instrument-engine.md)) IS a Layer whose
stages are Modules of Blocks: the Soundsource is the source, the Filter is a
Block, the Amp is a Block, each FX is a Block. Anything a rig can do, it does by
composing Blocks and Modules.

## Blocks

r[signal.block]
A block represents a single FX/DSP/plugin instance with its parameter state.
Every controllable value it exposes is a [Parameter](parameter.md).

r[signal.block.type]
Blocks are categorized by type: Amp, Drive, Reverb, Delay, EQ, Compressor,
Modulation, Chorus, Flanger, Phaser, Cabinet, Gate, Special, Wah, **Filter**,
Pitch, Volume, Boost, Custom, and others — including the instrument stages
(Filter, Amp, and each source module) that synths/samplers use.

r[signal.block.snapshot]
Each block preset contains one or more **snapshots** (named parameter value
sets). The default snapshot loads on instantiation; others provide alternatives.
A filter block's Lowpass / Highpass / Bandpass / Notch / Specialty settings are
snapshots (block presets) of the one Filter block — not distinct block types.
Switching a snapshot swaps the block's parameter state atomically.

r[signal.block.state]
Block state is stored as either binary plugin state or as chunk text (for
CLAP/VST3 plugins that support chunk-based state); native DSP blocks store their
parameters directly.

## Modules

r[signal.module]
A module is an ordered chain of blocks forming one processing stage.

r[signal.module.type]
Module types follow the signal-chain order: Input → Drive → Pre-FX → Amp →
Modulation → Time → Motion → Dynamics → Master. An instrument Layer's stages
(source, filter, amp, FX rack) are modules in this same ordering.

r[signal.module.snapshot]
A module snapshot captures the state of all blocks within it; switching a module
snapshot swaps every child block's state atomically.

r[signal.module.container]
A backend MAY represent a module as a container that holds its child blocks
(REAPER: a Container FX), preserving the module's identity and block ordering.

## Layers

r[signal.layer]
A layer is one complete signal path — a source plus a full chain of modules and
blocks (e.g. "Clean Guitar", or a synth's Layer A). It is the meeting point with
the [instrument engine](instrument-engine.md): an instrument Layer additionally
carries a [Soundsource](soundsource.md) as its source and a keyboard zone.

r[signal.layer.nest]
A layer is **nestable**: it either holds a Soundsource directly (a leaf) or
contains **child layers** (a group/multi), recursively. A single Omnisphere-style
preset is imported as *one* Layer, which may itself nest sub-layers — its A/B/C/D
parts, or a full multi — so "an external preset is a Layer" holds whether that
preset is one sound or many. Child layers each keep their own zone, source,
filter/amp/FX, and modulation (`signal.instrument.layer.independent`) and sum to
their parent's output.

r[signal.layer.template]
A layer is savable as a self-contained template capturing its full state — all
modules, blocks, parameters, source, and routing (REAPER: `.RTrackTemplate`).

## Engines

r[signal.engine]
An engine groups multiple layers with **scene**-based variant switching; each
scene selects which layer variant is active.

r[signal.engine.type]
Engine types match instrument categories: Guitar, Bass, Vocal, Keys, Synth,
Organ, Pad.

## Rigs

r[signal.rig]
A rig is the runtime host that loads **one or more Presets simultaneously** and
combines them — key splits, velocity layers, and stacks — plus shared FX sends.
A rig is itself saveable (a Rig/Rack Preset).

r[signal.rig.scene]
A rig **scene** is a recallable snapshot of which Presets/variants are active
across the rig, so one switch changes the whole combination at once.

r[signal.rig.fx-sends]
A rig MAY include FX send buses for shared effects (reverb, delay) that multiple
Presets/layers route to.

## Presets

r[signal.preset]
A **Preset** is a complete, recallable instrument sound — one or more Engines
with their Layers, sources, filters, amps, and FX. It is the primary unit a
musician browses, loads, and plays, and the primary artifact the
[browser](browser.md) indexes.

r[signal.preset.component]
Each lower level is independently saveable as a **component preset** — Block
preset, Module preset, Layer preset, Engine preset — reusable across sounds. A
higher level references a component preset by id + selected snapshot rather than
copying its state, so editing the component updates every user (unless detached).

r[signal.preset.kind]
A preset's kind is one of: Block (with block type), Module, Layer, Engine, Rig,
Profile, Song, Rack — matching the hierarchy. The complete-sound Preset a musician
loads is realized as an **Engine** preset (single) or a **Rig/Rack** preset
(multi — several Presets combined). Layer-and-above presets are instrument-scoped
(stored per instrument); Blocks and Modules are shared across instruments.

r[signal.preset.multi]
A Rig/Rack preset holds **multiple Presets loaded at once**, each with its
key/velocity zone, level, and stack assignment — the combi/multi. Loading it
instantiates all member Presets together; a rig MAY load any number of Presets
simultaneously with no fixed cap.

r[signal.preset.sidecar]
Every preset carries a sidecar (`.signal.styx`) with a stable UUID, its kind,
descriptive **tags**, a description, and a display parameter list. The sidecar is
what the [browser](browser.md) indexes; it is readable without loading the
preset's payload.

r[signal.preset.snapshot-relation]
A Block/Module preset MAY hold multiple snapshots (`signal.block.snapshot`,
`signal.module.snapshot`); the preset is the file, the snapshots are its selectable
states. Higher levels reference presets + a selected snapshot rather than copying
state.
