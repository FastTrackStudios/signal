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

/// Bands in the full EQ (matches eq-ui's `NUM_BANDS`).
pub const EQ_BANDS: usize = 24;
/// Per-band fields, in wire order: used, on, freq, gain, q, shape.
pub const EQ_FIELDS: usize = 6;

/// Shape indices, matching eq-ui's `EqBandShape` ordering: 0 Bell,
/// 1 LowShelf, 2 HighShelf, 3 LowCut, 4 HighCut, 5 Notch, 6 BandPass,
/// 7 TiltShelf, 8 FlatTilt, 9 AllPass.
fn eq_shape_to_filter(shape: u32) -> eq::FilterType {
    match shape {
        1 => eq::FilterType::LowShelf,
        2 => eq::FilterType::HighShelf,
        3 => eq::FilterType::Highpass,
        4 => eq::FilterType::Lowpass,
        5 => eq::FilterType::Notch,
        6 => eq::FilterType::Bandpass,
        7 => eq::FilterType::TiltShelf,
        8 => eq::FilterType::FlatTilt,
        9 => eq::FilterType::Allpass,
        _ => eq::FilterType::Peak,
    }
}

/// Param name for `(band, field)` — `b{band+1}_{used|on|freq|gain|q|shape}`.
pub fn eq_param_name(band: usize, field: usize) -> String {
    let f = ["used", "on", "freq", "gain", "q", "shape"][field];
    format!("b{}_{}", band + 1, f)
}

fn eq_param_id_of(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('b')?;
    let (num, field) = rest.split_once('_')?;
    let band: usize = num.parse().ok()?;
    if band == 0 || band > EQ_BANDS {
        return None;
    }
    let fidx = ["used", "on", "freq", "gain", "q", "shape"]
        .iter()
        .position(|f| *f == field)?;
    Some(((band - 1) * EQ_FIELDS + fidx) as u32)
}

