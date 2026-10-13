# Streaming Design

How daw-reaper exposes real-time event streams (transport position,
state changes, marker/region/track/fx/item edits) to in-process and
external consumers — sync engine, web UI, sync between two REAPER
sessions.

Companion to [`service-composition.md`](./service-composition.md).
The composition surface (Layer<B>, Services) is settled; this doc
covers the streaming surface that was retired during the
architect-rpc port and needs to come back.

## Where we are

The `#[architect::rpc]` derive only emits bridges for sync-shaped
trait methods. Streaming methods (`subscribe_*(tx: Tx<Event>) ->
()`) were dropped from every domain trait during the port:

| Domain          | Status                                  |
|-----------------|-----------------------------------------|
| transport       | `subscribe_state`, `subscribe_all_projects` retired |
| markers         | `subscribe` retired                     |
| regions         | `subscribe` retired                     |
| tempo_map       | `subscribe` retired                     |
| tracks          | `subscribe` retired                     |
| items           | `subscribe_items` retired               |
| takes           | `subscribe` retired                     |
| fx              | `subscribe_events` retired              |
| routing         | `subscribe` retired                     |
| action_registry | `subscribe_actions` retired             |
| live_midi       | `subscribe_input` retired               |

Two surviving broadcasters in daw-reaper (`init_item_broadcaster`,
`init_tempo_map_broadcaster`) hint at the pre-port shape. Sync,
session, and the web UI all need this surface restored.

## Reference: helgobox / ReaLearn

