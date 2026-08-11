# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root, or
- **`CONTEXT-MAP.md`** at the repo root if it exists — it points at one `CONTEXT.md` per context. Read each one relevant to the topic.
- **`docs/adr/`** — read ADRs that touch the area you're about to work in. In this repo, also check `crates/<domain>/docs/adr/` and `features/<capability>/docs/adr/` for context-scoped decisions.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest creating them upfront. The `/domain-modeling` skill (reached via `/grill-with-docs` and `/improve-codebase-architecture`) creates them lazily when terms or decisions actually get resolved.

At the time of writing none of them exist yet. That is the expected state, not a gap to fill.

## File structure

This is a **multi-context** repo: one root map, one `CONTEXT.md` per domain.

The contexts are the directories already described in the root `CLAUDE.md` — domain cores under `crates/`, capabilities under `features/`, libraries under `libs/`, products under `apps/`. A context is a thing with its own vocabulary, not every crate: `signal` is one context, not eleven, even though it is eleven crates.

```
/
├── CONTEXT-MAP.md
├── docs/adr/                          ← system-wide decisions
├── crates/
│   ├── signal/
│   │   ├── CONTEXT.md
│   │   └── docs/adr/                  ← context-specific decisions
│   ├── daw/CONTEXT.md
│   ├── session/CONTEXT.md
│   └── patchbay/CONTEXT.md
└── features/
    ├── reaper/CONTEXT.md
    └── fx/CONTEXT.md
```

### Relationship to the per-domain `CLAUDE.md` files

Several domains already carry a `CLAUDE.md` holding their rules, and the root `CLAUDE.md` says to read it before working in that domain. Those are **instructions to agents**; a `CONTEXT.md` is a **glossary of the domain's language**. They sit side by side and answer different questions — "how do I work here" against "what do these words mean here".

Where a domain has both, read both. Where its `CLAUDE.md` already defines a term, don't restate it in `CONTEXT.md` with different words; move it or point at it.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `CONTEXT.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal — either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (event-sourced orders) — but worth reopening because…_
