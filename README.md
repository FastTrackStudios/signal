# signal

The Signal Suite: the headless audio engine, the instrument rigs, the
sampler, and the FTS plugin set (CLAP/VST3) that ships alongside it.

Split out of the FastTrackStudio monorepo in August 2026.

## Domain model

Signal's vocabulary is small, and almost all of its power comes from one
idea being applied over and over. Read this section before naming
anything.

There are **four kinds of relationship** between domain objects, and
keeping them apart is the whole game:

| | |
|---|---|
| **Composition** | small things are built into bigger things |
| **Containment** | a **Rig** (an instrument) holds the tones you can play on it |
| **Variation** | *any* collection can have named variants |
| **Selection** | performance-time layers that **point at** tones rather than owning them |

### The levels, defined

Five words do most of the work. Learn these first; everything else is
built on them.

**Only a Block does anything.** It is the single leaf of the tree; every
level above it is a *container* that holds other nodes and nothing of its
own. Containers differ in what they **mean**, not in what they **are** —
which is why one recursive type expresses all of them.

| Term | | What it means |
|---|---|---|
| **Block** | leaf | A single-purpose DSP unit. One compressor, one delay, one amp. |
| **Module** | container | Nodes grouped for a purpose — the *Time* module, the *Drive board*. Nests freely: a Module of Modules is still a Module. |
| **Layer** | container | A playable Engine sound. Load as many as you want into one Engine; they stack. |
| **Engine** | container | The **type** of playable thing — Keys, Organ, Pad, Bass. Holds only Layers. |
| **Preset** | container | The whole program — every Engine sounding together. This is what you load and play. |

`EngineType` in `signal-proto` is exactly that list: `Guitar`, `Bass`,
`Vocal`, `Keys`, `Synth`, `Organ`, `Pad`.

The worship keys rig is the model at full stretch — seven Engines, each
holding the Layers that give it sound:

```
Preset "Worship"
├─ Engines                       (parallel — they all sound at once)
│  ├─ Keys   → Keys 1 · Keys 2 · Keys 3    the piano stack
│  ├─ Pad    → Pad · Shimmer               the wash, and its sparkle
│  ├─ Organ  → Organ A · Organ B           drawbar upper / lower
│  ├─ Bass   → Bass                        its own register, its own tail
│  ├─ Aux    → Synth 1 · Synth 2           whatever the song needs
│  ├─ Drone  → Drone                       the bed a moment sits on
│  └─ SFX    → SFX A · SFX B               risers, impacts — fired
└─ Global    → master FX tail
```

An Engine is a *kind* of part, so "Keys" holds no piano — **Keys 1**
holds the piano. A Layer can load more than one sound: the Pad engine's
*Shimmer* lane stacks a men's and a women's choir, which is why that wash
sounds vocal rather than bright.

The worship guitar rig uses the same words for a completely different
shape — one tone at a time, so it is Modules all the way down (*Front of
chain*, *Drive board*, *Amp*, *Time*) with no Engines or Layers at all.

`cargo run -p signal-sampler --example domain_tour` builds both rigs and
prints them side by side.

### Leaf and container, in the code

Because only Blocks do anything, the audio tree needs exactly two cases:

```rust
RigNode = Block { block }          // the leaf
        | Container { container }  // everything else   ← recursive
```

A `Container` carries a `Role` — `Preset`, `Engine`, `Layer`, `Module` —
and that role is **a label**. It names what the node means to a player
and tells a UI how to draw it. The audio behaviour comes from `Combine`:
`Serial` chains the children, `Parallel` sums them. Nothing in the
renderer branches on `Role`.

That separation is load-bearing. A keys Engine is `Serial` so its FX can
sit after the sum, but the Layers inside it hang off a `Parallel` bag so
they *stack* — a serial Engine would let the last lane overwrite the ones
before it. Same Role, different Combine, completely different instrument.

### One definition of each idea

`Role`, `Combine`, `Zone` and a node's settings each used to exist twice —
once in `signal-proto` for the domain, once in `signal-sampler` for the
audio — defined identically and kept in step by hand. They are now defined
once, in `signal-proto`, and re-exported by the sampler:

| the idea | where it lives |
|---|---|
| `Role`, `Combine` | `signal_proto::node` |
| `Zone`, `Setting`, `AudioSend`, `ModRoute` | `signal_proto::node_routing` |

