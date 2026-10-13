# Synchronization Engine Lift — Continuation Plan

Handoff doc for finishing the `daw-synchronization` engine lift.
Companion to [`streaming-design.md`](./streaming-design.md). Read
that first if you haven't — the engine consumes the streaming
surface it describes.

## Status when this doc was written

**Streaming is partially live.** 5 of 11 domains stream end-to-end
through `DawEventHub`:

- `transport` (state + position) — done
- `markers` — done
- `regions` — done
- `tracks` (coarse Added/Removed; per-field events deferred) — done
- `tempo_map` (wholesale `MapChanged`) — done

Remaining domains (replicate the same five-step pattern from
`streaming-design.md`): items, takes, fx, routing, action_registry,
project, live_midi.

**`daw-synchronization` crate exists** at `crates/daw-synchronization/`
and compiles. Inside:

- `Cargo.toml` — depends only on `daw` (facade), `facet`, `tokio`,
  `tokio-util`, `tracing`. **No** direct dep on `daw-proto` or
  `daw-reaper` — backend-agnostic by construction.
- `src/lib.rs` — embeds the sync types that used to live in
  `sync-proto`:
  - `SyncSession`, `SyncPeer`, `SyncConfig`, `ConflictPolicy`
  - `SyncDomain` (discriminated union over domain events)
  - `SyncEvent` (envelope: origin peer, sequence, project guid,
    domain, created_at_ms)
  - `SyncStatus`
- `src/suppression.rs` — echo suppression set (lifted as-is).
- `src/drift.rs` — drift corrector for follower playhead realignment
  (lifted as-is).
- `src/heartbeat.rs` — master peer position broadcast (lifted,
  retargeted to `tokio::task::spawn` and `daw::rpc::Daw`).

**Workspace builds clean.** `cargo check --workspace` passes.

## Architectural decisions already made

Locked in earlier in the conversation, don't reopen unless something
breaks:

1. **Type name is `SynchronizationEngine`.** The event/peer/session
   types stay as `SyncEvent`, `SyncPeer`, etc. — only the engine
   type itself takes the long name to dodge the sync-vs-sync
   collision with our sync trait API.

2. **Engine is backend-agnostic.** It depends only on `daw` (the
   facade). Subscribes to streaming traits (`TransportStream`,
   `MarkersStream`, …) and applies through sync clients
   (`MarkersClient`, …). Works against any backend that implements
   those traits: REAPER↔REAPER, REAPER↔Standalone (in-memory test
   backend), REAPER↔future-ProTools, etc.

3. **Engine runs in-process with the backend.** No vox round-trip
   for local subscriptions — the engine holds an `Arc<DawEventHub>`
   reference (or fetches via `daw::reaper::event_hub()`) and
   subscribes directly to `broadcast::Receiver<T>`. The vox stream
   clients exist for *out-of-process* consumers; the engine is one
   of the in-process consumers.

4. **Wire-protocol concerns live separately.** Peer discovery, mDNS,
   the actual socket between two peers, Ableton Link — all stay in
   the FastTrackStudio/sync repo. That crate will eventually depend
   on `daw-synchronization` and provide a thin transport adapter
   over the engine's `outbound()` stream and `apply_remote()` sink.

5. **Sync types are embedded, not in a separate proto crate.** The
   types belong with the engine that owns them. The sync repo's
   network adapter imports them from `daw-synchronization` when it
   needs to serialize over the wire.

## What's left

Three modules to lift, one to rewrite, plus integration.

| Module | Lines | Action | Notes |
|---|---|---|---|
| `daw_module.rs` | ~30 | Lift + minor edits | DawModule adapter; just enough to expose the engine as a daw extension. |
| `engine.rs` | ~540 | Lift + rename | Orchestrator. Rename `Engine` → `SynchronizationEngine`. Adjust imports. |
| `apply.rs` | ~1660 | Lift + import fixup | Uses `daw::rpc::*` async clients (already-correct paths after the f9ae48a facade reorg). Should mostly work; expect import path adjustments. |
| `subscriptions.rs` | ~580 | **Rewrite** | Old version calls retired `subscribe_state()`/`subscribe_markers()`/etc. methods. New version subscribes directly to `daw::reaper::event_hub()` broadcast channels. |

### Why `subscriptions.rs` is a rewrite, not a lift

