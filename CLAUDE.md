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

## Active modernization queue

1. **daw streams**: convert the five Tx-parameter subscriptions
   (track / tempo_map / event_bus / marker / region — all
   `subscribe(project: ProjectContext, tx)`) to `#[subscribe]` streams.
   Design: events carry their `ProjectContext` (most already embed
   project ids), subscribers filter client-side; per-service PubSub hub
   on each backend (daw-standalone + daw-reaper), pumps replacing the
   per-subscriber spawn loops — exactly the session SetlistService
   conversion. ~29 subscriber call sites in daw-control/consumers.
2. **fasttrackstudio app = daw-standalone player**: embed
   daw-standalone (bootstrap + audio features) + the session domain
   in-process so the live setlist is DATA the app can PLAY — transport
   over the setlist without REAPER. session facade already dev-deps
   this combination (memory-link bootstrap) — promote it to the app.
3. **FTS-Guide → session feature**: port
   legacy FTS-Plugins/apps/fts-guide (~3k lines: click_player,
   count_player, guide_player, trigger_scheduler, count-in
   calculator/pattern) into `session/features/guide` as a portable
   engine (drop the REAPER cdylib shell; drive it from song sections +
   tempo map). Click + guide tracks become session data.
4. **TTS in the guide**: bearcove/cbx (`chatterbox-rs` — local ONNX
   Chatterbox TTS, lib + CLI). Embed as an optional session feature to
   speak section names / notes into the guide bus ("Chorus in 2…").
   Pre-render section cues to wav at setlist-build time (TTS is not
   realtime-safe); cache by text hash next to the styx library.

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
