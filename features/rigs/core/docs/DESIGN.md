# Signal — Architecture Design

> **Status**: Restructure in progress. Phase 1 (folder move to the
> architect-shape feature-slice layout) is landing on the
> `refactor/architect-structure` branch. Later phases split the domain
> monolith by bounded context and migrate entities onto the architect
> macros. See "Staged migration" below.

## Shape

Signal is being restructured from the **crate-facade** layout (`crates/*`
+ one public `signal` facade) into the **feature-slice** layout used by
[architect](https://codeberg.org/FastTrackStudios/architect): top-level
`features/<name>/` folders, each grouping the crates for one slice, plus
`apps/`, `xtask/`, and shared tooling at the root.

```
features/
  signal/                 # the domain slice (monolithic for now — split in Phase 2)
    signal-proto/         # domain types, IDs, service traits
    signal-storage/       # SeaORM/SQLite repos  (→ signal-db when architect-migrated)
    signal-live/          # runtime services + rig/scene morph engine
    signal-controller/    # SignalController facade + ops namespaces
    signal-import/        # vendor-preset importer (FabFilter / RfxChain)
    signal-daw-bridge/    # infer domain structure from a live REAPER FX tree
    signal/               # the public facade crate (apps depend on this)
    signal-ui/            # Dioxus component library
    signal-browser/       # headless (no-Dioxus) collection-browser data layer
  sampler/                # real-time sample-playback engine + FX/NAM
    signal-sampler/
    signal-sampler-clap/  # CLAP instrument plugin wrapping the engine
  plugin-host/
    signal-plugin-host/   # thin CLAP/VST3 host over `daw`
  nam/
    nam-manager/          # .nam / IR model catalog
  macromod/
    macromod/             # macro + modulation data model + runtime
  reaper-integration/
    signal-extension/     # in-process REAPER extension
    fts-signal-controller/# CLAP plugin managing signal chains on a REAPER track
apps/                     # cli · desktop · mobile · native · tui
xtask/
```

`crates/signal-daw` remains parked (excluded from the workspace) pending
upstream `daw` features.

## Dependency discipline

- Apps depend only on the `signal` facade (and `signal-ui` / `signal-sampler`),
  never on the internal domain crates directly.
- **All intra-workspace crate deps are `x.workspace = true`.** Only the root
  `Cargo.toml` `[workspace.dependencies]` table carries paths — so future
  restructure phases edit paths in exactly one place. Sibling-repo deps
  (`../daw`, `../../architect`, `../Plugins/*`, `../neural-amp-modeler-rs`)
  also live in that table and resolve relative to the workspace root.

## Architect adoption

`architect`, `architect-atom`, and `architect-form` are declared in the root
dependency table (path deps to the local migrated checkout, which is on the
same upstream `facet 0.50-rc` / `vox 0.10-rc` as this workspace). They are
**declared but unused** until Phase 3, when domain entities migrate from
hand-written facet structs + hand-written SeaORM repos onto
`#[derive(architect::Entity)]` + `#[architect::rpc]`/vox, with generated
`-db` / `-memory` / `-crdt` backends and `atom`/`form` client twins in the UI.

## Staged migration

1. **Phase 1 — Structure (this branch).** Move crates into the feature-slice
   folders, centralize deps as `workspace = true`, wire architect as a
   declared dep. Crate names unchanged; every batch stays `cargo check`-green.
2. **Phase 1.5 — Edition/toolchain.** Bump `edition 2021 → 2024`, converge on
   the architect toolchain, one crate-batch at a time (`cargo fix --edition`).
3. **Phase 2 — Context split.** Carve `signal-proto` (and `-storage` / `-live`)
   into bounded contexts: `tone` (Block→Module→Layer→Engine→Preset), `rig`
   (Rig, library), `perform` (Profile/Song/Scene/Setlist), each its own
   `features/<ctx>/` slice. The `signal` facade re-exports all three.
4. **Phase 3 — Architect macro adoption.** Per feature, adopt the
   `<feat>/<feat>-proto/<feat>-db/<feat>-memory/<feat>-crdt/<feat>-ui/tests`
   subcrate shape; replace hand-written structs/services/repos with the
   architect derive + vox; rename `signal-storage` → `*-db`.
5. **Phase 4 — UI + app assembly.** Migrate `signal-ui` onto `architect-atom`
   / `architect-form`; optionally move `apps/` under an app-assembly dir.

See `DOMAIN.md` for the canonical Signal domain vocabulary that the bounded
contexts in Phase 2 follow.
