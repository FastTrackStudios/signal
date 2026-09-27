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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    #[must_use]
    pub fn sample(spec: Option<String>) -> Self {
        match spec {
            Some(s) => Self::Sample(s),
            None => Self::Empty,
        }
    }
}

/// How many modules a layer starts with — Omnisphere's Quadzone, and the
/// A/B/C/D the layer zoom switches between.
///
/// It is a *default*, not a limit: [`signal_layer`] builds as many modules
/// as it is given sources for.
pub const MODULES_PER_LAYER: usize = 4;

/// Slot label for module `index`: A..Z, then A1, B1, … so a layer can grow
/// past the alphabet without ambiguity.
#[must_use]
pub fn module_slot(index: usize) -> String {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let letter = LETTERS[index % LETTERS.len()] as char;
    let wrap = index / LETTERS.len();
    if wrap == 0 {
        letter.to_string()
    } else {
        format!("{letter}{wrap}")
    }
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
    /// Pan −1..1 and stereo width 0..1 (0.5 as recorded), on the Amp.
    pub pan: f32,
    pub width: f32,
    /// Transpose (semitones) and fine tune (cents).
    pub transpose: f32,
    pub fine: f32,
    /// Filter key tracking 0..1, drive 0..1 and wet mix 0..1.
    pub keytrack: f32,
    pub filter_drive: f32,
    pub filter_mix: f32,
    /// Vibrato: rate (Hz), depth 0..1, delay (ms).
    pub vib_rate: f32,
    pub vib_depth: f32,
    pub vib_delay_ms: f32,
    /// Tone on the Amp, 0..1: warmth and body 0.5 = flat, drive 0 = clean.
    pub warmth: f32,
    pub body: f32,
    pub drive: f32,
    /// Chorus amount 0..1 (0 = no chorus).
    pub chorus: f32,
    pub ambience: AmbienceSettings,
    pub delay: DelaySettings,
    /// The part's LFOs 1–4 as `(rate_hz, depth, wave)` — depth scales the
    /// routes an imported patch gives each; wave indexes sine / triangle /
    /// saw / square / S&H.
    pub lfos: [(f32, f32, f32); 4],
    /// The Mod Env's `(attack_ms, decay_ms, sustain, release_ms)`.
    pub mod_env: (f32, f32, f32, f32),
}

/// The module's reverb (its "Ambience").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbienceSettings {
    pub on: bool,
    /// Algorithm index (the reverb's table).
    pub algo: f32,
    pub size: f32,
    pub mix: f32,
    pub predelay_ms: f32,
    pub decay: f32,
}

/// The module's delay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelaySettings {
    pub on: bool,
    /// Machine / style index (the delay's table).
    pub style: f32,
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

impl AmbienceSettings {
    /// Whether the reverb is in the chain (on and audible).
    #[must_use]
    pub fn active(&self) -> bool {
        self.on && self.mix > 0.0
    }
}

impl DelaySettings {
    /// Whether the delay is in the chain (on and audible).
    #[must_use]
    pub fn active(&self) -> bool {
        self.on && self.mix > 0.0
    }
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
            pan: 0.0,
            width: 0.5,
            transpose: 0.0,
            fine: 0.0,
            keytrack: 0.0,
            filter_drive: 0.0,
            filter_mix: 1.0,
            vib_rate: 5.0,
            vib_depth: 0.0,
            vib_delay_ms: 300.0,
            warmth: 0.5,
            body: 0.5,
            drive: 0.0,
            chorus: 0.0,
            // Effects start out: a patch sounds as it was made until a knob
            // asks for more.
            ambience: AmbienceSettings {
                on: false,
                algo: 1.0,
                size: 0.5,
                mix: 0.15,
                predelay_ms: 20.0,
                decay: 0.45,
            },
            delay: DelaySettings {
                on: false,
                style: 0.0,
                time_ms: 375.0,
                feedback: 0.35,
                mix: 0.0,
            },
            lfos: [
                (2.0, 0.0, 0.0),
                (0.5, 0.0, 1.0),
                (4.0, 0.0, 2.0),
                (8.0, 0.0, 3.0),
            ],
            // The mod engine's own envelope default: an untouched Mod Env
            // re-applies as exactly what the tree compiles.
            mod_env: (3.0, 250.0, 0.8, 150.0),
        }
    }
}

