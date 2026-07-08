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
    // Machine voice (0 = MX, 1 = Classic; Digital deep pass adds more).
    ParamSpec { id: 10, name: "voice", min: 0.0, max: 3.0, default: 0.0 },
    // dTape / dBucket character macros.
    ParamSpec { id: 11, name: "tape_age", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 12, name: "crinkle", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 13, name: "bucket_loss", min: 0.0, max: 1.0, default: 0.0 },
    // Ice (Pitch style): MX interval menu index (30 = Free), slice
    // (0 short / 1 medium / 2 long / 3 free grain), dry<->ice blend.
    ParamSpec { id: 14, name: "interval", min: 0.0, max: 30.0, default: 30.0 },
    ParamSpec { id: 15, name: "slice", min: 0.0, max: 3.0, default: 3.0 },
    ParamSpec { id: 16, name: "blend", min: 0.0, max: 1.0, default: 1.0 },
    // Dual 1+2 (TimeLine MX): routing (0 Single / 1 Series 1>2 /
    // 2 Series 2>1 / 3 Parallel / 4 Split / 5 Split Swap) + delay B.
    // Ids 0-16 keep addressing delay A.
    ParamSpec { id: 17, name: "routing", min: 0.0, max: 5.0, default: 0.0 },
    ParamSpec { id: 18, name: "style_b", min: 0.0, max: 12.0, default: 1.0 },
    ParamSpec { id: 19, name: "time_b", min: 20.0, max: 2500.0, default: 300.0 },
    ParamSpec { id: 20, name: "feedback_b", min: 0.0, max: 0.95, default: 0.30 },
    ParamSpec { id: 21, name: "mix_b", min: 0.0, max: 0.10, default: 0.08 },
    // Spectral machine (grain_shape: 0 Soft/1 Swell/2 SoftPluck/
    // 3 Pluck/4 Bounce; direction: 0 Fwd/1 Rev/2 Both; density = n in
    // Synced(1/n); density_ms >= 6 switches to free 6-250 ms).
    ParamSpec { id: 22, name: "grain_shape", min: 0.0, max: 4.0, default: 0.0 },
    ParamSpec { id: 23, name: "direction", min: 0.0, max: 2.0, default: 0.0 },
    ParamSpec { id: 24, name: "density", min: 1.0, max: 32.0, default: 8.0 },
    ParamSpec { id: 25, name: "density_ms", min: 0.0, max: 250.0, default: 0.0 },
    ParamSpec { id: 26, name: "spread", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 27, name: "stretch", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 28, name: "octave", min: 0.0, max: 1.0, default: 0.0 },
    // Lo-Fi machine (filter_shape: 0 Off .. 8 Intercom; grit rides the
    // shared "drive" engine field via id 29).
    ParamSpec { id: 29, name: "grit", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 30, name: "bit_depth", min: 4.0, max: 32.0, default: 12.0 },
    ParamSpec { id: 31, name: "sr_div", min: 1.0, max: 64.0, default: 4.0 },
    ParamSpec { id: 32, name: "lofi_mix", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 33, name: "vinyl", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 34, name: "filter_shape", min: 0.0, max: 8.0, default: 0.0 },
];

