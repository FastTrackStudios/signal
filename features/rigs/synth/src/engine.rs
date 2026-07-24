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
//! The shape is Omnisphere's Quadzone: a **layer** holds four **modules**,
//! and a module is the engine — one Source Block into filters → amp → FX.
//! (See [`crate::omni`] for the full Part, including the Common/Aux/Master
//! racks.)
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

/// How many modules a layer starts with — Omnisphere's Quadzone, and the
/// A/B/C/D the layer zoom switches between. It is a *default*, not a limit:
/// [`signal_layer`] builds as many modules as it is given sources for.
pub const MODULES_PER_LAYER: usize = 4;

/// Slot label for module `index`: A..Z, then A1, B1, … so a layer can grow
/// past the alphabet without ambiguity.
pub fn module_slot(index: usize) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let letter = LETTERS[index % LETTERS.len()] as char;
    let wrap = index / LETTERS.len();
    if wrap == 0 { letter.to_string() } else { format!("{letter}{wrap}") }
}

/// The first four slot labels — the common case, kept for call sites that
/// want a quick array.
pub const MODULE_SLOTS: [&str; MODULES_PER_LAYER] = ["A", "B", "C", "D"];

/// Build one **module** — the engine itself: a single Source Block feeding
/// filters → amp → FX, with the envelopes attached.
///
/// ```text
/// Module "<name>"
/// ├─ Source     ONE generator: sampler | oscillator | wavetable
/// ├─ Filters    Filter 1 → Filter 2
/// ├─ Amp        Amp
/// ├─ FX         4 slots
/// └─ modulators Amp Env · Filter Env · Mod Env
/// ```
///
/// Exactly one block in a module generates. That is not a style choice: a
/// source ignores its input and writes its own output, so a second generator
/// in series *replaces* the first — put a wavetable after a sampler and every
/// patch plays the wavetable. The oscillator-zoom extras (Harmonia, FM, Ring
/// Mod, granular…) join as they land as real processors, or as alternative
/// Source Blocks.
pub fn signal_module(name: &str, source: Source) -> Container {
    let src = match source {
        // The sampler: a Keyscape piano, an Omnisphere soundsource, a kit.
        Source::Sample(spec) => Container::module("Source").sample_block("Soundsource", spec),
        // Synthesis: the native oscillator is the generator.
        Source::Synth => Container::module("Source").block(BlockType::Oscillator, "Soundsource"),
        // Nothing loaded — a placeholder that passes audio (silent lane).
        Source::Empty => Container::module("Source").block(BlockType::Sampler, "Soundsource"),
    };
    Container::module(name)
        .param("module_level", "0")
        .param("filter_routing", "Series")
        .add(src)
        .add(
            Container::module("Filters")
                .block(BlockType::Filter, "Filter 1")
                .block(BlockType::Filter, "Filter 2"),
        )
        .add(Container::module("Amp").block(BlockType::Amp, "Amp"))
        .add(fx_rack("FX"))
        // Each module routes to the Part's aux rack independently (rigs
        // without an Aux Rack container simply drop the send).
        .send(AUX_RACK, "To Aux")
        .modulator(BlockType::Envelope, "Amp Env")
        .modulator(BlockType::Envelope, "Filter Env")
        .modulator(BlockType::MultisegEnvelope, "Mod Env")
}

/// Build one **layer**: four modules in parallel (they sum — a layer is a
/// stack of voices, not a chain), plus the layer's aux send.
///
/// `sources[i]` realizes module `MODULE_SLOTS[i]`; `Source::Empty` leaves a
/// structured but silent module, so the shape is always the full quad.
pub fn signal_layer(name: &str, sources: &[Source]) -> Container {
    let mut modules = Container::parallel(format!("{name} Modules"));
    for (i, source) in sources.iter().enumerate() {
        modules = modules.add(signal_module(
            &format!("{name} {}", module_slot(i)),
            source.clone(),
        ));
    }
    Container::layer(name).add(modules)
}

/// A layer whose module A holds `source` and B/C/D are empty — the common
/// "one patch in this lane" case.
pub fn signal_layer_single(name: &str, source: Source) -> Container {
    let mut sources = vec![source];
    sources.resize(MODULES_PER_LAYER, Source::Empty);
    signal_layer(name, &sources)
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
    fn a_module_has_exactly_one_generator() {
        // The invariant that keeps a sampler audible: a second source in
        // series would overwrite it.
        for src in [
            Source::Empty,
            Source::Synth,
            Source::Sample("/tmp/none.signalpack".into()),
        ] {
            let m = signal_module("Mod", src);
            let sources = m.find("Source").expect("source module");
            assert_eq!(sources.blocks().len(), 1, "exactly one Source Block");
            assert_eq!(m.find("Filters").expect("filters").blocks().len(), 2);
            assert!(m.find("Amp").is_some());
            assert_eq!(m.find("FX").expect("rack").blocks().len(), 4);
        }
    }

    #[test]
    fn a_layer_holds_four_modules() {
        let l = signal_layer_single("Keys A", Source::Sample("/tmp/a.signalpack".into()));
        assert_eq!(l.role, Role::Layer);
        for slot in MODULE_SLOTS {
            assert!(l.find(&format!("Keys A {slot}")).is_some(), "module {slot}");
        }
        // Not limited to four — a layer takes as many sources as it's given.
        let big: Vec<Source> = (0..6).map(|_| Source::Empty).collect();
        let l = signal_layer("Wide", &big);
        assert!(l.find("Wide F").is_some(), "sixth module");
    }
}