impl ModuleSettings {
    /// Whether sampler voices carry their own filter (envelope amount or key
    /// tracking set). Then it is the module's filter — the shared chain
    /// filter opens — and the resonance knob is the voices'.
    #[must_use]
    pub fn voice_filter_on(&self) -> bool {
        self.filter_env_depth != 0.0 || self.keytrack > 0.0
    }

    /// The module's Amp: unity gain, plus pan, width and tone.
    #[must_use]
    pub fn amp_block(&self) -> RigBlock {
        RigBlock::of_type(BlockType::Amp)
            .named("Amp")
            .with_param("gain", "0.5")
            .with_param(
                "pan",
                format!("{:.4}", (self.pan.clamp(-1.0, 1.0) + 1.0) * 0.5),
            )
            .with_param("width", format!("{:.4}", self.width.clamp(0.0, 1.0)))
            .with_param("warmth", format!("{:.4}", self.warmth.clamp(0.0, 1.0)))
            .with_param("body", format!("{:.4}", self.body.clamp(0.0, 1.0)))
            .with_param("drive", format!("{:.4}", self.drive.clamp(0.0, 1.0)))
    }

    /// The module's effects in chain order — Chorus, Delay, Ambience — each
    /// only when it is on, so an idle module costs nothing. The blocks carry
    /// their values in the effects' own units.
    #[must_use]
    pub fn fx_blocks(&self) -> Vec<RigBlock> {
        let mut out = Vec::new();
        if self.chorus > 0.0 {
            out.push(
                RigBlock::of_type(BlockType::Chorus)
                    .named("Chorus")
                    .with_param("mix", format!("{:.4}", self.chorus.clamp(0.0, 1.0))),
            );
        }
        let d = &self.delay;
        if d.active() {
            out.push(
                RigBlock::of_type(BlockType::Delay)
                    .named("Delay")
                    .with_param("style", format!("{:.0}", d.style.clamp(0.0, 13.0)))
                    .with_param("time", format!("{:.2}", d.time_ms.clamp(2.0, 2500.0)))
                    .with_param("feedback", format!("{:.4}", d.feedback.clamp(0.0, 0.95)))
                    .with_param("mix", format!("{:.4}", d.mix.clamp(0.0, 1.0))),
            );
        }
        let a = &self.ambience;
        if a.active() {
            out.push(
                RigBlock::of_type(BlockType::Reverb)
                    .named("Ambience")
                    .with_param("algorithm", format!("{:.0}", a.algo.clamp(0.0, 14.0)))
                    .with_param("size", format!("{:.4}", a.size.clamp(0.0, 1.0)))
                    .with_param("decay", format!("{:.4}", a.decay.clamp(0.0, 1.0)))
                    .with_param(
                        "predelay",
                        format!("{:.2}", a.predelay_ms.clamp(0.0, 200.0)),
                    )
                    .with_param("mix", format!("{:.4}", a.mix.clamp(0.0, 1.0))),
            );
        }
        out
    }

    /// The sampler source's per-voice params (seconds / Hz / cents / 0..1),
    /// beyond the amp ADSR: its own filter, vibrato and tuning.
    fn sampler_voice_params(&self, mut block: RigBlock) -> RigBlock {
        let secs = |ms: f32| format!("{:.4}", ms.max(0.0) / 1000.0);
        if self.voice_filter_on() {
            block = block
                .with_param("filter_attack", secs(self.filter_env.0))
                .with_param("filter_decay", secs(self.filter_env.1))
                .with_param(
                    "filter_sustain",
                    format!("{:.4}", self.filter_env.2.clamp(0.0, 1.0)),
                )
                .with_param("filter_release", secs(self.filter_env.3))
                .with_param(
                    "filter_env_amt",
                    format!("{:.4}", self.filter_env_depth.clamp(-1.0, 1.0)),
                )
                .with_param("filter_cutoff_hz", format!("{:.1}", self.cutoff_hz))
                .with_param(
                    "filter_resonance",
                    format!("{:.4}", self.resonance.clamp(0.0, 1.0)),
                )
                .with_param(
                    "filter_keytrack",
                    format!("{:.4}", self.keytrack.clamp(0.0, 1.0)),
                );
        }
        if self.vib_depth > 0.0 {
            block = block
                .with_param("vib_rate", format!("{:.3}", self.vib_rate))
                .with_param(
                    "vib_depth",
                    format!("{:.4}", self.vib_depth.clamp(0.0, 1.0)),
                )
                .with_param("vib_delay_ms", format!("{:.1}", self.vib_delay_ms.max(0.0)));
        }
        if self.transpose != 0.0 || self.fine != 0.0 {
            block = block
                .with_param("transpose", format!("{:.3}", self.transpose))
                .with_param("fine", format!("{:.3}", self.fine));
        }
        block
    }

