# Signal Domain Model

This is the canonical domain for Signal — the vocabulary everything else should
follow. There are **three kinds of relationship** between domain objects; keeping
them straight is the whole game:

1. **Composition** — small things are built up into bigger things
   (`Block → Module → Layer → Engine → Preset`).
2. **Containment** — a **Rig** (the instrument) holds **Presets**.
3. **Variation** — any collection can have named variants ("**Snapshots**"),
   via the `Collection` / `Variant` trait.
4. **Selection** — performance-time layers (**Profile/Patch**, **Song/Scene**,
   **Setlist**) that *point at* variants rather than owning them.

---

## 1. Composition: `Block → Preset`

Each level is composed of the level below it.

| Level | What it is |
|---|---|
| **Block** | The smallest unit — one processor (amp, drive, cab, EQ, reverb, sampler, oscillator…). |
| **Module** | An ordered chain of Blocks — a signal-chain segment / pedalboard. |
| **Layer** | A processing lane — composes Modules and standalone Blocks. |
| **Engine** | An instrument voice/part — composes Layers. |
| **Preset** | A complete, playable tone — Engines composed together. *This is what you load and play.* At the instrument (Rig) level it's a **Rig Preset**. |

### A Block has two orthogonal axes

A Block's **role** is independent of **how that role is realized**:

| Axis | Meaning | Values |
|---|---|---|
| **BlockType** | *What the block does* (semantic role) | `Amp`, `Drive`, `Cabinet`, `EQ`, `Reverb`, `Delay`, `Compressor`, `Sampler`, `Oscillator`, … |
| **BlockKind** | *How it's realized* at runtime | `Native` (built-in DSP), `Nam` (a `.nam` neural model), `HostedPlugin` (CLAP/VST3), `Custom` |

So an `Amp` block can be a `Native` waveshaper, a `Nam` neural amp, or a hosted
amp-sim plugin — same role, different realization. (This is exactly what the
standalone guitar rig uses: a chain of `Nam` amp/drive blocks + a cabinet IR +
hosted `Plugin` blocks.)

---

## 2. Containment: a `Rig` holds `Presets`

A **Rig** is the **instrument** — "Guitar Rig", "Keys Rig", "Bass Rig". It holds
the set of Presets available for that instrument; a Preset under a Rig is a
**Rig Preset**.

```
Rig "Guitar Rig"
 ├─ Rig Preset "Worship Rhythm"
 ├─ Rig Preset "Lead Tones"
 └─ Rig Preset "Ambient Pads"
```

---

## 3. Variation: `Snapshots`

Every collection level can have named **variants**. A variant is a **Snapshot** —
a saved variation of a collection that records *which child variant each member
uses* plus any parameter **overrides**. This is the generic `Collection` /
`Variant` trait pair: a *Collection* owns N *Variants* and names a default.

- A **Preset Snapshot** is a variation of a Preset (e.g. the same Preset voiced
  "Clean" vs "Pushed").
- The same pattern recurs at every level (a Layer can have snapshots, an Engine
  can, etc.) — they're all "a variant of a collection."

---

## 4. Selection / performance

These layers don't *own* tone — they **point at** Snapshots and can add overrides.

| Level | Is a collection of… | Each entry points to… |
|---|---|---|
| **Profile** | **Patches** | a **Preset Snapshot** (+ overrides) |
| **Song** | **Scenes** | a **Patch** *or* a **Snapshot** (+ overrides) |
| **Setlist** | **Songs** | (an ordered gig) |

- **Profile** — quick sound switching for a context. A "Worship" profile with
  "Clean" / "Lead" / "Ambient" **Patches**, each pointing at a Preset Snapshot.
- **Song** — the arrangement: **Scenes** ("Intro", "Verse", "Chorus") each
  pointing at a Patch or a Snapshot, stepped through during the song.
- **Setlist** — an ordered set of Songs for a performance.

---

## The whole picture

```
                  COMPOSITION                              CONTAINMENT
   Block ─▶ Module ─▶ Layer ─▶ Engine ─▶ Preset  ◀──────── Rig (instrument)
     │                                      │               holds Rig Presets
  (BlockType × BlockKind)         VARIATION │
                                  Preset ─▶ Snapshots
                                              ▲   ▲
                          SELECTION           │   │ (point at)
   Profile ─▶ Patch ──────────────────────────┘   │
                  ▲                                 │
                  │ (a Scene points at a Patch …)   │ (… or a Snapshot)
   Setlist ─▶ Song ─▶ Scene ──────────────────────┴─
```

