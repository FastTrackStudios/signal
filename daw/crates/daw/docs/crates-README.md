# DAW Crates

This directory contains the DAW protocol, control wrappers, REAPER
implementation, bridge extension, and facade crates used by FastTrackStudio.
The current service transport is vox.

## Architecture

The DAW system is split into multiple crates with explicit ownership:

```
daw-proto          # Vox service traits, generated clients, shared types
daw-control        # Ergonomic async Rust API over a vox Caller
daw-control-sync   # Blocking/local helpers, including in-process LocalCaller
daw-reaper         # REAPER service implementations
daw-bridge         # REAPER extension exposing daw-reaper over a Unix socket
daw-standalone     # Reference/mock service implementation
daw                # Public facade crate for consumers
apps/daw           # `daw` CLI, a thin service client
```

## Crates

### `daw-proto` - Protocol Definitions

`daw-proto` is the shared service contract. It owns the vox service traits,
request/response types, event types, and generated clients for services such as
transport, tracks, project, FX, markers, regions, batch operations, dock host,
action registry, toolbar, and FTS screensets.

This crate stays implementation-agnostic. Service implementations live in
`daw-reaper` and `daw-standalone`; higher-level Rust ergonomics live in
`daw-control` and the `daw` facade.

Example:
```rust
use daw_proto::TransportServiceClient;

let transport = TransportServiceClient::new(handle);
transport.play(None).await?;
```

### `daw-control` - Ergonomic API Wrapper

`daw-control` provides a reaper-rs-style async API for Rust consumers. It wraps
the raw service clients in lightweight handles such as `Daw`, `Project`,
`Transport`, `Track`, and `FxChain`.

Example:
```rust
use daw_control::Daw;

Daw::init(handle)?;

let project = Daw::current_project().await?;
project.transport().play().await?;

let track = project.tracks().get("track-123").await?;
track.set_volume(0.8).await?;
```

### `daw-reaper` - REAPER Implementation

`daw-reaper` implements the `daw-proto` service traits against REAPER through
the project's pinned `reaper-rs` fork. It owns the main-thread dispatch bridge,
REAPER-safe wrappers, service state, broadcaster initialization, and direct
in-process APIs such as `DawMainThread`.

Async vox requests that arrive on worker threads are routed back to REAPER's
main thread where required.

### `daw-bridge` - REAPER Extension Socket Bridge

`daw-bridge` is the REAPER extension that composes the `daw-reaper` service
implementations into a routed vox handler. It binds a Unix socket at
`/tmp/fts-daw-{pid}.sock` by default, or the path supplied in `FTS_SOCKET`.

External processes use this bridge:

```bash
daw --socket /tmp/fts-daw-12345.sock transport
daw tracks --json
```

When no `--socket` is provided, `apps/daw` discovers live `/tmp/fts-daw-*.sock`
files and connects to the newest instance.

### `apps/daw` - CLI Client

The `daw` binary is intentionally thin. It discovers or receives a socket,
establishes a vox session, builds a `daw-control` handle, and maps commands to
service calls. It is useful for scripts, tests, and operator diagnostics, but
it should not become a second implementation of DAW behavior.

### `daw-bridge` vs. Integrated Extensions

Out-of-process tools use the Unix socket served by `daw-bridge`. Code that is
already loaded inside REAPER should prefer local access:

- `daw::init(raw_host_context)` for DAW-aware plugins.
- `daw::main_thread_daw()` for synchronous REAPER-main-thread work.
- `daw-control-sync::LocalCaller` for in-process vox service dispatch without a socket.

This keeps integrated extensions off the socket path while preserving the same
service contracts used by external clients.

### `daw-standalone` - Standalone Implementation

`daw-standalone` is the mock/reference implementation for tests and development
without REAPER.

### `daw` - Public Facade

The facade is the public API surface for the DAW domain. External crates should
prefer `daw` and its feature-gated modules instead of depending directly on
internal implementation crates.

## Design Philosophy

### Separation of Concerns

**Protocol (`daw-proto`):**
- Service contracts and shared types only
- No REAPER-specific implementation state
- Stable names for code generation and external tooling

**Control (`daw-control` / `daw-control-sync`):**
- Ergonomic Rust wrapper
- Adds convenience, not functionality
- Supports socket-backed callers and local in-process callers
- Follows reaper-rs-style handles

**Implementations (`daw-reaper`, `daw-standalone`):**
- Implement the protocol
- Handle DAW-specific details
- Manage internal state

### Reaper-RS Style API

Inspired by `reaper-rs`, `daw-control` provides:

1. **Global Singleton** - `Daw::init()` / `Daw::current_project()`
2. **Lightweight Handles** - Just IDs, no connections stored
3. **Every Method Accesses Singleton** - `DawConnection::get()`
4. **Hierarchical Navigation** - `project.transport().play()`

Compare to reaper-rs:
```rust
// reaper-rs
let project = Reaper::get().current_project();
project.tracks().next().unwrap().set_name("Lead");

// daw-control (same pattern!)
let project = Daw::current_project().await?;
project.tracks().get("track-1").await?.set_volume(0.8).await?;
```

### Streaming

Services that expose live state use vox channels for streaming updates:

```rust
let mut updates = project.transport().subscribe().await?;
while let Some(update) = updates.next().await {
    println!("Position: {:?}", update.state.playhead_position);
}
```

### FTS Screensets

FTS screensets are named, host-managed workspace snapshots exposed through
`ScreensetService`. They are separate from REAPER's built-in numbered screenset
slots and support the current universal screenset model:

- `Window` captures window, dock, monitor, and panel layout state.
- `TrackSet` captures TCP/MCP visibility by stable track GUID.
- `SelectionSet` captures selected tracks plus loop/time selection.

The CLI exposes these through `daw screensets`, `daw screenset-capture`,
`daw screenset-apply`, and related commands.

## See Also

- [reaper-rs](https://github.com/helgoboss/reaper-rs) - Inspiration for API design