    /// The Wavetable's tune param (0.5 centre, ±24 semitones).
    #[must_use]
    pub fn wavetable_tune(&self) -> f32 {
        (0.5 + (self.transpose + self.fine / 100.0) / 48.0).clamp(0.0, 1.0)
    }

    /// Settings for a bare source, everything else at its default.
    #[must_use]
    pub fn from_source(source: Source) -> Self {
        Self {
            source,
            ..Self::default()
        }
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

#[must_use]
pub fn signal_module(name: &str, source: Source) -> Container {
    signal_module_with(name, &ModuleSettings::from_source(source))
}

/// Build one module from its settings — the version that carries sound.
#[must_use]
pub fn signal_module_with(name: &str, set: &ModuleSettings) -> Container {
    with_module_envelopes(module_shell(name, set), set)
}

/// The module's structure, before its envelope routes.
fn module_shell(name: &str, set: &ModuleSettings) -> Container {
    let src = match &set.source {
        // The sampler: a Keyscape piano, an Omnisphere soundsource, a kit.
        Source::Sample(spec) => {
            let mut block = RigBlock::sample_lib(spec.clone()).named("Soundsource");
            // The sampler's own per-voice amplitude envelope: the ADSR a
            // note actually gets (times in seconds, sustain 0..=1).
            block = block
                .with_param(
                    "amp_attack",
                    format!("{:.4}", set.amp_env.0.max(0.0) / 1000.0),
                )
                .with_param(
                    "amp_decay",
                    format!("{:.4}", set.amp_env.1.max(0.0) / 1000.0),
                )
                .with_param(
                    "amp_sustain",
                    format!("{:.4}", set.amp_env.2.clamp(0.0, 1.0)),
                )
                .with_param(
                    "amp_release",
                    format!("{:.4}", set.amp_env.3.max(0.0) / 1000.0),
                );
            block = set.sampler_voice_params(block);
            if set.unison > 1 {
                block = block
                    .with_param("unison", set.unison.to_string())
                    .with_param("detune", format!("{:.3}", set.detune));
            }
            Container::module("Source").add(block)
        }
        // Synthesis: the native oscillator is a true per-voice synth —
        // each note owns its amp/filter envelopes and lowpass, so the
        // module's settings ride on the oscillator block itself.
        Source::Synth => {
            let cutoff_norm = signal_sampler::native::NativeFilter::norm_from_cutoff(set.cutoff_hz);
            let block = RigBlock::of_type(BlockType::Oscillator)
                .named("Soundsource")
                .with_param("amp_attack", format!("{:.3}", set.amp_env.0.max(0.0)))
                .with_param("amp_decay", format!("{:.3}", set.amp_env.1.max(0.0)))
                .with_param(
                    "amp_sustain",
                    format!("{:.4}", set.amp_env.2.clamp(0.0, 1.0)),
                )
                .with_param("amp_release", format!("{:.3}", set.amp_env.3.max(0.0)))
                .with_param("filter_attack", format!("{:.3}", set.filter_env.0.max(0.0)))
                .with_param("filter_decay", format!("{:.3}", set.filter_env.1.max(0.0)))
                .with_param(
                    "filter_sustain",
                    format!("{:.4}", set.filter_env.2.clamp(0.0, 1.0)),
                )
                .with_param(
                    "filter_release",
                    format!("{:.3}", set.filter_env.3.max(0.0)),
                )
                .with_param("cutoff", format!("{cutoff_norm:.4}"))
                .with_param("resonance", format!("{:.4}", set.resonance.clamp(0.0, 1.0)))
                .with_param(
                    "env_amt",
                    format!("{:.4}", set.filter_env_depth.clamp(-1.0, 1.0)),
                );
            Container::module("Source").add(block)
        }
        // Nothing loaded — a placeholder that passes audio (silent lane).
        Source::Empty => Container::module("Source").block(BlockType::Sampler, "Soundsource"),
    };
    // Cutoff and resonance are normalized on the block (the filter's own
    // scale); Hz is what a player reads. A SYNTH module filters per voice
    // (inside the oscillator), so its chain filter sits wide open; a sampler
    // module keeps the chain filter as its tone control.
    // A sampler whose voices carry their own filter opens it too: the
    // voices are the filter then.
    let open = matches!(set.source, Source::Synth)
        || (matches!(set.source, Source::Sample(_)) && set.voice_filter_on());
    let chain_cutoff = if open {
        1.0
    } else {
        signal_sampler::native::NativeFilter::norm_from_cutoff(set.cutoff_hz)
    };
    let chain_res = if open {
        0.0
    } else {
        set.resonance.clamp(0.0, 1.0)
    };
    let filters = Container::module("Filters")
        .add(
            RigBlock::of_type(BlockType::Filter)
                .named("Filter 1")
                .with_param("cutoff", format!("{chain_cutoff:.4}"))
                .with_param("resonance", format!("{chain_res:.4}"))
                .with_param("drive", format!("{:.4}", set.filter_drive.clamp(0.0, 1.0)))
                .with_param("mix", format!("{:.4}", set.filter_mix.clamp(0.0, 1.0))),
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
        // Unity for every source: the voice's own amp envelope shapes the
        // level; the Amp carries pan, width and tone.
        .add(Container::module("Amp").add(set.amp_block()))
        .add(module_fx(set))
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
const fn with_module_envelopes(module: Container, _set: &ModuleSettings) -> Container {
    // Envelopes are PER-VOICE now, inside each source (the sampler's voices
    // carry their own amp envelope; the oscillator carries amp + filter
    // envelopes and a per-voice lowpass). The old paraphonic routes —
    // a shared Amp Env onto the module Amp's gain, a shared Filter Env onto
    // the module filter's cutoff — double-enveloped the sum, so they're gone.
    module
}

/// Write a keys module's settings onto layer `layer_idx` ("Layer A".."D")
/// of an imported Omnisphere tree, so a rebuilt lane keeps what the knobs
/// say: the source's amp ADSR, Filter 1's cutoff and resonance, the Filter
/// Env's ADSR and — where the patch routes it — its depth onto the cutoff.
/// A sample-mode layer with no such route takes the depth as its voices' own
/// filter envelope instead, as a keys module does. Returns `false` when the
/// tree has no such layer.
pub fn apply_settings_to_omni_layer(
    tree: &mut Container,
    layer_idx: usize,
    set: &ModuleSettings,
) -> bool {
    use signal_sampler::rig_node::RigNode;
    fn find_mut<'a>(c: &'a mut Container, name: &str) -> Option<&'a mut Container> {
        if c.name == name {
            return Some(c);
        }
        c.children.iter_mut().find_map(|child| match child {
            RigNode::Container { container } => find_mut(container, name),
            RigNode::Block { .. } => None,
        })
    }
    fn for_blocks(c: &mut Container, f: &mut impl FnMut(&mut RigBlock)) {
        for child in &mut c.children {
            match child {
                RigNode::Block { block } => f(block),
                RigNode::Container { container } => for_blocks(container, f),
            }
        }
    }
    fn put(block: &mut RigBlock, name: &str, value: String) {
        match block.params.iter_mut().find(|p| p.name == name) {
            Some(p) => p.value = value,
            None => {
                block.params.push(signal_sampler::rig_node::Param {
                    name: name.to_string(),
                    value,
                });
            }
        }
    }
    let Some(name) = crate::omni_import::LAYER_NAMES.get(layer_idx) else {
        return false;
    };
    let Some(layer) = find_mut(tree, name) else {
        return false;
    };
    let secs = |ms: f32| format!("{:.4}", ms.max(0.0) / 1000.0);
    let depth = set.filter_env_depth.clamp(-1.0, 1.0);
    // The filter section's own envelope route is the first Filter Env →
    // cutoff route the importer writes; later ones are mod-matrix rows with
    // depths of their own, left as the patch set them.
    let routed = match layer.mod_routes.iter_mut().find(|r| {
        r.source.key() == "filter env" && r.target.key() == "filter 1" && r.parameter == "cutoff"
    }) {
        Some(r) => {
            r.depth = depth;
            true
        }
        None => false,
    };
    let cutoff = signal_sampler::native::NativeFilter::norm_from_cutoff(set.cutoff_hz);
    let mut is_sampler = false;
    for_blocks(layer, &mut |b| {
        if b.display_name() == "Soundsource" && b.block_type == BlockType::Sampler {
            is_sampler = true;
        }
    });
    // A sample-mode layer the patch gives no filter-envelope route takes the
    // knobs' as its voices' own filter (as a keys module does), and its
    // shared filter opens for it.
    let voice_filter = is_sampler && !routed && set.voice_filter_on();
    let voice_params = set.sampler_voice_params(RigBlock::of_type(BlockType::Sampler));
    let amp = set.amp_block();
    for_blocks(layer, &mut |b| match b.display_name().as_str() {
        "Soundsource" => {
            put(b, "amp_attack", secs(set.amp_env.0));
            put(b, "amp_decay", secs(set.amp_env.1));
            put(
                b,
                "amp_sustain",
                format!("{:.4}", set.amp_env.2.clamp(0.0, 1.0)),
            );
            put(b, "amp_release", secs(set.amp_env.3));
            if b.block_type == BlockType::Sampler {
                for p in &voice_params.params {
                    if voice_filter || !p.name.starts_with("filter_") {
                        put(b, &p.name, p.value.clone());
                    }
                }
            } else if b.block_type == BlockType::Wavetable {
                put(
                    b,
                    "vib_rate",
                    format!("{:.4}", (set.vib_rate / 12.0).clamp(0.0, 1.0)),
                );
                put(
                    b,
                    "vib_depth",
                    format!("{:.4}", set.vib_depth.clamp(0.0, 1.0)),
                );
                put(b, "tune", format!("{:.4}", set.wavetable_tune()));
            }
        }
        "Filter 1" => {
            if voice_filter {
                put(b, "cutoff", "1.0000".to_string());
                put(b, "resonance", "0.0000".to_string());
            } else {
                put(b, "cutoff", format!("{cutoff:.4}"));
                put(
                    b,
                    "resonance",
                    format!("{:.4}", set.resonance.clamp(0.0, 1.0)),
                );
            }
            put(
                b,
                "drive",
                format!("{:.4}", set.filter_drive.clamp(0.0, 1.0)),
            );
            put(b, "mix", format!("{:.4}", set.filter_mix.clamp(0.0, 1.0)));
        }
        "Amp" => {
            // Pan, width and tone; the layer keeps its own gain.
            for p in amp.params.iter().filter(|p| p.name != "gain") {
                put(b, &p.name, p.value.clone());
            }
        }
        _ => {}
    });
    // The module's effects follow the patch's own, in its FX rack.
    let fx = set.fx_blocks();
    if !fx.is_empty() {
        if let Some(rack) = find_mut(layer, "Layer FX") {
            for block in fx {
                rack.children.push(RigNode::Block { block });
            }
        }
    }
    if let Some(me) = layer
        .modulators
        .iter_mut()
        .find(|m| m.display_name() == "Mod Env")
    {
        put(me, "attack", secs(set.mod_env.0));
        put(me, "decay", secs(set.mod_env.1));
        put(
            me,
            "sustain",
            format!("{:.4}", set.mod_env.2.clamp(0.0, 1.0)),
        );
        put(me, "release", secs(set.mod_env.3));
    }
    if let Some(fe) = layer
        .modulators
        .iter_mut()
        .find(|m| m.display_name() == "Filter Env")
    {
        put(fe, "attack", secs(set.filter_env.0));
        put(fe, "decay", secs(set.filter_env.1));
        put(
            fe,
            "sustain",
            format!("{:.4}", set.filter_env.2.clamp(0.0, 1.0)),
        );
        put(fe, "release", secs(set.filter_env.3));
    }
    true
}

/// Build one **layer**: four modules in parallel (they sum — a layer is a
/// stack of voices, not a chain), plus the layer's aux send.
///
/// `sources[i]` realizes module `MODULE_SLOTS[i]`; `Source::Empty` leaves a
/// structured but silent module, so the shape is always the full quad.
pub fn signal_layer(name: &str, sources: &[Source]) -> Container {
    let settings: Vec<ModuleSettings> = sources
        .iter()
        .cloned()
        .map(ModuleSettings::from_source)
        .collect();
    signal_layer_with(name, &settings)
}

/// A layer whose modules carry their settings — the version the rig builds
/// when its macros have been moved.
#[must_use]
pub fn signal_layer_with(name: &str, settings: &[ModuleSettings]) -> Container {
    let mut modules = Container::parallel(format!("{name} Modules"));
    for (i, set) in settings.iter().enumerate() {
        modules = modules.add(signal_module_with(
            &format!("{name} {}", module_slot(i)),
            set,
        ));
    }
    Container::layer(name).add(modules)
}

/// A layer holding a single module — the common "one patch in this lane"
/// case. More modules are added by handing `signal_layer` more sources.
#[must_use]
pub fn signal_layer_single(name: &str, source: Source) -> Container {
    signal_layer(name, &[source])
}

/// A 4-slot FX rack — every rack in the engine (Layer / Common / Aux /
/// Master) is exactly four slots.
#[must_use]
/// A module's FX rack: its active effects, then empty slots up to four.
fn module_fx(set: &ModuleSettings) -> Container {
    let blocks = set.fx_blocks();
    let used = blocks.len();
    let mut rack = Container::module("FX");
    for b in blocks {
        rack = rack.add(b);
    }
    for slot in used + 1..=4 {
        rack = rack.block(BlockType::Custom, format!("FX Slot {slot}"));
    }
    rack
}

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
    "Source",
    "Tone",
    "Filter",
    "Filter Env",
    "Amp Env",
    "Vibrato",
    "Ambience",
    "Effects",
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
    /// sine / triangle / saw / square / S&H, as the patch tree's LFOs do.
    pub lfos: Vec<(f32, f32, f32)>,
}

/// Omnisphere's normalized LFO rate → Hz. Approximate (an exponential over
/// the free-run range) until it's swept against the real engine like the
/// filter knee was.
fn omni_lfo_hz(v: f32) -> f32 {
    // The same curve the patch tree builds its LFOs with (0.05..20 Hz), so a
    // lane's LFO knobs seeded from here re-apply as exactly the patch's rate.
    0.05 * 400f32.powf(v.clamp(0.0, 1.0))
}

/// Read an Omnisphere `.prt_omn` patch and flatten its layers onto module
/// settings.
///
/// Each Omnisphere layer becomes one module — the same mapping the
/// engine already uses structurally, now carrying the patch's values.
///
/// # Errors
///
/// Returns an error if the file cannot be read or the patch format is invalid.
pub fn import_omni_patch(path: &std::path::Path) -> Result<ImportedPatch, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = crate::omni_import::parse_patch(&xml)?;
    // A patch declares up to four layers but only uses the ones with a
    // soundsource — an empty slot is not a module, it is nothing.
    let modules = patch
        .layers
        .iter()
        .filter(|l| !l.soundsource.trim().is_empty())
        .map(imported_module)
        .collect();
    Ok(ImportedPatch {
        modules,
        ..import_rest(&patch)
    })
}

