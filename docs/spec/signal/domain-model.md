# Signal Domain Model

The signal domain manages guitar/instrument rig presets — from individual FX blocks
up through complete performance profiles and setlist configurations.

## Hierarchy

r[signal.hierarchy]
The signal domain is organized as a nested hierarchy where each level composes
the level below it:

```
Block → Module → Layer → Engine → Rig → Profile → Song → Setlist
```

Each level adds a new dimension of organization:
- **Block**: A single FX plugin with parameter state
- **Module**: An ordered chain of blocks (e.g., "Drive Module" = boost + overdrive + EQ)
- **Layer**: A track with a complete FX chain (modules + blocks)
- **Engine**: Groups layers with scene-based switching
- **Rig**: Top-level instrument setup with engines and FX sends
- **Profile**: Named collection of patches (tone presets) for a rig
- **Song**: Section-based performance structure with patch assignments
- **Setlist**: Ordered list of songs for a performance

## Blocks

r[signal.block]
A block represents a single FX plugin instance with its parameter state.

r[signal.block.type]
Blocks are categorized by type: Amp, Drive, Reverb, Delay, EQ, Compressor,
Modulation, Chorus, Flanger, Phaser, Cabinet, Gate, Special, Wah, Filter,
Pitch, Volume, Boost, Custom, and others.

r[signal.block.snapshot]
Each block preset contains one or more snapshots (parameter value sets).
The default snapshot is loaded when the block is first instantiated.
Additional snapshots provide alternative settings (e.g., "Clean" vs "Crunch" amp settings).

r[signal.block.state]
Block state can be stored as either binary plugin state data or as
RPP chunk text (for CLAP/VST3 plugins that support chunk-based state).

## Modules

r[signal.module]
A module is an ordered chain of blocks that form a processing stage.
Modules are typed by their position in the signal chain.

r[signal.module.type]
Module types follow the standard signal chain order:
Input → Drive → Pre-FX → Amp → Modulation → Time → Motion → Dynamics → Master.

r[signal.module.container]
In REAPER, modules are represented as Container FX that hold their child
block plugins. The container preserves the module's identity and ordering.

r[signal.module.snapshot]
Module snapshots capture the state of all blocks within the module.
Switching module snapshots swaps all block states atomically.

## Layers

r[signal.layer]
A layer is a REAPER track with a complete FX chain of modules and blocks.
Layers represent a single processing path (e.g., "Clean Guitar", "Drive Guitar").

r[signal.layer.track-prefix]
Layer tracks use the `[L]` name prefix: `[L] Clean Layer`.

r[signal.layer.template]
Layers can be saved as `.RTrackTemplate` files that capture the full
track state including all FX, their parameters, and routing.

## Engines

r[signal.engine]
An engine groups multiple layers with scene-based variant switching.
Each scene selects which layer variant is active.

r[signal.engine.track-prefix]
Engine tracks use the `[E]` name prefix: `[E] Guitar Engine`.

r[signal.engine.type]
Engine types match instrument categories: Guitar, Bass, Vocal, Keys,
Synth, Organ, Pad.

## Rigs

r[signal.rig]
A rig is the top-level instrument setup containing engines and FX sends.
Rigs are represented as folder tracks in REAPER.

r[signal.rig.track-prefix]
Rig tracks use the `[R]` name prefix: `[R] Guitar Rig`.

r[signal.rig.scene]
Rig scenes select which engine scene is active for each engine in the rig.
Scene switching changes the active tone across all engines simultaneously.

r[signal.rig.fx-sends]
Rigs can include FX send tracks for shared effects (reverb, delay) that
multiple layers route to.

## Profiles

r[signal.profile]
A profile is a named collection of patches — preset tone configurations
that can be recalled by index or name.

r[signal.profile.patch]
Each patch in a profile targets a specific level of the hierarchy:
a block snapshot, module snapshot, layer snapshot, engine scene, or rig scene.

r[signal.profile.patch.target]
Patch targets can be:
- `BlockSnapshot` — swap a single block's parameter state
- `ModuleSnapshot` — swap an entire module's state
- `LayerSnapshot` — swap a layer's FX chain
- `EngineScene` — switch an engine's active scene
- `RigScene` — switch the rig's active scene
- `Patch` — cross-reference another patch

r[signal.profile.activation]
Activating a profile patch resolves its target and applies the corresponding
state change to the DAW. For dynamic loading, this modifies FX parameters
in-place. For full-load setlists, this mutes/unmutes track groups.

## Songs

r[signal.song]
A song defines a section-based performance structure. Each section maps
to a patch from a profile, enabling automatic tone changes during playback.

r[signal.song.section]
Sections have a name (Intro, Verse, Chorus, etc.) and a source that
specifies which tone to activate — either a patch reference or a direct
rig scene reference.

## Setlists

r[signal.setlist]
A setlist is an ordered sequence of songs for a performance. Setlists
drive navigation (next/previous song) and can be loaded as a complete
REAPER project with all tracks pre-instantiated.
