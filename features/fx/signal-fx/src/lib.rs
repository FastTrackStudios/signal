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

mod factory;
pub use factory::NativeFxFactory;

// ── Param helpers ──────────────────────────────────────────────────────────

/// Hard cap on time-effect (reverb/delay) dry-wet mix for now (10%).
/// Full wet/dry travel — the merged MX/reverb engines are pedal-parity, so
/// the mix knob covers the whole range (defaults stay subtle).
const TIME_MIX_MAX: f64 = 1.0;

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

/// Live compressor telemetry — the rolling waveform + GR the FTS-Comp
/// editor shows. A single global ring: only the *active* chain's comp
/// processes audio, so whichever instance is running owns the meters
/// (multiband/per-instance telemetry comes with real per-block channels).
pub mod comp_meter {
    use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    /// Ring length — 960 slots at ~240 Hz ≈ a 4-second window (matches the
    /// FTS-Comp editor).
    pub const WAVE_LEN: usize = 960;
    /// Samples between ring writes (~240 Hz at 48 kHz).
    pub const WAVE_INTERVAL: usize = 200;
    /// GR normalization for the ring (dB full scale).
    pub const GR_FS_DB: f32 = 30.0;

    static WAVE_IN: [AtomicU32; WAVE_LEN] = [const { AtomicU32::new(0) }; WAVE_LEN];
    static WAVE_GR: [AtomicU32; WAVE_LEN] = [const { AtomicU32::new(0) }; WAVE_LEN];
    static POS: AtomicUsize = AtomicUsize::new(0);
    static GR_DB: AtomicU32 = AtomicU32::new(0);

    pub(crate) fn push(input_peak: f32, gr_norm: f32) {
        let pos = POS.load(Ordering::Relaxed) % WAVE_LEN;
        WAVE_IN[pos].store(input_peak.to_bits(), Ordering::Relaxed);
        WAVE_GR[pos].store(gr_norm.to_bits(), Ordering::Relaxed);
        POS.store(pos + 1, Ordering::Relaxed);
    }

    pub(crate) fn set_gr_db(gr: f32) {
        GR_DB.store(gr.to_bits(), Ordering::Relaxed);
    }

    /// Current gain reduction (dB, positive = reducing) — straight from the
    /// DSP's detector.
    pub fn gr_db() -> f32 {
        f32::from_bits(GR_DB.load(Ordering::Relaxed))
    }

    /// Snapshot the ring in time order (oldest → newest), downsampled by
    /// `stride`. Returns `(input_peaks 0..1, gr 0..1)`.
    ///
    /// Downsample groups are anchored to **absolute ring slots** (not the
    /// write position): a display column always summarises the same
    /// samples until they scroll out, so the trace crawls smoothly instead
    /// of shimmering as the head moves through a group.
    pub fn wave_snapshot(stride: usize) -> (Vec<f32>, Vec<f32>) {
        let stride = stride.max(1);
        let n_groups = WAVE_LEN / stride;
        let pos = POS.load(Ordering::Relaxed) % WAVE_LEN;
        // First complete group after the write head (the head's own group
        // mixes oldest and newest data — skip it).
        let g0 = (pos / stride + 1) % n_groups;
        let mut input = Vec::with_capacity(n_groups - 1);
        let mut gr = Vec::with_capacity(n_groups - 1);
        for k in 0..n_groups - 1 {
            let g = (g0 + k) % n_groups;
            let (mut pi, mut pg) = (0.0f32, 0.0f32);
            for j in 0..stride {
                let idx = g * stride + j;
                pi = pi.max(f32::from_bits(WAVE_IN[idx].load(Ordering::Relaxed)));
                pg = pg.max(f32::from_bits(WAVE_GR[idx].load(Ordering::Relaxed)));
            }
            input.push(pi);
            gr.push(pg);
        }
        (input, gr)
    }
}

/// Global DI sidechain — the rig's input probe publishes the clean guitar
/// peak per block; the gate keys off it so gating stays tight regardless
/// of what the tone (amp/EQ/drive) does to the signal it actually gates.
pub mod sidechain {
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    static PEAK: AtomicU32 = AtomicU32::new(0);
    static ENABLED: AtomicBool = AtomicBool::new(false);

