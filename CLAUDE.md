# studio — Repo Instructions

**studio is FastTrackStudio's audio/music product in one repo**: the DAW
(`daw`) and everything built on it — Session (the full DAW), Signal (the
live-rig app), Patchbay (studio routing), the CLAP/VST3 plugins, and
frame (the design / control-surface engine). It is the merger, in October
2026, of the former session, daw, processor, signal, patchbay and frame
repos. One Cargo workspace, one lockfile, one `target/`, one flake.

## Layout

```
apps/       session/{desktop,daw,web,daw-web}  Session: the desktop app, the DAW
                                               view, the site, the DAW in a browser
            signal/{desktop,mobile,web}        Signal: the desktop/iOS app (and its
                                               headless `--engine`), Android, the site
            patchbay/                          fts-patchbay: app, agent CLI, browser remote
            plugins/*                          the CLAP/VST3 plugins (`<name>-plugin`),
                                               verify/ (bundle load checks)
            themer/
crates/     daw/*      the DAW alone: the domain core (`daw`), proto, control,
                       the standalone engine (`daw-standalone`), audio-graph,
                       allocator, transport-sync, actions, module, utils
            studio/    the umbrella over daw: REAPER adapter, project files,
                       UI, CSI, the daw CLI, the integration-test harness —
                       all cargo features of `studio`
            frame/     the design / control-surface engine (+ tools/: cli,
                       desktop, mcp, library-server)
            keyflow/   keyflow-proto, keyflow-syntax (the rest of keyflow is a
                       git dep)
features/   one crate per feature, sub-folders allowed: dsp/* (the effects),
            rigs/* (Signal's rigs; rigs/core/* is the signal domain), audio,
            midi, session, song, chords, formats, editing, expression-editor,
            input, sync, ui/*, faces, control-surfaces, patchbay,
            adapters/reaper/*, dynamic-template, …
vendor/     patched third-party crates (each with UPSTREAM.md or FTS-PATCH.md)
xtask/      devtools, frame (Figma parity), plugins (the bundler), session-reaper
docs/       design/ (one-engine.md), per-domain notes (daw, signal, frame,
            patchbay, …), agents/, guides, specs
nix/        flake modules (toolchain, shells, packages), reaper-config,
            vendored flakes
scripts/    helper scripts the Justfile calls
```

Some features carry their own `AGENTS.md` / `README.md`
(`features/input`, `features/dynamic-template`,
`features/expression-editor`, …) — read it before working there.

## The model

- **daw is the DAW alone; `studio` is the umbrella.** `crates/daw/*` knows
  nothing of REAPER or of any product. REAPER, the project formats, the
  UI, control surfaces and the daw CLI are features of `crates/studio`.
- **One engine.** A process runs ONE daw engine: one `Standalone`, one
  project, one realtime audio engine on one device. A rig is a folder of
  tracks in that project (`signal_rig_host::engine::{Engine, Rig}`), not an
  engine of its own. Signal and Session both just run the DAW: Signal is
  the isolated live-rig app, Session the full DAW, and rigs live in Session
  as tracks. More engines means more processes of the same engine. Read
  **`docs/design/one-engine.md`** before touching rigs, engines, meters or
  MIDI.
- **The apps share their shell**: the log ring / crash log / flight log
  and the update check are `features/app-shell`; prefs are `utils::prefs`.
  Session and Signal do not keep copies.
- **The apps share their shell**: the log ring / crash log / flight log
  and the update check are `features/app-shell`; prefs are `utils::prefs`.
  Session and Signal do not keep copies.
- **Faces are frame documents.** Control surfaces and plugin faces are
  `.frame` documents rendered by `crates/frame`. `just faces` generates the
  plugin faces into `features/faces/generated` (not kept in git; the apps
  read them at runtime).

## Crate rules

- **One crate per feature, with cargo features** — not `-proto` / `-ui` /
  `-dsp` sibling crates. A feature's wire types, UI and engine are features
  of its one crate (`features/dsp/eq`: `native`, `ui`, `profiles`, …;
  `features/patchbay`: `server`, `ui`, `vox`).
