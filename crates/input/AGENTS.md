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
btca ask -r facet -q "How do I use the Facet derive macro with enums?"
btca resources   # list all available resources
```

### Relevant Resources for This Repo

| Resource | Repo | Description |
|----------|------|-------------|
| `facet` | facet-rs/facet | Rust reflection — shapes, derive macros, serialization, pretty-printing |
| `figue` | bearcove/figue | Config parsing from CLI args, env vars, and config files using facet reflection |
| `styx` | bearcove/styx | Cleaner serialization format — alternative to JSON/YAML with schema support |
