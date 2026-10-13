# daw

The DAW platform: a DAW-agnostic domain core, the backends that drive
real hosts, the per-DAW project-file parsers, and the shared audio /
MIDI / input substrate everything else in FastTrackStudio is built on.

Split out of the FastTrackStudio monorepo in August 2026. This is the
bottom of the stack — it depends on no other FastTrackStudio product
repo, and `session`, `signal` and `Ignition` all depend on it.

## Layout

```
crates/daw/          the domain core — items, tracks, tempo/measure
                     conversion, transport, ext-state, undo
crates/audiocore/    audio primitives, DSP, sample handling, metering
crates/midicore/     MIDI types, ports, the midir backend
crates/input/        keybinds, input config, the Dioxus input layer
crates/kontrol/      Native Instruments hardware (S88 light guide)
features/dawfile/    per-DAW project parsers — reaper, protools,
                     ableton, logic, aaf, dawproject
features/reaper/     the REAPER backend + theming + extension runtime
features/audio/      the audio graph, IO, allocator
features/sync/       transport sync, Link, network
features/standalone/ daw-standalone — the headless workstation backend
features/daw-ui/     the DAW UI surface and theme art
features/expression-editor/  MPE + Melodyne-style expression editing
libs/                dock, utils, adaptive-grid, devtools, installer-core
```

## Build

```bash
nix develop          # or: direnv allow
cargo check --workspace
cargo nextest run --workspace
```

`rust-version` is a **floor** (1.87), not a pin, so consumers on newer
toolchains — Ignition runs 1.95 — can depend on this repo directly.

## Licence

GPL-3.0-or-later, except two vendored crates that keep the licence of
the code they vendor: `libs/vendor/world` (WORLD vocoder, M. Morise,
BSD-3-Clause) and `libs/vendor/dioxus-test` (DioxusLabs, MIT OR
Apache-2.0).
