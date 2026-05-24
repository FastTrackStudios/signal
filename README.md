# FastTrackStudio

**Fast. Efficient. Opinionated workflow management for REAPER.**

FastTrackStudio is a distributed control platform for live performance and
studio production. It extends REAPER with signal chain management, session
navigation, chord chart rendering, and multi-DAW sync — accessible from a
desktop app, a REAPER extension, or any device on your local network via a web
interface.

## Domain Projects

FastTrackStudio ties together four domain-specific projects, each in its own
repository:

| Project                                                    | Description                                                                                                                                         |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| [**Signal**](https://github.com/FastTrackStudios/signal)   | Signal chain, plugin, and parameter management for live and studio use                                                                              |
| [**Session**](https://github.com/FastTrackStudios/session) | Setlist, song, and section management with transport and navigation controls                                                                        |
| [**Keyflow**](https://github.com/FastTrackStudios/keyflow) | A text-based music format that understands song structure and music theory — powering chart generation and music-aware features across the platform |
| [**Sync**](https://github.com/FastTrackStudios/sync)       | Real-time DAW-to-DAW synchronization across multiple machines for collaboration or playback redundancy                                              |

These are backed by a shared [**DAW**](https://github.com/FastTrackStudios/daw)
abstraction layer that provides unified REAPER integration, transport control,
and file management.

## How It Works

The domain logic (Signal, Session, Sync, Keyflow) is hosted **inside REAPER**
by [`fts-extensions`](https://github.com/FastTrackStudios/fts-extensions), which
loads each domain as an in-process module and exposes them over the
[Vox](https://github.com/bearcove/roam) RPC framework.

**This repository is the user-facing application.** The desktop app discovers
and connects to the REAPER-hosted `fts-extensions` over Vox, then re-exposes
the same API through a WebSocket gateway so a browser or phone on the local
network can control the session.

```
┌──────────────── REAPER ────────────────┐
│  fts-extensions (in-process modules)    │
│   Signal · Session · Sync · Keyflow     │
│              ▲  Vox RPC                  │
└──────────────┼──────────────────────────┘
               │
        ┌──────┴───────┐
        │  Desktop App  │  (this repo)
        │  + Gateway WS │
        └──────┬───────┘
        ┌──────┴──────┐
        ▼             ▼
       Web          Mobile
       App           App
```

## Apps

| App         | Description                     |
| ----------- | ------------------------------- |
| `desktop`   | Dioxus desktop client           |
| `web`       | Browser-based control UI (WASM) |
| `mobile`    | Mobile client                   |
| `installer` | Installation wizard             |
| `fts-cli`   | Command-line interface          |

## Quick Start

```bash
# Enter the dev environment
nix develop        # or: direnv allow

# Build
cargo build

# Run the desktop app
cargo run -p fasttrackstudio-desktop
```

## Development

```bash
cargo check -p <crate>       # Type-check a single crate
cargo test -p <crate>        # Run tests for a crate
cargo test                   # Run all tests
```

## License

See [LICENSE.md](./LICENSE.md)
