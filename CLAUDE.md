# FastTrackStudio — Claude Code Instructions

## Architecture Guides

Read these before implementing features or making architectural decisions:

| Guide | Path | When to Read |
|-------|------|-------------|
| **Crate Facade Pattern** | `docs/crate-facade-pattern.md` | Adding deps, creating crates, refactoring modules |
| **Roam & Moire Best Practices** | `docs/roam-best-practices.md` | RPC services, async patterns, streaming, locks |
| **Facet Guide** | `docs/facet-guide.md` | Deriving Facet, serialization, roam type requirements |
| **Styx Guide** | `docs/styx-guide.md` | Writing `.styx` config files. Use `/styx` skill for interactive help. |
| **Tracey Guide** | `docs/tracey-guide.md` | Adding spec annotations, checking coverage. Use `/tracey` skill for interactive help. |

## Key Rules

### Crate Dependencies

Domain crates (signal, session, sync) live in **sibling repos** and are consumed
via path deps (local dev) or git deps (CI). Apps must depend only on the facade
crates (`signal`, `session`, `sync`), never on internal crates like signal-proto.

The same applies to other sibling domains (daw, keyflow) — use the facade,
not the internal crates. See `docs/crate-facade-pattern.md`.

### Async & Concurrency

- Use `moire::task::spawn` instead of `tokio::spawn` — same API, adds instrumentation
- Use `moire::sync::Mutex` / `moire::sync::RwLock` instead of tokio/std equivalents
- Name all spawned tasks, channels, and locks for dashboard visibility
- **Never hold `std::sync::RwLock` or `std::sync::Mutex` across `.await`** — clone data out first
- See `docs/roam-best-practices.md` for complete patterns

### RPC Services

- Service traits use `#[roam::service]` — no `Result<T, E>` in signatures
- Use response enums for structured errors
- Max 4 params per method (Facet constraint) — group extras into request structs
- Use `Tx<T>` / `Rx<T>` for streaming
- See `docs/roam-best-practices.md` for ship case study patterns

### Spec Traceability

- Use Tracey annotations (`// r[impl feature.name]`) in production code
- Use `// r[verify feature.name]` in test code
- Run `tracey query validate` to check for broken references
- See `docs/tracey-guide.md` for annotation patterns

## Build & Test

```bash
cargo check -p <crate>           # Type-check a single crate
cargo test -p <crate>            # Run tests for a crate
xtask test <filter>              # Run tests matching filter
```

## Issue Tracking

Use `bd` (beads) for all task tracking. See AGENTS.md for workflow.

## Bearcove Ecosystem Reference

When implementing features using these dependencies, consult the actual source:

```bash
btca ask -r roam -q "How does roam dispatch service methods?"
btca ask -r facet -q "How to derive Facet for enums?"
btca ask -r tracey -q "How to add spec annotations?"
```

Or clone repos for deep reference:

| Crate | Repo | Local |
|-------|------|-------|
| roam | bearcove/roam | `/tmp/roam` |
| moire | bearcove/moire | `/tmp/moire` |
| tracey | bearcove/tracey | `/tmp/tracey` |
| ship | bearcove/ship (case study) | `/tmp/ship` |
| facet | facet-rs/facet | — |