The rule that decides this: **if a player sets it, it is domain.** A Layer's
key split and its fader are things someone dials in and expects to still be
there tomorrow, so they belong in the wire contract beside `Node`, not in the
crate that renders them.

So a `Node` carries the whole node, not just its shape — `input_db` /
`output_db` (the fader), `zone` (which notes reach it), `settings`, `sends`,
`mod_routes`, `modulators`, `bypassed`, and `engine_type` (which kind of
playable thing it is, when its Role is Engine). `signal_sampler::from_node`
turns a resolved tree into a `Container` tree carrying all of it; `to_chain`
remains as the flat-list shortcut, exact only for a serial chain.

One thing crosses that boundary unconverted: the domain addresses a send or
a mod route by **id**, the renderer still by **display name**, so the
conversion translates. Until the renderer takes ids, renaming a block can
still miss a route on the audio side.

### Ranges: what a parameter's number means

A parameter's stored value is a **position**, 0..=1, with no units. That is
the right thing to automate, modulate, learn a MIDI CC onto and save —
every one of those wants a single dimensionless number.

It is the wrong thing to hand a DSP, which wants 5500 Hz, and the wrong
thing to show a player. A `ParameterRange` is the missing half: the span,
the taper and the unit. `BlockParameter::ranged(id, name, real, range)`
takes the real value and stores the position; `param.real()` gives it
back.

**Taper is not decoration.** A frequency knob with a linear taper is
unusable — half its travel sits above 10 kHz. Frequency, time and Q are
heard as ratios and want `Logarithmic`, where the midpoint is the
geometric mean; dB is already logarithmic in the ear and wants `Linear`.
And because a position means whatever its range says, changing a taper
later *moves* every stored value of that parameter. It has to be right
before values are stored.

Where a range comes from, in order:

1. **The DSP itself.** Every native block declares its parameters'
   min/max, so `native::range_of` reads them off one cached instance per
   block type — 800-odd parameters across 22 types, never a copy that can
   drift.
2. **Declared by hand**, for the control-rate blocks (envelope, LFO,
   arpeggiator) that have no instance to ask, against the code that reads
   them.
3. **Nowhere.** A hosted plugin's parameters are the plugin's, readable
   only once it is loaded. Those values are carried verbatim rather than
   guessed into the unit interval, which would clamp 2.5 Hz to 1.0.

The one thing that cannot be inferred is what a name means: an envelope's
`attack` is in **seconds** and a compressor's is in **milliseconds**. Only
the code reading a value knows what the number is, which is why a range is
keyed by block type and not by parameter name.

### Lifting an audio tree into the domain

`signal_sampler::to_node::lift` takes a `Container` tree — what the rigs'
builders produce — and puts it in a `NodeLibrary`, where it gains stable
identity, variants at every level, overrides, persistence and gapless
recall. `from_node::to_container` renders it back.

That is how both rigs adopted the domain without being rewritten: the
builders still hold the knowledge, and the library is the source of truth.
`KeysProfile::build_library` and the guitar rig's `to_nodes` are the two
entry points, and both are covered by tests that resolve the real rig out
of the library and compare it block for block against what the builder
makes.

A lift reports any parameter whose range it could not determine. It is
empty for the worship keys profile and the Nord reference program; for the
guitar rig it lists exactly the empty drive-board slots, whose `drive`
value reaches no DSP because those block types have none yet.

### Authoring formats stay; there is one domain

Each rig has its own flat, styx-friendly def types — `ProfileDef`,
`PatchDef`, `DrivePresetDef`, `OverrideDef` for the guitar; `KeysProfile`,
`EngineDef`, `LayerDef` for the keys. They look like duplication of the
node model and they are not: they are an **authoring format**, and they
are a good one. A profile is a file a person edits at nine on a Sunday
morning, and a flat list of lanes with patch names beats a graph of
id-referenced nodes for that job every time.

So the decision, stated so it stops being re-litigated: **the def types
stay, and they compile to nodes.** `KeysProfile::build_library` and the
guitar rig's `to_nodes` are the two compilers. What is *not* allowed is a
second model — anything that resolves, overrides, stores variants or
reaches the audio path goes through `Node`.

