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
use signal_sampler::rig::RigBlock;
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
/// What one module is *set to* — the values behind its macro panel.
///
/// A module is built from these, so the Filter block and the envelopes are
/// real DSP with real numbers rather than a structure waiting for them: the
/// Filter block gets its cutoff and resonance, the Amp Env drives the Amp's
/// gain, and the Filter Env drives the cutoff by the module's env amount.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleSettings {
    pub source: Source,
    /// Filter cutoff in Hz and resonance 0..1.
    pub cutoff_hz: f32,
    pub resonance: f32,
    /// How far the Filter Env opens the cutoff, −1..1 of the normalized range.
    pub filter_env_depth: f32,
    /// Amp and Filter envelopes as `(attack_ms, decay_ms, sustain, release_ms)`.
    pub amp_env: (f32, f32, f32, f32),
    pub filter_env: (f32, f32, f32, f32),
    /// Unison voices + detune, for sampler sources that support them.
    pub unison: u32,
    pub detune: f32,
}

impl Default for ModuleSettings {
    fn default() -> Self {
        Self {
            source: Source::Empty,
            // Wide open: a module with no filter move sounds like its source.
            cutoff_hz: 20_000.0,
            resonance: 0.0,
            filter_env_depth: 0.0,
            amp_env: (0.0, 0.0, 1.0, 120.0),
            filter_env: (0.0, 0.0, 1.0, 120.0),
            unison: 1,
            detune: 0.1,
        }
    }
}

impl ModuleSettings {
    /// Settings for a bare source, everything else at its default.
    pub fn from_source(source: Source) -> Self {
        Self { source, ..Self::default() }
    }
}

/// An envelope modulator carrying its times (the block params the mod engine
/// reads: seconds, sustain 0..1).
fn envelope(name: &str, (a, d, s, r): (f32, f32, f32, f32)) -> RigBlock {
    RigBlock::of_type(BlockType::Envelope)
        .named(name)
        .with_param("attack", format!("{:.4}", a.max(0.0) / 1000.0))
        .with_param("decay", format!("{:.4}", d.max(0.0) / 1000.0))
        .with_param("sustain", format!("{:.4}", s.clamp(0.0, 1.0)))
        .with_param("release", format!("{:.4}", r.max(0.0) / 1000.0))
}

pub fn signal_module(name: &str, source: Source) -> Container {
    signal_module_with(name, &ModuleSettings::from_source(source))
}

/// Build one module from its settings — the version that carries sound.
pub fn signal_module_with(name: &str, set: &ModuleSettings) -> Container {
    with_module_envelopes(module_shell(name, set), set)
}

/// The module's structure, before its envelope routes.
fn module_shell(name: &str, set: &ModuleSettings) -> Container {
    let src = match &set.source {
        // The sampler: a Keyscape piano, an Omnisphere soundsource, a kit.
        Source::Sample(spec) => {
            let mut block = RigBlock::sample_lib(spec.clone()).named("Soundsource");
            // The sampler's own per-voice amplitude envelope: the attack and
            // release a note actually gets.
            block = block
                .with_param("amp_attack", format!("{:.4}", set.amp_env.0.max(0.0) / 1000.0))
                .with_param("amp_release", format!("{:.4}", set.amp_env.3.max(0.0) / 1000.0));
            if set.unison > 1 {
                block = block
                    .with_param("unison", set.unison.to_string())
                    .with_param("detune", format!("{:.3}", set.detune));
            }
            Container::module("Source").add(block)
        }
        // Synthesis: the native oscillator is the generator.
        Source::Synth => Container::module("Source").block(BlockType::Oscillator, "Soundsource"),
        // Nothing loaded — a placeholder that passes audio (silent lane).
        Source::Empty => Container::module("Source").block(BlockType::Sampler, "Soundsource"),
    };
    // Cutoff and resonance are normalized on the block (the filter's own
    // scale); Hz is what a player reads.
    let cutoff_norm = signal_sampler::native::NativeFilter::norm_from_cutoff(set.cutoff_hz);
    let filters = Container::module("Filters")
        .add(
            RigBlock::of_type(BlockType::Filter)
                .named("Filter 1")
                .with_param("cutoff", format!("{cutoff_norm:.4}"))
                .with_param("resonance", format!("{:.4}", set.resonance.clamp(0.0, 1.0))),
        )
        .block(BlockType::Filter, "Filter 2");

    Container::module(name)
        .param("module_level", "0")
        .param("filter_routing", "Series")
        .add(src)
        .add(filters)
        // The Amp's gain param is normalized: unity at 0.5. A synth module
        // starts at zero and is opened by its envelope; a sampler's Amp is
        // just a gain stage at unity, because its voices carry their own
        // envelopes.
        .add(Container::module("Amp").add(
            RigBlock::of_type(BlockType::Amp).named("Amp").with_param(
                "gain",
                if matches!(set.source, Source::Sample(_)) { "0.5" } else { "0.0" },
            ),
        ))
        .add(fx_rack("FX"))
        // Each module routes to the Part's aux rack independently (rigs
        // without an Aux Rack container simply drop the send).
        .send(AUX_RACK, "To Aux")
        .modulator_block(envelope("Amp Env", set.amp_env))
        .modulator_block(envelope("Filter Env", set.filter_env))
        .modulator(BlockType::MultisegEnvelope, "Mod Env")
}

