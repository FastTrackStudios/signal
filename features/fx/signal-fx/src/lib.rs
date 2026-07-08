//! Built-in FX for signal.
//!
//! Thin [`PluginInstance`] wrappers over the built-in FX facades (`eq`, `comp`,
//! `reverb`, `delay`) so signal's FX chain can host them as native blocks — no
//! CLAP/VST3 hosting, no GUI framework. Each wrapper adapts a DSP `Chain`/
//! processor to the daw `PluginInstance` contract (`prepare` / `process_block`
//! / params) and exposes a small controllable parameter set.
//!
//! **Param wiring.** Each wrapper declares a [`ParamSpec`] table (stable id +
//! name + range). Runtime writes (mod matrix / UI) arrive by id through
//! `process_block`'s events; build-time `RigBlock` params are applied by name
//! via [`set_named`](NativeReverb::set_named), which the native-block registry
//! in `signal-sampler` calls when constructing the block.

use audiocore_dsp::{AudioConfig, Processor};
use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};

// ── Param helpers ──────────────────────────────────────────────────────────

/// Hard cap on time-effect (reverb/delay) dry-wet mix for now (10%).
const TIME_MIX_MAX: f64 = 0.10;

/// One controllable parameter: stable id, display name, range, default.
struct ParamSpec {
    id: u32,
    name: &'static str,
    min: f64,
    max: f64,
    default: f64,
}

fn param_infos(specs: &[ParamSpec]) -> Vec<PluginParamInfo> {
    specs
        .iter()
        .map(|s| PluginParamInfo {
            id: s.id,
            name: s.name.into(),
            min: s.min,
            max: s.max,
            default: s.default,
        })
        .collect()
}

fn param_id(specs: &[ParamSpec], name: &str) -> Option<u32> {
    specs
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .map(|s| s.id)
}

/// Convert an f32 stereo block to two f64 scratch buffers, run `f`, convert back.
#[inline]
fn process_f64_inplace(
    scratch_l: &mut Vec<f64>,
    scratch_r: &mut Vec<f64>,
    in_l: &[f32],
    in_r: &[f32],
    out_l: &mut [f32],
    out_r: &mut [f32],
    f: impl FnOnce(&mut [f64], &mut [f64]),
) {
    let n = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
    if scratch_l.len() < n {
        scratch_l.resize(n, 0.0);
        scratch_r.resize(n, 0.0);
    }
    for i in 0..n {
        scratch_l[i] = in_l[i] as f64;
        scratch_r[i] = in_r[i] as f64;
    }
    f(&mut scratch_l[..n], &mut scratch_r[..n]);
    for i in 0..n {
        out_l[i] = scratch_l[i] as f32;
        out_r[i] = scratch_r[i] as f32;
    }
}

// ── EQ ───────────────────────────────────────────────────────────────────────

