# FastTrackStudio — Monorepo Instructions

One tree for the whole stack. Domains keep their own workspaces
(phase 1); every intra-stack dependency is a **path dep** — never add a
git dep on anything that lives in this tree.

## Layout

```
signal/        live guitar rig (chains, NAM, perform surfaces, rigd)
daw/           engine core, audio-io, proto, standalone, reaper backend
session/       setlists, songs, charts — session domain + gateway
keyflow/       chart/keys analysis + writing (+ engraver)
midicore/      MIDI facade (device I/O, canonical MidiEvent)
input_actions/ actions / keybindings / input framework
Plugins/FTS-Audiocore/  shared DSP + gui primitives
FastTrackStudio/        shared utils + the LEGACY app (superseded)
FTS-Plugins/forks/      fts-plug (nice-plug, nice-plug-dioxus)
fts-ui/  fts-story/  dock-dioxus/   UI component + story libraries
neural-amp-modeler-rs/  NAM binding (C++ core vendored, no submodule)
apps/fasttrackstudio/   THE unified app (features: signal / session / full)
```

**architect stays external** at `../architect` (consumed like a
crates.io dep; per-domain `[patch]` blocks point the codeberg URL at the
sibling checkout).

## Rules

- **Path deps only** between tree members. If a `[patch]` block's URL is
  a repo that no longer exists, it is either dead (delete it) or
  load-bearing for a *transitive* git dep (keyflow's self-patch —
  Editor.git references keyflow.git). Check before touching.
- **Architect idiom everywhere**: services are `#[architect::rpc]`
  traits; live updates are `#[subscribe]` streams served from
  `architect::PubSub` hubs (never Tx-parameter subscriptions or
  reverse-dispatch "client services"); transports are
  `architect::axum_ws` + `LayerRouter` server-side and
  `vox_core::initiator_on(link).establish::<Client>()` client-side.
  `session/` is the reference conversion; `signal/` was born this way.
- Async: `moire::task::spawn`, `moire::sync::*` — never raw tokio in
  domain crates.
- Each domain has its own CLAUDE.md with domain rules; read it before
  working in that domain.

## Build

Per-domain, from the domain dir (each is its own workspace for now):

```bash
cd signal  && cargo check --workspace     # rig
cd session && cargo check --workspace     # session domain
cd apps/fasttrackstudio && cargo check    # the unified app
```

Live rig: `signal/target/debug/signal-rigd` (ws://:4040/vox), web remote
built with `cd signal/apps/web && dx build --platform web`, config in
`~/.config/signal/rig/*.styx`.

## Phase 2 (in progress, do opportunistically)

1. Root workspace: merge domain workspaces into one root Cargo.toml —
   single lockfile, shared `target/`, one `[workspace.dependencies]`.
   Start from the leaves (audiocore → midicore → input → daw → …).
2. Feature-gate heavy backends (reaper, standalone-audio) so cold builds
   only compile what's used.
3. Retire `FastTrackStudio/apps/*` (old app; hand-rolled vox) in favor
   of `apps/fasttrackstudio`.
4. Root flake.nix (adopt signal's — it already pins the shared
   dioxus-flake toolchain).