/// Attach the module's envelope routes — but only where a module-level
/// envelope is the right thing.
///
/// The mod engine's envelopes are **per module**, not per voice. On a
/// synthesised source that is exactly right: one oscillator, one envelope.
/// On a SAMPLER it is wrong and audible — a polyphonic instrument would be
/// gated by a single envelope, so the last note-off closes the module over
/// everything still sounding (a held pad dies when you stop playing, and the
/// release of one note takes the others with it). A sampler's amplitude
/// envelope belongs to its voices, and it has one: `amp_attack` /
/// `amp_release` on the Source block above.
fn with_module_envelopes(module: Container, set: &ModuleSettings) -> Container {
    if matches!(set.source, Source::Sample(_)) {
        return module;
    }
    module
        .route("Amp Env", "Amp.gain", 0.5)
        .route("Filter Env", "Filter 1.cutoff", set.filter_env_depth.clamp(-1.0, 1.0))
}

/// Build one **layer**: four modules in parallel (they sum — a layer is a
/// stack of voices, not a chain), plus the layer's aux send.
///
/// `sources[i]` realizes module `MODULE_SLOTS[i]`; `Source::Empty` leaves a
/// structured but silent module, so the shape is always the full quad.
pub fn signal_layer(name: &str, sources: &[Source]) -> Container {
    let settings: Vec<ModuleSettings> =
        sources.iter().cloned().map(ModuleSettings::from_source).collect();
    signal_layer_with(name, &settings)
}

/// A layer whose modules carry their settings — the version the rig builds
/// when its macros have been moved.
pub fn signal_layer_with(name: &str, settings: &[ModuleSettings]) -> Container {
    let mut modules = Container::parallel(format!("{name} Modules"));
    for (i, set) in settings.iter().enumerate() {
        modules = modules.add(signal_module_with(&format!("{name} {}", module_slot(i)), set));
    }
    Container::layer(name).add(modules)
}

/// A layer holding a single module — the common "one patch in this lane"
/// case. More modules are added by handing `signal_layer` more sources.
pub fn signal_layer_single(name: &str, source: Source) -> Container {
    signal_layer(name, &[source])
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

/// One module's worth of imported settings — an Omnisphere layer flattened
/// onto the Signal Engine's macro surface.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedModule {
    /// Soundsource name as the patch names it (resolve against the library).
    pub source: String,
    /// Module level in dB.
    pub level_db: f32,
    /// Filter cutoff (Hz), resonance 0..1, envelope depth −1..1.
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub filter_env_depth: f32,
    /// Amp / filter envelopes as `(attack_ms, decay_ms, sustain, release_ms)`.
    pub amp_env: Option<(f32, f32, f32, f32)>,
    pub filter_env: Option<(f32, f32, f32, f32)>,
    /// Unison voices + detune.
    pub unison: u32,
    pub detune: f32,
    /// The layer's FX rack slot names ("No Effect" filtered out).
    pub fx: Vec<String>,
}

