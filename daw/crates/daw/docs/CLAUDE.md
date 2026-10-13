# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

## Issue Tracking

Use GitHub Issues for follow-up work. File a GitHub issue for any remaining
known bug, missing test, or implementation task that should survive the current
session.


## Build & Test

```bash
cargo check -p daw              # Type-check facade
cargo check --workspace         # Type-check all
cargo test -p daw-audio-graph   # Run audio graph tests
```

## Architecture

This repo follows the **crate facade pattern**. Apps depend only on `daw`
(the facade), never on internal crates.

### Directory layout

`crates/` holds the **spine** — the always-on contract + the published
facade. `features/` holds every **optional, feature-gated** crate; each
subtree maps to a cargo feature of `daw` (so "Reaper compatibility is a
feature of `daw`" is literal, not a metaphor).

```
crates/                  # spine — mandatory contract + published facade
  daw                    # facade — the only public API surface / crates.io artifact
  daw-proto              # protocol/domain types: service traits + capability
  daw-control            # high-level control API
  daw-module             # module-host interface
  daw-test-macro         # shared test tooling
  fts-devtools

features/                # each subtree = an optional cargo feature of `daw`
  backends/              # integration targets — read/drive other DAWs
    reaper/              # daw-reaper, dawfile-reaper, *-embed, *-dioxus, bridge, …
    protools/ ableton/ logic/ aaf/ dawproject/   # the dawfile-* codecs
  standalone/            # our own DAW (top-level: it is NOT a backend)
  sync/                  # daw-synchronization, daw-network, daw-link, daw-audio-sync
  audio/                 # daw-audio-graph, daw-allocator, audio-controls, fts-audio-proto
  ui/                    # daw-ui
```

### Feature flags (on the `daw` facade)

- `reaper`, `reaper-ui`, `standalone` (+ `standalone-audio`, `standalone-rpp`)
- Source-format **parsers** (format → daw-proto types): `protools`, `ableton`,
  `logic`, `aaf`, `dawproject`, `formats` (= all). Exposed as `daw::<format>`.
- File→RPP **conversion**: `convert` (all importers) or per-format
  `convert-protools` / `convert-ableton` / `convert-aaf` / `convert-dawproject`
  so a specialized build compiles only the parsers it needs.

`daw-reaper` carries matching per-format importer features
(`default = all-formats`); take it with `default-features = false` to gate.

**Not foldable into the facade** (each depends on `daw` → cycle): the sync
stack (`daw-synchronization`/`daw-network`/`daw-link`) and
`daw-extension-runtime` stay public sibling crates. Extension authors depend
on `daw` + `daw-extension-runtime` together.

## Platform Targets

The processing-core crates (`daw-audio-graph`, `daw-builtin-fx`) must run
in all three environments. Only I/O adapter crates are platform-specific.

| Target | Notes |
|---|---|
| **Native** (Linux/macOS/Pi) | Full `std`, JACK/ALSA/CoreAudio via `cpal` |
| **WASM / Browser** | AudioWorklet drives `AudioGraph::process()`; no `cpal` |
| **Embedded `no_std`** | `#![no_std]` + `alloc`; no OS, no threads |

### Processing-core crate rules (`daw-audio-graph`, `daw-builtin-fx`)

- `#![no_std]` compatible — depend only on `core` and `alloc`, never `std`
  directly. Gate `std`-only code behind `#[cfg(feature = "std")]`; keep
  the `std` feature additive/default.
- **No heap allocation on the hot path** — pre-allocate in `reset()`; the
  `process()` path must never call `Vec::push`, `Box::new`, or any allocator.
- **No threads** — the graph is driven synchronously by whatever callback
  owns it (cpal, AudioWorklet, bare-metal ISR). Never spawn tasks or threads
  inside processing crates.
- **No platform I/O** — no `cpal`, no `web-sys`, no MIDI drivers inside the
  graph core. I/O lives only in adapter crates.
- **`AudioNode: Send`** — keep the bound; auto-satisfied in single-threaded
  WASM, required for multi-threaded WASM / native.

## Key Rules

### Async & Concurrency
- Use `architect::platform::spawn` instead of `tokio::spawn` (wasm-cfg-split:
  tokio on native, `spawn_local` on wasm)
- Use `tokio::sync::{Mutex, RwLock}` for locks/channels (moire is retired — Jul 2026)
- Never hold std sync primitives across `.await`
- Processing-core crates (`daw-audio-graph`, `daw-builtin-fx`) must never use async

### RPC Services
- Service traits use `#[architect::rpc]` (sync trait → hidden async
  vox mirror + client + `Service`/`layer`/`serve`/`Dispatcher`/`descriptor`,
  emitted under the consumer crate's `vox` feature). Never apply
  `#[vox::service]` directly.
- The capability system (`daw_proto::capability`) is advisory metadata
  a backend can publish — it is not wired into `LayerRouter` dispatch.
- Max 4 params per method (Facet constraint)
- Use `Tx<T>` / `Rx<T>` for streaming
