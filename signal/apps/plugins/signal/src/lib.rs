//! FTS Signal — the all-in-one rig-platform plugin (v0).
//!
//! The vision (issue #31): one CLAP/VST3 that hosts any FTS-Signal rig —
//! guitar rig, drum rig, vocal rig, keys — inside the DAW, with the same rig
//! presets the live engine (`fasttrackstudio --engine`) plays.
//!
//! ## What v0 actually is
//!
//! An INSTRUMENT+FX hybrid shell (MIDI in, stereo audio in AND out) over
//! [`signal_sampler::SamplerBank`] — the one rig backend the signal stack
//! exposes headlessly today. It loads a `.signalpack` rig at `initialize()`
//! (see [`config`] for resolution: `$FTS_SIGNAL_RIG`, else
//! `~/.config/signal/plugin/rig.styx`), plays it from live MIDI, and sums the
//! rig output onto the (gain-staged) audio passthrough. If nothing loads, the
//! plugin is a plain gain-staged passthrough that ignores MIDI.
//!
//! Rig browsing/management and rig-internal parameters arrive with the GUI
//! (nice-plug-dioxus editor + param rescan); v0 exposes only input/output
//! gain.
//!
//! ## Why sampler-only (the facade gaps)
//!
//! The `signal` facade (`crates/signal/signal`) is a control plane:
//! `SignalController`/`SignalLive` manage rigs/blocks/presets in a database
//! and *apply* them to DAW-hosted FX via the daw-bridge
//! (`DawPatchApplier`, morph engine, state chunks). Audio for FX-chain rigs
//! is hosted by `features/plugin-host/signal-plugin-host` (a CLAP/VST3 host
//! over `daw`), which the facade does not re-export — there is no
//! "give me a rig chain I can push audio blocks through" API yet. Until that
//! exists, guitar/vocal FX-chain rigs cannot be hosted here and v0 mirrors
//! `signal-sampler-clap`'s engine usage instead.

pub mod config;
pub mod plugin;

pub use plugin::FtsSignal;

use nice_plug::prelude::*;

nice_export_clap!(FtsSignal);
nice_export_vst3!(FtsSignal);