/// Native EQ block — the full FTS-EQ engine: 24 dynamic bands over
/// [`eq::EqChain`]'s Pro-Q ZPK pipeline, each with used/on/freq/gain/Q/shape
/// (all ten eq-ui shapes). Bands start unused → transparent passthrough.
pub struct NativeEq {
    eq: eq::EqChain,
    /// (used, on) per band — a band renders only when both are set.
    state: [(bool, bool); EQ_BANDS],
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeEq {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let mut chain = eq::EqChain::new();
        chain.set_sample_rate(sample_rate);
        for _ in 0..EQ_BANDS {
            let idx = chain.add_band();
            if let Some(band) = chain.band_mut(idx) {
                band.enabled = false; // unused until claimed
                band.freq_hz = 1000.0;
                band.gain_db = 0.0;
                band.q = 0.707;
            }
            chain.update_band(idx);
        }
        Self {
            eq: chain,
            state: [(false, false); EQ_BANDS],
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        let (band, field) = ((id as usize) / EQ_FIELDS, (id as usize) % EQ_FIELDS);
        if band >= EQ_BANDS {
            return;
        }
        match field {
            0 => self.state[band].0 = v >= 0.5,
            1 => self.state[band].1 = v >= 0.5,
            _ => {}
        }
        let (used, on) = self.state[band];
        if let Some(b) = self.eq.band_mut(band) {
            match field {
                0 | 1 => b.enabled = used && on,
                2 => b.freq_hz = v.clamp(10.0, 30000.0),
                3 => b.gain_db = v.clamp(-30.0, 30.0),
                4 => b.q = v.clamp(0.025, 40.0),
                _ => b.filter_type = eq_shape_to_filter(v as u32),
            }
        }
        self.eq.update_band(band);
    }

    /// Apply a parameter by name (`b{i}_{used|on|freq|gain|q|shape}`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = eq_param_id_of(name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeEq {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.eq", "EQ")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        (0..EQ_BANDS * EQ_FIELDS)
            .map(|i| {
                let (band, field) = (i / EQ_FIELDS, i % EQ_FIELDS);
                let (min, max, default) = match field {
                    0 | 1 => (0.0, 1.0, 0.0),
                    2 => (10.0, 30000.0, 1000.0),
                    3 => (-30.0, 30.0, 0.0),
                    4 => (0.025, 40.0, 0.707),
                    _ => (0.0, 9.0, 0.0),
                };
                PluginParamInfo {
                    id: i as u32,
                    name: eq_param_name(band, field),
                    min,
                    max,
                    default,
                }
            })
            .collect()
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
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        for &(id, value) in events.params {
            self.set(id, value);
        }
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
    ParamSpec { id: 1, name: "time", min: 20.0, max: 2000.0, default: 400.0 },
    ParamSpec { id: 2, name: "feedback", min: 0.0, max: 0.95, default: 0.30 },
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
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (`mix`/`time`/`feedback`).
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

// ── Gain (Volume / Boost utility) ──────────────────────────────────────────

const GAIN_PARAMS: &[ParamSpec] = &[ParamSpec { id: 0, name: "gain_db", min: -24.0, max: 24.0, default: 0.0 }];

/// Native gain block — a clean dB trim (the "Boost" utility). Gain changes
/// glide over ~10 ms so footswitch boosts never click.
pub struct NativeGain {
    target: f64,
    current: f64,
    coeff: f64,
    prepared: bool,
}

impl NativeGain {
    pub fn new(_sample_rate: f64) -> Self {
        Self { target: 1.0, current: 1.0, coeff: 0.0, prepared: false }
    }

    fn set(&mut self, id: u32, v: f64) {
        if id == 0 {
            self.target = 10f64.powf(v.clamp(-24.0, 24.0) / 20.0);
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(GAIN_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeGain {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.gain", "Gain")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(GAIN_PARAMS)
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
        // One-pole toward the target with a ~10 ms time constant.
        self.coeff = (-1.0 / (0.010 * sample_rate.max(1.0))).exp();
        self.current = self.target;
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
        let (t, c) = (self.target, self.coeff);
        let mut g = self.current;
        for i in 0..out_l.len() {
            g = t + (g - t) * c;
            out_l[i] = in_l.get(i).copied().unwrap_or(0.0) * g as f32;
            out_r[i] = in_r.get(i).copied().unwrap_or(0.0) * g as f32;
        }
        self.current = g;
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Gate ────────────────────────────────────────────────────────────────────

const GATE_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "threshold", min: -90.0, max: 0.0, default: -50.0 },
    ParamSpec { id: 1, name: "attack", min: 0.1, max: 50.0, default: 1.0 },
    ParamSpec { id: 2, name: "release", min: 5.0, max: 500.0, default: 120.0 },
];

/// Native noise gate — peak-follower downward gate. Opens fast (attack),
/// closes smoothly (release); full mute below threshold.
pub struct NativeGate {
    threshold: f64,
    attack_ms: f64,
    release_ms: f64,
    env: f64,
    gain: f64,
    attack_coeff: f64,
    release_coeff: f64,
    env_coeff: f64,
    sample_rate: f64,
    prepared: bool,
}

impl NativeGate {
    pub fn new(sample_rate: f64) -> Self {
        let mut g = Self {
            threshold: 10f64.powf(-50.0 / 20.0),
            attack_ms: 1.0,
            release_ms: 120.0,
            env: 0.0,
            gain: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            env_coeff: 0.0,
            sample_rate: sample_rate.max(1.0),
            prepared: false,
        };
        g.update_coeffs();
        g
    }

    fn update_coeffs(&mut self) {
        let sr = self.sample_rate;
        self.attack_coeff = (-1.0 / (self.attack_ms.max(0.1) / 1000.0 * sr)).exp();
        self.release_coeff = (-1.0 / (self.release_ms.max(5.0) / 1000.0 * sr)).exp();
        // Envelope follower decay ~30 ms.
        self.env_coeff = (-1.0 / (0.030 * sr)).exp();
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.threshold = 10f64.powf(v.clamp(-90.0, 0.0) / 20.0),
            1 => {
                self.attack_ms = v;
                self.update_coeffs();
            }
            2 => {
                self.release_ms = v;
                self.update_coeffs();
            }
            _ => {}
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(GATE_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeGate {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.gate", "Gate")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(GATE_PARAMS)
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
        self.sample_rate = sample_rate.max(1.0);
        self.update_coeffs();
        self.env = 0.0;
        self.gain = 0.0;
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
        for i in 0..out_l.len() {
            let l = in_l.get(i).copied().unwrap_or(0.0);
            let r = in_r.get(i).copied().unwrap_or(0.0);
            let peak = l.abs().max(r.abs()) as f64;
            // Peak follower: instant rise, ~30 ms fall.
            self.env = if peak > self.env { peak } else { peak + (self.env - peak) * self.env_coeff };
            let target = if self.env >= self.threshold { 1.0 } else { 0.0 };
            let coeff = if target > self.gain { self.attack_coeff } else { self.release_coeff };
            self.gain = target + (self.gain - target) * coeff;
            out_l[i] = l * self.gain as f32;
            out_r[i] = r * self.gain as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}
