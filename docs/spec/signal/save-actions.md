# Signal Save Actions

REAPER actions for saving presets from the current track state.

## Overview

r[signal.save]
The signal save system provides REAPER actions that capture FX/track state
from the selected track and write it to the appropriate preset directory
with a `.signal.styx` sidecar.

## Save Block

r[signal.save.block]
Save the selected track's FX chain as a Block preset (`.RfxChain`).
Auto-detects block type from the first FX's name using `FxRole::parse()`.

r[signal.save.block.dialog]
Shows a dialog with: Preset Name (from FX name), Variation (Default),
Block Type (from FX role or "custom").

r[signal.save.block.output]
Writes to `FXChains/FTS-Signal/01-Blocks/{BlockType}/{PresetName}/{Variation}.RfxChain`
plus a `.signal.styx` sidecar with `kind @Block{block_type ...}`.

## Save Module

r[signal.save.module]
Save the selected track's FX chain as a Module preset (`.RfxChain`).
Captures the entire FX chain (all blocks in order).

r[signal.save.module.dialog]
Shows a dialog with: Preset Name (from track name, strip `[L]` prefix),
Variation (Default).

r[signal.save.module.output]
Writes to `FXChains/FTS-Signal/02-Modules/{PresetName}/{Variation}.RfxChain`
plus sidecar with `kind @Module@`.

## Save Layer

r[signal.save.layer]
Save the selected track as a Layer preset (`.RTrackTemplate`).
Captures the full track chunk including FX chain, routing, and metadata.

r[signal.save.layer.dialog]
Shows a dialog with: Preset Name (strip `[L]` prefix), Variation (Default),
Instrument (from ExtState `FTS/rig_type` or default Guitar).

r[signal.save.layer.output]
Writes via `save_track_template()` to
`TrackTemplates/FTS-Signal/{instrument}/01-Layers/{name}/{variation}.RTrackTemplate`.

## Save Rig

r[signal.save.rig]
Save the selected folder track and all its children as a Rig preset.
Concatenates track chunks from the folder and all child tracks.

r[signal.save.rig.folder-walk]
Walks child tracks by tracking `folder_depth`: starts at depth 1 when
entering the folder, increments for sub-folders, decrements for folder
closers. Stops when depth reaches 0.

r[signal.save.rig.output]
Writes combined chunks to `TrackTemplates/FTS-Signal/{instrument}/03-Rigs/`.

## Load Profile

r[signal.save.load-profile]
Load a profile by reading all `.RTrackTemplate` variations from a profile
folder, creating a folder track + one child track per variation.

r[signal.save.load-profile.scan]
Scans `TrackTemplates/FTS-Signal/*/04-Profiles/` for profile folders.
Each subfolder is a profile, each `.RTrackTemplate` inside is a variation.

r[signal.save.load-profile.create]
For each variation: creates a track via `add_track()`, applies the saved
chunk via `set_track_chunk()`. The folder track gets `ISBUS 1 1` (folder start),
the last child gets `ISBUS 2 -1` (folder close).

## DAW Facade Usage

r[signal.save.daw-facade]
All save actions use the `daw` crate facade for REAPER API access:
- `TrackService` for track queries and chunk capture
- `FxService` for FX chain chunk text and FX list
- `UiService` (via `ReaperUi`) for GetUserInputs dialogs
- `signal::track_template` and `signal::sidecar` for file I/O

r[signal.save.async]
Save actions are async functions called from `tokio::task::spawn_local`
in action handlers. They run on the main thread via the timer callback's
task middleware.
