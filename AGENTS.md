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

Use **btca** to query the actual source code of key dependencies before implementing features or debugging.

```bash
btca ask -r <resource> -q "your question"
btca resources   # list all available resources
```

### Relevant Resources

| Resource | Repo | Description |
|----------|------|-------------|
| `facet` | facet-rs/facet | Rust reflection — shapes, derive macros, serialization |
| `roam` | bearcove/roam | RPC service framework — service traits, streaming, SHM |
| `moire` | bearcove/moire | Instrumentation — task spawning, sync primitives |