- **Path deps inside the repo, declared once** in the root
  `[workspace.dependencies]` and consumed as `x.workspace = true`.
  `default-features = false` cannot be set on a workspace-inherited dep —
  put it on the `[workspace.dependencies]` entry.
- **Git deps across repos, pinned to a tag** (architect, keyflow, editor,
  music-convention, neural-amp-modeler-rs, the `vendor` repo). The branch
  pins that remain say why in Cargo.toml (nice-plug's `fts-baseview-03`,
  reaper-rs, task's `editor`).
- **The root `[patch]` tables**, all live: `crates-io` (the vendor repo's
  vox-phon / phon / phon-jit / styx-format / facet-core forks, the FTS
  blitz tag, pipewire-rs past 0.10.0, in-tree `vendor/baseview` and
  `vendor/anyrender_*`); `DioxusLabs/dioxus` (in-tree
  `vendor/dioxus-native{,-dom}` and `vendor/dioxus-core`);
  `FastTrackStudios/daw` (hands keyflow's git dep the in-tree `daw`,
  `dawfile-reaper` and `fts-sample` — goes when keyflow takes them from
  studio); `FastTrackStudios/architect` (architect's unreleased
  `action-list` branch through the `www.github.com` host — repin to a tag
  when architect cuts one). A `[patch]` **cannot rename a crate**.
- **Co-developing across repos**: override with a local checkout —
  `[patch."https://github.com/FastTrackStudios/architect"] architect =
  { path = "../architect/architect" }` — and never commit it; the paths are
  machine-specific.
- **`include_str!` / `@import` across a repo boundary does not work** (a
  git dep has no stable path on disk). Export the bytes from the owning
  crate (`architect_ui::THEME_CSS`), or resolve the path with
  `cargo metadata` the way `just _lumen-link` and `just _web-tw-link` do.
- **Architect idiom**: services are `#[architect::rpc]` traits (at most 4
  params per method — a Facet constraint); live updates are `#[subscribe]`
  streams from `architect::PubSub` hubs, never Tx-parameter subscriptions;
  transports are `architect::axum_ws` + `LayerRouter` server-side and
  `vox_core::initiator_on(link).establish::<Client>()` client-side.
- **Async**: `tokio::sync::*` for locks and channels;
  `architect::platform::{spawn, sleep, timeout}` for tasks and timers
  (tokio on native, browser timers on wasm).

## Build and test

Dev shell: `nix develop` (or direnv `use flake`) — the Rust 1.94 pin with
wasm (plus iOS on darwin, Android on Linux), `dx`, wasm-bindgen,
tailwindcss, tracey, mold, and the native headers (alsa, pipewire, jack).
It sources `.env` (gitignored) on entry. `nix develop .#ci` is what CI
runs in, `.#reaper-test` the REAPER suites, `.#stems` stem separation.

```bash
just check                 # cargo check --workspace
just test                  # cargo nextest run --workspace --features session-desktop/cli,signal-desktop/cli,web-audio-worklet/guitar,web-audio-worklet/keys
just test-doc              # doctests (nextest cannot run them)
just app [PROJECT]         # Session (session-desktop, release-fast)
just guitar / just keys    # Signal's rigs (signal-desktop)
just signal-engine         # signal-desktop --engine: the headless rig engine (ws://:4040/vox)
just reaper-test [FILTER]  # Session's #[reaper_test] suites in a real REAPER
just reaper daw-test       # daw-reaper's suites (module: features/adapters/reaper/reaper.just)
just plugins-bundle [names] && just plugins-link   # .clap/.vst3 into target/bundled, linked into ~/.clap, ~/.vst3
just faces                 # generate the plugin faces
just assets                # link the big binaries (see Assets)
```

The test feature list matters: the app CLIs and the worklet rigs sit
behind features, and without them their tests never build.

**Before you push: `just ci`.** It runs exactly what
`.github/workflows/checks.yml` runs, in the same order, and stops at the
first failure: lockfile drift, `cargo fmt --check`, the flow gate
(`just daw-flows`), the web tailwind sheet, `cargo check --workspace`, the
Signal wasm check, then nextest as above. `just ci <step>` starts from a
step (`lockfile`, `fmt`, `flows`, `tailwind`, `check`, `wasm`, `nextest`).
It refuses to start with less than 25 GB free: a full disk shows up as
mold dying with a bus error, which reads as a broken toolchain
(`just sweep-incremental` is the always-safe reclaim). It says nothing
about `session-ios` / `signal-ios` (Xcode, signing), `deploy` or
`reaper-integration`, which have their own runners and reasons to be red.

**Build performance / disk** — read the `build-performance` skill before
touching a `[profile.*]` knob, benchmarking a build, or when the disk
fills up. The short version:

- Dev debuginfo is `line-tables-only`; need a debugger? `--profile dev-dbg`.
- mold is the Linux linker — `direnv reload` after pulling, or links fail
  with `cannot find -fuse-ld=mold`. `target.<triple>.rustflags` REPLACES
  `build.rustflags`, so a new global flag goes in both.
- Dependencies build at opt-level 1, with an allowlist back at 3 for
  audio-thread crates. A dev run of a rig must never xrun: extend the
  allowlist, never raise `package."*"`.
- `release-fast` is the incremental release profile the app recipes
  share; build an app with one feature set, or cargo rebuilds everything.
- Cargo never GCs `target/`: `just disk`, `just sweep`, `just sweep-all`,
  `just sweep-incremental`.
- There is no `RUSTC_WRAPPER` (sccache breaks `dx`). Check `uptime` /
  `pgrep -c rustc` before benchmarking.

## Assets

Big binaries (test fixtures over 1 MB, research captures, the rigs' NAM
captures and IRs) are not in git. They live in the private repos
`studio-assets` and `rig-models`; `assets.txt` lists each file or
directory and where it goes, and `just assets` clones those repos into
`.assets/` and symlinks everything into place. A new large file goes into
one of those repos plus a line in `assets.txt` (and `.gitignore`), never
into this repo.

## Licence

**GPL-3.0-or-later.** The workspace `license` field, every FTS-owned crate
and `LICENSE` at the root say so; a new crate inherits it with
`license.workspace = true` and does not declare its own.

Vendored code keeps the licence of the code it vendors, because it is not
ours to change: everything under `vendor/` (the dioxus crates, baseview,
anyrender_*, wgpu-hal, winit-uikit, raw-window-metal, dioxus-test — MIT
and/or Apache-2.0; `vendor/world`, the WORLD vocoder — BSD-3-Clause), and
`features/patchbay/driver` (forked from MARS mars-hal:
`GPL-3.0-or-later AND MIT`). Incoming code must be GPL-compatible (MIT,
BSD, Apache-2.0 — keep the notice and licence text); code cannot go the
other way.

## Signal, plugin and DSP rules

Signal is the live-rig domain: `features/rigs/*` (the domain core in
`features/rigs/core/*` — proto, live, storage, controller, rig-host, grid,
tone3000, …; docs in `features/rigs/core/docs/`), the effects in
`features/dsp/*`, `features/adapters/reaper/signal-*` and `apps/signal/*`.
`docs/signal/domain-model.md` explains the rig model.

**Detachable GUI (strict)**: the rig engine is headless; every GUI —
desktop, phone, browser, plugin editor — is a vox client of it.

**GUI rendering** — signal and plugin UI renders identically standalone,
as a CLAP/VST3 editor and embedded in REAPER, through one pipeline:
`nice-plug-dioxus` / `dioxus_native` → Blitz (Vello + wgpu):

- Never `dioxus::desktop::LaunchBuilder` for this UI (WebKit/WRY breaks
  plugin parity; `signal-desktop --features webview` is an escape hatch
  that nothing painted can mount under).
- **Inline styles only** — Blitz does not load external stylesheets.
  Inline `style`, or a static string via `document::Style { {CSS} }`;
  never `document::Stylesheet { href }`, no Tailwind `asset!()` —
  `include_str!` the compiled sheet (`just tailwind`, `just signal-tailwind`,
  `just tailwind-eq`, …). Components must lay out correctly without
  Tailwind; classes are additive only.
- Root `App` components take no props (context via
  `use_context_provider`), so one component serves standalone and as a
  plugin editor.
- Blitz's gaps: no `position: fixed`/`sticky` (absolute positions against
  the immediate parent; mount overlays at the root with `inset: 0`), no
  `overflow: auto` (use `overflow-y: scroll`, or `hidden` to clip), no
  `text-overflow`, no CSS `fill`/`stroke` on SVG (use presentation
  attributes), no `vertical-align`, `text-shadow`, blend modes, 3D
  transforms, multi-column or container queries. Inside a plugin
  `use_effect` never runs, `autofocus` is ignored unless the node is
  already in the document, `MountedData::set_focus` panics from an event
  handler, and there is no `placeholder`. Load the **`blitz-design`
  skill** before building or refactoring this UI.

