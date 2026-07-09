# FastTrackStudio — Monorepo Instructions

One tree, ONE root Cargo workspace (~160 members), one lockfile, one
`target/`, one flake. Every intra-stack dependency is declared once in
root `[workspace.dependencies]` as a **path dep** and consumed as
`x.workspace = true` — never add a git dep on anything that lives in
this tree. See LAYOUT.md for the full map.

## Layout

```
crates/    domain cores — daw, session, keyflow, signal (facade+proto+
           ui+live+storage+...), midicore, input, audiocore
features/  capabilities — audio, sync, dawfile, reaper, standalone,
           surfaces, daw-ui, guide, engraver, dynamic-template,
           fx (built-in FX), rigs, sampler, nam, plugin-host
libs/      UI + infra libraries — fts-ui, fts-story, dock, nice-plug,
           utils, vox-discover, installer-core, neural-amp-modeler,
           monarchy, devtools, moire-trace-capture
apps/      fasttrackstudio (THE app: signal / session / full / tts),
           rigd (headless rig daemon), signal-web (browser remote),
           daw-cli, keyflow-cli, installer,
           site (fts-site — fasttrackstudio.app website, dioxus web),
           docs-site (docs.fasttrackstudio.app — dodeca + kf docs, NOT a
           cargo member; `just docs-build` / `just docs-serve`)
attic/     parked code (dead apps, legacy shells) — excluded, never built
docs/      cross-domain guides (facet, styx, tracey, spec/)
```

**architect lives in-tree** at `libs/architect/` (subtree-imported with
history; `architect` + derive macros + atom/form/auth/crdt are ordinary
workspace members, consumed as `architect.workspace = true`). Framework
changes are ordinary in-tree refactors — no patch blocks, no sibling
checkout. External consumers (the `task` project) take a git dep on this
monorepo.

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

Everything builds from the repo root (one workspace):

```bash
cargo check --workspace --exclude vox-discover   # the whole tree
cargo build -p signal-engine                     # the signal engine (headless rig core)
cargo build -p fasttrackstudio                   # THE app (signal/session/full/tts)
cargo build -p fts-cli                           # the unified `fts` CLI (fts daw / fts kf / fts signal engine / fts status)
cargo check -p signal-web --target wasm32-unknown-unknown  # browser remote
```

Dev shell: `nix develop` (or direnv `use flake`) — root `flake.nix`
carries the FTS 1.94 toolchain pin, `dx`, wasm target, tailwindcss, and
the native headers (alsa, pipewire, jack, avahi for vox-discover).

Live rig: `cargo build -p signal-engine` from the repo root →
`target/debug/signal-engine` (ws://:4040/vox); web remote built with
`cd apps/signal-web && dx build --platform web`, config in
`~/.config/signal/rig/*.styx`. (The PREVIOUS deployment ran from
`signal/target/debug/signal-rigd` — that gitignored target/ dir is left
in place so a running engine keeps its binary.)

## Signal domain rules (from the dissolved signal/CLAUDE.md)

Signal is the signal-chain / plugin-management domain: `crates/signal/*`
(facade `signal` + proto/ui/live/storage/controller/import/browser/grid/
grid-ui/daw-bridge), `features/{fx,rigs,sampler,nam,plugin-host}`,
`features/reaper/signal-*`, `apps/signal-engine`, `apps/signal-web`. The `signal`
facade is the only public API surface: apps depend on `signal`,
`signal-ui`, or `signal-sampler`, never on the internal domain crates.
Docs: `crates/signal/docs/` (DESIGN.md, DOMAIN.md).

**Detachable GUI (STRICT)**: the rig core is 100% headless; every GUI is
a vox remote via architect (`signal-guitar-proto` is the wire contract;
`apps/signal-engine` serves the router; browser/desktop/tablet UIs are clients).

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

## Phase 2

1. ~~Root workspace: merge domain workspaces into one root Cargo.toml.~~
   DONE — all waves complete; apps/fasttrackstudio, FastTrackStudio and
   Plugins dissolved in the finale wave (legacy remnants parked in
   `attic/fasttrackstudio-legacy` + `attic/fx-apps`).
2. Feature-gate heavy backends (reaper, standalone-audio) so cold builds
   only compile what's used.
3. ~~Retire `FastTrackStudio/apps/*`.~~ DONE — the legacy app is parked;
   `apps/installer` (fts-installer) survived as a root member.
4. ~~Root flake.nix.~~ DONE — adopted signal's flake at the root
   (dioxus-flake toolchain, rust 1.94 + wasm, avahi/pipewire/jack shells).

### Dedup queue (from LAYOUT.md, after the merge)

- keyflow-daw-analysis's daw types → daw-proto only
- audio-controls (vendored) → fold into features/daw-ui or delete after
  signal-ui migrates off it
- signal-audio remnants, duplicate wav/resampler helpers → libs/utils
  or audiocore-dsp
- three CLIs → one `fts` CLI in apps/ (subcommands)
