# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## btca — Source Code Search

Use **btca** to query the actual source code of key dependencies before implementing features or debugging. Prefer this over web searches or docs that may be outdated.

```bash
btca ask -r <resource> -q "your question"
btca ask -r facet -r vox -q "How does vox handle service versioning?"
btca resources   # list all available resources
```

### Relevant Resources for This Repo

| Resource | Repo | Description |
|----------|------|-------------|
| `facet` | facet-rs/facet | Rust reflection — shapes, derive macros, serialization, pretty-printing |
| `vox` | bearcove/vox | Rust-native RPC framework where Rust traits are the schema, with TS/Swift codegen |
| `figue` | bearcove/figue | Config parsing from CLI args, env vars, and config files using facet reflection |
| `styx` | bearcove/styx | Cleaner serialization format — alternative to JSON/YAML with schema support |
| `capn` | bearcove/capn | Git pre-commit/pre-push automation for formatting and validation |

## Before you push: `just ci`

**Run `just ci` and get a green run before every push.** It runs exactly
what `.github/workflows/checks.yml` runs, in the same order — lockfile
drift, `cargo fmt --check`, the flow verification gate, the web tailwind
sheet, `cargo check --workspace`, `cargo nextest run --workspace` — and
stops at the first failure, because CI does too and a later step built
on a broken one tells you nothing.

It refuses to start with less than 25 GB free: a build that runs out of
room dies in the linker with a bus error that reads as a broken
toolchain, not as a full disk. `just sweep-incremental` reclaims the
most, `just disk` says where it went.

`just ci <step>` starts from a step when you have already passed the
cheap ones and are iterating: `lockfile`, `fmt`, `flows`, `tailwind`,
`check`, `nextest`.

A full run is a few minutes against a warm `target/`. A red CI round
trip is far longer, and on a shared self-hosted runner it blocks
everyone else's checks while it fails.

