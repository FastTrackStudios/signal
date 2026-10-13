# Session

**Setlist, song, and section management for live and studio use.**

Session provides the performance navigation layer for
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio) —
managing setlists, songs, sections, and transport controls for live
performance and studio workflows.

## Core Concepts

- **Setlist** — An ordered collection of songs for a performance
- **Song** — A track with sections, chart data, detected chords, and comments
- **Section** — A named segment (verse, chorus, bridge, etc.) with timing info

Session exposes navigation actions like `SMART_NEXT`, `NEXT_SONG`,
`NEXT_SECTION`, `TOGGLE_PLAYBACK`, and `BUILD_SETLIST` that other parts of the
system (desktop app, web UI, REAPER extension) can trigger over RPC.

## Workspace Crates

```
session/
├── session-proto      Shared types and RPC service definitions — Setlist, Song,
│                      Section, plus SetlistService and SongService traits.
│                      WASM-compatible.
├── session-ui         Dioxus UI components for session management.
├── session-extension  REAPER SHM guest — connects via daw-bridge.
└── session            Facade crate — public API, builders, and cell runtime.
```

## Apps

| App | Description |
|-----|-------------|
| `apps/cli` | Command-line session tool |
| `apps/desktop` | Dioxus desktop application |
| `apps/web` | Web application |

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Type-check the facade
cargo check -p session
```

## Part of FastTrackStudio

Session is one of the domain projects in the
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio)
ecosystem, alongside
[Signal](https://github.com/FastTrackStudios/signal),
[Keyflow](https://github.com/FastTrackStudios/keyflow),
[Sync](https://github.com/FastTrackStudios/sync), and
[DAW](https://github.com/FastTrackStudios/daw).

## License

See [LICENSE.md](./LICENSE.md)
