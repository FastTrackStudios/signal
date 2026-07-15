//! **Native block registry** — the single place a block type's built-in DSP
//! is declared. `native_dsp_available` and the render-tree backend builder
//! both derive from this table, so implementing a new block type is one
//! entry here (plus its module), nothing else.
//!
//! Each entry is a constructor from `(&RigBlock, sample_rate)` — build-time
//! `RigBlock.params` configure the instance; runtime parameter writes (the
//! mod matrix, a UI, a host) arrive later through `PluginEvents.params`.

use signal_plugin_host::PluginInstance;
use signal_proto::block::BlockType;

use crate::rig::RigBlock;

use super::{
    FilterCharacter, FilterMode, HarmVoice, NativeAmp, NativeDfs, NativeFilter, NativeModal,
    NativeWaveguide, NativeWaveshaper, NativeWavetable, NativeWurli, SynthConfig,
};
use crate::native_osc::NativeOscillator;
use crate::soundsource::SoundsourceLeaf;

type Ctor = fn(&RigBlock, u32) -> Box<dyn PluginInstance>;

/// The registry: `(block type, constructor)`.
const REGISTRY: &[(BlockType, Ctor)] = &[
    (BlockType::Oscillator, build_oscillator),
    // City Grand physically-modeled piano (coupled waveguide; env-switchable).
    (BlockType::Harmonic, build_modal),
    // City Wurli physically-modeled Wurlitzer 200A (vendored openwurli-dsp).
    (BlockType::Formant, build_wurli),
    (BlockType::Wavetable, build_wavetable),
    (BlockType::Filter, build_filter),
    (BlockType::Amp, build_amp),
    (BlockType::Waveshaper, build_waveshaper),
    (BlockType::Dfs, build_dfs),
    // Built-in FX (features/fx/, wrapped in signal-fx).
    (BlockType::Eq, build_eq),
    (BlockType::Compressor, build_comp),
    (BlockType::Reverb, build_reverb),
    (BlockType::Delay, build_delay),
    // Modulation — chorus/flanger/vibrato share chorus-dsp; tremolo is trem-dsp.
    (BlockType::Chorus, build_chorus),
    (BlockType::Flanger, build_flanger),
    (BlockType::Vibrato, build_vibrato),
    (BlockType::Trem, build_trem),
    (BlockType::Gate, build_gate),
    (BlockType::Volume, build_volume),
    // No DSP yet — transparent placeholders so the block exists (bypassed).
    (BlockType::Phaser, build_phaser),
    (BlockType::Rotary, build_rotary),
    (BlockType::Boost, build_boost_pedal),
    (BlockType::Drive, build_drive),
];