[helgobox](https://github.com/helgoboss/helgobox) ships a working
streaming surface over REAPER. We're adopting three patterns from
it verbatim:

### Pattern 1: Central event hub

Helgobox has `ProtoHub` owning all broadcast senders
(`infrastructure/proto/hub.rs` + `senders.rs`). One bag of
`tokio::sync::broadcast::Sender<EventBatch>` channels. Every code
site that detects a change calls `hub.notify_*()` — single source
of truth for "where do events go?"

Why this beats per-service ad-hoc broadcasters:
- One place to register a new subscriber.
- One place to debug "why isn't my event arriving?"
- Service impls hold an `Arc<DawEventHub>` and `subscribe()` on
  whichever channel they expose. The hub doesn't depend on the
  services; the services don't depend on each other.

### Pattern 2: Occasional vs continuous channels

For high-rate domains (transport position, FX param sweeps, audio
meters) helgobox keeps **two** channels per domain:

- **Occasional** — config/structural changes. Low frequency,
  batched per main-loop tick. Every event matters.
  Example: `OccasionalGlobalUpdateBatch`, transport play/stop.
- **Continuous** — real-time samples. High frequency, lossy is
  fine. broadcast channel drops oldest on backpressure.
  Example: `ContinuousMatrixUpdate`, transport position.

Net: a sleepy client doesn't accumulate hundreds of position ticks
in its queue. They drop. The next one is "close enough."

### Pattern 3: ChangeDetectionMiddleware (not polling)

`reaper-high::ChangeDetectionMiddleware` wraps REAPER's Control
Surface API. REAPER **calls us** when something changes —
`TrackAdded`, `FxParameterTouched`, `MarkerSet`, etc. — and the
middleware translates into a typed `ChangeEvent` enum. Helgobox
drains those events on every main-loop tick.

```rust
// helgobox/main/src/domain/control_surface.rs:432
while let Ok(event) = self.control_surface_event_receiver.try_recv() {
    let mut q = self.change_event_queue.borrow_mut();
    self.handle_event_internal(&event, &mut q);
}
```

Net: no Rust-side polling timer. Lower CPU, deterministic ordering,
nothing missed.

## Target architecture

```
       REAPER main thread                 Tokio runtime
  ┌──────────────────────────┐       ┌────────────────────────┐
  │                          │       │                        │
  │  Control Surface tick    │       │   *Stream services     │
  │           │              │       │      (per domain)      │
  │           ▼              │       │           │            │
  │  ChangeDetectionMiddle-  │       │           │            │
  │  ware  →  ChangeEvent    │       │     subscribe rx       │
  │           │              │       │           ▼            │
  │           ▼              │       │   tokio::broadcast     │
  │  DawEventHub.notify_*    │  ───▶ │     ::Receiver         │
  │  (one method per dom.)   │       │           │            │
  │                          │       │           ▼            │
  │  Position poll @30Hz     │       │    forward into        │
  │           │              │       │   vox::Tx<Event>       │
  │           └─────────────────────▶│           │            │
  │                          │       │           ▼            │
  └──────────────────────────┘       │   client receives      │
                                     │   via WebSocket / IPC  │
                                     └────────────────────────┘
```

**DawEventHub** (new, in daw-reaper):
```rust
pub struct DawEventHub {
    // Occasional channels — every event matters, low freq.
    pub markers_tx:        Sender<MarkerEvent>,
    pub regions_tx:        Sender<RegionEvent>,
    pub tracks_tx:         Sender<TrackEvent>,
    pub items_tx:          Sender<ItemEvent>,
    pub takes_tx:          Sender<TakeEvent>,
    pub fx_tx:             Sender<FxEvent>,
    pub routing_tx:        Sender<RoutingEvent>,
    pub tempo_map_tx:      Sender<TempoMapEvent>,
    pub action_tx:         Sender<ActionEvent>,
    pub project_tx:        Sender<ProjectEvent>,
    pub transport_state_tx: Sender<TransportState>,

    // Continuous channels — high freq, drop-old policy.
    pub position_tx:       Sender<PositionTick>,
}
```

Buffer sizes: 100 for occasional (matches helgobox), 16 for
continuous (drop oldest fast).

## Sibling streaming trait pattern

Each domain gets a separate trait, not folded into the existing
sync trait. Pure `#[vox::service]` (the architect-rpc bridge is
for sync methods only; streaming methods are already async).

```rust
// daw-proto/src/transport/stream.rs

#[vox::service]
pub trait TransportStream {
    /// Subscribe to play/stop/record/loop state changes.
    /// Occasional channel — every state transition is delivered.
    async fn subscribe_state(&self, tx: Tx<TransportState>);

    /// Subscribe to position ticks. Continuous channel — pushed at
    /// ~30Hz; consumer may miss intermediate ticks under load.
    async fn subscribe_position(&self, tx: Tx<PositionTick>);
}
```

Each trait gets its own module: `daw-proto::{transport_stream,
markers_stream, regions_stream, …}`. The `Service` token slots into
`Reaper::layers()` alongside the existing sync-trait service tokens.

```rust
// daw-reaper/src/services.rs (after streaming is wired)
impl Services for Reaper {
    fn layers() -> impl Layer<Reaper> {
        layers![
            transport::Service, transport_stream::Service,    // sync + stream
            markers::Service,   markers_stream::Service,
            regions::Service,   regions_stream::Service,
            // ... 9 more pairs
        ]
    }
}
```

## Implementation plan

### Phase 0 — design (this doc)
✓ Captured pattern, target architecture, sibling-trait shape.

### Phase 1 — DawEventHub + transport stream end-to-end

Smallest end-to-end deliverable. Validates the full stack.

1. **Create `DawEventHub`** in `daw-reaper/src/event_hub.rs`. Two
   senders to start: `transport_state_tx` (occasional) and
   `position_tx` (continuous). `OnceLock<Arc<DawEventHub>>` so
   service impls can fetch the same hub instance.

2. **Install `ChangeDetectionMiddleware`** at REAPER plugin
   bootstrap (`daw-reaper/src/bootstrap.rs`). Drain change events on
   each main-loop tick; map `ChangeEvent::PlayStateChanged` etc.
   into `TransportState` and push to `transport_state_tx`.

3. **30Hz position timer** — REAPER's `OnTimer` callback (the same
   one used for surviving broadcasters). Snapshot
   `GetPlayPosition()` on the main thread, push to `position_tx`.

4. **Define `TransportStream`** in `daw-proto/src/transport/stream.rs`
   with the two subscribe methods above. Export from
   `daw-proto/src/lib.rs`.

5. **Implement `TransportStream` for Reaper** in
   `daw-reaper/src/transport_stream.rs`. Each `subscribe_*` method:
   - Fetch `DawEventHub::global()`.
   - Subscribe a `Receiver` off the appropriate sender.
   - Spawn a forwarder task that pumps `recv()` into `tx.send()`
     until either side disconnects.

