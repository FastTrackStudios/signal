# Processor

The DSP, the effects, and the plugins.

Everything here is about turning audio into other audio: the filter design
and biquad cascades, the compressor and saturator engines, the reverb
algorithms and delay lines, the faces that drive them, and the CLAP/VST3
plugins they ship as.

Split out of [signal](https://github.com/FastTrackStudios/signal) so that
signal and [session](https://github.com/FastTrackStudios/session) can both
use the effects without depending on each other. Signal's business is the
sampling path, loading, and switching rigs gaplessly; session's is
setlists and songs. The effects are neither, and both want them.

## What is here

```
features/fx/       the effects, each as core / facade / profiles / ui
  eq comp saturate delay reverb  the five that matter most
  musical-time     note values, dotted and triplet, in seconds at a tempo
  stack multiband modulation macromod trigger tune pitch level hit-detect
  dsp-core         shared DSP primitives
  dsp-golden       bit-exact golden-master harness
  fx-blocks        the FX as native `PluginInstance` blocks, for a host
                   that wants them without CLAP or a GUI
features/preset-browser/   preset UI the plugin faces share
apps/plugins/      the CLAP/VST3 bundles, plus:
  host             fts-clap-host — open any CLAP plugin's GUI in a window
  xtask            the bundler
vendor/            two crates carrying patches we have not upstreamed;
                   each has an FTS-PATCH.md saying what and why
```

## Build

```bash
cargo check -p eq-plugin -p comp-plugin -p saturate-plugin -p delay-plugin -p reverb-plugin
just plugins-bundle            # all of them, as .clap and .vst3
just host eq                   # open one in the standalone host
```

`cargo check --workspace` pulls PipeWire in through `daw`'s audio backend
and does not build on macOS. That is inherited, not new — build the
plugins, or the crate you are working on.

## Licence

GPL-3.0-or-later, except the two vendored crates, which keep the licence
of the code they vendor.