/// A patch flattened into modules — what "open this preset into a layer"
/// produces.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedPatch {
    pub name: String,
    pub modules: Vec<ImportedModule>,
    /// The patch's LFOs as `(rate_hz, depth, shape)` — Omnisphere's LFOs are
    /// per-part, so every module of the patch shares them. `shape` indexes
    /// sine / triangle / square / saw / random, matching the LFO panel.
    pub lfos: Vec<(f32, f32, f32)>,
}

/// Omnisphere's normalized LFO rate → Hz. Approximate (an exponential over
/// the free-run range) until it's swept against the real engine like the
/// filter knee was.
fn omni_lfo_hz(v: f32) -> f32 {
    0.05 * 2f32.powf(9.6 * v.clamp(0.0, 1.0))
}

/// Read an Omnisphere `.prt_omn` patch and flatten its layers onto module
/// settings. Each Omnisphere layer becomes one module — the same mapping the
/// engine already uses structurally, now carrying the patch's values.
pub fn import_omni_patch(path: &std::path::Path) -> Result<ImportedPatch, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = crate::omni_import::parse_patch(&xml)?;
    // Omnisphere times are seconds; the macro surface is milliseconds.
    let secs = |t: (f32, f32, f32, f32)| (t.0 * 1000.0, t.1 * 1000.0, t.2, t.3 * 1000.0);
    // A patch declares up to four layers but only uses the ones with a
    // soundsource — an empty slot is not a module, it is nothing.
    let modules = patch
        .layers
        .iter()
        .filter(|l| !l.soundsource.trim().is_empty())
        .map(|l| ImportedModule {
            source: l.soundsource.clone(),
            // `level` is normalized; unity sits at 1.0.
            level_db: if l.level > 0.0 { 20.0 * l.level.log10() } else { -60.0 },
            cutoff_hz: crate::omni_import::omni_cutoff_hz(l.filter_freq),
            resonance: l.filter_res,
            filter_env_depth: l.filter_env_depth,
            amp_env: l.amp_env.map(secs),
            filter_env: l.filter_env.map(secs),
            unison: l.unison_count.max(1),
            detune: l.unison_detune,
            fx: l
                .fx
                .iter()
                .filter(|f| !f.is_empty() && f.as_str() != "No Effect")
                .cloned()
                .collect(),
        })
        .collect();
    // The mod matrix carries the LFO depths: an LFO with no route is idle,
    // however its rate reads.
    let lfos = patch
        .lfos
        .iter()
        .enumerate()
        .take(4)
        .map(|(i, (rate, kind, _synced, _retrig))| {
            let tag = format!("LFO{}", i + 1);
            let depth = patch
                .mod_routes
                .iter()
                .filter(|r| r.source.starts_with(&tag))
                .map(|r| r.depth.abs())
                .fold(0.0f32, f32::max);
            (omni_lfo_hz(*rate), depth.clamp(0.0, 1.0), (kind * 4.0).clamp(0.0, 4.0))
        })
        .collect();
    Ok(ImportedPatch { name: patch.name.clone(), modules, lfos })
}

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
    fn a_layer_holds_exactly_its_sources() {
        // One source, one module — no empty slots padding it out to a quad.
        let l = signal_layer_single("Keys A", Source::Sample("/tmp/a.signalpack".into()));
        assert_eq!(l.role, Role::Layer);
        assert!(l.find("Keys A A").is_some(), "module A");
        assert!(l.find("Keys A B").is_none(), "no padded module B");
        // Not limited to four either — a layer takes as many as it is given.
        let big: Vec<Source> = (0..6).map(|_| Source::Empty).collect();
        let l = signal_layer("Wide", &big);
        assert!(l.find("Wide F").is_some(), "sixth module");
    }
}
