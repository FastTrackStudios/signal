//! **The Signal Engine** — the one instrument program every sound-generating
//! rig loads a patch into.
//!
//! There is no "sampler patch" vs "synth patch" in this rig: a Keyscape piano,
//! an Omnisphere soundsource and a wavetable all land in the *same* layer
//! program — source stack → dual filters → amp → FX rack, with the envelopes
//! and LFOs attached as modulators. What differs is only which source block is
//! realized. That is what makes one control surface (the layer zoom) correct
//! for every patch, and it's why this lives in `signal-synth` rather than in
//! any one rig: Keys, Synth, Drums and Orchestra all build lanes with it.
//!
//! The shape is the Omnisphere 3 per-layer chain (see [`crate::omni`] for the
//! full Part, including the Quadzone grid and the Common/Aux/Master racks).
//! Every block except the source is a placeholder until its DSP lands —
//! placeholders render as pass-throughs, so a lane sounds exactly like its
//! source until the engine grows into the structure.

use signal_proto::block::BlockType;
use signal_sampler::rig_node::Container;

/// Send target for a layer's aux route — a rig that offers an Aux rack names
/// its container this, and the send resolves; rigs without one drop it.
pub const AUX_RACK: &str = "Aux Rack";

/// What realizes a layer's source block.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Source {
    /// Nothing loaded — the lane is silent but fully structured.
    #[default]
    Empty,
    /// A sample library: a `.signalpack` or a `library.styx` spec path. This
    /// is a Keyscape piano, an Omnisphere soundsource, a drum kit — the
    /// engine does not care which.
    Sample(String),
    /// A wavetable / synth-mode source (no sample library).
    Synth,
}

impl Source {
    /// A sample source from an optional spec path.
    pub fn sample(spec: Option<String>) -> Self {
        match spec {
            Some(s) => Self::Sample(s),
            None => Self::Empty,
        }
    }
}

/// Build one Signal Engine layer named `name`, its source realized by
/// `source`.
///
/// ```text
/// Layer "<name>"
/// ├─ Oscillator   Soundsource → Synth Osc → Unison → Harmonia → FM →
/// │               Ring Mod → Dual Freq Shifter → Waveshaper → Granular
/// ├─ Filters      Filter 1 → Filter 2        (series | parallel)
/// ├─ Amp          Amp
/// ├─ Layer FX     4 slots
/// └─ modulators   Amp Env · Filter Env · Mod Env
/// ```
pub fn signal_layer(name: &str, source: Source) -> Container {
    let mut osc = Container::module("Oscillator");
    osc = match source {
        Source::Sample(spec) => osc.sample_block("Soundsource", spec),
        Source::Empty => osc.block(BlockType::Sampler, "Soundsource"),
        Source::Synth => osc.block(BlockType::Wavetable, "Soundsource"),
    };
    let osc = osc
        .block(BlockType::Wavetable, "Synth Osc")
        .block(BlockType::Unison, "Unison")
        .block(BlockType::Harmonic, "Harmonia")
        .block(BlockType::FmOperator, "FM")
        .block(BlockType::RingModulator, "Ring Mod")
        .block(BlockType::Dfs, "Dual Freq Shifter")
        .block(BlockType::Waveshaper, "Waveshaper")
        .block(BlockType::Granular, "Granular");

    Container::layer(name)
        .param("layer_level", "0")
        .param("filter_routing", "Series")
        .add(osc)
        .add(
            Container::module("Filters")
                .block(BlockType::Filter, "Filter 1")
                .block(BlockType::Filter, "Filter 2"),
        )
        .add(Container::module("Amp").block(BlockType::Amp, "Amp"))
        .add(fx_rack("Layer FX"))
        .send(AUX_RACK, "To Aux")
        .modulator(BlockType::Envelope, "Amp Env")
        .modulator(BlockType::Envelope, "Filter Env")
        .modulator(BlockType::MultisegEnvelope, "Mod Env")
}

/// A 4-slot FX rack — every rack in the engine (Layer / Common / Aux /
/// Master) is exactly four slots.
pub fn fx_rack(name: &str) -> Container {
    let mut rack = Container::module(name);
    for slot in 1..=4 {
        rack = rack.block(BlockType::Custom, format!("{name} Slot {slot}"));
    }
    rack
}

/// The macro groups the layer-zoom "Play" page exposes, in display order.
/// These name the *panels*; each group's parameters are declared by the rig
/// that owns the lane (see `signal-keys`'s layer detail).
pub const MACRO_GROUPS: [&str; 8] = [
    "Source", "Tone", "Filter", "Filter Env", "Amp Env", "Vibrato", "Ambience", "Effects",
];

#[cfg(test)]
mod tests {
    use super::*;
    use signal_sampler::rig_node::Role;

    #[test]
    fn every_source_kind_builds_the_same_stack() {
        for src in [
            Source::Empty,
            Source::Synth,
            Source::Sample("/tmp/none.signalpack".into()),
        ] {
            let l = signal_layer("Lane", src);
            assert_eq!(l.role, Role::Layer);
            let osc = l.find("Oscillator").expect("oscillator module");
            for sub in ["Soundsource", "Unison", "Granular"] {
                assert!(
                    osc.blocks().iter().any(|b| b.display_name() == sub),
                    "osc stack has {sub}"
                );
            }
            assert_eq!(l.find("Filters").expect("filters").blocks().len(), 2);
            assert!(l.find("Amp").is_some());
            assert_eq!(l.find("Layer FX").expect("rack").blocks().len(), 4);
        }
    }
}
