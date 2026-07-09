+++
title = "Sampler File Formats & Mapping"
weight = 25
+++

Reference for the Signal sampler's on-disk formats and how MIDI mapping works
across them. Source of truth lives in `crates/signal-sampler/src` — this page
summarizes it; field-level docs are on the structs themselves.

## The hierarchy

A playable kit/instrument is assembled from five file types that nest:

```text
Preset  (.signalpreset)   kit layout + note routing + master graph
  └─ Engine  (.signalengine)   "a kick" / "a piano" — one source + per-mic layers
       ├─ Block   (.signalblock)   thin wrapper: one pack + gain/pan/transpose
       │    └─ Pack  (.signalpack)   binary container: decoded PCM + embedded spec
       └─ Module  (.signalmodule)   reusable bus / FX node (parallel comp, room sum)
```

| Extension | Format | Holds | Loader |
|-----------|--------|-------|--------|
| `.signalpreset` | styx (text) | engines + `note_routing` + routing graph + master FX + macros | `PresetSpec::from_file` |
| `.signalengine` | styx (text) | one `block` (pack ref) + `layers` (per-mic) + `ports` + `voice` | `EngineSpec::from_file` |
| `.signalblock` | styx (text) | one `pack` ref + `BlockParams` (gain/pan/transpose/tune) | `BlockSpec::from_file` |
| `.signalmodule` | styx (text) | `inputs` + `outputs` + `fx_chain` (parse-only v1) | `ModuleSpec::from_file` |
| `.signalpack` | **binary** | decoded PCM (FLAC i24) + embedded styx spec in the index | `SignalPcmPack::open` |

Text formats are [styx](/) (`.styx`) parsed via `facet_styx`; a `.toml` spec is
also accepted where a `LibrarySpec` is loaded directly (format auto-detected by
extension). They all round-trip: fields whose runtime is deferred (FX chains,
routing graph, modules, macros) still parse and re-serialize cleanly.

## Where mapping lives — two layers

MIDI mapping is defined at **two different levels**, and it's important not to
confuse them:

1. **Kit level — `note_routing` in the `.signalpreset`.** Maps an incoming MIDI
   note to one or more engine ids. This is "note 38 plays the snare engine". A
   note may target several engines (layering). This is the layer you edit to
   lay out a drum kit across the keyboard.

2. **Sample level — `zones` (ZoneSpec) in a pack's embedded spec.** Maps a note
   *within an engine* to the actual sample(s): key range, velocity range,
   round-robin slot, root key, mic, articulation. This is how a pitched
   instrument spreads samples across the keyboard, or how a drum picks the
   right velocity layer / round-robin.

For a drum kit, the preset's `note_routing` does the keyboard layout and each
drum's pack usually has its zones spanning a fixed note (velocity/RR variation
only). For a pitched sampler, `note_routing` may send a whole range to one
engine and the zones do the per-key sample selection.

### `note_routing` (PresetSpec)

```styx
note_routing (
    { note 36 targets ( "kick" ) }
    { note 38 targets ( "snare-a" "snare-b" ) }       // layered
    {
        note 49
        targets ( "hats" )
        articulation "Closed Tip"                     // percussion: which hit
    }
)
```

`NoteRoute { note, targets, articulation }`. Multiple targets = layering. The
optional `articulation` (percussion) fires that articulation on the target
engine ignoring key — see *Percussion mode* above. See the
[GGD Modern & Massive 2 map](/ggd-modern-massive-2-mapping/) for a full kit
built this way.

Per-engine `transpose` (on `PresetEngineRef`) and per-block `transpose` (on
`BlockParams`) shift incoming notes before they reach the zones — handy for
octave-shifted kits without re-authoring zones.

### Percussion mode & per-route articulation

Drum-kit packs pin each zone to a key (kick @36, hats Closed @42 / Open @46,
…) — that key is the pack's *articulation selector*, not a fixed performance
note. So the preset can lay drums out however it likes, the engine adapts:

- **Percussion mode** — auto-detected when `LibrarySpec.category` is a drum-kit
  (or `instrument` names a percussion piece). Such engines always play at
  **natural pitch** (the routed note never transposes the sample). If every
  zone shares one `key_min` (kick, a single tom), the engine fires on **any**
  routed note — so an L/R pair is just two notes on the same drum.
- **Per-route articulation** — `NoteRoute.articulation` makes the target engine
  fire only that articulation's zones, ignoring key. One shared engine serves
  many notes (hats Closed/Tight/Open/Pedal each on a different note) with no
  duplication. Implemented as a per-trigger override (`note_on_articulated`),
  so the same `hats` engine handles every hat note.

Net effect: **packs stay audio + organization; the preset is the sole mapping
authority.** `key_min`/`key_max`/`root_key` remain in the spec (pitched
samplers need them) — percussion mode just makes them runtime no-ops.

Percussion mode also makes voices **one-shot**: they play to the sample's
natural end and ignore note-off (a drum is struck, not held), so MIDI gate
length doesn't matter.

### Choke groups

`PresetEngineRef.choke_group` makes an engine monophonic-with-choke: every
note-on silences any voice still ringing in that group and joins it. Hi-hats
set it so open / closed / tight / pedal all cut each other (you can't half-open
a ringing hat) — kick / snare / toms leave it empty and ring freely.

`choke_on` controls *which* hits choke. Empty = monophonic (every hit chokes —
hi-hats). Non-empty = only those articulations choke, so cymbals ring and
overlap but the explicit *Choke* articulation grabs the ring:

```styx
engines (
    { id "hats"  engine "…Hats.signalengine"  choke_group "hats" }            // mono
    {                                                                          // selective
        id "crash-l"
        engine "…Crash.signalengine"
        choke_group "crash-l"
        choke_on ( "Choke" )
    }
)
```