/// Native EQ block — wraps [`eq::EqChain`]. Starts flat (no bands) and passes
/// audio through transparently until bands are configured (band editing is a
/// later step). No controllable params yet.
pub struct NativeEq {
    eq: eq::EqChain,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeEq {
    pub fn new(sample_rate: f64) -> Self {
        let mut eq = eq::EqChain::new();
        eq.set_sample_rate(sample_rate.max(1.0));
        Self {
            eq,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    /// Apply a build-time parameter by name (no-op — EQ has no scalar params).
    pub fn set_named(&mut self, _name: &str, _value: f64) {}
}

impl PluginInstance for NativeEq {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.eq", "EQ")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.eq.set_sample_rate(sample_rate.max(1.0));
        self.eq.reset();
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let eq = &mut self.eq;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| eq.process(l, r),
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Compressor ─────────────────────────────────────────────────────────────

const COMP_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "threshold", min: -60.0, max: 0.0, default: -18.0 },
    ParamSpec { id: 1, name: "ratio", min: 1.0, max: 20.0, default: 4.0 },
    ParamSpec { id: 2, name: "attack", min: 0.1, max: 200.0, default: 10.0 },
    ParamSpec { id: 3, name: "release", min: 5.0, max: 1000.0, default: 120.0 },
];

/// Native Compressor block — wraps [`comp::ProC3Compressor`] (ProC3-style).
/// Seeded with a musical default (−18 dB / 4:1).
pub struct NativeComp {
    comp: comp::ProC3Compressor,
    prepared: bool,
}

impl NativeComp {
    pub fn new(sample_rate: f64) -> Self {
        let mut comp = comp::ProC3Compressor::new(sample_rate.max(1.0));
        comp.set_threshold(-18.0);
        comp.set_ratio(4.0);
        comp.set_attack_ms(10.0);
        comp.set_release_ms(120.0);
        Self { comp, prepared: false }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.comp.set_threshold(v),
            1 => self.comp.set_ratio(v),
            2 => self.comp.set_attack_ms(v),
            3 => self.comp.set_release_ms(v),
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (`threshold`/`ratio`/`attack`/`release`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(COMP_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeComp {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.comp", "Compressor")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(COMP_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.comp.update(sample_rate.max(1.0));
        self.comp.reset();
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let n = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        for i in 0..n {
            out_l[i] = self.comp.process(in_l[i] as f64, 0) as f32;
            out_r[i] = self.comp.process(in_r[i] as f64, 1) as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Reverb ─────────────────────────────────────────────────────────────────

const REVERB_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "mix", min: 0.0, max: 0.10, default: 0.08 },
    ParamSpec { id: 1, name: "decay", min: 0.0, max: 1.0, default: 0.45 },
    ParamSpec { id: 2, name: "size", min: 0.0, max: 1.0, default: 0.5 },
];

/// Native Reverb block — wraps [`reverb::ReverbChain`]. Defaults to a subtle
/// Hall (low mix) so it sits under the tone rather than washing it out.
pub struct NativeReverb {
    rev: reverb::ReverbChain,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeReverb {
    pub fn new(_sample_rate: f64) -> Self {
        let mut rev = reverb::ReverbChain::new();
        rev.set_algorithm(reverb::AlgorithmType::Hall);
        rev.mix = 0.08;
        rev.params.decay = 0.45;
        rev.params.size = 0.5;
        rev.update_params();
        Self {
            rev,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.rev.mix = v.min(TIME_MIX_MAX),
            1 => {
                self.rev.params.decay = v;
                self.rev.update_params();
            }
            2 => {
                self.rev.params.size = v;
                self.rev.update_params();
            }
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (`mix`/`decay`/`size`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(REVERB_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeReverb {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.reverb", "Reverb")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(REVERB_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.rev.update(AudioConfig {
            sample_rate: sample_rate.max(1.0),
            max_buffer_size: block_size.max(1) as usize,
        });
        self.rev.reset();
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let rev = &mut self.rev;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| rev.process(l, r),
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Delay ──────────────────────────────────────────────────────────────────

const DELAY_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "mix", min: 0.0, max: 0.10, default: 0.08 },
    ParamSpec { id: 1, name: "time", min: 20.0, max: 2500.0, default: 400.0 },
    ParamSpec { id: 2, name: "feedback", min: 0.0, max: 0.95, default: 0.30 },
    // TimeLine MX parity params (style index: see delay::DelayStyle).
    ParamSpec { id: 3, name: "style", min: 0.0, max: 12.0, default: 1.0 },
    ParamSpec { id: 4, name: "swell", min: 0.0, max: 4.0, default: 0.0 },
    ParamSpec { id: 5, name: "freeze", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 6, name: "tempo_bpm", min: 0.0, max: 300.0, default: 0.0 },
    ParamSpec { id: 7, name: "tap_div", min: 0.0, max: 7.0, default: 7.0 },
    ParamSpec { id: 8, name: "high_pass", min: 0.0, max: 900.0, default: 0.0 },
    ParamSpec { id: 9, name: "repeat_dyn", min: 0.0, max: 1.0, default: 0.0 },
];

/// Native Delay block — wraps [`delay::DelayChain`]. Defaults to a subtle clean
/// quarter-note-ish delay with modest feedback.
pub struct NativeDelay {
    dly: delay::DelayChain,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeDelay {
    pub fn new(_sample_rate: f64) -> Self {
        let mut dly = delay::DelayChain::new();
        dly.set_style(delay::DelayStyle::Clean);
        dly.mix = 0.08;
        dly.delay_l.time_ms = 400.0;
        dly.delay_r.time_ms = 400.0;
        dly.delay_l.feedback = 0.30;
        dly.delay_r.feedback = 0.30;
        Self {
            dly,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.dly.mix = v.min(TIME_MIX_MAX),
            1 => {
                self.dly.delay_l.time_ms = v;
                self.dly.delay_r.time_ms = v;
            }
            2 => {
                self.dly.delay_l.feedback = v;
                self.dly.delay_r.feedback = v;
            }
            3 => self
                .dly
                .set_style(delay::DelayStyle::from_index(v.round().max(0.0) as usize)),
            4 => self.dly.swell_time_s = v,
            5 => self.dly.freeze = v > 0.5,
            6 => self.dly.tempo_bpm = if v > 0.0 { Some(v) } else { None },
            7 => {
                let div = delay::TapDivision::from_index(v.round().max(0.0) as usize);
                self.dly.tap_div_l = div;
                self.dly.tap_div_r = div;
            }
            8 => self.dly.high_pass_hz = v,
            9 => self.dly.repeat_dynamics = v > 0.5,
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (`mix`/`time`/`feedback`/
    /// `style`/`swell`/`freeze`/`tempo_bpm`/`tap_div`/`high_pass`/`repeat_dyn`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(DELAY_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeDelay {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.delay", "Delay")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(DELAY_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.dly.update(AudioConfig {
            sample_rate: sample_rate.max(1.0),
            max_buffer_size: block_size.max(1) as usize,
        });
        self.dly.reset();
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let dly = &mut self.dly;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| dly.process(l, r),
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Modulation (chorus / flanger / vibrato) ────────────────────────────────

use modulation::chorus::chain::ChorusChain;
use modulation::chorus::engine::EffectType;

const MOD_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "mix", min: 0.0, max: 1.0, default: 0.4 },
    ParamSpec { id: 1, name: "depth", min: 0.0, max: 1.0, default: 0.4 },
    ParamSpec { id: 2, name: "rate", min: 0.05, max: 10.0, default: 1.0 },
];

/// Native modulation block — wraps [`ChorusChain`], selecting Chorus / Flanger /
/// Vibrato via its effect type. One struct, three constructors.
pub struct NativeMod {
    ch: ChorusChain,
    label: &'static str,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeMod {
    pub fn chorus(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Chorus, "Chorus", 1.0)
    }
    pub fn flanger(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Flanger, "Flanger", 0.3)
    }
    pub fn vibrato(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Vibrato, "Vibrato", 5.0)
    }

    fn new(_sample_rate: f64, effect: EffectType, label: &'static str, rate: f64) -> Self {
        let mut ch = ChorusChain::new();
        ch.effect_type = effect;
        // Vibrato is pitch-only (fully wet); chorus/flanger blend.
        ch.mix = if matches!(effect, EffectType::Vibrato) { 1.0 } else { 0.4 };
        ch.depth = 0.4;
        ch.rate_hz = rate;
        Self {
            ch,
            label,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.ch.mix = v,
            1 => self.ch.depth = v,
            2 => self.ch.rate_hz = v,
            _ => {}
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(MOD_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeMod {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.mod", self.label)
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(MOD_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.ch.update(AudioConfig {
            sample_rate: sample_rate.max(1.0),
            max_buffer_size: block_size.max(1) as usize,
        });
        self.ch.reset();
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let ch = &mut self.ch;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| ch.process(l, r),
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Tremolo ────────────────────────────────────────────────────────────────

use modulation::trem::chain::TremChain;
use modulation::trem::tremolo::TremMode;

const TREM_PARAMS: &[ParamSpec] = &[ParamSpec { id: 0, name: "depth", min: 0.0, max: 1.0, default: 0.5 }];

/// Native Tremolo block — wraps [`TremChain`] (amplitude modulation).
pub struct NativeTrem {
    tr: TremChain,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeTrem {
    pub fn new(_sample_rate: f64) -> Self {
        let mut tr = TremChain::new();
        tr.set_mode(TremMode::Stereo);
        tr.set_depth(0.5);
        Self {
            tr,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        if id == 0 {
            self.tr.set_depth(v);
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(TREM_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeTrem {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.trem", "Tremolo")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(TREM_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.tr.update(AudioConfig {
            sample_rate: sample_rate.max(1.0),
            max_buffer_size: block_size.max(1) as usize,
        });
        self.tr.reset();
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let tr = &mut self.tr;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| tr.process(l, r),
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Passthrough (block types without DSP yet: Phaser, Rotary) ──────────────

/// A transparent placeholder block for a type whose DSP isn't written yet. It
/// passes audio through unchanged so the block can exist in the chain (bypassed)
/// until its real DSP lands.
pub struct NativePassthrough {
    label: &'static str,
    prepared: bool,
}

impl NativePassthrough {
    pub fn new(label: &'static str) -> Self {
        Self { label, prepared: false }
    }
    pub fn set_named(&mut self, _name: &str, _value: f64) {}
}

impl PluginInstance for NativePassthrough {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.passthrough", self.label)
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        Vec::new()
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, _id: u32, _value: f64) -> Option<String> {
        None
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, _sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        self.prepared = true;
        Ok(())
    }
    fn is_prepared(&self) -> bool {
        self.prepared
    }
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        _events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let n = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        out_l[..n].copy_from_slice(&in_l[..n]);
        out_r[..n].copy_from_slice(&in_r[..n]);
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── shared ─────────────────────────────────────────────────────────────────

fn descriptor(id: &str, name: &str) -> PluginDescriptor {
    PluginDescriptor {
        id: id.into(),
        name: name.into(),
        vendor: "FTS".into(),
        version: String::new(),
        format: PluginFormat::Synthetic,
    }
}
