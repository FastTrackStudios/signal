//! The full **Omnisphere 3** per-Part signal chain, built as a composition
//! tree ([`sampler::rig_node`]) — the routing experiment companion to
//! [`sampler::nord`].
//!
//! If the engine can express both the Nord Stage 4 and the Omnisphere 3 routing,
//! there aren't many synth architectures it can't.
//!
//! Source: the official Omnisphere 3 Reference Guide (v3.0.2c). One Part:
//!
//! ```text
//! Preset "Omnisphere"
//! ├─ Quadzone (parallel)             ← the v3 layer grid (Fader/Notes/Velo)
//! │  ├─ Layer A..D, each:
//! │  │  ├─ Oscillator module (serial, all pre-filter; UI tab order):
//! │  │  │    Soundsource|Wavetable → Unison → Harmonia (4 extra oscs) →
//! │  │  │    FM → Ring Mod → Dual Freq Shifter → Waveshaper → Granular
//! │  │  ├─ Filters module — 2 filters, SERIES (or parallel), 70 types each
//! │  │  ├─ Amp module
//! │  │  ├─ Layer FX rack (4 slots, Independent mode)
//! │  │  ├─ → send to the Part Aux rack
//! │  │  └─ modulators: Amp Env, Filter Env, Mod Env (4 of each per Part,
//! │  │       one per layer here), MSEG-capable
//! │  ├─ …
//! ├─ Common FX rack (4 slots, insert)  → Part output
//! ├─ Aux FX rack (4 slots, send/return)
//! ├─ Master FX rack (4 slots — Multi-level in Omnisphere; kept in the
//! │    preset tail here, like the Nord global tail)
//! └─ Part modulators: 8 LFOs, 48-slot Mod Matrix, Arpeggiator (32-step)
//! ```
//!
//! Everything is a placeholder until its DSP lands, except Sampler blocks —
//! [`omnisphere_soundsource_preset`] realizes Layer A/B with real extracted
//! Omnisphere soundsources ([`BlockImpl::Sample`](sampler::rig::BlockImpl)).
//! The Multi level (8 Parts, mixer, 4 shared Aux racks) is a Rig concern and
//! comes later.

use signal_proto::block::BlockType;

use sampler::rig_node::Container;

/// Send target for every layer's aux route (the Part's 4-slot Aux rack).
use crate::engine::AUX_RACK;

// ── Layers ───────────────────────────────────────────────────────────────────

/// One Omnisphere Layer: oscillator stack → dual filters → amp → layer FX.
/// `soundsource` = a library spec path realizing the Sample-mode source;
/// `None` keeps a placeholder Soundsource block (structure only).
fn layer(name: &str, soundsource: Option<String>) -> Container {
    // Omnisphere's "layer" is what this engine calls a MODULE: one Source
    // Block into filters → amp → FX. Four of them make the Quadzone, which is
    // exactly one `crate::engine::signal_layer`.
    crate::engine::signal_module(name, crate::engine::Source::sample(soundsource))
}

use crate::engine::fx_rack;

// ── The Part ─────────────────────────────────────────────────────────────────

/// Build the Part shell shared by the placeholder and realized variants.
fn build_part(name: &str, sources: [Option<String>; 4]) -> Container {
    let [a, b, c, d] = sources;
    let mut preset = Container::preset(name)
        // The v3 Quadzone layer grid: Fader (modulatable scan), Notes (splits)
        // and Velo (velocity switch) modes — Notes/Velo map onto each layer's
        // Zone; the Fader scan needs the mod matrix (control-rate, later).
        .add(
            Container::parallel("Quadzone")
                .param("mode", "Fader") // Fader | Notes | Velo
                .add(layer("Layer A", a))
                .add(layer("Layer B", b))
                .add(layer("Layer C", c))
                .add(layer("Layer D", d)),
        )
        // Insert rack on the Part output.
        .add(fx_rack("Common FX"))
        // Send/return rack (layers route here via their To-Aux sends; the
        // module name matches the send target).
        .add(fx_rack(AUX_RACK))
        // Multi-level in Omnisphere proper; kept as the preset tail here.
        .add(fx_rack("Master FX"))
        // Part-level modulators: 8 LFOs, the 48-slot Mod Matrix, the Arp.
        .modulator(BlockType::ModMatrix, "Mod Matrix")
        .modulator(BlockType::Arpeggiator, "Arp");
    for n in 1..=8 {
        preset = preset.modulator(BlockType::Lfo, format!("LFO {n}"));
    }
    preset
}

/// The complete Omnisphere 3 Part as a placeholder routing tree. Inspect with
/// [`Container::dump`].
#[must_use]
pub fn omnisphere_preset() -> Container {
    build_part("Omnisphere", [None, None, None, None])
}

// ── Sample-realized variant (machine-local soundsource extraction) ──────────

/// Root of the local Omnisphere soundsource extraction — family dirs holding
/// per-soundsource `library.styx` dirs (multisample) or flat `<name>.styx` +
/// FLAC pairs (one-shots). Override with `FTS_OMNISPHERE_ROOT`.
pub(crate) const OMNISPHERE_ROOT: &str = "/run/media/AudioHaven/Sampled/Keys/Omnisphere";

/// Spec path for one extracted soundsource (relative to the Core
/// Soundsources dir), if present.
fn soundsource_spec(relative: &str) -> Option<String> {
    let root = std::env::var("FTS_OMNISPHERE_ROOT").unwrap_or_else(|_| OMNISPHERE_ROOT.into());
    let p = std::path::Path::new(&root)
        .join("Core Soundsources")
        .join(relative);
    p.exists().then(|| p.to_string_lossy().into_owned())
}

/// The Omnisphere Part with Layers A/B realized by real extracted
/// soundsources (Dream Piano + CS-80 Strings). `None` on machines without
/// the extraction — callers register it conditionally.
#[must_use]
pub fn omnisphere_soundsource_preset() -> Option<Container> {
    let a = soundsource_spec("Keyboards/Dream Piano/library.styx")?;
    let b = soundsource_spec("Synth Classic/CS-80 Strings/library.styx");
    Some(build_part(
        "Omnisphere (Soundsources)",
        [Some(a), b, None, None],
    ))
}