**Processing cores** (`crates/daw/audio-graph`, the engines in
`features/dsp/*`, the sampler engine) build for native, WASM/AudioWorklet
and embedded `no_std`: `#![no_std]` + `alloc` (gate `std` behind an
additive feature); no heap allocation on the hot path (pre-allocate in
`reset()`; `process()` never calls `Vec::push`/`Box::new`); no threads and
no task spawning; no platform I/O (cpal, web-sys and MIDI drivers live in
adapter crates); keep the `AudioNode: Send` bound.

**Effects do not depend on products**: nothing in `features/dsp/*` may
depend on `features/rigs`, `features/session` or an app. If an effect
needs something from a product, the dependency points the wrong way — the
product adapts to the effect (`builtin` makes the effects daw engine
blocks, a contract both products already speak).

**Parameters must parse the string they print** — a host's parameter list
is text in, text out; each effect's `tests/param_round_trip.rs` holds that
line. Tempo-synced times use `dsp_core::time` (note values, dotted and
triplet); do not grow a second table.

**Realtime lints**: `features/{dsp,rigs,audio}/clippy.toml` add the
audio-thread rules (`disallowed-methods` for locks, `env::var`, sleeps;
`too-large-for-stack`). Clippy reads the nearest `clippy.toml`, so they hold
under those trees and nowhere else — keep the three copies identical, and
keep the shared settings in step with the root one.

