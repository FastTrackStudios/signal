# Signal — Claude Code Instructions

Signal is the signal chain / plugin management domain for FastTrackStudio.

## Architecture

This repo follows the **crate facade pattern**:
- `signal` — the facade crate, the only public API surface
- `signal-proto` — protocol/domain types (internal)
- `signal-controller` — controller logic (internal)
- `signal-live` — live signal chain management (internal)
- `signal-storage` — persistence layer (internal)
- `signal-import` — import logic (internal)
- `signal-daw-bridge` — DAW integration bridge (internal)
- `signal-ui` — Dioxus UI components (public, feature-gated)
- `signal-extension` — SHM guest process binary
- `nam-manager` — NAM model management (internal)
- `macromod` — macro module types (internal)

Apps must depend only on `signal` (facade) or `signal-ui`, never on internal crates.

## Key Rules

### Async & Concurrency
- Use `moire::task::spawn` instead of `tokio::spawn`
- Use `moire::sync::Mutex` / `moire::sync::RwLock` instead of tokio/std equivalents
- Never hold std sync primitives across `.await`

### RPC Services
- Service traits use `#[vox::service]`
- Max 4 params per method (Facet constraint)
- Use `Tx<T>` / `Rx<T>` for streaming

## Build & Test

```bash
cargo check -p signal           # Type-check facade
cargo check --workspace         # Type-check all
cargo test -p signal            # Run tests
```

## Issue Tracking

Use `bd` (beads) for all task tracking. See AGENTS.md for workflow.
