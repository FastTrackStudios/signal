# legacy/

What was left of the five folded repos (daw, processor, signal, patchbay,
frame) once their crates moved into `apps/`, `crates/`, `features/`,
`vendor/` and `xtask/`: their Justfiles, flakes, nix modules, CI
workflows, agent guides (`CLAUDE.md`, `AGENTS.md`) and design notes.

Nothing here is built or read. Each piece is waiting to be folded into
studio's own — a recipe into the root `Justfile`, a CI job into
`.github/workflows`, a nix module into `nix/`, a rule into the root
`CLAUDE.md`, a note into `docs/` — and deleted once it has been. When this
directory is empty, it goes.
