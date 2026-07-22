# Platform Primitives

The north star: a rig, app, or feature contains **only business logic** — DSP,
domain rules, UI intent. Every cross-cutting concern — transport, serving, device
I/O, MIDI lifecycle, process bootstrap — is owned by a shared **primitive**:
`architect` (RPC / transport / serving / rig glue), `daw-audio-io` +
`daw-standalone` (audio device + callback), and `midicore` (MIDI). A hand-rolled
copy of anything a primitive owns — or should own — is a defect.

This is a monorepo-wide contract (tracey prefix `r`, `r[primitives.*]`). It exists
because an audit found the same plumbing copy-pasted across binaries and rigs, with
drift; the requirements below say where each concern must live.

## Principles

r[primitives.only-business-logic]
A rig/app/feature MUST contain only business logic. Every cross-cutting
concern — transport wiring, request serving, audio-device and MIDI I/O, and
process bootstrap (runtime, logging, panic handling, bind address, health) — is
delegated to a primitive. The measure: reading a consumer, a person sees domain
logic, not plumbing.

r[primitives.no-reinvention]
No consumer re-implements what a primitive owns, and no parallel crate may
**shadow** a primitive (orphaned duplicates are deleted, not kept in sync). Shared
native-dependency versions (cpal, pipewire, jack, midir) are pinned in **one**
place; a consumer MUST NOT pin a divergent version.

r[primitives.drift-is-a-bug]
Plumbing that is copy-pasted across two or more consumers belongs in a primitive.
When copies have **drifted** (a fix or feature landed in one copy but not the
others), that divergence is a correctness bug; the resolution is to hoist the
concern into a primitive, never to hand-resync the copies.

## architect owns transport, serving & rig glue

r[primitives.architect.serve-router]
Serving an `architect::LayerRouter` is a one-call operation. architect MUST expose
a router→acceptor helper so the closure
`lane_acceptor_fn(|_req, c| { c.handle_with(router.clone()); Ok(()) })` is never
hand-written (a `LayerRouter` already *is* a handler), a ready axum vox
route/handler (`router_route(router)`), and an iroh serve-router helper
(load-or-create key + bind + write endpoint-id + serve). A consumer serves its
router over WebSocket and/or iroh without writing the transport dance.

r[primitives.architect.engine-host]
One helper assembles a full headless engine host: the multi-thread tokio runtime,
tracing/EnvFilter, the backtrace panic hook, the `/health` route, bind-address
resolution, and the router exposed over **every** enabled transport (WS + iroh),
plus an optional embedded SPA. A new engine binary is then business-logic-only:
construct the backends, mount the router. (Today each binary re-assembles runtime
+ tracing + panic + health + bind + WS-handler by hand.)

r[primitives.architect.web]
Serving a router alongside an embedded single-page web bundle is a primitive
(embedding via `include_dir`, the content-type table, the SPA `index.html`
fallback, and dev-vs-deployed bundle discovery). No app hand-rolls a `WebBundle`
enum, `content_type_for`, or `embedded_asset` — the ~130 lines of pure plumbing in
the engine binary become one call.

r[primitives.architect.rig-backend]
The RigBackend glue is generated, not hand-written. A `RigBackend` trait/macro
provides: `router()` = `self.clone().into_router()`, the `events_hub()` accessor,
the `PubSub::sliding(N)` event hub, and the **meter/status pump** — the fixed-
interval loop that publishes meters + MIDI per tick, publishes a full status only
on the running-edge, and rescans MIDI ports for hot-plug. The pump ships with one
interval constant, the once-start guard, and the hot-plug rescan built in, so rigs
**cannot drift** (today guitar/drums/keys each hand-roll it; keys lacks hot-plug,
guitar lacks the once-guard, and each declares its own interval constant). A rig
implements only what is instrument-specific: its event enum, its service method
set, and how a program/kit/preset is realized.

## Audio I/O owns device & callback

r[primitives.audio.engine]
Realtime audio — device selection, sample-rate/buffer negotiation, the callback
loop, and duplex — is owned by `daw-audio-io` (host/device/open/duplex) plus the
`daw-standalone` audio engine. A rig NEVER opens cpal/pipewire/jack directly; it
supplies a render/process closure and holds only DSP. (Guitar and the sampler
already satisfy this — it is an invariant to preserve, not a goal.)

