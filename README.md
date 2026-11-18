# signal

The live-rig domain of FastTrackStudios' audio stack: the instrument rigs,
the sampler (KONTRA), NAM, Signal's domain crates and the Signal plugins.
They came here from [FastTrackStudios/studio](https://github.com/FastTrackStudios/studio)
with their history (studio#510). Studio, the product, takes them from here
by git, every crate at one revision.

| Path | What it is |
|---|---|
| `features/signal` | Signal's domain: rigs, presets, scenes, the browser, storage, the controller, the grid, macros, the content library, Tone3000, import (the facade crate `signal` at the root) |
| `features/rigs` | The instruments (guitar and bass, keys with the Wurlitzer, drums, ekit, synth, the orchestra, fretted), the rig host on the one engine, the rig frame (`signal-rig-shell`), the perform screens and store, the rigs' shared proto, and `kontrol` (the Komplete Kontrol Light Guide driver the drum rig lights a keybed with) |
| `features/sampler` | The sampler engine and its packs |
| `features/dsp/nam` | `signal-nam`, NAM captures as blocks |
| `apps/plugins/{signal,sampler,nam}` | The CLAP/VST3 plugins: Signal (preset chains in any DAW), the sampler, NAM |
| `vendor/tone3000` | tone3000 0.1, patched onto reqwest 0.12's TLS (`vendor/tone3000/FTS-PATCH.md`) |
| `xtask/plugins` | The plugin bundler (`fts-plugin-xtask`, nice-plug's) |

Below these: [daw](https://github.com/FastTrackStudios/daw) (the base and
the engine), [fts-fx](https://github.com/FastTrackStudios/fts-fx) (the
effects), Frame and Architect, all by git. Nothing below depends on
anything here.

## Build

```sh
devenv shell          # or direnv: the fleet toolchain (rust 1.99.1), kache
cargo check --workspace --all-targets
cargo test --workspace
```

The plugins:

```sh
cargo run -p fts-plugin-xtask -- bundle signal-plugin --release   # target/bundled/FTS Signal.{clap,vst3}
cargo run -p fts-plugin-xtask -- bundle nam-plugin --release
cargo run -p fts-plugin-xtask -- bundle signal-sampler-clap --release
```

## The rigs' captures are not here

The guitar and bass rigs embed their captures, cab IRs and frozen Cores at
build time (`features/rigs/guitar/build.rs`). They are private (the
rig-models repository) and this repository is public, so they are not in
it. The build script reads them from `$SIGNAL_RIG_LIBRARY/{guitar,bass}/
{models,frozen}` when the app that builds the crate says where its copy is
(Studio does, from its `.cargo/config.toml`), and from
`default-config*/models` and `default-config*/frozen` otherwise (both
ignored by git). Without them a debug build has rigs with no captures;
a release build refuses to start.

## In Studio

Studio pins every crate here at one `rev` in its root `Cargo.toml`. A
change here is a PR here, then a bump of that revision in Studio. To work
on both at once, check this repo out beside Studio and patch the git
source with paths in Studio's root manifest, never committed:

```toml
[patch."https://github.com/FastTrackStudios/signal"]
guitar = { path = "../signal/features/rigs/guitar" }
sampler = { path = "../signal/features/sampler" }
# … every crate of this repo that Studio builds, so their own path
# dependencies come from the checkout too
```

Studio keeps the Signal site (`apps/signal/web`) and the Android app
(`apps/signal/mobile`) for now: Studio builds, images and deploys the
site, and both depend only on crates here.

## Licence

GPL-3.0-or-later (`LICENSE`). Vendored code keeps its own licence
(`vendor/tone3000`: MIT).