Even finer per-sample relationships use the `choke_group` / `off_by` /
`group_polyphony` fields on `ZoneSpec` directly.

### `zones` (ZoneSpec)

The richer, sample-level map. Key fields:

| Field | Meaning |
|-------|---------|
| `key_min` / `key_max` | inclusive MIDI note range the zone covers |
| `root_key` | note at which the sample plays back un-pitched |
| `vel_min` / `vel_max` | velocity range (default 0..127) |
| `rr_index` / `rr_mode` | round-robin slot + `cycle` / `random` / `no-repeat-random` |
| `mic` | which `LibrarySpec.mics` entry → which output bus |
| `articulation` | hit type tag (Hit / Sidestick / Flam …) |
| `dynamic` | CC1 crossfade layer (`ppp`…`fff`) for sustains |
| `variant` | `Mixed` vs `Unmixed` stems (GGD Luke Holland / Pridgen) |
| `choke_group` / `off_by` | mutually-exclusive groups (hi-hat open/closed) |
| `group_polyphony` | max simultaneous hits for the group |
| `sample_start/end`, `loop_start/end`, `playback_mode` | window + loop control |
| `trigger_mode`, `trigger_cc`, `trigger_value_min/max` | attack / one-shot / release / pedal / CC / aftertouch triggers |

Zone mode is engaged whenever `LibrarySpec.zones` is non-empty: every note-on
looks up matching zones by key+velocity, RR-cycles within the match, and fires
the matching zone **for every active mic**. Many zones share the same
`(key, vel, rr, articulation)` and differ only by `mic`, `dynamic`, or
`variant` — those form a multi-mic / crossfade / variant group.

## The `.signalpack` binary container

Packs are the only binary format. They hold decoded PCM (re-encoded as FLAC
i24) for fast streaming, plus a UTF-8 text index that carries the embedded
`LibrarySpec` and a per-sample offset table. Layout:

```text
[ 64-byte header ] [ FLAC audio body ] [ UTF-8 text index ]
```

Header (64 bytes, little-endian) — see `engine/cache.rs` / `pack_rewrite.rs`:

| Offset | Size | Field |
|--------|------|-------|
| 0  | 8 | magic `"SIGPACK\0"` |
| 8  | 4 | version (u32) = 1 |
| 12 | 4 | kind (u32) = 5 (`FLAC_I24`) |
| 16 | 8 | body offset (u64) = 64 |
| 24 | 8 | index offset (u64) |
| 32 | 8 | index length (u64) |
| 40 | 8 | sample count (u64) |
| 48 | 16 | reserved (zero) |

Index (text) format:

```text
# signalpack-index-v1
# spec_path	<original spec path>
# spec_format	styx        ← or toml
# spec_begin
<embedded LibrarySpec text>
# spec_end
# source	offset	bytes	channels	sample_rate	num_frames	samples
<rel-path>	<offset>	<bytes>	<ch>	<sr>	<frames>	<samples>
…
```

Rows are tab-separated and offsets are relative to the body start. The embedded
spec between `# spec_begin` / `# spec_end` is the same `LibrarySpec` styx you'd
otherwise keep as a sidecar `.styx`.

### Editing a pack's spec without touching audio

The audio body is heavy (multi-GB libraries) and immutable; only the embedded
spec changes after a pack ships. Use `pack_rewrite`:

```rust
use signal_sampler::pack_rewrite::{read_embedded_spec, rewrite_embedded_spec};

let text = read_embedded_spec(&pack_path)?;          // header-only read, no decode
rewrite_embedded_spec(&pack_path, |old| mutate(old))?; // splice spec, copy audio verbatim
```

It copies the audio bytes through to a sibling temp file, splices the new spec
into the index, recomputes the index offset/length in the header, and
atomically renames over the original. ~30 KB/s regardless of audio size, vs.
minutes to re-pack from source samples. **Always edit packs through this path,
never by hand** — the header offsets must stay consistent with the body.

## CLI

`signal sampler …` (see `apps/cli`):

| Subcommand | Does |
|-----------|------|
| `pack <spec> --samples-root <dir> [--output x.signalpack]` | build a pack from a spec + samples (decodes → FLAC i24) |
| `prepare <spec> --samples-root <dir>` | build a reusable decoded PCM cache dir instead of a pack |
| `export <pack> --output-dir <dir>` | restore audio files out of a pack |
| `retag [root] [--skip ..] [--dry-run]` | re-derive instrument/category/tags from directory layout, audio copied verbatim |
| `inspect <files…>` | sample duration + level stats |
| `midi` | list MIDI input ports |

## Editing checklist

Text specs (`.signalpreset` / `.signalengine` / `.signalblock` / `.signalmodule`):

1. Back up first (`cp x.signalpreset x.signalpreset.bak`).
2. Edit the styx; keep the file structure/shape intact (the deferred-runtime
   sections — `modules () routing () master_fx () macros ()` — must stay present).
3. Referential integrity: every `note_routing` target must be an `engines` id
   (ids unique); every `ports.from` / `layers.id` must resolve; pack refs are
   relative to the file holding them.
4. Validate by loading through the matching `*Spec::from_file` before shipping.

Binary packs: never hand-edit. Use `pack_rewrite` (spec only) or rebuild with
`signal sampler pack`.

## See also

- [GGD Modern & Massive 2 Map](/ggd-modern-massive-2-mapping/) — concrete
  hand-pair mapping built on these formats.
- [Sampler Roadmap](/sampler-roadmap/) — where the playback engine is headed.
