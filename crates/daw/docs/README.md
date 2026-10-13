# DAW

**REAPER integration, service-first DAW control, and project-file tooling.**

DAW provides the unified interface between
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio) and
REAPER. It handles transport control, track management, project files, REAPER
extension services, and the command-line client used by FastTrackStudio tools.

## Architecture

DAW follows a layered design separating protocol, client API, host
implementations, and transport:

```
daw-proto (vox service definitions + shared types)
     ↓
daw-control (ergonomic async Rust API)
     ↓
daw-reaper / daw-standalone (service implementations)
     ↓
daw-bridge (REAPER extension socket bridge)
     ↓
daw-cli / external tools (Unix socket clients)
```

**Protocol** defines the vox services and shared data model. **Control** wraps
those services in a reaper-rs-style API with lightweight handles and
hierarchical navigation. **Implementations** either call REAPER in-process or
provide standalone mock behavior for development and tests. **daw-bridge** loads
inside REAPER, registers the service dispatchers, and exposes them over a Unix
socket for out-of-process tools.

Integrated extensions and audio plugins do not need to round-trip through the
socket. They use `daw::init`, `daw::main_thread_daw`, or
`daw-control-sync::LocalCaller` for local/in-process service access when they
are already hosted inside REAPER.

## Workspace Crates

```
daw/
├── daw-proto              Vox service definitions — Transport, Track, Project,
│                          FX, Screenset, Batch, and streaming update types.
├── daw-control            Ergonomic API — global singleton, lightweight handles,
│                          hierarchical navigation (project.transport().play()).
├── daw-control-sync       Sync-aware and local/in-process caller helpers.
├── daw-reaper             REAPER implementation via reaper-rs.
├── daw-standalone         Standalone mock implementation for development.
├── daw-bridge             REAPER extension that serves the vox services over
│                          `/tmp/fts-daw-{pid}.sock` or `FTS_SOCKET`.
├── apps/daw               `daw` CLI, a thin client over the service surface.
├── dawfile-reaper         REAPER project file handling.
├── daw-ui                 Dioxus UI components.
├── audio-controls         Standalone audio widget library.
├── daw-extension-runtime  Integrated REAPER extension helpers.
├── daw-allocator          RT-aware memory allocator.
├── fts-audio-proto        Audio protocol definitions.
├── fts-devtools           Development utilities.
└── daw                    Facade crate — the only public API surface.
```

Named FTS screensets are part of the service model. They are host-managed
workspace snapshots, separate from REAPER's numbered screenset slots, and
currently cover window layouts, track visibility sets, and selection/time-range
sets through the shared `ScreensetService`.

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Type-check the facade
cargo check -p daw
```

## Part of FastTrackStudio

DAW is the shared abstraction layer used by all domain projects in the
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio)
ecosystem:
[Signal](https://github.com/FastTrackStudios/signal),
[Session](https://github.com/FastTrackStudios/session),
[Keyflow](https://github.com/FastTrackStudios/keyflow), and
[Sync](https://github.com/FastTrackStudios/sync).

## License

See [LICENSE.md](./LICENSE.md)