/// Native Delay block — wraps [`delay::DualDelay`] (two full chains +
/// TimeLine MX 1+2 routing; `Single` = chain A only, bit-compatible with
/// the previous single-chain wrapper). Defaults to a subtle clean
/// quarter-note-ish delay with modest feedback.
pub struct NativeDelay {
    dly: delay::DualDelay,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeDelay {
    pub fn new(_sample_rate: f64) -> Self {
        let mut dly = delay::DualDelay::new();
        for chain in [&mut dly.a, &mut dly.b] {
            chain.set_style(delay::DelayStyle::Clean);
            chain.mix = 0.08;
            chain.delay_l.time_ms = 400.0;
            chain.delay_r.time_ms = 400.0;
            chain.delay_l.feedback = 0.30;
            chain.delay_r.feedback = 0.30;
        }
        // B seeds slightly shorter so engaging a dual routing is
        // immediately audible before any params are set.
        dly.b.delay_l.time_ms = 300.0;
        dly.b.delay_r.time_ms = 300.0;
        Self {
            dly,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        // Ids 0-16 address delay A; 17+ are the dual-routing block.
        let a = &mut self.dly.a;
        match id {
            0 => a.mix = v.min(TIME_MIX_MAX),
            1 => {
                a.delay_l.time_ms = v;
                a.delay_r.time_ms = v;
            }
            2 => {
                a.delay_l.feedback = v;
                a.delay_r.feedback = v;
            }
            3 => a.set_style(delay::DelayStyle::from_index(v.round().max(0.0) as usize)),
            4 => a.swell_time_s = v,
            5 => a.freeze = v > 0.5,
            6 => a.tempo_bpm = if v > 0.0 { Some(v) } else { None },
            7 => {
                let div = delay::TapDivision::from_index(v.round().max(0.0) as usize);
                a.tap_div_l = div;
                a.tap_div_r = div;
            }
            8 => a.high_pass_hz = v,
            9 => a.repeat_dynamics = v > 0.5,
            10 => {
                let voice = v.round().max(0.0) as u8;
                a.delay_l.voice = voice;
                a.delay_r.voice = voice;
            }
            11 => {
                a.delay_l.tape_age = v;
                a.delay_r.tape_age = v;
            }
            12 => {
                a.delay_l.crinkle = v;
                a.delay_r.crinkle = v;
            }
            13 => {
                a.delay_l.bbd_bucket_loss = v;
                a.delay_r.bbd_bucket_loss = v;
            }
            14 => {
                let i = v.round().max(0.0) as usize;
                let interval = if i >= delay::IceInterval::MENU_LEN {
                    delay::IceInterval::Free
                } else {
                    delay::IceInterval::from_index(i)
                };
                a.delay_l.pitch_interval = interval;
                a.delay_r.pitch_interval = interval;
            }
            15 => {
                let slice = match v.round().max(0.0) as usize {
                    0 => Some(delay::IceSlice::Short),
                    1 => Some(delay::IceSlice::Medium),
                    2 => Some(delay::IceSlice::Long),
                    _ => None,
                };
                a.delay_l.pitch_slice = slice;
                a.delay_r.pitch_slice = slice;
            }
            16 => {
                a.delay_l.pitch_blend = v;
                a.delay_r.pitch_blend = v;
            }
            17 => {
                self.dly.routing = delay::DualRouting::from_index(v.round().max(0.0) as usize)
            }
            18 => self
                .dly
                .b
                .set_style(delay::DelayStyle::from_index(v.round().max(0.0) as usize)),
            19 => {
                self.dly.b.delay_l.time_ms = v;
                self.dly.b.delay_r.time_ms = v;
            }
            20 => {
                self.dly.b.delay_l.feedback = v;
                self.dly.b.delay_r.feedback = v;
            }
            21 => self.dly.b.mix = v.min(TIME_MIX_MAX),
            22 => {
                let shape = match v.round().max(0.0) as usize {
                    1 => delay::GrainShape::Swell,
                    2 => delay::GrainShape::SoftPluck,
                    3 => delay::GrainShape::Pluck,
                    4 => delay::GrainShape::Bounce,
                    _ => delay::GrainShape::Soft,
                };
                a.delay_l.spectral_shape = shape;
                a.delay_r.spectral_shape = shape;
            }
            23 => {
                let dir = match v.round().max(0.0) as usize {
                    1 => delay::GrainDirection::Reverse,
                    2 => delay::GrainDirection::Both,
                    _ => delay::GrainDirection::Forward,
                };
                a.delay_l.spectral_direction = dir;
                a.delay_r.spectral_direction = dir;
            }
            24 => {
                let n = v.clamp(1.0, 32.0);
                let d = delay::DensityMode::Synced(1.0 / n);
                a.delay_l.spectral_density = d;
                a.delay_r.spectral_density = d;
            }
            25 => {
                // >= 6 ms switches to free-running density; 0 returns
                // to the synced default (set via id 24).
                if v >= 6.0 {
                    let d = delay::DensityMode::FreeHz(1000.0 / v);
                    a.delay_l.spectral_density = d;
                    a.delay_r.spectral_density = d;
                }
            }
            26 => {
                a.delay_l.spectral_spread = v;
                a.delay_r.spectral_spread = v;
            }
            27 => {
                a.delay_l.spectral_stretch = v;
                a.delay_r.spectral_stretch = v;
            }
            28 => {
                a.delay_l.spectral_octave = v;
                a.delay_r.spectral_octave = v;
            }
            29 => {
                a.delay_l.drive = v;
                a.delay_r.drive = v;
            }
            30 => {
                a.delay_l.lofi_bit_depth = v;
                a.delay_r.lofi_bit_depth = v;
            }
            31 => {
                a.delay_l.lofi_sr_div = v;
                a.delay_r.lofi_sr_div = v;
            }
            32 => {
                a.delay_l.lofi_mix = v;
                a.delay_r.lofi_mix = v;
            }
            33 => {
                a.delay_l.lofi_vinyl = v;
                a.delay_r.lofi_vinyl = v;
            }
            34 => {
                let shape = delay::LoFiFilterShape::from_index(v.round().max(0.0) as usize);
                a.delay_l.lofi_filter_shape = shape;
                a.delay_r.lofi_filter_shape = shape;
            }
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (`mix`/`time`/`feedback`/
    /// `style`/`swell`/`freeze`/`tempo_bpm`/`tap_div`/`high_pass`/
    /// `repeat_dyn`/`voice`/`tape_age`/`crinkle`/`bucket_loss`/
    /// `interval`/`slice`/`blend`, plus the dual block: `routing`/
    /// `style_b`/`time_b`/`feedback_b`/`mix_b`).
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
