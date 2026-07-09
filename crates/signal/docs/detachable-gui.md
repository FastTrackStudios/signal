# Detachable GUI — the remote-control architecture

> **Status**: Live for the guitar rig (desktop, in-process transport).
> This is a **strict requirement** for all Signal UI going forward: the core
> is 100% headless, and every GUI is a remote.

## The rule

**UI code never calls the engine. It only speaks generated vox clients.**

The rig core (`GuitarRigBackend` in `signal-guitar`) is headless: it owns the
audio engine, profiles, and footswitch state, and exposes everything through
`#[architect::rpc]` service traits defined in `signal-guitar-proto`. Where the
GUI runs — and how it connects — is decided at the app root, nowhere else:

| Front-end | Transport |
|---|---|
| Desktop app (today) | `architect::LocalServer` — in-process vox memory link |
| Browser (wasm) | `vox-websocket` → `architect::axum_ws::serve` on the core |
| Plugin editor | in-process link, or remote to a headless plugin host |
| Embedded box / pedalboard | headless binary serving the same router |
| External editors / controllers | any vox transport (unix socket, WS, …) |

Only the UI has to port to wasm — the engine never does.

## The pieces

```
features/rigs/guitar/
  proto/            signal-guitar-proto — wire types (facet) + service traits.
                    wasm-clean: no audio backend, no Dioxus, no platform I/O.
  ui/               signal-guitar-ui — wasm-clean rig components (perform grid,
                    meters, chain strip, settings modal, RigGraph, use_rig_state,
                    GuitarRigRemote root). Shared by desktop + web.
  (root)            signal-guitar — AmpEngine + the headless GuitarRigBackend
                    (session.rs: owns Arc<Mutex<ProfileRig>>, implements the
                    services; profiles.rs: hardcoded Worship profile).
  examples/headless_rig.rs   smoke test: open + drive the core, no GUI.

features/signal/
  signal-grid/      GridSlot + the pure template/chain → slot conversions
                    (extracted from signal-browser; deps: signal-proto + uuid).
  signal-grid-ui/   the zoomable/pannable module/wire graph (DynamicGridView,
                    RigGridPanel, inspector, Knob) — extracted from signal-ui,
                    wasm-clean (CSS transforms + Dioxus events only; timing via
                    architect::platform::sleep). signal-ui re-exports it under
                    the old paths, so desktop consumers were untouched.
```

- **Services** (one `#[architect::rpc]` trait per module — the macro emits a
  `Service` token, `serve`/`layer` verbs, and a `<T>Client` per module):
  - `signal_guitar_proto::rig::Rig` — start/stop, `status()` (batched
    running+meters+active patch), `perf()`, `chain()`, `press_stack`,
    `toggle_fx/boost`, `tap_tempo`, `toggle_block_bypass`, `set_block_param`.
  - `signal_guitar_proto::audio::AudioSettings` — `devices()`, `prefs()`,
    `save_prefs()`.
- **Backend**: `GuitarRigBackend: Services` → `backend.router()` yields the
  `LayerRouter` that mounts on any vox transport. `ProfileRig` is `Send` but
  not `Sync` (pipewire owns a `*mut pw_thread_loop`), so it lives behind
  `Arc<Mutex<…>>`; sync service methods marshal through
  `CurrentThreadDispatcher`.
- **Events**: `#[subscribe] fn events(&self) -> RigEvent` on the `Rig` trait
  emits the `RigStream` sibling service. The backend owns a
  `PubSub<RigEvent>` hub: every mutation publishes full-state `Perf`/`Chain`
  events; a meter-pump thread publishes `Status` at ~30 Hz while running
  (meters cross from the RT thread via atomics — `PubSub::publish` locks, so
  it never runs on the audio callback).
- **UI** (`signal-ui`): `GuitarRigView` consumes `RigClient` /
  `RigStreamClient` / `AudioSettingsClient` from Dioxus context
  (`try_consume_context`), seeds state with one `status/perf/chain` fetch,
  then goes live via `architect::use_stream` — no polling; event handlers
  spawn async client calls. The mirror types (`PerformanceModel`,
  `LiveBlock`, `AudioPrefs`, …) are re-exports of the proto crate.
