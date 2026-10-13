# DAW Bridge

How the signal domain interacts with REAPER through the DAW abstraction layer.

## Architecture

r[signal.daw.facade]
The signal crate accesses REAPER through the `daw` facade crate, never
through raw REAPER API calls. This enables the same signal logic to work
with any DAW backend (REAPER, standalone test, future DAWs).

r[signal.daw.services]
Key DAW services used by signal:
- `TrackService`: Create/query/modify tracks, get/set track chunks
- `FxService`: FX chain operations, chunk capture, state snapshots
- `UiService`: User input dialogs (GetUserInputs)

## State Capture

r[signal.daw.capture.track-chunk]
`TrackService::get_track_chunk()` captures a track's full RPP state as
a `<TRACK>` block string. This is the format used by `.RTrackTemplate` files.

r[signal.daw.capture.fx-chain]
`FxService::get_fx_chain_chunk_text()` captures the `<FXCHAIN>` block
from a track. This is stripped of its wrapper for `.RfxChain` files.

r[signal.daw.capture.container]
Container FX (REAPER Containers holding child plugins) must have their
state chunks captured and restored including child plugin data. The
`get_fx_block_via_track_chunk` and `set_fx_block_via_track_chunk` functions
handle both Plugin and Container node types.

## State Application

r[signal.daw.apply.set-chunk]
`TrackService::set_track_chunk()` replaces a track's entire state from
a chunk string. Note: this changes the track's GUID, so the track handle
becomes stale after the call.

r[signal.daw.apply.insert-chunk]
`FxService::insert_fx_chain_chunk()` inserts FX blocks into an existing
chain. Expects bare FX block content (no `<FXCHAIN>` wrapper).

r[signal.daw.apply.folder-depth]
When applying chunks that need folder hierarchy, the `ISBUS` line in the
chunk must be patched before `set_track_chunk`:
- `ISBUS 1 1` for folder start
- `ISBUS 2 -1` for folder close
- Remove ISBUS for normal tracks

## Patch Application

r[signal.daw.patch-applier]
`DawPatchApplier` applies resolved signal graphs to the DAW: sets FX
parameters, loads state chunks, and manages plugin instantiation.

r[signal.daw.rig-scene]
`RigSceneApplier` handles preloaded rig scene switching with <5ms latency
by maintaining cached state for each scene.

## Main Thread

r[signal.daw.main-thread]
All REAPER API calls must run on the main thread. The `daw-reaper` service
implementations use `main_thread::query()` to dispatch work from async
contexts to the main thread via `TaskSupport`.

r[signal.daw.spawn-local]
REAPER action handlers are synchronous (`fn() -> ActionResult`). Async
signal operations use `tokio::task::spawn_local()` to bridge sync→async.
The spawned tasks run on the main thread via the timer callback's
`process_tasks()` middleware.
