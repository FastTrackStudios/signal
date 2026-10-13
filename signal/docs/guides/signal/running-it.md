---
title: Running it
order: 3
summary: Building the engine and pointing something at it
---

# Running it

Everything builds from the repo root — one Cargo workspace, one lockfile.

```bash
# The app: desktop GUI, and `--engine` for the headless rig.
cargo build -p signal-desktop
target/debug/signal-desktop --engine
```

The engine serves on `ws://:4040/vox`. Any remote — the desktop GUI, the
browser build, a tablet — connects to that.

## The dev shell

`nix develop` (or direnv `use flake`) gets you the pinned toolchain, `dx`, the
wasm target, and the native audio headers (alsa, pipewire, jack). The
toolchain is pinned deliberately: a floating `wasm-bindgen` silently resolves
past the CLI in the shell and breaks the web build.

## Configuration

Rig configuration lives in `~/.config/signal/rig/*.styx`. Styx is the
configuration language used across FastTrackStudio repos; it is typed and
schema-checked, so a malformed rig fails at load with a location rather than
at the first note.