The one thing a compiler cannot recover is what was never written down:
`engine_type` — which kind of playable thing an Engine is. A container
tree only ever knew that a node *was* an Engine, so `EngineDef` names it
(inferring from the engine's name when unstated).

### The one pattern: a Collection of Variants

This is the part worth internalising, because it repeats at every level.

At each level of composition there is a **collection** — a named,
reusable thing — and the **variants** inside it. A variant is not a copy:
it records *which variant each of its children uses*, plus any parameter
**overrides**. So a variant is a diff against a base, and bases nest.

```
Collection ──┬── Variant "A"   (child choices + overrides)
             ├── Variant "B"
             └── Variant "C"   ← one is the default
```

In code that is `traits::{Collection, Variant, DefaultVariant}`, usually
wired by the `impl_collection!` macro. `Variant::base_ref()` is what it
points at; `Variant::overrides()` is what it changes.

Because the pattern is uniform, **every level gets presets with
variations**:

| Level | The collection | Its variants |
|---|---|---|
| **Block** | a Block Preset — one `BlockType`'s presets | Snapshots |
| **Module** | a Module Preset | Module Snapshots |
| **Layer** | a Layer | Layer Snapshots |
| **Engine** | an Engine | Engine Snapshots |
| **Preset** | the playable tone | Preset Snapshots |

An EQ block preset called "Vocal Air" with Snapshots for "Subtle" and
"Aggressive"; a Module preset "Ambient Wash" whose Snapshots pick
different delay and reverb Snapshots; a Preset whose Snapshots voice the
whole rig "Clean" or "Pushed". Same mechanism, five levels.

### Overrides: reaching down without editing what you reach

This is what makes the variants worth having. **A container can change any
individual parameter of anything beneath it, at any depth, without
modifying the thing it is changing.** The referenced preset stays
untouched and every other user of it is unaffected — the change lives in
the variant that reached down.

An override is a **path** plus an **operation**:

```rust
Override { path: NodePath, op: NodeOverrideOp }
```

The path is typed, segment by segment, and walks the same levels as the
tree — all the way down to **one parameter inside one block**:

```
engine "Pad" → layer "Shimmer" → module "Tone" → block "Hi Cut" → param "freq"
```

**A path is relative, and its segments need not be consecutive.** Each one
matches a *descendant*, nearest first — so `block "Hi Cut" → param "freq"`
reaches that block wherever it sits, and naming the Engine and Layer above
it only narrows which "Hi Cut" is meant when there are several.

That is deliberate, and it was learned the hard way. When a segment had to
match a direct child, an override's survival depended on how deeply the
block happened to be grouped: grouping the guitar chain into Module
containers silently stopped *every* patch override from applying. Nothing
errored — the patches just no longer changed what they promised to change.
Selections and `ReplaceRef` reach the same way, and for the same reason:
all three address a node by id, ids are unique, so none of them needs to
say how deep the node is.

Stop anywhere on that walk and you have a legal target, so the same
mechanism covers every grain:

| Target the path ends on | What you are changing |
|---|---|
| `engine "Pad"` | the whole Engine |
| `layer "Shimmer"` | one Layer of it |
| `module "Tone"` | one Module inside that Layer |
| `block "Hi Cut"` | one Block inside that Module |
| `param "freq"` | **one parameter of that Block** — the finest grain, and the everyday case |

The operations:

| Op | What it does |
|---|---|
| `Set(value)` | Put a parameter at an absolute **normalized position** — see below. |
| `ReplaceRef(id)` | **Swap the preset/variant referenced at this path** — recall an Engine but with a different Layer preset loaded, or a different Module inside one of its Layers. |
| `Bypass(bool)` | Bypass a block or a whole module. |
| `Enable(bool)` | Enable / disable the node. |
| `InsertBefore(id)` / `InsertAfter(id)` | Splice a node into the flow. |
| `Remove` | Take a node out of the flow. |

So "recall this Engine, but swap Shimmer's preset" is one `ReplaceRef` on
`engine "Pad" → layer "Shimmer"`, and "recall it with Shimmer's hi-cut
200 Hz lower" is one `Set` on the full five-segment path. Nothing is
copied either way, and the Shimmer preset itself never changes — which is
the point: the same preset can be loaded in twenty songs, each bending a
different parameter of it, with one thing on disk.

A `Set` carries a position, not a value in the parameter's units, because
that is what the domain stores everywhere — see *Ranges* below. Whatever
authors an override in real units has to convert first; the guitar rig's
patch defs do, and before they did, a delay feedback of 0.4 on a 0..0.95
control came back as 0.38 on every patch that bent a parameter.

**Not every level may do everything.** Which operations are legal depends
on what is carrying the override, enforced by `override_policy`:

| Policy | May do | What it costs |
|---|---|---|
| `SnapshotPolicy` | `Set` only, and only on a parameter | **Nothing.** Values move; nothing is loaded or freed. |
| `ScenePolicy` | `Set`, `ReplaceRef`, `Bypass`, `Enable` | **A load.** `ReplaceRef` changes what is resident. |
| `FreePolicy` | everything | Editing — rewiring is the point. |

#### Snapshot or Scene is a runtime-cost decision

The line between the two is not taste, and it is not really permissions.
It is whether recalling the variant has to **load or unload anything**:

- A **Snapshot** only moves parameter values. Nothing is loaded, nothing
  is unloaded — so recalling one is free and inherently gapless. That is
  why `SnapshotPolicy` allows `Set` and nothing else: the restriction is
  not bureaucracy, it is what makes the guarantee true.
- A **Scene** may change what is loaded. That is the flexibility you want
  — swap the pad for the bridge, drop the organ entirely — and it is not
  free. Something must be brought in or released, and doing that at the
  moment of the switch is a gap the audience hears.

The way out is to pay early. `ProfileRig` **pre-installs every patch's
chain into the rig's resident bank** when the profile loads, so switching
mid-set is a single lock-free atomic — no reload, no dropout. Loading is
an *edit-time* operation; performance-time switching only ever swaps a
pointer. A Scene is gapless exactly when what it can reach is already
resident.

So: reach for a Snapshot when the change is values, and you get gapless
for free. Reach for a Scene when the change is *material*, and budget for
pre-loading it. And note the ceiling that survives either way — no Scene
may rewire the chain, so a Scene fired mid-song can never hand the player
a signal path they have not heard in rehearsal.

#### The exception: a variant that is a load but still a Snapshot

Some Blocks have no parameters to move, because **the model is the
voicing**. A NAM block is the case: switching between captures of the
same amp — every AC30 variant in one Block Preset — is exactly what a
Snapshot is for, and there is no value to set. The change *is* a load.

That does not demote it to a Scene. The rule is:

> A Snapshot must be free to **recall**. When a variant's change is a
> load, the load moves to load time — it does not move the variant to a
> Scene.

Which means the same trick as the patch bank, one level down: **load
every variant of the Block Preset in parallel and keep them resident**,
so selecting one is a swap rather than a fetch. Several AC30 captures sit
in memory together and the Snapshot picks between them atomically.

The rig already has this at *chain* granularity — `install_chain` stores
a prepared chain and hands back a `ModelId` **without activating it**, and
`set_active` swaps atomically between the resident chains. That is what
makes footswitching gapless.

Block granularity needs no extra machinery, which is worth stating
because it looks like it should. A variant selects **which variant each
child uses**, so a chain variant can say "this patch, with the Klon on
its high-gain capture". Chain variants are what the bank installs — so a
capture swap is reached by the same pointer swap as a patch change, and
`signal_sampler::gapless` covers both.

What *is* slow is `set_block_option`, and the reason is that it is a
different kind of act. It rewrites profile state — which capture the
slot holds, for every patch — and that is an **edit**, so it rebuilds
and the code says so ("a full reload (brief gap)"). Performing is
selecting a variant; editing is changing what the variants are. Only the
second needs to cost anything.

#### Where a capture becomes a Preset, and where it becomes a variant

This boundary decides how a library of captures reads, so it is worth
being exact about.

**One captured rig is one Block Preset.** A 1964 AC30 Super Twin through
a Neve and a Royer is a *different preset* from a 1965 AC30 through an
SSL — different year, different unit, different room, different person
holding the mic. You choose between them and you have a favourite. They
are not variants of each other.

**Its settings are the variants.** The same capture session at different
settings — edge of breakup, full, lite, plus whatever block settings you
dial on top — are Snapshots inside that one Preset, pre-loaded together so
you can move between them gaplessly.

On TONE3000 that lands exactly on the catalog's own unit: **a tone is a
Block Preset, its models are the variants.** A tone is one session, one
rig, one creator, and it carries the creator, licence, photographs and
link that the Preset should display.

Do **not** group captures by their `makes` field to collect "all the
AC30s" under one preset. It is wrong by the model, and wrong in practice:
`makes` is free text, so one AC30 tone lists its console, preamp, two
mics and its speaker in it, and three creators spell the same amp three
ways. Merging two tones into one Preset is a decision a person makes, not
one inferred from a string.

### A Block has two orthogonal axes

A Block's **role** is independent of **how that role is realized**:

| Axis | Means | Values |
|---|---|---|
| **BlockType** | *what the block does* | `Drive` (the default), `Amp`, `Cabinet`, `Eq`, `Compressor`, `Gate`, `Delay`, `Reverb`, `Rotary`, … — 26 of them, defined by the `block_types!` table in `signal-proto` |
| **BlockKind** | *how it is achieved* | `Native` (built-in DSP), `Nam` (a `.nam` neural capture), `HostedPlugin` (CLAP/VST3), `Custom` |

So an `Amp` can be a native waveshaper, a neural capture, or a hosted
plugin — same role, three realizations. This is why a NAM **drive pedal**
needs no special case anywhere: it is a `Drive` block that happens to be
`Nam`, and the chain builder never asks which.

### The whole picture

```
  COMPOSITION                                     CONTAINMENT
  Block → Module → Layer → Engine → Preset  ◀───  Rig (the instrument)
    └────── each one a Collection ──────┘         holds its Rig Presets
              of named Variants          │
                                         │ VARIATION
                              Preset → Snapshots
                                         ▲   ▲
  SELECTION                              │   │   (point at, + overrides)
  Profile → Patch ───────────────────────┘   │
                    ▲                        │
  Setlist → Song → Scene ────────────────────┘
```

### Selection: pointing, not owning

These layers hold no tone of their own. They reference a Snapshot and may
add overrides on top.

- **Profile → Patch** — context switching. A "Worship" Profile holds
  Clean / Crunch / Lead / Ambient Patches, each pointing at a Preset
  Snapshot.
- **Song → Scene** — the arrangement. Intro / Verse / Chorus, each
  pointing at a Patch or a Snapshot, stepped through as the song runs.
- **Setlist → Song** — an ordered gig.

### The names in code do not all match yet

The code implements this shape, but several names differ from the model
above — `Preset` in particular means the **block-level** collection in
`signal-proto`, while the canonical top-level Preset is called `Rig`.
The `Collection`/`Variant` traits are wired for Engine, Layer, Rig,
Profile, Song and Setlist; the Block and Module levels have the types but
not yet the trait.

**`crates/signal/docs/DOMAIN.md` is the source of truth for that gap** —
it carries the full canonical ↔ code table and the open renames. Align
deliberately; do not assume a name means what it says.

## Detachable GUI (strict)

The rig core is 100% headless. Every GUI is a vox remote over architect:
`signal-desktop --engine` serves the router, and the browser, desktop
and tablet surfaces are all clients of the same wire contract. Signal
UI must render identically standalone, as a VST3/CLAP plugin, and
embedded in REAPER, so all three share one pipeline:
`nice-plug-dioxus` -> Blitz (Vello + wgpu) -> baseview.

## Layout

```
crates/signal/       the signal domain — proto, live, storage, browser,
                     grid, rig-host, controller, import, space, widgets,
                     account, tone3000 (the TONE3000 capture library)
features/rigs/       the instrument rigs — guitar, keys, drums, bass,
                     synth, orchestra, ekit, wurli
features/sampler/    the sampler engine and .signalpack format
features/nam/        neural amp modeler
features/fx/         macromod — the macro/modulation data model
features/plugin-host/  hosting third-party plugins
features/reaper/     the REAPER signal extension + controller
apps/desktop/        the Signal app + `--engine` + browser remote
apps/cli/            the `signal` command (signal-cli)
apps/plugins/        the CLAP/VST3 cdylibs
```

The DSP and the effects left for
[processor](https://github.com/FastTrackStudios/processor) in September
2026 and come back as a tagged git dep. Two things deliberately stayed:
**NAM**, because it needs `signal-proto` / `signal-sampler` and the
Tone3000 model browser rather than being an effect; and **macromod**,
which is the macro/modulation data model and is embedded in
`signal-proto`'s wire format.

## Build

```bash
nix develop
cargo check --workspace
cargo build -p signal-desktop          # the app; --engine is headless
cargo build -p signal-cli              # the `signal` command
```

## Licence

GPL-3.0-or-later.
