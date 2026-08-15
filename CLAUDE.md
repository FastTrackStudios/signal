# FastTrackStudio — Repo Instructions

**This repo is the audio/music product.** It was split into four repos in
August 2026; Task, the framework, and the third-party forks now live
elsewhere:

| repo | holds | consumed as |
|---|---|---|
| **fasttrackstudio** (here) | signal, daw, session, keyflow, patchbay, fx, sampler, reaper, the app, the site | — |
| [architect](https://github.com/FastTrackStudios/architect) | the framework (entity/RPC, atom, form, auth, permissions, crdt), `architect-ui`, `architect-story-*`, `architect-telemetry` | git dep, tag `v0.1.1` |
| [task](https://github.com/FastTrackStudios/task) | the Task product + the Editor stack | git dep, `branch = "main"` (repin to a tag) |
| [vendor](https://github.com/FastTrackStudios/vendor) | `phon`, `phon-jit`, `styx-format` forks (pinned rc.5) | `[patch.crates-io]`, tag `v0.1.0` |

One root Cargo workspace (249 members), one lockfile, one `target/`, one
flake. Intra-repo dependencies are path deps in root
`[workspace.dependencies]`, consumed as `x.workspace = true`. Cross-repo
dependencies are **git deps pinned to a tag**.

**Co-developing across repos**: override the tag with a local checkout
rather than pushing a tag to test:

```toml
[patch."https://github.com/FastTrackStudios/architect"]
architect = { path = "../architect/architect" }
```

Never commit those overrides — the paths are machine-specific.

**The dependency arrow is bidirectional** (a deliberate choice): this repo
takes `editor`, `editor-keyflow*`, `view-knowledge-graph`,
`collection-proto` and `attachments-proto` from task, while task takes
`daw`, `session`, `keyflow*`, `engraver`, `input`, `actions-*`, `song` and
`dioxus-test` from here. Cargo's package graph stays acyclic, but a change
spanning both repos needs two bumps in sequence — land here, tag, bump
there. Use the local `[patch]` while developing; only the release needs
the round trip.

## Layout

```
crates/    domain cores — daw, session, keyflow, signal (facade+proto+
           ui+live+storage+...), midicore, input, audiocore,
           patchbay (PipeWire studio routing: facade+proto+ui)
features/  capabilities — audio, sync, dawfile, reaper, standalone,
           surfaces, daw-ui, guide, engraver, dynamic-template,
           fx (built-in FX), rigs, sampler, nam, plugin-host,
           expression-editor, chord-tool, song,
           launcher/ (fts-launcher — the REAPER DawModule glue; the
           launcher engine itself is architect-launcher-*)
libs/      infra libraries — dock, nice-plug, utils,
           installer-core, neural-amp-modeler, monarchy, devtools,
           ui/ (fts-plug-ui, fts-audio-ui — the audio-specific UI that
           did NOT move to architect-ui, plus a vendored copy of
           fts-theme.css that Tailwind builds @import),
           vendor/ (dioxus-test, facet-swift, world)
apps/      fasttrackstudio (THE app: desktop GUI = signal / session /
           full / tts; `fasttrackstudio --engine` = the headless signal
           engine; the dx web build is the browser remote, embeddable in
           the binary via feature embed-web),
           daw-cli, keyflow-cli, installer, plugins/, extensions/,
           site (fts-site — fasttrackstudio.app website, dioxus web),
           docs-site (docs.fasttrackstudio.app — dodeca + kf docs, NOT a
           cargo member; `just docs-build` / `just docs-serve`)
docs/      cross-domain guides (facet, styx, tracey, spec/)
```

**architect is a separate repo** as of the August 2026 split. Framework
changes are made there, tagged, and pulled in by bumping the tag here —
with a local `[patch]` override for the edit/test loop. `architect-ui`
(formerly `fts-ui`) and `architect-story-*` (formerly `fts-story-*`) went
with it; `fts-plug-ui` and `fts-audio-ui` stayed here because they link
`audiocore-core` and `nice-plug`.

## Rules

- **Path deps within this repo; tagged git deps across repos.** If a
  `[patch]` block's URL is a repo that no longer exists, it is either dead
  (delete it) or load-bearing for a *transitive* git dep. Check the
  lockfile before touching — and note that a `[patch]` **cannot rename a
  crate**. That is what briefly broke `fts-launcher` at the split: its
  `launcher-ui` dep wanted a crate literally named `fts-ui`, with nothing
  left to redirect to, so it resolved a stale Codeberg copy carrying its
  own dioxus. Fixed by vendoring the engine into architect as
  `architect-launcher-*` (v0.2.0), where it is an ordinary path dep on
  `architect-ui`. `fts-launcher` is now only the REAPER DawModule glue.
- **`default-features = false` cannot be applied to a workspace-inherited
  dep.** Put it on the `[workspace.dependencies]` entry, not the consumer.
- **`include_str!` / `@import` across a repo boundary does not work.** A
  git dep has no stable path on disk, and these are invisible to cargo's
  dependency graph, so they fail at compile time rather than resolution
  time. Export the bytes from the owning crate instead — that is what
  `architect_ui::THEME_CSS` is. (This class of break bit the split three
  times.)
  (The last dead patch table — keyflow.git for Editor.git — died
  when Editor was imported in-tree 2026-07-10.)
- **Architect idiom everywhere**: services are `#[architect::rpc]`
  traits; live updates are `#[subscribe]` streams served from
  `architect::PubSub` hubs (never Tx-parameter subscriptions or
  reverse-dispatch "client services"); transports are
  `architect::axum_ws` + `LayerRouter` server-side and
  `vox_core::initiator_on(link).establish::<Client>()` client-side.
  `session/` is the reference conversion; `signal/` was born this way.
- Async (moire is retired — Jul 2026): `tokio::sync::*` for locks/channels
  (`Mutex`/`RwLock`/`broadcast`/`watch`/`mpsc`); `architect::platform::{spawn,
  sleep, timeout}` for tasks/timers — the wasm-cfg-split seam (tokio on native,
  `spawn_local`/browser timers on wasm). Drop the old moire instrumentation
  name-arg on constructors.
- Each domain has its own CLAUDE.md with domain rules; read it before
  working in that domain.

## Build

Everything builds from the repo root (one workspace):

```bash
cargo check --workspace                          # the whole tree
cargo build -p fasttrackstudio                   # THE app (GUI; `--engine` = headless signal engine)
cargo build -p fts-cli                           # the unified `fts` CLI (fts daw / fts kf / fts signal engine / fts status)
cargo check -p fasttrackstudio --target wasm32-unknown-unknown --no-default-features --features signal  # browser remote (web build)
```

Dev shell: `nix develop` (or direnv `use flake`) — root `flake.nix`
carries the FTS 1.94 toolchain pin, `dx`, wasm target, tailwindcss, and
the native headers (alsa, pipewire, jack). Also
mold, sccache, cargo-sweep (see build performance below).

**Build performance / disk** — read the `build-performance` skill before
touching a `[profile.*]` knob in the root Cargo.toml, benchmarking a
build-time change, or when the dev disk fills up. The short version:

- Dev debuginfo is `line-tables-only` + `split-debuginfo = "unpacked"`
  (98% of a debug binary was DWARF; the fat test binary went 1.62 GB →
  211 MB). Need a debugger? `--profile dev-dbg`.
- mold is the Linux linker — after pulling, `direnv reload` or links
  fail with `cannot find -fuse-ld=mold`. Note that
  `target.<triple>.rustflags` REPLACES `build.rustflags`; it does not
  merge, so a new global rustflag must go in both.
- Dependencies build at opt-level 1, with an explicit allowlist back at
  3 for audio-thread crates. A dev run of the rig must never xrun — if
  one does, run `--release` or extend the allowlist, never raise
  `package."*"` wholesale.
- Cargo never GCs `target/`. `just disk`, `just sweep`, `just sweep-all`.
  Never delete another agent's worktree target dir.
- sccache is on, but it does NOT dedupe across worktrees (measured 0%);
  it only makes wiping `target/` cheap to recover from.
- Benchmarking: check `uptime` / `pgrep -c rustc` first — other agents
  and background `cargo rail` runs on this 32-core box will invalidate
  an A/B silently.

Live rig: `cargo build -p fasttrackstudio` from the repo root →
`target/debug/fasttrackstudio --engine` (ws://:4040/vox); browser remote
= the fts web build (`just web-stage` stages it to
apps/fasttrackstudio/web-dist/, embedded by `--features embed-web`),
config in `~/.config/signal/rig/*.styx`. Deployed: `just rig-install` →
ONE binary at `~/.local/lib/fts/fasttrackstudio` behind the
`signal-engine` systemd user unit.

## Signal domain rules (from the dissolved signal/CLAUDE.md)

Signal is the signal-chain / plugin-management domain: `crates/signal/*`
(facade `signal` + proto/ui/live/storage/controller/import/browser/grid/
grid-ui/daw-bridge), `features/{fx,rigs,sampler,nam,plugin-host}`,
`features/reaper/signal-*`, the engine mode of `apps/fasttrackstudio`. The `signal`
facade is the only public API surface: apps depend on `signal`,
`signal-ui`, or `signal-sampler`, never on the internal domain crates.
Docs: `crates/signal/docs/` (DESIGN.md, DOMAIN.md).

**Detachable GUI (STRICT)**: the rig core is 100% headless; every GUI is
a vox remote via architect (`signal-guitar-proto` is the wire contract;
`fasttrackstudio --engine` serves the router; browser/desktop/tablet UIs are clients).

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
  only (built via `just tailwind` → `apps/fasttrackstudio/assets/tailwind-signal.css`).
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
  owns it. No task spawning (`architect::platform::spawn` / `tokio`) inside
  processing crates.
- No platform I/O in processing crates — `cpal`/`web-sys`/MIDI drivers
  live only in adapter crates (the engine mode, web builds, future embedded).
- Keep the `AudioNode: Send` bound.

**RPC**: service traits use `#[architect::rpc]`; max 4 params per method
(Facet constraint); `Tx<T>`/`Rx<T>` for streaming.

## Active modernization queue

(Items 1–4 are DONE as of 2026-07-14 — the five daw subscriptions are
`#[subscribe]` streams, the app embeds daw-standalone + session
in-process, the guide lives at features/guide with TTS. Newest completed
item: the architect CRDT doc-sync rework for vox 0.10 channel scoping —
held-open sync/presence calls, `SyncDown::Attached` envelope; see
crates/signal/docs/detachable-gui.md. Kept below for context until the
queue is refilled.)

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
2. Feature-gate heavy backends (reaper, standalone-audio) so cold builds
   only compile what's used.
3. ~~Retire `FastTrackStudio/apps/*`.~~ DONE — the legacy app is parked;
   `apps/installer` (fts-installer) survived as a root member.
4. ~~Root flake.nix.~~ DONE — adopted signal's flake at the root
   (dioxus-flake toolchain, rust 1.94 + wasm, pipewire/jack shells).

### Dedup queue

- keyflow-daw-analysis's daw types → daw-proto only
- audio-controls (vendored) → fold into features/daw-ui or delete after
  signal-ui migrates off it
- signal-audio remnants, duplicate wav/resampler helpers → libs/utils
  or audiocore-dsp
- three CLIs → one `fts` CLI in apps/ (subcommands)

## Logging & tracing — wide events, ALWAYS

Before writing ANY log or debug output, load the
`logging-best-practices` skill (`.claude/skills/logging-best-practices/`
— read `rules/fts-rust.md` first). The rules are not optional:

- **The span IS the wide event.** `architect` opens one span per vox
  RPC, `tower_http` one per HTTP request. Enrich it with
  `architect_telemetry::wide::set("namespace.field", value)` — one
  context-rich event per request, never scattered log lines.
- **Never `println!`/`eprintln!`/`dbg!` in server or library code** —
  not in committed code, and not as debug scaffolding either. To chase
  a bug, reproduce it in a failing unit test (the artifact that
  outlives the session) or query the span fields; if you must watch a
  live process, use `tracing` with structured fields behind `RUST_LOG`
  and delete it before committing.
- Follow the established field names (`org.slug`, `auth.*`, `perm.*`,
  `media.*`, `share.*`); record the **shape**, never the secret (no
  tokens, no passwords, no raw note paths/URIs in fields).
- Denials/refusals get ONE `tracing::warn!` line (alertable); allowed
  outcomes ride the span only.
- New surface = new fields: any new HTTP route or RPC service must set
  its outcome fields on the span the way `authorize_media`
  (`media.authorized`, `media.auth_via`) and the share gate
  (`share.outcome`, `share.target_kind`) do.

## Agent skills

### Issue tracker

GitHub Issues on `FastTrackStudios/FastTrackStudio`, via the `gh` CLI.
See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, unchanged: `needs-triage`, `needs-info`,
`ready-for-agent`, `ready-for-human`, `wontfix`.
See `docs/agents/triage-labels.md`.

### Domain docs

Per-domain `CONTEXT.md` files, written by `/domain-modeling` when a term
actually needs resolving. (The root `CONTEXT-MAP.md` pointing at them was
removed — it indexed files that never got written.) See
`docs/agents/domain.md`.
