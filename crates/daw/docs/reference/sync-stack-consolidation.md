# Sync-stack consolidation into daw

Successor plan to [`synchronization-engine-lift.md`](./synchronization-engine-lift.md).
Folds the rest of the sync repo into daw and re-shapes the wire-protocol code
as reusable library crates other domains (`session`, future Pro-Tools sync,
etc.) can consume.

## End state

```text
daw/
  crates/
    daw-synchronization/    engine — already lifted (de98a46 + this branch)
    daw-network/            NEW: TCP peer mesh, handshake, clock calibration
    daw-link/               NEW: Ableton Link adapter (feature-gated)
    daw-bridge/             absorbs sync-extension role: hosts engine, wires
                            it to peer mesh + Link
sync/
  (retired — or thin compat shim during the migration window)
```

`daw-network` is the load-bearing new crate: it owns the peer-mesh shape
(handshake, framed envelopes, clock calibration, fan-out broadcast) and lets
other consumers (session sharing, telemetry, etc.) pick it up without
pulling the sync engine. Start concrete on `SyncEvent`; generalize over
event type in a follow-up if a second consumer arrives.

## Phases

### Phase 1 — `daw-network` crate
- Create `crates/daw-network/` with `network.rs` lifted from sync repo.
- Deps: `daw-synchronization` (for `SyncEvent` + `SuppressionSet`),
  `tokio`, `tokio-util`, `facet`, `facet-postcard`, `tracing`.
- No `daw` facade dep needed — pure wire layer.
- `cargo check -p daw-network` clean.

### Phase 2 — `daw-link` crate
- Create `crates/daw-link/` with `link.rs` lifted.
- Deps: `rusty_link` (required, no feature gate — the crate exists to host
  Link). Apps that don't want Link just don't depend on `daw-link`.
- `cargo check -p daw-link` clean.

### Phase 3 — Tests
- Move `sync/crates/sync/tests/reaper_sync_*.rs` into either
  `daw-synchronization/tests/` (engine-level: `actions`, `action_trigger`,
  `shm`) or `daw-network/tests/` (network-level: `network_latency`,
  `multi_transport`, `position`, `full_session`).
- `link_engine.rs` → `daw-link/tests/`.
- The multi-instance tests need a REAPER extension to load the engine in
  each spawned instance — that's Phase 4.

### Phase 4 — Extension role in `daw-bridge` *(done)*
- ✓ `sync` cargo feature on `daw-bridge` (default-on) pulls in
  `daw-synchronization` + `daw-network`.
- ✓ `daw-bridge::sync_runtime` constructs `SynchronizationEngine` + `PeerMesh`,
  writes `FTS_SYNC_EXT/{status,pid,peer_id,mesh_port,peer_count}`, polls
  `FTS_SYNC_EXT/connect_peers` for direct-connect orchestration.
- ✓ Spawned from `plugin_main` when `FTS_SYNC_ENABLED=1`. `reaper_test`
  macro now injects that env per-instance so multi-instance tests get the
  runtime automatically.
- ✓ Tests lifted to `daw-bridge/tests/`:
  - `sync_position` (4-instance drift verification)
  - `sync_multi_transport` (3-instance transport propagation)
  - `sync_network_latency`
  - `sync_full_session`
- Skipped for now: mDNS (avoid avahi system dep). Tests use
  `connect_sync_peers_direct` instead of `connect_sync_peers`.

### Phase 4 — Extension role in `daw-bridge` *(legacy outline)*
- Lift `sync-extension/src/lib.rs` + `link_bridge.rs` into `daw-bridge`
  (or a new `daw-bridge-sync` sibling if it'd bloat daw-bridge).
- Provides a REAPER extension entry point that constructs:
  - `SynchronizationEngine` (from daw-synchronization)
  - `PeerMesh` (from daw-network)
  - Link bridge (from daw-link)
  - Wires `engine.outbound()` → `mesh.send_all()` and
    `mesh.inbound()` → `engine.apply_remote()`.
- Replaces the `FTS_SYNC_EXT` ext_state beacons the tests poll for.
- Result: tests in Phase 3 can run via `cargo xtask reaper-test`.

### Phase 5 — `daw_module.rs` placement
- The `SyncModule` (REAPER actions registration) belongs alongside the
  engine. Move into `daw-synchronization::module` or into `daw-bridge`
  next to the extension entry. Decide based on whether the action defs
  (currently in `sync-proto`) move too — recommend lifting them into
  `daw-synchronization::actions` so the whole sync domain is in one place.

### Phase 6 — Retire sync repo
- After Phases 1–5, `sync/crates/sync` has nothing left except the
  re-export shim `lib.rs` and `daw_module.rs`. Either:
  - Delete the sync repo entirely; or
  - Keep `sync` as a thin compat crate re-exporting from
    `daw-synchronization` + `daw-network` + `daw-link` for any
    out-of-tree consumers, with a deprecation note.

## Architectural rules
- **Engine ↛ network.** `daw-synchronization` must not import
  `daw-network`. The engine exposes `outbound()` and `apply_remote()`;
  the extension wires them up.
- **Network ↛ engine internals.** `daw-network` only sees `SyncEvent` +
  `SuppressionSet`. No reach into engine state.
- **Library-shaped.** Both new crates should be usable by `session` or
  any other domain — no hardcoded REAPER assumptions in the network or
  link crates. REAPER-specific glue lives in `daw-bridge`.

## Why this order
- Phase 1 + 2 are pure code moves — they unblock everything downstream
  and produce immediate `cargo check` wins.
- Phase 3 (tests) moves files but can't run until Phase 4 lands.
- Phase 4 is the gnarly one (REAPER extension entry, ABI surface) — best
  tackled with the rest already in place.
- Phase 6 (retire sync repo) is bookkeeping; do last when nothing depends
  on it.

## Gotchas carried over from prior session
- Workspace `[patch]` table needs `keyflow-daw-analysis` + `keyflow-midi`
  whenever local path deps activate (see sync repo `Cargo.toml`).
- `session` crate currently has 17 API-drift errors against the local
  daw facade — unrelated to this work but blocks any sync-repo
  full-workspace check until session is updated.
- avahi system headers required (vox-discover dep) — sync repo's nix
  devshell provides them; tests on bare host fail without
  `BINDGEN_EXTRA_CLANG_ARGS`.