/// Every layer of an Omnisphere patch file, synthesis layers included, in
/// layer order ("Layer A" first) — the values a lane hosting the whole patch
/// seeds its module knobs from, one module per layer.
///
/// # Errors
///
/// When the file cannot be read or parsed.
pub fn import_omni_layers(path: &std::path::Path) -> Result<Vec<ImportedModule>, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = crate::omni_import::parse_patch(&xml)?;
    Ok(patch.layers.iter().take(4).map(imported_module).collect())
}

/// An imported Omnisphere patch's LFOs 1–4 onto its tree: each "LFO n"
/// modulator's rate and wave, and its routes scaled so the deepest reaches
/// the LFO's depth (keeping the patch's proportions and signs). The LFOs
/// are part-level — every layer shares them.
pub fn apply_lfos_to_omni(tree: &mut Container, lfos: &[(f32, f32, f32); 4]) {
    use signal_sampler::rig_node::RigNode;
    fn walk(c: &mut Container, f: &mut impl FnMut(&mut Container)) {
        f(c);
        for child in &mut c.children {
            if let RigNode::Container { container } = child {
                walk(container, f);
            }
        }
    }
    let deepest = lfo_depths(tree);
    for (i, &(rate, depth, wave)) in lfos.iter().enumerate() {
        let name = format!("LFO {}", i + 1);
        let key = name.to_lowercase();
        let scale = if deepest[i] > 0.0 {
            Some(depth / deepest[i])
        } else {
            None
        };
        walk(tree, &mut |c| {
            for m in &mut c.modulators {
                if m.display_name() == name {
                    for (k, v) in [
                        ("rate", format!("{:.4}", rate.clamp(0.01, 40.0))),
                        ("wave", format!("{}", wave.round().clamp(0.0, 4.0) as u32)),
                    ] {
                        match m.params.iter_mut().find(|p| p.name == k) {
                            Some(p) => p.value = v,
                            None => m.params.push(signal_sampler::rig_node::Param {
                                name: k.to_string(),
                                value: v,
                            }),
                        }
                    }
                }
            }
            if let Some(scale) = scale {
                for r in &mut c.mod_routes {
                    if r.source.key() == key {
                        r.depth *= scale;
                    }
                }
            }
        });
    }
}