The old subscriptions code (`/home/cody/Development/FastTrackStudio/sync/crates/sync/src/subscriptions.rs`) does this for transport:

```rust
let mut rx = project.transport().subscribe_state().await?;
```

That method doesn't exist anymore — `subscribe_state` retired with
the architect-rpc port. The replacement in our streaming surface is
the new `TransportStreamClient::subscribe_state` async client. But
since the engine runs in-process, going through a vox client adds a
serialize/deserialize round-trip for no reason.

The rewrite is simpler than the original:

```rust
// Old (vox round-trip):
let mut rx = project.transport().subscribe_state().await?;

// New (in-process direct subscribe):
let mut rx = daw::reaper::event_hub().subscribe_transport_state();
```

That's the whole pattern. Replicate per domain that's wired through
`DawEventHub`. Domains not yet wired (items, takes, fx, routing,
action_registry, project, live_midi) get TODOs — they'll plug in as
streaming Phase 2 finishes.

## Migration steps, in order

### Step 1 — lift `apply.rs`

Easiest piece. Copy verbatim, fix imports:

```bash
cp /home/cody/Development/FastTrackStudio/sync/crates/sync/src/apply.rs \
   /home/cody/Development/FastTrackStudio/daw/crates/daw-synchronization/src/apply.rs
```

Then in the new file:

- `use sync_proto::{...}` → `use crate::{...}`
- `use daw::Daw` → `use daw::rpc::Daw` (facade reorg)
- Any `moire::task::spawn(...).named("...")` → `tokio::task::spawn(...)` (drop `.named()`)
- `use moire::sync::Mutex` → `use tokio::sync::Mutex`

Add to `lib.rs`: `pub mod apply;`

Compile-check (`cargo check -p daw-synchronization`). Expect
import-path errors; fix as they appear.

### Step 2 — lift `daw_module.rs`

Same drill, smaller. Copy, fix imports, wire into `lib.rs`.

### Step 3 — lift `engine.rs` (the meat)

Copy. Then:

- Rename the public type: `pub struct Engine` → `pub struct SynchronizationEngine`.
- Rename all internal references (`Engine::new` → `SynchronizationEngine::new`, etc.).
- Re-export from `lib.rs`: `pub use engine::SynchronizationEngine;`
- Fix imports per Step 1's rules.
- The engine's `subscriptions: ProjectSubscriptions` field will fail
  to compile because we haven't rewritten subscriptions yet. Either
  stub it temporarily (`type ProjectSubscriptions = ();`) or do
  Step 4 first.

### Step 4 — rewrite `subscriptions.rs`

Most invasive but smallest LOC. The shape:

```rust
//! Local stream subscriptions for the synchronization engine.
//!
//! Subscribes directly to `daw::reaper::event_hub()` broadcast
//! channels and wraps every event in a `SyncEvent` envelope. No vox
//! round-trip — the engine and the hub live in the same process.

use crate::{SyncDomain, SyncEvent, SyncConfig};
use crate::suppression::SuppressionSet;
use daw::reaper;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

pub struct ProjectSubscriptions {
    cancel: CancellationToken,
}

impl ProjectSubscriptions {
    pub fn cancel(&self) { self.cancel.cancel(); }
}

impl Drop for ProjectSubscriptions {
    fn drop(&mut self) { self.cancel.cancel(); }
}

/// Spawn one forwarder task per enabled domain. Each reads from the
/// hub, wraps in SyncEvent, runs echo suppression, broadcasts on
/// `event_tx`.
pub fn spawn(
    config: &SyncConfig,
    peer_id: String,
    sequence: Arc<AtomicU64>,
    event_tx: broadcast::Sender<SyncEvent>,
    suppression: Arc<tokio::sync::Mutex<SuppressionSet>>,
) -> ProjectSubscriptions {
    let cancel = CancellationToken::new();

    if config.transport {
        spawn_transport_forwarder(peer_id.clone(), sequence.clone(),
                                  event_tx.clone(), suppression.clone(),
                                  cancel.clone());
    }
    if config.markers {
        spawn_marker_forwarder(/* … */);
    }
    if config.regions {
        spawn_region_forwarder(/* … */);
    }
    if config.tracks {
        spawn_track_forwarder(/* … */);
    }
    if config.tempo_map {
        spawn_tempo_map_forwarder(/* … */);
    }
    // TODO: items, takes, fx, routing once streaming Phase 2 finishes.

    ProjectSubscriptions { cancel }
}

fn spawn_transport_forwarder(
    peer_id: String,
    sequence: Arc<AtomicU64>,
    event_tx: broadcast::Sender<SyncEvent>,
    suppression: Arc<tokio::sync::Mutex<SuppressionSet>>,
    cancel: CancellationToken,
) {
    let mut rx = reaper::event_hub().subscribe_transport_state();
    tokio::task::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                result = rx.recv() => match result {
                    Ok(event) => {
                        // Extract project_guid from the TransportEvent
                        // (the event variants embed it).
                        let project_guid = match &event {
                            daw::service::TransportEvent::Snapshot { project_guid, .. } => project_guid.clone(),
                            // … other variants
                            _ => String::new(),
                        };
                        let sync_event = SyncEvent {
                            origin_peer: peer_id.clone(),
                            sequence: sequence.fetch_add(1, Ordering::Relaxed),
                            project_guid,
                            domain: SyncDomain::Transport(/* convert TransportEvent → Transport snapshot */),
                            created_at_ms: SyncEvent::now_ms(),
                        };
                        let _ = event_tx.send(sync_event);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    });
}

// Repeat for marker, region, track, tempo_map. ~30 LOC each.
```