r[primitives.audio.one-version]
The native audio-backend versions (cpal, pipewire, jack) are pinned once in the
workspace. A consumer that opens capture/output MUST go through `daw-audio-io` at
the workspace-pinned version rather than depending on a divergent one.

## midicore owns MIDI

r[primitives.midi.facade]
All MIDI I/O goes through `midicore`: the wire types + byte codec
(`midicore-proto`) and the OS backend (`midicore-midir`). No parallel MIDI crate
may shadow it; a duplicate that re-implements `MidiInput`/`MidiStream`/`input_ports`
is deleted, not maintained.

r[primitives.midi.client-identity]
`midicore` mints a **unique client name per open** (an atomic counter), so N
concurrent rigs never merge into one node under pipewire-jack. Client identity is
owned by the primitive, not a shared leaf-crate constant — the rig-merge failure
(every rig opening the same-named client and only one callback firing) is fixed
*in midicore*, once, for every consumer.

r[primitives.midi.attach]
The MIDI attach lifecycle is a midicore helper, not re-derived per rig: port
selection (a stored name → `NameContains`, none → `All`), **drop-the-old-handle
before opening the new one** (the ALSA-seq queue-exhaustion invariant), the
hot-plug rescan, and the monitor-tap → live-MIDI-sink wiring. A rig calls one
helper; it does not re-implement the selection mapping or the teardown ordering
(copy-pasted across all rigs today, with drift).

r[primitives.midi.output]
`midicore` provides MIDI **output** symmetric to input (port enumeration, connect,
send), so a consumer that needs to send MIDI (e.g. the Kontrol light-guide) uses
the primitive instead of re-rolling raw `midir`.

## Enforcement

r[primitives.audit]
New plumbing is reviewed against the primitives: a change that hand-rolls
transport, serving, device I/O, or MIDI lifecycle that a primitive owns — or
should own — is either rejected or accompanied by extending the primitive. The
repo's duplication/god-node graph (graphify) is the periodic check for regressions.

---

## Refactor backlog (non-normative)

Concrete sites the requirements above target, from the audit. This section is a
map for implementers, not part of the contract.

**architect (extend, then collapse consumers):**
- router→acceptor helper — collapses the identical closure at
  `apps/fasttrackstudio/src/engine_main.rs:165` & `:205`,
  `apps/fasttrackstudio/cli/src/session_engine.rs:41`,
  `apps/task/server/src/lib.rs:1257` & `:1712`, and architect's own
  `libs/architect/architect/src/local.rs:60` & `:139`.
- vox axum route — collapses `vox_handler` (`engine_main.rs:162`), `ws_handler`
  (`fts-cli/session_engine.rs:37`), and the two task-server copies.
- `serve_iroh` (`engine_main.rs:179`) and the client-side identity bootstrap
  (`apps/fasttrackstudio/src/rig_view.rs:104`) → one iroh serve/dial helper.
- engine-host helper — folds the tokio-runtime (4+ copies), tracing (5 copies),
  panic hook, `/health` + bind (4 copies) into one assembler.
- SPA web serving — `engine_main.rs:41-160` (`WebBundle`/`web_bundle`/
  `content_type_for`/`embedded_asset`).
- `RigBackend` trait/macro — `router()`, `events_hub()`, `PubSub::sliding(64)`,
  and `spawn_meter_pump` across `features/rigs/keys/src/backend.rs:201`,
  `features/rigs/drums/src/backend.rs:502`, `features/rigs/guitar/src/session.rs:222`.

**midicore:**
- Delete the orphaned duplicate `features/audio/daw-midi-io` (zero real consumers;
  clones `crates/midicore/midicore-midir`).
- Add the unique-per-open client name (atomic) in `midicore-midir` (the shared
  `const CLIENT = "midicore-midir"` at `crates/midicore/midicore-midir/src/lib.rs:28`
  is the merge-bug root; no unique-name fix currently exists in the primitive).
- Add an attach/hot-plug helper; fold `sampler_rig.rs:1083`, `keys/src/lib.rs:260`,
  `keys/src/backend.rs:169`, `drums/src/backend.rs:410`, `guitar/src/session.rs:248`.
- Add MIDI output; fold `crates/kontrol/kontrol/src/output.rs`.

**audio:**
- Fold `features/fx/eq/apps/eq-standalone/src/pipewire_capture.rs` (private
  cpal 0.15 / pipewire 0.8 capture) onto `daw-audio-io` at the workspace versions.
