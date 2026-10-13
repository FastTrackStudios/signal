# Signal Storage

How signal presets are persisted — both in the SQLite database and as
REAPER-native files on disk.

## Database Storage

r[signal.storage.db]
Signal domain entities (blocks, modules, layers, engines, rigs, profiles,
songs, setlists) are stored in a SQLite database at
`~/Music/FastTrackStudio/Library/signal.db`.

r[signal.storage.db.repos]
Each entity type has a repository trait (`BlockRepo`, `ModuleRepo`, etc.)
with a live implementation backed by SeaORM.

r[signal.storage.db.seed]
On first launch, the database is seeded with default presets from the
`signal-storage` seed data module. Subsequent launches refresh
file-backed presets without deleting existing data.

## REAPER-Native Files

r[signal.storage.rfxchain]
Block and module presets are stored as `.RfxChain` files — REAPER's native
FX chain format. These are the raw FX plugin blocks without the `<FXCHAIN>`
wrapper, suitable for direct insertion into a track's FX chain.

r[signal.storage.rfxchain.layout]
FXChain presets live under `FXChains/FTS-Signal/`:
- `01-Blocks/{category}/{vendor}/{name}.RfxChain` — individual block presets
- `02-Modules/{name}.RfxChain` — module chain presets

r[signal.storage.tracktemplate]
Layer, engine, rig, profile, and song presets are stored as `.RTrackTemplate`
files — REAPER's native track template format containing full `<TRACK>` chunks.

r[signal.storage.tracktemplate.layout]
Track templates live under `TrackTemplates/FTS-Signal/{instrument}/`:
- `01-Layers/{name}/{variation}.RTrackTemplate`
- `02-Engines/{name}/{variation}.RTrackTemplate`
- `03-Rigs/{name}/{variation}.RTrackTemplate`
- `04-Profiles/{name}/{variation}.RTrackTemplate`
- `05-Songs/{name}/{variation}.RTrackTemplate`

r[signal.storage.tracktemplate.variation]
Each preset is a folder containing variation files. The first save uses
`Default` as the variation name. Additional saves create new variations
(e.g., `Bright.RTrackTemplate`, `Warm.RTrackTemplate`).

## Sidecar Metadata

r[signal.storage.sidecar]
Each `.RfxChain` or `.RTrackTemplate` file can have an optional
`.signal.styx` sidecar file providing signal-specific metadata.

r[signal.storage.sidecar.format]
Sidecar format (styx):
```styx
version 1
id "uuid-here"
kind @Block{block_type amp}
tags (neural-dsp clean)
description "Clean amp tone"
```

r[signal.storage.sidecar.kind]
The `kind` field identifies the preset type: Block, Module, Layer, Engine,
Rig, Profile, Song, or Rack.

r[signal.storage.sidecar.auto-index]
Files without sidecars are auto-indexed from their path: folder name
becomes category, file stem becomes display name, path hash becomes
stable ID.

## FX Chain Wrapper

r[signal.storage.strip-fxchain]
When capturing FX chain chunk text from REAPER, the `<FXCHAIN>` wrapper
and metadata lines (SHOW, LASTSEL, DOCKED) must be stripped before writing
to `.RfxChain` files. REAPER's native format expects bare FX blocks only.

## Scanning

r[signal.storage.scan.blocks]
The block scanner recursively walks `FXChains/FTS-Signal/01-Blocks/`,
creating `Preset` entries from each `.RfxChain` file. Block type is
inferred from the folder name (e.g., `Amps/` → `BlockType::Amp`).

r[signal.storage.scan.modules]
The module scanner walks `FXChains/FTS-Signal/02-Modules/`, creating
`ModulePreset` entries from each `.RfxChain` file.

r[signal.storage.scan.templates]
The track template scanner walks the `TrackTemplates/FTS-Signal/` tree,
discovering all `.RTrackTemplate` files organized by instrument, tier,
preset name, and variation.