**Gotcha**: `TransportEvent` is a discriminated enum (Snapshot,
PlayStateChanged, …) but `SyncDomain::Transport` takes a `Transport`
snapshot. Either convert each variant to a `Transport` snapshot
(reading current state to fill missing fields) or extend `SyncDomain`
to carry the full `TransportEvent`. Recommend the latter — preserves
the granularity the receiving side needs to apply correctly.

For the other domains, the `<Domain>StreamEvent` envelope already
carries `project_guid` + the per-domain event; just unwrap:

```rust
let stream_event: MarkerStreamEvent = result?;
SyncEvent {
    project_guid: stream_event.project_guid,
    domain: SyncDomain::Marker(stream_event.event),
    // …
}
```

### Step 5 — wire engine to subscriptions

Engine.rs has an `Engine::new` that constructs `ProjectSubscriptions`
internally. The signature should accept a per-project bootstrap
config and call `subscriptions::spawn(...)` for each project the
engine should mirror.

The engine also exposes:
- `outbound() -> broadcast::Receiver<SyncEvent>` — sync-repo
  network adapter subscribes here.
- `apply_remote(SyncEvent)` — sync-repo pushes inbound events here.

Both already exist in the original `engine.rs`; verify they survive
the rename.

### Step 6 — integration test against the streaming surface

In `crates/daw-synchronization/tests/`, add a reaper-test integration
test:

```rust
#[reaper_test]
async fn marker_change_emits_sync_event() {
    let engine = SynchronizationEngine::new("test-peer".into(), SyncConfig::all()).await;
    let mut outbound = engine.outbound();

    // Drive REAPER: add a marker.
    let daw = daw::rpc::Daw::get();
    let project = daw.current_project().await.unwrap();
    project.markers().add("test", 0.0).await.unwrap();

    // Wait for the next tick.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Engine should emit a SyncEvent::Marker(Added).
    let event = tokio::time::timeout(Duration::from_secs(1), outbound.recv())
        .await.expect("timeout").unwrap();
    assert_eq!(event.origin_peer, "test-peer");
    matches!(event.domain, SyncDomain::Marker(MarkerEvent::Added(_)));
}
```

Equivalent tests for transport/regions/tracks/tempo_map. This is the
acceptance gate — if a real REAPER instance produces real events that
the engine wraps correctly, the foundation is solid.

### Step 7 — sync repo refactor

Once the engine is solid in daw, refactor the sync repo to depend on
`daw-synchronization`. The sync crate becomes:

- mDNS peer discovery (already there)
- Network transport (already there — `network.rs`)
- Ableton Link adapter (already there — `link.rs`)
- A thin glue layer: subscribe to `engine.outbound()`, push to peers;
  receive from peers, call `engine.apply_remote(...)`.

Roughly: delete `apply.rs`, `subscriptions.rs`, `engine.rs`,
`suppression.rs`, `drift.rs`, `heartbeat.rs`, `daw_module.rs` from
the sync crate. Replace with `use daw_synchronization::*` imports.
The `sync-proto` crate can either be retired (types lifted into
`daw-synchronization`) or kept as a wire-format shim if there's a
need.

