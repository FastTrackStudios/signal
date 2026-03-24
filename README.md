# Signal

**Signal chain and plugin management for live and studio use.**

Signal handles the complete lifecycle of audio signal processing in
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio) — from
building rigs and managing effects chains to morphing parameters in real time
during a live performance.

## Core Concepts

Signal organizes audio processing into a layered hierarchy:

```
Rig → Engine → Layer → Module → Block
```

- **Block** — A single plugin or processor
- **Module** — A group of blocks forming one signal path
- **Layer** — Parallel modules (e.g. dry/wet, A/B)
- **Engine** — A complete processing chain
- **Rig** — Your full signal setup combining multiple engines

On top of this, a performance layer manages **Profiles** (collections of
patches), **Songs** (sections and variations), and **Setlists** for live use.

## Workspace Crates

```
signal/
├── signal-proto         Domain types — Blocks, Modules, Layers, Engines, Rigs,
│                        Profiles, Songs, Setlists. Facet-derived for RPC.
├── signal-storage       SQLite persistence via Sea-ORM.
├── signal-live          Live execution engine — patch application, parameter
│                        morphing, macro setup, snapshots.
├── signal-controller    RPC controller logic via Vox service traits.
├── signal-import        Import logic for signal chains and data.
├── signal-daw-bridge    Integration bridge between Signal and DAW domains.
├── signal-ui            Dioxus UI components for signal management.
├── signal-extension     REAPER SHM guest — connects via daw-bridge.
├── nam-manager          Neural Amp Modeler (NAM) model management.
├── macromod             Macro module types and parameter bindings.
├── fts-signal-controller  FTS-specific controller integration.
└── signal               Facade crate — the only public API surface.
```

## Quick Start

```bash
# Build
cargo build

# Run tests
cargo test

# Type-check the facade
cargo check -p signal
```

## Architecture

```
signal-proto (domain types)
       ↓
signal-storage (persistence)
       ↓
signal-live (execution engine)
       ↓
signal-controller (RPC services)
       ↓
signal (facade — public API)
```

Apps depend only on the `signal` facade crate, never on internal crates
directly.

## Part of FastTrackStudio

Signal is one of the domain projects in the
[FastTrackStudio](https://github.com/FastTrackStudios/FastTrackStudio)
ecosystem, alongside
[Session](https://github.com/FastTrackStudios/session),
[Keyflow](https://github.com/FastTrackStudios/keyflow),
[Sync](https://github.com/FastTrackStudios/sync), and
[DAW](https://github.com/FastTrackStudios/daw).

## License

See [LICENSE.md](./LICENSE.md)