6. **Wire `transport_stream::Service` into `Reaper::layers()`**.

7. **Web UI consumer** — sketch a small example showing
   `TransportStreamClient` subscribing over the bridge's existing
   Unix-socket / future axum WS surface.

Acceptance: web UI demo shows the transport cursor updating in
near-real-time when REAPER is playing.

### Phase 2 — replicate to other domains

For each of markers, regions, tracks, items, takes, fx, routing,
tempo_map, action_registry, project, live_midi:

- Add a `Sender<DomainEvent>` to `DawEventHub`.
- Wire the change-detection mapping (or REAPER notifier callback)
  into the hub.
- Define `<Domain>Stream` trait in daw-proto.
- Implement on Reaper subscribing off the hub.
- Add `Service` token to `Reaper::layers()`.

11 domains × ~30 lines each. Mechanical once Phase 1 is solid.

### Phase 3 — daw-control facade re-exposure

In `daw-control`, expose the stream clients alongside the sync
clients. So consumers write:

```rust
let daw = daw::rpc::Daw::get();
let mut rx = daw.project().transport().subscribe_state().await?;
while let Some(state) = rx.next().await {
    update_ui(state);
}
```

(Method names may rebadge to `transport_stream()` to keep them
separate from sync-side `transport()`; design TBD.)

### Phase 4 — sync engine consumer update

Rewire `sync/crates/sync/src/subscriptions.rs` to use the new
client paths. Each `project.transport().subscribe_state()` →
`project.transport_stream().subscribe_state()`. Mechanical.

### Phase 5 — session consumer update

Same for `session/crates/session/src/setlist_service/*` and
`apps/desktop/src/services.rs`. Their `subscribe_*` calls become
`*_stream().subscribe()`.

### Phase 6 — sibling non-streaming API drift

Separate from streaming, but parallel work:
- `Markers::add_in_lane`, `set_lane` — restore (these are sync
  methods, not streaming; were dropped in the port for unclear
  reasons).
- `daw::Error` — restore the root re-export.

Discovered during the session migration; needed before session
fully compiles.

## Non-goals for this work

- **Atom/reactive UI integration.** That's UI state mgmt, separate
  concern. See the effect-atom analysis in earlier conversations.
- **Cross-process via WebRTC / QUIC.** Vox transports are
  sufficient (Unix socket, WebSocket).
- **Backpressure semantics beyond drop-old-on-continuous.**
  Tokio's `broadcast` channel is the only mechanism we need.
- **Replacing `init_item_broadcaster` / `init_tempo_map_broadcaster`
  yet.** Fold them into the hub during Phase 2.

## Open questions

1. **Where does `DawEventHub` live structurally?** Probably a
   field on `Reaper` (after we wire dependency-injection-via-struct-
   fields), exposed via `Reaper::hub() -> &DawEventHub`. Today
   `Reaper` is zero-sized; the hub forces it to grow an `Arc`. Fine
   as long as `Clone` stays cheap.

2. **Initial state snapshot on subscribe?** Effect's pattern is "send
   current state immediately, then deltas." Tokio `broadcast` doesn't
   do this — subscriber only sees post-subscription events. Helgobox
   solves it with `send_initial_events`. We should do the same:
   each `subscribe_*` method does an initial read + push before
   handing the channel to the forwarder.

3. **Echo suppression for sync use case.** Sync needs to filter out
   events that originated from a remote peer (avoid round-trips).
   The hub-level pattern doesn't help; suppression has to live in
   the sync engine (it already does, via `SuppressionSet`).

4. **`PositionTick` shape.** Just `f64` seconds, or `{ position:
   f64, qn: f64, project_id: String }`? Probably the latter — the
   web UI may show multiple projects.

## Reference

- helgobox: `/tmp/helgobox/main/src/infrastructure/proto/hub.rs`,
  `senders.rs`, `control_surface.rs::process_change_events`.
- Existing daw-reaper broadcasters:
  `daw::reaper::init_item_broadcaster`,
  `daw::reaper::init_tempo_map_broadcaster`.
- Sync engine consumer: `sync/crates/sync/src/subscriptions.rs`.
- Service composition: [`service-composition.md`](./service-composition.md).
- Layer/Services primitives: architect/macros/architect/src/layer.rs.