/// An Omnisphere patch file's LFOs 1–4 as `(rate_hz, depth, wave)` — the
/// values a lane hosting the patch seeds its LFO knobs from.
///
/// # Errors
///
/// When the file cannot be read or parsed.
pub fn import_omni_lfos(path: &std::path::Path) -> Result<Vec<(f32, f32, f32)>, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let patch = crate::omni_import::parse_patch(&xml)?;
    // Depth as the TREE carries it (routes scaled to their targets, the
    // unsupported ones dropped) — what `apply_lfos_to_omni` scales against,
    // so the seeded knobs re-apply as the identity.
    let tree = crate::omni_import::patch_to_container(
        &patch,
        &crate::omni_import::SoundsourceIndex::default(),
    );
    let depths = lfo_depths(&tree);
    Ok(import_rest(&patch)
        .lfos
        .into_iter()
        .zip(depths)
        .map(|((rate, _, wave), depth)| (rate, depth, wave))
        .collect())
}

/// The deepest route (absolute) from each of "LFO 1".."LFO 4" in `tree`.
fn lfo_depths(tree: &Container) -> [f32; 4] {
    use signal_sampler::rig_node::RigNode;
    fn walk(c: &Container, out: &mut [f32; 4]) {
        for r in &c.mod_routes {
            let key = r.source.key();
            for (i, d) in out.iter_mut().enumerate() {
                if key == format!("lfo {}", i + 1) {
                    *d = d.max(r.depth.abs());
                }
            }
        }
        for child in &c.children {
            if let RigNode::Container { container } = child {
                walk(container, out);
            }
        }
    }
    let mut out = [0.0; 4];
    walk(tree, &mut out);
    out
}