---

## How this maps to the code today (`crates/signal-proto`)

The code implements this shape, but **some names differ from the canonical model
above** — most importantly around `Preset` / `Rig` and `Scene` / `Patch`. This
table is the source of truth for the gap; align deliberately, don't assume.

| Canonical term | Today's `signal-proto` type | Note |
|---|---|---|
| Block | `model::Block` | ✅ same. `block_kind::BlockKind` = the realization axis; `block::BlockType` = the role axis. |
| Module | `model::Module` (+ `ModulePreset`, `ModuleSnapshot`) | ✅ |
| Layer | `layer::Layer` (+ `LayerSnapshot`) | ✅ |
| Engine | `engine::Engine` (+ `EngineScene`) | ✅ Engine variants are called *EngineScene*. |
| **Preset** (engine composition you play) | `rig::Rig` (+ `RigScene`) | ⚠️ **Name clash.** The code's `Rig` is the engine-composition = the canonical **Preset**. Its variants are `RigScene`s = canonical **Preset Snapshots**. |
| **Rig** (instrument holding Presets) | *(no distinct type yet)* | ⚠️ The instrument-level container is not its own type today; `RigType` / `catalog` carry some of this. |
| Snapshot (variant of a collection) | `Variant` trait impls: `Snapshot` (block), `ModuleSnapshot`, `LayerSnapshot`, `EngineScene`, `RigScene` | ⚠️ Inconsistent — "Snapshot" at low levels, "Scene" at engine/rig. Worse now that **Scene** is the Song entry: `EngineScene`/`RigScene` should be `EngineSnapshot`/`RigSnapshot`. |
| `model::Preset` | `model::Preset` | ⚠️ **In code, `Preset` is a *block-level* collection of `Snapshot`s for one `BlockType`** — NOT the canonical top-level Preset. This is the biggest source of confusion. |
| **Profile** | `profile::Profile` | ✅ holds **`Patch`** — matches. |
| **Patch** (Profile entry → Preset Snapshot) | `profile::Patch` (`PatchTarget::RigScene`) | ✅ matches. `PatchTarget` can also point at lower levels. |
| **Song** | `song::Song` | ✅ name; ⚠️ holds **`Section`**, canonical = **`Scene`**. |
| **Scene** (Song entry → Patch or Snapshot) | `song::Section` (`SectionSource::{ Patch, RigScene }`) | ⚠️ canonical **Scene** = code **`Section`**. Targets already match (`Patch` = Patch, `RigScene` = a Preset Snapshot). |
| **Setlist** | `setlist::Setlist` | ✅ |
| Variation trait | `traits::{ Collection, Variant, DefaultVariant }` | ✅ "our trait" — the generic collection-of-variants. |
| Overrides | `overrides::Override`, `override_policy` | ✅ applied by Scenes/Sections/Patches. |

### Open naming decisions (canonical ↔ code)

These are renames to make the code match this document. None are done yet:

1. **`Preset` collision.** Canonical `Preset` = the engine composition (today's
   `Rig`); but the code already uses `Preset` for a block-level collection.
   Renaming requires resolving both at once (e.g. block-level `Preset` →
   `BlockPreset` / `BlockCollection`, and `Rig` → `Preset`).
2. **Instrument-level `Rig`.** Introduce a real container type for the canonical
   `Rig` (instrument holding Rig Presets), or formalize `RigType`/catalog into it.
3. **`Section` → `Scene`** in `Song` (canonical Song entries are Scenes).
   `Profile`→`Patch` already matches and needs no change.
4. **Unify variant naming as `Snapshot`** across levels: `EngineScene` →
   `EngineSnapshot`, `RigScene` → `RigSnapshot` — freeing "Scene" for the Song
   entry exclusively.

> **Standalone rig note:** `signal-sampler`'s `RigProfile` / `RigPatch` is a
> repo-free, audio-side projection of this model for the live guitar rig. Its
> `RigPatch` ≈ a canonical **Patch** (a Profile entry), and each patch's `chain`
> of `RigBlock`s (`nam` / `cab_ir` / `plugin`) is a flattened, realized Rig
> Preset. `RigProfile::from_proto` converts a proto `Profile` into it via a
> `PatchResolver`.
