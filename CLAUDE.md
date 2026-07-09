# FastTrackStudio — Monorepo Instructions

One tree for the whole stack. Domains keep their own workspaces
(phase 1); every intra-stack dependency is a **path dep** — never add a
git dep on anything that lives in this tree.

## Layout

```
crates/signal/ signal domain core (facade+proto+ui+live+storage+...)
features/fx|rigs|sampler|nam|plugin-host/  signal capabilities (built-in
               FX, live rigs, sampler engine, NAM models, plugin hosting)
apps/rigd  apps/signal-web   headless rig daemon + its browser remote
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
cargo check --workspace --exclude vox-discover   # the root workspace (rig, daw, session, ...)
cargo build -p signal-rigd                       # the headless rig daemon
cd apps/fasttrackstudio && cargo check           # the unified app
```

Live rig: `cargo build -p signal-rigd` from the repo root →
`target/debug/signal-rigd` (ws://:4040/vox); web remote built with
`cd apps/signal-web && dx build --platform web`, config in
`~/.config/signal/rig/*.styx`. (The PREVIOUS deployment ran from
`signal/target/debug/signal-rigd` — that gitignored target/ dir is left
in place so a running rigd keeps its binary.)

## Signal domain rules (from the dissolved signal/CLAUDE.md)

Signal is the signal-chain / plugin-management domain: `crates/signal/*`
(facade `signal` + proto/ui/live/storage/controller/import/browser/grid/
grid-ui/daw-bridge), `features/{fx,rigs,sampler,nam,plugin-host}`,
`features/reaper/signal-*`, `apps/rigd`, `apps/signal-web`. The `signal`
facade is the only public API surface: apps depend on `signal`,
`signal-ui`, or `signal-sampler`, never on the internal domain crates.
Docs: `crates/signal/docs/` (DESIGN.md, DOMAIN.md).

**Detachable GUI (STRICT)**: the rig core is 100% headless; every GUI is
a vox remote via architect (`signal-guitar-proto` is the wire contract;
`apps/rigd` serves the router; browser/desktop/tablet UIs are clients).

**GUI rendering** — signal UI must render identically standalone, as a
VST3/CLAP plugin, and embedded in REAPER, so all contexts share one
pipeline: `nice-plug-dioxus` → Blitz (Vello + wgpu) → baseview:

- Never `dioxus::desktop::LaunchBuilder` (WebKit/WRY breaks VST parity);
  standalone windows use `nice_plug_dioxus::open_standalone_with_state`.
- **Inline styles only in signal UI crates** — Blitz does not load
  external CSS files reliably. Inline `style="..."` or embed CSS as a
  static string via `document::Style { {CSS_STR} }`; never
  `document::Stylesheet { href: ... }`, no Tailwind `asset!()` calls
  (embed via `include_str!()`).
- Components must render correctly without Tailwind — explicit style
  values for layout-critical properties; Tailwind classes are additive
  only (built via `just tailwind` → `apps/signal-web/assets/tailwind.css`).
- Root `App` components take no props (context via `use_context_provider`)
  so the same component works standalone and as a plugin editor.

**Platform targets** — processing-core crates (`daw-audio-graph`,
signal DSP cores in `features/fx/*-dsp`, sampler engine) must support
native, WASM/AudioWorklet, and embedded `no_std`:

- `#![no_std]` + `alloc` compatible; gate unavoidable `std` behind an
  additive `std` feature.
- No heap allocation on the hot path — pre-allocate at `reset()`;
  `process()` never calls `Vec::push`/`Box::new`.
- No threads — the graph is driven synchronously by whichever callback
  owns it. No `moire::task::spawn` inside processing crates.
- No platform I/O in processing crates — `cpal`/`web-sys`/MIDI drivers
  live only in adapter crates (rigd, signal-web, future embedded).
- Keep the `AudioNode: Send` bound.

**RPC**: service traits use `#[architect::rpc]`; max 4 params per method
(Facet constraint); `Tx<T>`/`Rx<T>` for streaming.

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
   DONE through wave 4 (libs, audiocore/midicore/input, daw, session,
   keyflow, signal); remaining: apps/fasttrackstudio, FastTrackStudio,
   Plugins.
2. Feature-gate heavy backends (reaper, standalone-audio) so cold builds
   only compile what's used.
3. Retire `FastTrackStudio/apps/*` (old app; hand-rolled vox) in favor
   of `apps/fasttrackstudio`.
4. Root flake.nix (adopt signal's — it already pins the shared
   dioxus-flake toolchain).