**Realtime lints**: `features/{dsp,rigs,audio}/clippy.toml` add the
audio-thread rules (`disallowed-methods` for locks, `env::var`, sleeps;
`too-large-for-stack`). Clippy reads the nearest `clippy.toml`, so they hold
under those trees and nowhere else — keep the three copies identical, and
keep the shared settings in step with the root one.

## Logging & tracing — wide events, ALWAYS

Before writing ANY log or debug output, load the `logging-best-practices`
skill (`.claude/skills/logging-best-practices/` — read `rules/fts-rust.md`
first). The rules are not optional:

- **The span IS the wide event.** `architect` opens one span per vox RPC,
  `tower_http` one per HTTP request. Enrich it with
  `architect_telemetry::wide::set("namespace.field", value)` — one
  context-rich event per request, never scattered log lines.
- **Never `println!`/`eprintln!`/`dbg!` in server or library code** — not
  in committed code, and not as debug scaffolding either. To chase a bug,
  reproduce it in a failing unit test or query the span fields; if you
  must watch a live process, use `tracing` with structured fields behind
  `RUST_LOG` and delete it before committing.
- Follow the established field names (`org.slug`, `auth.*`, `perm.*`,
  `media.*`, `share.*`); record the **shape**, never the secret (no
  tokens, no passwords, no raw note paths/URIs in fields).
- Denials/refusals get ONE `tracing::warn!` line (alertable); allowed
  outcomes ride the span only.
- New surface = new fields: any new HTTP route or RPC service sets its
  outcome fields on the span (`rules/fts-rust.md` lists the field
  conventions, e.g. `auth.outcome`).

## Agent skills and process

- Issues: GitHub Issues on `FastTrackStudios/studio` via `gh` — see
  `docs/agents/issue-tracker.md`; the triage labels (`needs-triage`,
  `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) in
  `docs/agents/triage-labels.md`; domain docs in `docs/agents/domain.md`.
- Project skills live in `.claude/skills/` (build-performance,
  logging-best-practices, blitz-design, reaper-testing, architect-actions,
  signalpack, reaper-theme-*, …) — load the matching one before the work
  it covers.
