# fts — the FastTrackStudio monorepo

One tree, ONE Cargo workspace for the whole stack. Every domain that
previously lived in its own repo is a subtree here (history preserved);
every intra-stack dependency is declared once in root
`[workspace.dependencies]` as a path dep, so cross-cutting changes are
one commit and version drift is structurally impossible.

```
crates/     domain cores — daw, session, keyflow, signal, midicore,
            input, audiocore
features/   capabilities — audio, sync, dawfile, reaper, standalone,
            surfaces, daw-ui, guide, engraver, dynamic-template,
            fx (built-in FX), rigs, sampler, nam, plugin-host
libs/       UI + infra — fts-ui, fts-story, dock, nice-plug, utils,
            vox-discover, installer-core, neural-amp-modeler, monarchy
apps/       fasttrackstudio (THE app — feature-configured, one binary),
            rigd (headless rig daemon), signal-web (browser remote),
            daw-cli, keyflow-cli, installer
attic/      parked code — excluded from the workspace, never built
docs/       cross-domain guides + specs
```

**architect stays external** (framework cadence, consumed like a
crates.io dependency; the root `[patch]` points it at the sibling
checkout `../architect`).

Build everything from the root:

```bash
cargo check --workspace --exclude vox-discover
cargo build -p signal-rigd          # headless live rig
cargo build -p fasttrackstudio      # THE app
```

Dev shell: `nix develop` (root `flake.nix`; direnv users get it via
`.envrc` → `use flake`).