- **App** (`apps/desktop`): ~90 lines of wiring — build backend, auto-`start()`,
  `LocalServer::serve(backend.router(), Scope::new())`, establish clients in a
  `use_resource`, provide via context, mount `GuitarRigView`.

## Sharp edges learned

- A vox `Caller` is **service-bound at client construction** — sibling
  services cannot share one caller (`assertion failed: Caller service
  mismatch`). Establish one link per client; in-process links are free.
- The `#[architect::rpc]` client/dispatcher codegen is gated on a `vox`
  cargo feature **in the proto crate** (`vox = ["dep:vox", "architect/vox"]`),
  mirroring architect's `example-proto`.
- One `#[rpc]` trait per module — the emitted `Service` token and
  `serve`/`layer` fns live at module scope and would collide.
- **vox 0.10 scopes channels to their request** (spec'd + tested upstream):
  delivering the subscribe response terminates the sink
  (`RequestTerminated(ResponseDelivered)`). We fixed architect for this
  (local checkout): the emitted stream hosts + Entity `subscribe` now hold
  the request open (`pending().await` after attach) — the in-flight call
  *is* the subscription, cancelling it unsubscribes — and
  `architect::use_stream` races the call against the event pump instead of
  awaiting "established". Raw-client subscribers (`tokio::spawn` the
  subscribe call, abort to unsubscribe) must wait for attach before
  mutating: no replay on the hub, so pre-attach publishes are lost (the
  first meter event doubles as the attach confirmation).
- CRDT doc-sync (`crdt/sync.rs`) still uses attach-and-return **and returns
  a value from the attach call** — it needs a protocol rework for vox 0.10
  channel scoping; untouched, currently broken over the wire.

## Next steps

1. ~~`#[subscribe]` event streams~~ — **done**: `RigEvent` stream replaces
   polling (see Events above). Later refinement: move meter capture onto
   `architect::rt::rt_channel` drained from the pump thread.
2. ~~`apps/web`~~ — **done**: `apps/signal-engine` (`signal-engine`, formerly `signal-rigd`) serves the same
   router at `ws://<host>:4040/vox` (axum_ws); `apps/web` (`signal-web`,
   dioxus wasm) mounts `GuitarRigRemote` from the new feature-scoped
   `features/rigs/guitar/ui/` crate (`signal-guitar-ui` — wasm-clean rig
   components + `use_rig_state`, shared with the desktop shell). Run:
   `cargo run -p signal-engine` then `cd apps/web && dx serve --platform web`.
   The Edit mode mounts the real graph (`signal-guitar-ui::RigGraph` →
   `signal_grid_ui::RigGridPanel`) with the live chain resolved onto the
   guitar-rig template canvas; param edits round-trip over the Rig service.
   Screenshots: `docs/web-remote-{perform,edit,graph,graph-select}.png`.
   Gotcha: the grid panel sizes itself with `flex-1 h-full` — its parent
   must be a flex column or the canvas collapses to zero height (renders in
   the DOM, paints nothing). Tailwind: the compiled
   sheet comes from `just tailwind` (`apps/desktop/input.css` `@source`
   globs — keep them pointing at every UI crate; a stale sheet silently
   drops state classes like `opacity-[…]`/`saturate-50`).
3. **architect atom adoption** — `Store`/`Mutation`/`use_store_stream` +
   `Connection`/`use_connect_supervised` (reconnect with backoff) instead of
   hand-rolled signals, per DESIGN.md Phase 4.
4. **Profiles as entities** — `worship_profile()` is still hardcoded;
   Profile/Stack/Patch become `#[architect::Entity]`s (DESIGN.md Phase 3) so
   the editor UI can CRUD them over the same link.
5. **Blitz validation** — architect atom is renderer-agnostic in principle,
   but the Blitz (prod/plugin) path is unproven with vox clients; validate
   `--no-default-features` at runtime.