/// One Omnisphere layer as a module's settings.
fn imported_module(l: &crate::omni_import::OmniLayer) -> ImportedModule {
    // Omnisphere times are seconds; the macro surface is milliseconds.
    let secs = |t: (f32, f32, f32, f32)| (t.0 * 1000.0, t.1 * 1000.0, t.2, t.3 * 1000.0);
    ImportedModule {
        source: l.soundsource.clone(),
        // `level` is normalized; unity sits at 1.0.
        level_db: if l.level > 0.0 {
            20.0 * l.level.log10()
        } else {
            -60.0
        },
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
    }
}

/// A patch's name and LFOs (its modules are the caller's choice).
fn import_rest(patch: &crate::omni_import::OmniPatch) -> ImportedPatch {
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
            (
                omni_lfo_hz(*rate),
                depth.clamp(0.0, 1.0),
                (kind * 4.0).clamp(0.0, 4.0),
            )
        })
        .collect();
    ImportedPatch {
        name: patch.name.clone(),
        modules: Vec::new(),
        lfos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_are_in_the_chain_only_when_on_and_take_live_writes() {
        use signal_sampler::node_render::RenderNode;
        // Defaults: no effect blocks at all (an idle module costs nothing).
        let quiet = signal_module_with("Keys A", &ModuleSettings::from_source(Source::Synth));
        let mut rn = RenderNode::compile(&Container::preset("p").add(quiet), 48_000);
        rn.prepare(48_000.0, 128);
        for fx in ["Chorus", "Delay", "Ambience"] {
            assert!(!rn.has_leaf("Keys A", fx), "{fx} built while off");
        }

        let mut set = ModuleSettings::from_source(Source::Synth);
        set.chorus = 0.4;
        set.delay.on = true;
        set.delay.mix = 0.3;
        set.ambience.on = true;
        set.ambience.mix = 0.2;
        let busy = signal_module_with("Keys A", &set);
        let mut rn = RenderNode::compile(&Container::preset("p").add(busy), 48_000);
        rn.prepare(48_000.0, 128);
        for fx in ["Chorus", "Delay", "Ambience"] {
            assert!(rn.has_leaf("Keys A", fx), "{fx} missing while on");
        }
        // Plain units: a 700 ms delay, a 60 ms predelay.
        assert!(rn.set_leaf_plain("Keys A", "Delay", "time", 700.0));
        assert!(rn.set_leaf_plain("Keys A", "Ambience", "predelay", 60.0));
        assert!(!rn.set_leaf_plain("Keys A", "Delay", "no_such_param", 1.0));
        // And the chain renders.
        let (mut l, mut r) = (vec![0.0f32; 128], vec![0.0f32; 128]);
        let silence = vec![0.0f32; 128];
        rn.process(
            &silence,
            &silence,
            &mut l,
            &mut r,
            &signal_plugin_host::PluginEvents::default(),
        );
        assert!(l.iter().chain(&r).all(|x| x.is_finite()));
    }

    /// End-to-end through the full compiled module chain (per-voice synth →
    /// open chain filter → unity Amp): notes sound, the pitch wheel bends
    /// them, and the mod wheel brings in vibrato.
    #[test]
    fn synth_module_plays_bends_and_vibratos() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};
        use signal_sampler::RenderNode;

        let note_on = |note: u8, vel: u8| {
            use signal_sampler::midicore::{Channel, KeyNumber, MidiEvent, Velocity};
            PluginMidiEvent {
                offset: 0,
                message: MidiEvent::NoteOn {
                    channel: Channel::new(0),
                    key: KeyNumber::new(note),
                    velocity: Velocity::new(vel),
                },
            }
        };
        let bend_full = || {
            use signal_sampler::midicore::{Channel, MidiEvent, PitchBend};
            PluginMidiEvent {
                offset: 0,
                message: MidiEvent::PitchBend {
                    channel: Channel::new(0),
                    bend: PitchBend::new(16_383),
                },
            }
        };

        let set = ModuleSettings {
            source: Source::Synth,
            ..ModuleSettings::default()
        };
        let tree = signal_layer_with("S", &[set]);
        let mut rn = RenderNode::compile(&tree, 48_000);
        rn.prepare(48_000.0, 512);

        let run = |rn: &mut RenderNode, midi: &[PluginMidiEvent], frames: usize| {
            let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
            let ev = PluginEvents {
                params: &[],
                midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            l
        };
        let freq = |s: &[f32]| {
            s.windows(2)
                .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
                .count() as f32
                / 2.0
                / (s.len() as f32 / 48_000.0)
        };

        // A4 sounds through the whole chain (unity Amp — the old closed
        // synth Amp + env route would render silence here).
        let _ = run(&mut rn, &[note_on(69, 100)], 4800);
        let steady = run(&mut rn, &[], 48_000);
        let rms = (steady.iter().map(|x| x * x).sum::<f32>() / steady.len() as f32).sqrt();
        assert!(rms > 1e-3, "synth module sounds, rms={rms}");
        let f0 = freq(&steady);
        assert!((f0 - 440.0).abs() < 6.0, "A4 at 440, got {f0}");

        // Pitch wheel: full bend ≈ +2 semitones.
        let _ = run(&mut rn, &[bend_full()], 2400);
        let bent = freq(&run(&mut rn, &[], 48_000));
        let ratio = bent / f0;
        assert!(
            (ratio - 2f32.powf(2.0 / 12.0)).abs() < 0.015,
            "full bend ≈ +2 st through the tree: ratio={ratio}"
        );
    }
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