    /// Publish the current DI block peak (linear). Enables the sidechain.
    pub fn set_peak(peak: f32) {
        PEAK.store(peak.to_bits(), Ordering::Relaxed);
        ENABLED.store(true, Ordering::Relaxed);
    }

    /// The DI peak, if a probe is publishing.
    pub fn peak() -> Option<f32> {
        ENABLED
            .load(Ordering::Relaxed)
            .then(|| f32::from_bits(PEAK.load(Ordering::Relaxed)))
    }
}

const COMP_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "threshold", min: -60.0, max: 0.0, default: -18.0 },
    ParamSpec { id: 1, name: "ratio", min: 1.0, max: 20.0, default: 4.0 },
    ParamSpec { id: 2, name: "attack", min: 0.1, max: 200.0, default: 10.0 },
    ParamSpec { id: 3, name: "release", min: 5.0, max: 1000.0, default: 120.0 },
    ParamSpec { id: 4, name: "knee", min: 0.0, max: 24.0, default: 6.0 },
    ParamSpec { id: 5, name: "range", min: 0.0, max: 60.0, default: 60.0 },
    ParamSpec { id: 6, name: "fold", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 7, name: "style", min: 0.0, max: 4.0, default: 0.0 },
];

/// Native Compressor block — wraps [`comp::ProC3Compressor`] (ProC3-style).
/// Seeded with a musical default (−18 dB / 4:1).
pub struct NativeComp {
    comp: comp::ProC3Compressor,
    prepared: bool,
    // Waveform decimation state (comp_meter ring).
    wave_counter: usize,
    wave_peak: f32,
    wave_gr_peak: f32,
}