That's a coordinated change across two repos — likely a separate
session.

## Known gotchas

1. **Capn pre-commit hook auto-staging.** The bearcove `capn` hook
   formats and auto-stages files between commits. If you have
   unrelated dirty files in the working tree, they'll get bundled
   into your commit under the wrong message. Either clean
   `git status` first or accept the noise.

2. **Linter reverting attribute additions on event.rs files.** Some
   times during Phase 2 streaming, the linter reverted the new
   `<Domain>StreamEvent` envelope structs out of `event.rs` files
   in daw-proto. If you hit "unresolved import" on a `StreamEvent`,
   check that the struct is still in the file and re-add if not.

3. **`tokio::task::spawn` vs `moire::task::spawn`.** The sync repo
   uses `moire::task::spawn(...).named("...")`. In daw, use plain
   `tokio::task::spawn(...)` and drop the `.named()`. No equivalent.

4. **`daw::Daw` → `daw::rpc::Daw`.** Facade reorg in commit f9ae48a
   moved the async surface under `rpc`. The sync repo's source was
   written against the old path.

5. **TransportEvent ≠ Transport snapshot.** The streaming
   `TransportEvent` is a delta-style enum (Snapshot, PlayStateChanged,
   …). `SyncDomain::Transport` currently takes a `Transport` (full
   snapshot). Either extend `SyncDomain` to carry the event, or
   collapse to snapshot. The event-based approach preserves more
   information; recommend that.

## After the engine: Phase 2 streaming finish

Once the engine lands, the remaining 6 streaming domains (items,
takes, fx, routing, action_registry, project, live_midi) need to be
wired through `DawEventHub`. Each is a known mechanical replication
of the pattern in `streaming-design.md`. As each lands, add the
corresponding forwarder to `daw-synchronization/src/subscriptions.rs`.

## Open architectural questions

1. **Initial state snapshot.** When a peer joins a session, it
   should receive the current state of the host's project before
   applying deltas. Helgobox calls this "send_initial_events". The
   sync repo's `engine.rs` has a `request_full_state(project_guid)`
   stub — wire it up by having the engine emit a `SyncDomain::*`
   for every entity in the project on demand.

2. **Where the engine instance lives.** Options:
   - Construct in daw-bridge's `register_daw_dispatcher`, store in
     a `OnceLock` like the event hub.
   - Construct from daw-extension-runtime when the sync `DawModule`
     loads.
   - Provide as a service through the layers! bundle.

   Recommend option 1 — single engine per process, alongside the
   hub. Sync-repo network adapter fetches it via
   `daw::synchronization::engine()`.

3. **Echo suppression key shape.** Current `SuppressionKey` uses
   `(peer_id, sequence)`. Works for ordered events. If we ever need
   content-addressed suppression (e.g. for idempotent operations
   that might arrive twice), revisit.

4. **Multi-project semantics.** Sync per-project? Sync only the
   current project? `SyncConfig` doesn't distinguish. The current
   subscriptions code iterates open projects in the poller and
   tags each event with `project_guid`. The engine just forwards;
   peers decide what to apply.

## Verification gates

Before declaring engine lift complete:

1. `cargo check --workspace` clean.
2. `cargo test -p daw-synchronization` clean (unit tests only at
   first; integration tests come with Step 6).
3. A reaper-test integration test that drives a real REAPER and
   observes the engine's outbound stream emitting the expected
   `SyncEvent` shape.
4. The sync repo, refactored to use `daw_synchronization`, still
   builds and its existing tests pass.

## Reference files

- Streaming design: [`streaming-design.md`](./streaming-design.md)
- Service composition: [`service-composition.md`](./service-composition.md)
- Original sync engine (source for lift):
  `/home/cody/Development/FastTrackStudio/sync/crates/sync/src/`
- Existing `daw-synchronization` foundation:
  `/home/cody/Development/FastTrackStudio/daw/crates/daw-synchronization/src/`
- Event hub the engine subscribes to:
  `crates/daw-reaper/src/event_hub.rs` (`DawEventHub`)
  `crates/daw-reaper/src/lib.rs` (`event_hub() -> &'static DawEventHub`)