fn build_chorus(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeMod::chorus(sample_rate as f64);
    apply_mod_params(block, &mut fx);
    Box::new(fx)
}
fn build_flanger(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeMod::flanger(sample_rate as f64);
    apply_mod_params(block, &mut fx);
    Box::new(fx)
}
fn build_vibrato(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeMod::vibrato(sample_rate as f64);
    apply_mod_params(block, &mut fx);
    Box::new(fx)
}
fn apply_mod_params(block: &RigBlock, fx: &mut signal_fx::NativeMod) {
    for name in ["mix", "depth", "rate", "engine"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
}
fn build_trem(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeTrem::new(sample_rate as f64);
    for name in ["depth", "mix", "rate", "mode"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
    Box::new(fx)
}
fn build_gate(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeGate::new(sample_rate as f64);
    for name in ["threshold", "attack", "release"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
    Box::new(fx)
}
fn build_volume(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeGain::new(sample_rate as f64);
    if let Some(v) = block.param_f32("gain_db") {
        fx.set_named("gain_db", v as f64);
    }
    Box::new(fx)
}
fn build_phaser(_block: &RigBlock, _sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(signal_fx::NativePassthrough::new("Phaser"))
}
fn build_rotary(_block: &RigBlock, _sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(signal_fx::NativePassthrough::new("Rotary"))
}
fn build_boost_pedal(_block: &RigBlock, _sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(signal_fx::NativePassthrough::new("Boost"))
}
fn build_drive(_block: &RigBlock, _sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(signal_fx::NativePassthrough::new("Drive"))
}

fn build_eq(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeEq::new(sample_rate as f64);
    // Full band set: b{1..24}_{used,on,freq,gain,q,shape}.
    for b in 0..signal_fx::EQ_BANDS {
        for f in 0..signal_fx::EQ_FIELDS {
            let name = signal_fx::eq_param_name(b, f);
            if let Some(v) = block.param_f32(&name) {
                fx.set_named(&name, v as f64);
            }
        }
    }
    Box::new(fx)
}

fn build_comp(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeComp::new(sample_rate as f64);
    for name in ["threshold", "ratio", "attack", "release", "knee", "range", "fold", "style"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
    Box::new(fx)
}

fn build_reverb(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeReverb::new(sample_rate as f64);
    for name in ["mix", "decay", "size"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
    if let Some(path) = block.param_str("ir_path") {
        if !path.is_empty() && !fx.load_ir_wav(&path) {
            tracing::warn!("reverb IR failed to load: {path}");
        }
    }
    Box::new(fx)
}

fn build_delay(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut fx = signal_fx::NativeDelay::new(sample_rate as f64);
    for name in ["mix", "time", "feedback", "pan", "tap_div_l", "tap_div_r"] {
        if let Some(v) = block.param_f32(name) {
            fx.set_named(name, v as f64);
        }
    }
    Box::new(fx)
}

/// Block types whose built-in `Native` DSP is implemented today.
pub fn native_dsp_available(block_type: BlockType) -> bool {
    REGISTRY.iter().any(|(t, _)| *t == block_type)
}

/// Construct the native backend for `block`, or `None` for types whose DSP
/// isn't written yet.
pub fn build_native(block: &RigBlock, sample_rate: u32) -> Option<Box<dyn PluginInstance>> {
    REGISTRY
        .iter()
        .find(|(t, _)| *t == block.block_type)
        .map(|(_, ctor)| ctor(block, sample_rate))
}

// ── Constructors ─────────────────────────────────────────────────────────────

fn build_oscillator(_block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    // A first-class Soundsource, hosted through the generic leaf adapter.
    Box::new(SoundsourceLeaf::new(NativeOscillator::new(sample_rate)))
}

fn build_modal(_block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    // The Harmonic block plays the LATEST City Grand engine: the coupled
    // waveguide (physical simulation). CITY_GRAND_ENGINE=modal falls back to
    // the additive/DDSP voice for A/B.
    if std::env::var("CITY_GRAND_ENGINE").as_deref() == Ok("modal") {
        Box::new(NativeModal::new(sample_rate))
    } else {
        Box::new(NativeWaveguide::new(sample_rate))
    }
}

fn build_wurli(_block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(NativeWurli::new(sample_rate))
}

fn build_wavetable(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut cfg = SynthConfig::default();
    let p = |name: &str| block.param_f32(name);
    if let Some(v) = p("shape") {
        cfg.shape = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("unison_voices") {
        cfg.unison_voices = (v.round() as u32).clamp(1, 8);
    }
    if let Some(v) = p("unison_detune") {
        // 0..2 → 0..200 cents (Omnisphere full detune ≈ 185 cents).
        cfg.unison_detune_cents = v.clamp(0.0, 2.0) * 100.0;
    }
    if let Some(v) = p("unison_width") {
        cfg.unison_width = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("unison_octave") {
        cfg.unison_octave = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("unison_analog") {
        cfg.unison_analog = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("unison_drift") {
        cfg.unison_drift = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("fm_depth") {
        cfg.fm_depth = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("fm_ratio") {
        cfg.fm_ratio = v.max(0.01);
    }
    if let Some(v) = p("fm_shape") {
        cfg.fm_shape = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("ring_mix") {
        cfg.ring_mix = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("ring_ratio") {
        cfg.ring_ratio = v.max(0.01);
    }
    if let Some(v) = p("amp_attack") {
        cfg.env.attack_s = v.max(0.0);
    }
    if let Some(v) = p("amp_decay") {
        cfg.env.decay_s = v.max(0.0);
    }
    if let Some(v) = p("amp_sustain") {
        cfg.env.sustain = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p("amp_release") {
        cfg.env.release_s = v.max(0.0);
    }
    for i in 0..4 {
        let n = i + 1;
        let level = p(&format!("harm{n}_level")).unwrap_or(0.0);
        if level > 0.0 {
            cfg.harmonia[i] = HarmVoice {
                on: true,
                level: level.clamp(0.0, 1.0),
                interval_semi: p(&format!("harm{n}_interval")).unwrap_or(0.0),
                pan: p(&format!("harm{n}_pan")).unwrap_or(0.0),
                shape: p(&format!("harm{n}_shape")).unwrap_or(cfg.shape),
            };
        }
    }
    // A first-class Soundsource, hosted through the generic leaf adapter.
    Box::new(SoundsourceLeaf::new(
        NativeWavetable::new(sample_rate).with_config(cfg),
    ))
}

fn build_filter(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut f = NativeFilter::new(sample_rate);
    if let Some(v) = block.param_f32("cutoff") {
        f = f.with_cutoff(NativeFilter::cutoff_from_norm(v));
    }
    if let Some(v) = block.param_f32("resonance") {
        f = f.with_q(NativeFilter::q_from_norm(v));
    }
    if let Some(m) = block
        .params
        .iter()
        .find(|p| p.name == "mode")
        .and_then(|p| FilterMode::parse(&p.value))
    {
        f = f.with_mode(m);
    }
    if let Some(v) = block.param_f32("poles") {
        f = f.with_poles(v.round() as u32);
    }
    if block
        .params
        .iter()
        .any(|p| p.name == "character" && p.value.eq_ignore_ascii_case("ladder"))
    {
        f = f.with_character(FilterCharacter::Ladder);
    }
    Box::new(f)
}

fn build_amp(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut a = NativeAmp::new(sample_rate);
    if let Some(v) = block.param_f32("gain") {
        a = a.with_gain_norm(v);
    }
    Box::new(a)
}

fn build_waveshaper(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    let mut w = NativeWaveshaper::new(sample_rate);
    if let Some(v) = block.param_f32("drive") {
        w = w.with_drive(v);
    }
    if let Some(v) = block.param_f32("crush") {
        w = w.with_crush(v);
    }
    if let Some(v) = block.param_f32("reduce") {
        w = w.with_reduce(v);
    }
    if let Some(v) = block.param_f32("mix") {
        w = w.with_mix(v);
    }
    Box::new(w)
}

fn build_dfs(block: &RigBlock, sample_rate: u32) -> Box<dyn PluginInstance> {
    Box::new(
        NativeDfs::new(sample_rate)
            .with_shifter_a(
                block.param_f32("shift_a_hz").unwrap_or(0.0),
                block.param_f32("mix_a").unwrap_or(0.0),
            )
            .with_shifter_b(
                block.param_f32("shift_b_hz").unwrap_or(0.0),
                block.param_f32("mix_b").unwrap_or(0.0),
            )
            .with_parallel(block.param_f32("parallel").unwrap_or(0.0) > 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_and_availability_agree() {
        for bt in [
            BlockType::Oscillator,
            BlockType::Wavetable,
            BlockType::Filter,
            BlockType::Amp,
            BlockType::Waveshaper,
            BlockType::Dfs,
        ] {
            assert!(native_dsp_available(bt), "{bt:?} registered");
            let block = RigBlock::of_type(bt);
            assert!(build_native(&block, 48_000).is_some(), "{bt:?} builds");
        }
        // Rotary is registered as a transparent placeholder (NativePassthrough)
        // so the block exists in chains until rotary DSP lands.
        assert!(native_dsp_available(BlockType::Rotary));
        assert!(build_native(&RigBlock::of_type(BlockType::Rotary), 48_000).is_some());
        // Pitch has no entry at all — no placeholder, no DSP.
        assert!(!native_dsp_available(BlockType::Pitch));
        assert!(build_native(&RigBlock::of_type(BlockType::Pitch), 48_000).is_none());
    }
}
