# signal

The Signal Suite: the headless audio engine, the instrument rigs, the
sampler, and the FTS plugin set (CLAP/VST3) that ships alongside it.

Split out of the FastTrackStudio monorepo in August 2026.

## Detachable GUI (strict)

The rig core is 100% headless. Every GUI is a vox remote over architect:
`fasttrackstudio --engine` serves the router, and the browser, desktop
and tablet surfaces are all clients of the same wire contract. Signal
UI must render identically standalone, as a VST3/CLAP plugin, and
embedded in REAPER, so all three share one pipeline:
`nice-plug-dioxus` -> Blitz (Vello + wgpu) -> baseview.

## Layout

```
crates/signal/       the signal domain — proto, live, storage, browser,
                     grid, rig-host, controller, import, widgets
features/rigs/       the instrument rigs — keys, drums, bass, synth,
                     guitar, orchestra, ekit
features/sampler/    the sampler engine and .signalpack format
features/fx/         the DSP — eq, comp, reverb, delay, saturate,
                     limiter, gate, tune, pitch, level, modulation
features/nam/        neural amp modeler
features/plugin-host/  hosting third-party plugins
apps/plugins/        the CLAP/VST3 cdylibs
apps/fasttrackstudio/  the Signal app + `--engine` + browser remote
```

## Build

```bash
nix develop
cargo check --workspace
cargo build -p fasttrackstudio          # the app; --engine is headless
```

## Licence

GPL-3.0-or-later.