impl NativeComp {
    pub fn new(sample_rate: f64) -> Self {
        let mut comp = comp::ProC3Compressor::new(sample_rate.max(1.0));
        comp.set_threshold(-18.0);
        comp.set_ratio(4.0);
        comp.set_attack_ms(10.0);
        comp.set_release_ms(120.0);
        Self {
            comp,
            prepared: false,
            wave_counter: 0,
            wave_peak: 0.0,
            wave_gr_peak: 0.0,
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.comp.set_threshold(v),
            1 => self.comp.set_ratio(v),
            2 => self.comp.set_attack_ms(v),
            3 => self.comp.set_release_ms(v),
            4 => self.comp.set_knee(v),
            5 => self.comp.set_range_db(v),
            6 => self.comp.set_fold(v),
            7 => self.comp.set_style(v as i32),
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
            // Telemetry: rolling input peak + GR ring (see `comp_meter`).
            let in_peak = in_l[i].abs().max(in_r[i].abs());
            self.wave_peak = self.wave_peak.max(in_peak);
            self.wave_counter += 1;
            if self.wave_counter >= comp_meter::WAVE_INTERVAL {
                let gr = self.comp.gain_reduction_db() as f32;
                self.wave_gr_peak = gr / comp_meter::GR_FS_DB;
                comp_meter::push(self.wave_peak.min(1.0), self.wave_gr_peak.clamp(0.0, 1.0));
                comp_meter::set_gr_db(gr);
                self.wave_counter = 0;
                self.wave_peak = 0.0;
            }
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Leveler ────────────────────────────────────────────────────────────────

const LEVEL_PARAMS: &[ParamSpec] = &[
    // Gate.
    ParamSpec { id: 0, name: "gate_threshold", min: -80.0, max: 0.0, default: -45.0 },
    ParamSpec { id: 1, name: "gate_range", min: -80.0, max: 0.0, default: -80.0 },
    ParamSpec { id: 2, name: "gate_attack", min: 0.1, max: 200.0, default: 1.0 },
    ParamSpec { id: 3, name: "gate_release", min: 5.0, max: 1000.0, default: 120.0 },
    // De-breath.
    ParamSpec { id: 4, name: "debreath_reduction", min: 0.0, max: 40.0, default: 12.0 },
    ParamSpec { id: 5, name: "debreath_max_level", min: -60.0, max: 0.0, default: -28.0 },
    // Rider.
    ParamSpec { id: 6, name: "ride_target", min: -40.0, max: 0.0, default: -18.0 },
    ParamSpec { id: 7, name: "ride_amount", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 8, name: "ride_max_gain", min: 0.0, max: 24.0, default: 12.0 },
    ParamSpec { id: 9, name: "ride_max_cut", min: 0.0, max: 24.0, default: 18.0 },
    // De-ess.
    ParamSpec { id: 10, name: "deess_freq", min: 2000.0, max: 12000.0, default: 6500.0 },
    ParamSpec { id: 11, name: "deess_threshold", min: -60.0, max: 0.0, default: -30.0 },
    ParamSpec { id: 12, name: "deess_ratio", min: 1.0, max: 20.0, default: 4.0 },
    ParamSpec { id: 13, name: "deess_range", min: 0.0, max: 40.0, default: 12.0 },
    // Shared adaptive silence floor.
    ParamSpec { id: 14, name: "silence", min: -90.0, max: -20.0, default: -45.0 },
];

/// Native Leveler block — wraps [`level_dsp::VocalLeveler`], the realtime vocal
/// chain (gate → de-breath → ride → de-ess). The core is mono, so L/R each get
/// their own leveler instance (independent state). Every stage parameter is an
/// enumerable, macro/modulation-targetable [`PluginParamInfo`].
pub struct NativeLevel {
    left: level_dsp::VocalLeveler,
    right: level_dsp::VocalLeveler,
    gate: level_dsp::GateConfig,
    debreath: level_dsp::DeBreathConfig,
    rider: level_dsp::RiderConfig,
    deess: level_dsp::DeEssConfig,
    silence_db: f64,
    /// A param changed this block → re-push stage configs before processing.
    dirty: bool,
    prepared: bool,
}

impl NativeLevel {
    pub fn new(sample_rate: f64) -> Self {
        let cfg = level_dsp::LevelerConfig::default();
        let sr = sample_rate.max(1.0);
        Self {
            left: level_dsp::VocalLeveler::new(sr, cfg),
            right: level_dsp::VocalLeveler::new(sr, cfg),
            gate: cfg.gate.unwrap_or_default(),
            debreath: cfg.debreath.unwrap_or_default(),
            rider: cfg.rider.unwrap_or_default(),
            deess: cfg.deess.unwrap_or_default(),
            silence_db: cfg.silence_db,
            dirty: false,
            prepared: false,
        }
    }

    fn config(&self) -> level_dsp::LevelerConfig {
        level_dsp::LevelerConfig {
            gate: Some(self.gate),
            debreath: Some(self.debreath),
            rider: Some(self.rider),
            deess: Some(self.deess),
            classify: level_dsp::LevelerConfig::default().classify,
            silence_db: self.silence_db,
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.gate.threshold_db = v,
            1 => self.gate.range_db = v,
            2 => self.gate.attack_ms = v,
            3 => self.gate.release_ms = v,
            4 => self.debreath.reduction_db = v,
            5 => self.debreath.max_level_db = v,
            6 => self.rider.target_db = v,
            7 => self.rider.amount = v,
            8 => self.rider.max_gain_db = v,
            9 => self.rider.max_cut_db = v,
            10 => self.deess.crossover_hz = v,
            11 => self.deess.threshold_db = v,
            12 => self.deess.ratio = v,
            13 => self.deess.range_db = v,
            14 => self.silence_db = v,
            _ => return,
        }
        self.dirty = true;
    }

    /// Apply a build-time parameter by name (`gate_threshold`, `ride_target`, …).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(LEVEL_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeLevel {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.level", "Leveler")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(LEVEL_PARAMS)
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
        let cfg = self.config();
        let sr = sample_rate.max(1.0);
        self.left = level_dsp::VocalLeveler::new(sr, cfg);
        self.right = level_dsp::VocalLeveler::new(sr, cfg);
        self.dirty = false;
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
        if self.dirty {
            // Audio-thread-safe, in-place (no allocation) — see VocalLeveler.
            for lv in [&mut self.left, &mut self.right] {
                lv.set_stage_configs(self.gate, self.debreath, self.rider, self.deess);
                lv.set_silence_db(self.silence_db);
            }
            self.dirty = false;
        }
        let n = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        for i in 0..n {
            out_l[i] = self.left.process_sample(in_l[i] as f64) as f32;
            out_r[i] = self.right.process_sample(in_r[i] as f64) as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Reverb ─────────────────────────────────────────────────────────────────

const REVERB_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "mix", min: 0.0, max: 1.0, default: 0.08 },
    ParamSpec { id: 1, name: "decay", min: 0.0, max: 1.0, default: 0.45 },
    ParamSpec { id: 2, name: "size", min: 0.0, max: 1.0, default: 0.5 },
    // BigSky MX dual-reverb block: routing (0 Single / 1 Series 1>2 /
    // 2 Series 2>1 / 3 Parallel / 4 Split / 5 Split Swap) + reverb B.
    // Ids 0-2 keep addressing reverb A.
    ParamSpec { id: 3, name: "routing", min: 0.0, max: 5.0, default: 0.0 },
    ParamSpec { id: 4, name: "algo_b", min: 0.0, max: 14.0, default: 2.0 },
    ParamSpec { id: 5, name: "decay_b", min: 0.0, max: 1.0, default: 0.45 },
    ParamSpec { id: 6, name: "mix_b", min: 0.0, max: 0.10, default: 0.08 },
    // Per-slot wet pan (-1..+1) and wet tremolo (shared A knob set).
    ParamSpec { id: 7, name: "pan_a", min: -1.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 8, name: "pan_b", min: -1.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 9, name: "trem_rate", min: 0.1, max: 12.0, default: 4.0 },
    ParamSpec { id: 10, name: "trem_depth", min: 0.0, max: 1.0, default: 0.0 },
    // BigSky MX Impulse live params (chain A; active with the
    // Convolution algorithm). tail: 0 = Envelope, 1 = Gate;
    // direction: 0 = Forward, 1 = Reverse.
    ParamSpec { id: 11, name: "imp_decay", min: 0.01, max: 1.0, default: 1.0 },
    ParamSpec { id: 12, name: "imp_tail", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 13, name: "imp_attack", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 14, name: "imp_stretch", min: 0.25, max: 4.0, default: 1.0 },
    ParamSpec { id: 15, name: "imp_direction", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 16, name: "imp_feedback", min: 0.0, max: 1.0, default: 0.0 },
    // BigSky MX Shimmer (chain A): two shift voices in semitones,
    // shared amount, feedback mode (0 Input / 1 Regenerative /
    // 2 Input+Regen). shim_voice2: 0 = single voice, 1 = dual.
    ParamSpec { id: 17, name: "shim_shift1", min: -12.0, max: 12.0, default: 12.0 },
    ParamSpec { id: 18, name: "shim_shift2", min: -12.0, max: 12.0, default: 7.0 },
    ParamSpec { id: 19, name: "shim_voice2", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 20, name: "shim_amount", min: 0.0, max: 1.0, default: 0.35 },
    ParamSpec { id: 21, name: "shim_fb_mode", min: 0.0, max: 2.0, default: 1.0 },
    // BigSky MX Magneto: taps alternate hard L/R.
    ParamSpec { id: 22, name: "mag_ping_pong", min: 0.0, max: 1.0, default: 0.0 },
    // BigSky MX NonLinear: Chop trem on the decay, explicit gate speed,
    // separate Late reverb stage.
    ParamSpec { id: 23, name: "nl_chop_rate", min: 0.1, max: 15.0, default: 4.0 },
    ParamSpec { id: 24, name: "nl_chop_depth", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 25, name: "nl_gate_speed", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 26, name: "nl_late_speed", min: 0.0, max: 1.0, default: 0.5 },
    ParamSpec { id: 27, name: "nl_late_decay", min: 0.0, max: 1.0, default: 0.5 },
    ParamSpec { id: 28, name: "nl_late_level", min: 0.0, max: 1.0, default: 0.0 },
    // BigSky MX input-analysis generators: Cloud Ensemble (pitch-tracked
    // synthetic string layer), Bloom Harmonics (overtone generator on
    // the trail), Chorale Choir level / Voice (0 Tenor / 1 Soprano) /
    // Mod (per-voice randomization).
    ParamSpec { id: 29, name: "cloud_ensemble", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 30, name: "bloom_harmonics", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 31, name: "cho_choir", min: 0.0, max: 1.0, default: 0.3 },
    ParamSpec { id: 32, name: "cho_voice", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 33, name: "cho_mod", min: 0.0, max: 1.0, default: 0.0 },
    // BigSky MX voices + Hall extras + named-size select (chain A).
    // voice: 0 MX / 1 Classic. hall_swell_type: 0 wet / 1 wet+dry.
    // size_sel maps named sizes (Hall Concert/Arena, Room Studio/Club,
    // else Small/Medium/Large) — applies when set.
    ParamSpec { id: 34, name: "voice", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 35, name: "hall_mid", min: -6.0, max: 6.0, default: 0.0 },
    ParamSpec { id: 36, name: "hall_swell_rise", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 37, name: "hall_swell_type", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 38, name: "size_sel", min: 0.0, max: 2.0, default: 0.0 },
    // Chain-A tone/space controls (kept from the control-surface work,
    // renumbered above the MX block).
    // Algorithm index (see reverb::AlgorithmType::ALL).
    ParamSpec { id: 39, name: "algorithm", min: 0.0, max: 14.0, default: 1.0 },
    ParamSpec { id: 40, name: "modulation", min: 0.0, max: 1.0, default: 0.2 },
    // High-frequency damping — the low-pass character control.
    ParamSpec { id: 41, name: "damping", min: 0.0, max: 1.0, default: 0.3 },
    // Tone tilt (−1 dark … +1 bright) — the high-pass-ish control.
    ParamSpec { id: 42, name: "tone", min: -1.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 43, name: "predelay", min: 0.0, max: 200.0, default: 0.0 },
];

/// Native Reverb block — wraps [`reverb::DualReverb`] (two full chains +
/// BigSky MX dual routing; `Single` = chain A only, bit-compatible with
/// the previous single-chain wrapper). Defaults to a subtle Hall (low
/// mix) so it sits under the tone rather than washing it out.
pub struct NativeReverb {
    rev: reverb::DualReverb,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeReverb {
    pub fn new(_sample_rate: f64) -> Self {
        let mut rev = reverb::DualReverb::new();
        rev.a.set_algorithm(reverb::AlgorithmType::Hall);
        rev.a.mix = 0.08;
        rev.a.params.decay = 0.45;
        rev.a.params.size = 0.5;
        rev.a.update_params();
        // B seeds as a plate so engaging a dual routing is immediately
        // audible before any params are set.
        rev.b.set_algorithm(reverb::AlgorithmType::Plate);
        rev.b.mix = 0.08;
        rev.b.params.decay = 0.45;
        rev.b.update_params();
        Self {
            rev,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        // Ids 0-2 address reverb A; 3+ are the dual-routing block.
        match id {
            0 => self.rev.a.mix = v.min(TIME_MIX_MAX),
            1 => {
                self.rev.a.params.decay = v;
                self.rev.a.update_params();
            }
            2 => {
                self.rev.a.params.size = v;
                self.rev.a.update_params();
            }
            3 => {
                self.rev.routing =
                    reverb::DualRouting::from_index(v.round().max(0.0) as usize)
            }
            4 => self
                .rev
                .b
                .set_algorithm(reverb::AlgorithmType::from_index(
                    v.round().max(0.0) as usize,
                )),
            5 => {
                self.rev.b.params.decay = v;
                self.rev.b.update_params();
            }
            6 => self.rev.b.mix = v.min(TIME_MIX_MAX),
            7 => self.rev.a.pan = v.clamp(-1.0, 1.0),
            8 => self.rev.b.pan = v.clamp(-1.0, 1.0),
            9 => {
                self.rev.a.trem_rate_hz = v;
                self.rev.b.trem_rate_hz = v;
            }
            10 => {
                self.rev.a.trem_depth = v.clamp(0.0, 1.0);
                self.rev.b.trem_depth = v.clamp(0.0, 1.0);
            }
            11 => {
                self.rev.a.impulse.decay = v.clamp(0.01, 1.0);
                self.rev.a.update_params();
            }
            12 => {
                self.rev.a.impulse.tail = if v >= 0.5 {
                    reverb::ImpulseTail::Gate
                } else {
                    reverb::ImpulseTail::Envelope
                };
                self.rev.a.update_params();
            }
            13 => {
                self.rev.a.impulse.attack = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            14 => {
                self.rev.a.impulse.stretch = v.clamp(0.25, 4.0);
                self.rev.a.update_params();
            }
            15 => {
                self.rev.a.impulse.direction = if v >= 0.5 {
                    reverb::ImpulseDirection::Reverse
                } else {
                    reverb::ImpulseDirection::Forward
                };
                self.rev.a.update_params();
            }
            16 => {
                self.rev.a.impulse.feedback = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            17 => {
                self.rev.a.shimmer.shift1_semitones = Some(v.clamp(-12.0, 12.0));
                self.rev.a.update_params();
            }
            18 => {
                self.rev.a.shimmer.shift2_semitones = Some(v.clamp(-12.0, 12.0));
                self.rev.a.update_params();
            }
            19 => {
                self.rev.a.shimmer.voice2 = v >= 0.5;
                self.rev.a.update_params();
            }
            20 => {
                self.rev.a.shimmer.amount = Some(v.clamp(0.0, 1.0));
                self.rev.a.update_params();
            }
            21 => {
                self.rev.a.shimmer.feedback_mode =
                    reverb::ShimmerFeedbackMode::from_index(v.round().max(0.0) as usize);
                self.rev.a.update_params();
            }
            22 => {
                self.rev.a.magneto.ping_pong = v >= 0.5;
                self.rev.a.update_params();
            }
            23 => {
                self.rev.a.nonlinear.chop_rate_hz = v.clamp(0.1, 15.0);
                self.rev.a.update_params();
            }
            24 => {
                self.rev.a.nonlinear.chop_depth = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            25 => {
                self.rev.a.nonlinear.gate_speed = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            26 => {
                self.rev.a.nonlinear.late_speed = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            27 => {
                self.rev.a.nonlinear.late_decay = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            28 => {
                self.rev.a.nonlinear.late_level = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            29 => {
                self.rev.a.cloud.ensemble = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            30 => {
                self.rev.a.bloom.harmonics = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            31 => {
                self.rev.a.chorale.choir_level = Some(v.clamp(0.0, 1.0));
                self.rev.a.update_params();
            }
            32 => {
                self.rev.a.chorale.voice = if v >= 0.5 {
                    reverb::ChoirVoice::Soprano
                } else {
                    reverb::ChoirVoice::Tenor
                };
                self.rev.a.update_params();
            }
            33 => {
                self.rev.a.chorale.mod_amount = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            34 => {
                self.rev.a.voice = if v >= 0.5 {
                    reverb::ReverbVoice::Classic
                } else {
                    reverb::ReverbVoice::Mx
                };
                self.rev.a.update_params();
            }
            35 => {
                self.rev.a.hall.mid_db = v.clamp(-6.0, 6.0);
                self.rev.a.update_params();
            }
            36 => {
                self.rev.a.hall.swell_rise = v.clamp(0.0, 1.0);
                self.rev.a.update_params();
            }
            37 => {
                self.rev.a.hall.swell_type = if v >= 0.5 {
                    reverb::SwellType::WetPlusDry
                } else {
                    reverb::SwellType::Wet
                };
                self.rev.a.update_params();
            }
            38 => {
                self.rev.a.set_size_index(v.round().max(0.0) as usize);
            }
            // Chain-A tone/space controls (39+ — see REVERB_PARAMS).
            39 => {
                self.rev
                    .a
                    .set_algorithm(reverb::AlgorithmType::from_index(v.round().max(0.0) as usize));
                self.rev.a.update_params();
            }
            40 => {
                self.rev.a.params.modulation = v;
                self.rev.a.update_params();
            }
            41 => {
                self.rev.a.params.damping = v;
                self.rev.a.update_params();
            }
            42 => {
                self.rev.a.params.tone = v;
                self.rev.a.update_params();
            }
            43 => self.rev.a.predelay_ms = v,
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (see [`REVERB_PARAMS`]).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(REVERB_PARAMS, name) {
            self.set(id, value);
        }
    }

    /// Load a custom impulse response from a wav file into chain A's
    /// convolution engine (switching it to the Convolution algorithm).
    /// Mono files duplicate to both sides; samples normalize to f64.
    pub fn load_ir_wav(&mut self, path: &str) -> bool {
        let Ok(mut reader) = hound::WavReader::open(path) else {
            return false;
        };
        let spec = reader.spec();
        let to_f64: Vec<f64> = match spec.sample_format {
            hound::SampleFormat::Float => {
                reader.samples::<f32>().filter_map(Result::ok).map(|s| s as f64).collect()
            }
            hound::SampleFormat::Int => {
                let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f64;
                reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f64 * scale).collect()
            }
        };
        if to_f64.is_empty() {
            return false;
        }
        let (l, r): (Vec<f64>, Vec<f64>) = if spec.channels >= 2 {
            let ch = spec.channels as usize;
            (
                to_f64.iter().step_by(ch).copied().collect(),
                to_f64.iter().skip(1).step_by(ch).copied().collect(),
            )
        } else {
            (to_f64.clone(), to_f64)
        };
        self.rev.a.set_algorithm(reverb::AlgorithmType::Convolution);
        let ok = self.rev.a.load_convolution_ir(&l, &r);
        self.rev.a.update_params();
        ok
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
    ParamSpec { id: 0, name: "mix", min: 0.0, max: 1.0, default: 0.08 },
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
    ParamSpec { id: 21, name: "mix_b", min: 0.0, max: 1.0, default: 0.08 },
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
    // Per-line tap divisions + line pan (the rig's delay surface
    // addresses these by name; id 7 "tap_div" still sets both lines at
    // once). pan: one knob places BOTH lines (-1 hard L .. +1 hard R,
    // 0 = centered); the engine's own default keeps the classic
    // hard-L/R split until the param is touched.
    ParamSpec { id: 35, name: "tap_div_l", min: 0.0, max: 7.0, default: 7.0 },
    ParamSpec { id: 36, name: "tap_div_r", min: 0.0, max: 7.0, default: 7.0 },
    ParamSpec { id: 37, name: "pan", min: -1.0, max: 1.0, default: 0.0 },
];

/// Native Delay block — wraps [`delay::DualDelay`] (two full chains +
/// TimeLine MX 1+2 routing; `Single` = chain A only, bit-compatible with
/// the previous single-chain wrapper). Defaults to a subtle clean
/// quarter-note-ish delay with modest feedback.
pub struct NativeDelay {
    dly: delay::DualDelay,
    prepared: bool,
    sample_rate: f64,
    block_size: usize,
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
            sample_rate: 48000.0,
            block_size: 512,
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
            35 => a.tap_div_l = delay::TapDivision::from_index(v.round().max(0.0) as usize),
            36 => a.tap_div_r = delay::TapDivision::from_index(v.round().max(0.0) as usize),
            37 => {
                let p = v.clamp(-1.0, 1.0);
                a.pan_l = p;
                a.pan_r = p;
            }
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
        self.sample_rate = sample_rate.max(1.0);
        self.block_size = block_size.max(1) as usize;
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
        if !events.params.is_empty() {
            for &(id, value) in events.params {
                self.set(id, value);
            }
            // Param writes only set fields; tempo-synced times, style
            // ranges, and head modes are derived in update() — re-run it
            // or tap divisions and tap tempo silently do nothing.
            self.dly.update(AudioConfig {
                sample_rate: self.sample_rate,
                max_buffer_size: self.block_size.max(1),
            });
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
    // Engine (algorithm): 0 Cubic / 1 BBD / 2 Tape / 3 Orbit / 4 Juno.
    ParamSpec { id: 3, name: "engine", min: 0.0, max: 4.0, default: 0.0 },
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
            3 => {
                use modulation::chorus::engine::EngineType;
                self.ch.set_engine(match v.round().max(0.0) as u32 {
                    1 => EngineType::Bbd,
                    2 => EngineType::Tape,
                    3 => EngineType::Orbit,
                    4 => EngineType::Juno,
                    _ => EngineType::Cubic,
                });
            }
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

const TREM_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "depth", min: 0.0, max: 1.0, default: 0.5 },
    ParamSpec { id: 1, name: "mix", min: 0.0, max: 1.0, default: 1.0 },
    // Free-running LFO rate (the trigger engine's free mode).
    ParamSpec { id: 2, name: "rate", min: 0.05, max: 12.0, default: 4.0 },
    // Mode (algorithm): 0 Mono / 1 Stereo / 2 Harmonic.
    ParamSpec { id: 3, name: "mode", min: 0.0, max: 2.0, default: 1.0 },
];

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
        // The chain defaults to transport-synced triggering, but the rig
        // has no transport — the LFO would freeze. Free-run at 4 Hz.
        tr.modulator.trigger.mode = modulation::trem::fts_modulation::trigger::TriggerMode::Free;
        tr.modulator.trigger.sync_index = 0;
        tr.modulator.trigger.rate_hz = 4.0;
        tr.mix = 1.0;
        Self {
            tr,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.tr.set_depth(v),
            1 => self.tr.mix = v.clamp(0.0, 1.0),
            2 => {
                // Free-running rate: force the trigger engine out of sync.
                self.tr.modulator.trigger.mode = modulation::trem::fts_modulation::trigger::TriggerMode::Free;
                self.tr.modulator.trigger.sync_index = 0;
                self.tr.modulator.trigger.rate_hz = v.max(0.01);
            }
            3 => self.tr.set_mode(match v.round().max(0.0) as u32 {
                0 => TremMode::Mono,
                2 => TremMode::Harmonic,
                _ => TremMode::Stereo,
            }),
            _ => {}
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
        // Detector source: the clean DI sidechain when the rig publishes
        // one (post-amp placement, input-accurate gating), else the block's
        // own input.
        let di = sidechain::peak().map(|p| p as f64);
        for i in 0..out_l.len() {
            let l = in_l.get(i).copied().unwrap_or(0.0);
            let r = in_r.get(i).copied().unwrap_or(0.0);
            let peak = di.unwrap_or_else(|| l.abs().max(r.abs()) as f64);
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

#[cfg(test)]
mod param_table_tests {
    use super::*;

    /// Duplicate ids silently shadow match arms in each block's `set()`
    /// (a duplicated delay id once routed "spread" writes to a tap
    /// division); duplicate names make `set_named` ambiguous.
    #[test]
    fn param_tables_have_unique_ids_and_names() {
        for (label, table) in [
            ("COMP_PARAMS", COMP_PARAMS),
            ("REVERB_PARAMS", REVERB_PARAMS),
            ("DELAY_PARAMS", DELAY_PARAMS),
            ("MOD_PARAMS", MOD_PARAMS),
            ("TREM_PARAMS", TREM_PARAMS),
            ("GAIN_PARAMS", GAIN_PARAMS),
            ("GATE_PARAMS", GATE_PARAMS),
        ] {
            let mut ids: Vec<u32> = table.iter().map(|p| p.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), table.len(), "{label}: duplicate param id");

            let mut names: Vec<&str> = table.iter().map(|p| p.name).collect();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), table.len(), "{label}: duplicate param name");
        }
    }

    /// The rig surfaces address these by name — they must resolve.
    #[test]
    fn rig_surface_names_resolve() {
        for name in ["mix", "time", "feedback", "pan", "tap_div_l", "tap_div_r"] {
            assert!(
                param_id(DELAY_PARAMS, name).is_some(),
                "delay param {name:?} missing from DELAY_PARAMS"
            );
        }
        for name in ["mix", "decay", "size", "algorithm", "modulation", "damping", "tone"] {
            assert!(
                param_id(REVERB_PARAMS, name).is_some(),
                "reverb param {name:?} missing from REVERB_PARAMS"
            );
        }
    }
}
