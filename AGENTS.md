# signal — Repo Instructions

The rigs, the sampler and the Signal plugins, out of
FastTrackStudios/studio with their history (studio#510). `README.md` has
the layout. Studio takes every crate here by git at one revision; nothing
below (daw, fts-fx, Frame, Architect) may depend on anything here.

## Rules (they came with the code from Studio's AGENTS.md)

- **One engine.** A process runs one daw engine; a rig is a folder of
  tracks in its project (`signal_rig_host::engine::{Engine, Rig}`), not an
  engine of its own.
- **Detachable GUI**: the rig engine is headless; every GUI (desktop,
  phone, browser, plugin editor) is a vox client of it.
- **GUI rendering**: one pipeline, `nice-plug-dioxus` / `dioxus_native`
  → Blitz (Vello + wgpu). Inline styles only (Blitz loads no external
  stylesheet); root `App` components take no props. No `position:
  fixed`/`sticky`, no `overflow: auto` (use `overflow-y: scroll`), no
  `text-overflow`, no CSS `fill`/`stroke` on SVG.
- **Processing cores** (the sampler engine, the rigs' DSP) build for
  native, wasm/AudioWorklet and embedded: no heap allocation, threads,
  task spawning or platform I/O on the audio path.
- **Realtime lints**: `features/rigs/clippy.toml` and
  `features/dsp/clippy.toml` add the audio-thread rules (locks,
  `env::var`, sleeps). Keep them identical to each other and to fts-fx's
  and daw's copies, and the shared settings in step with the root one.
- **Parameters parse the string they print**.
- **No `include_str!` / `include_bytes!` across a repo boundary.** A file
  of Studio's is not here; take it at run time (rig_shot's
  `RIG_SHOT_TAILWIND`) or from the crate that owns it.
- **No `env!("CARGO_MANIFEST_DIR")` outside tests** (kache keys), no
  absolute paths in build-script output.
- **The rigs' captures are private**: never commit a capture, IR, frozen
  Core, sample library, mic model or anything captured from a commercial
  product. They reach a build through `SIGNAL_RIG_LIBRARY` (README).
- **Logging**: wide events. Never `println!`/`eprintln!`/`dbg!` in library
  code; enrich the span with `architect_telemetry::wide::set`.
- **Async**: `tokio::sync::*`; `architect::platform::{spawn, sleep,
  timeout}` for tasks and timers.
- **Crates**: one crate per feature, with cargo features; path deps
  declared once in `[workspace.dependencies]`; git deps pinned to a tag or
  a `rev` (the daw repo and fts-fx at one revision each, Studio's).
  `license.workspace = true` (GPL-3.0-or-later).

## Pins

The external pins (`architect`, `dioxus`, `vox`, `facet`, nice-plug's
branch, the `[patch]` tables, the daw and fts-fx revisions) are a copy of
Studio's, and `Cargo.lock` was seeded from Studio's: move a pin here and
in Studio together, or Studio builds two of a crate.

## Process

- Dev shell: `devenv shell` (devenv-fleet's rust module: rust 1.99.1, the
  wasm target, kache, prek).
- CI (`.github/workflows/checks.yml`) runs on PRs that are not drafts:
  fmt, `cargo check --workspace --all-targets`, `cargo test --workspace`,
  and the wasm32 checks for what Studio builds for the browser. Mark a PR
  ready to get checks.
- A change file per PR (`.changes/README`).
- Commit messages and PRs: conventional titles, one concern per PR.
