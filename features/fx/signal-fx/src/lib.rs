//! Built-in FX for signal.
//!
//! Thin [`PluginInstance`] wrappers over the built-in FX facades (`eq`, `comp`,
//! `reverb`, `delay`) so signal's FX chain can host them as native blocks — no
//! CLAP/VST3 hosting, no GUI framework. Each wrapper adapts a DSP `Chain`/
//! processor to the daw `PluginInstance` contract (`prepare` / `process_block`
//! / params) and exposes a small controllable parameter set.
//!
//! **Param wiring.** Each wrapper declares a `ParamSpec` table (stable id +
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

// Appended param blocks (ids are APPEND-ONLY — the 0..143 layout is
// frozen by saved rig patches):
/// `b{i}_slope` — canonical slope index 0..10 (eq_dsp::Slope table).
pub const EQ_SLOPE_BASE: u32 = 144;
/// `output_gain` dB.
pub const EQ_OUTPUT_GAIN_ID: u32 = 168;
/// `gain_scale` — scales every band gain (ZL-style, 1.0 = neutral).
pub const EQ_GAIN_SCALE_ID: u32 = 169;
/// `b{i}_dyn_range` dB (0 = static band).
pub const EQ_DYN_RANGE_BASE: u32 = 176;
/// `b{i}_dyn_thr` dB (auto flag separate).
pub const EQ_DYN_THR_BASE: u32 = 200;
/// `b{i}_dyn_atk` percent (50 = auto).
pub const EQ_DYN_ATK_BASE: u32 = 224;
/// `b{i}_dyn_rel` percent (50 = auto).
pub const EQ_DYN_REL_BASE: u32 = 248;
/// `b{i}_dyn_auto` 0/1 (learned threshold).
pub const EQ_DYN_AUTO_BASE: u32 = 272;
/// `b{i}_dyn_relative` 0/1 (band-vs-program detection).
pub const EQ_DYN_RELATIVE_BASE: u32 = 296;
/// `b{i}_placement` — 0 Stereo, 1 Left, 2 Right, 3 Mid, 4 Side.
pub const EQ_PLACEMENT_BASE: u32 = 320;
// ── Transient (SplitEQ-style dual-stream) mode, engine-level ──
/// `transient_mode` 0/1 — the whole EQ splits into transient/steady
/// streams; bands then process per their `b{i}_stream` assignment.
pub const EQ_TRANSIENT_MODE_ID: u32 = 170;
/// `split_balance` −50..50, `split_attack`/`split_hold`/`split_smooth`
/// 0..100, `split_solo` 0 none / 1 transient / 2 steady.
pub const EQ_SPLIT_BALANCE_ID: u32 = 171;
pub const EQ_SPLIT_ATTACK_ID: u32 = 172;
pub const EQ_SPLIT_HOLD_ID: u32 = 173;
pub const EQ_SPLIT_SMOOTH_ID: u32 = 174;
pub const EQ_SPLIT_SOLO_ID: u32 = 175;
/// `b{i}_stream` — 0 Both, 1 Transient, 2 Steady (transient mode).
pub const EQ_STREAM_BASE: u32 = 344;
/// `transient_gain` / `steady_gain` dB (transient mode masters).
pub const EQ_TRANSIENT_GAIN_ID: u32 = 368;
pub const EQ_STEADY_GAIN_ID: u32 = 369;
/// `b{i}_spectral` 0/1 — the band's dynamics act per-bin (Pro-Q
/// Spectral): its freq/Q footprint + dyn range/threshold drive the
/// shared spectral engine instead of the whole-band gain ride.
pub const EQ_SPECTRAL_BASE: u32 = 376;
/// `b{i}_listen` — 0 off, 1 solo the band's frequency region,
/// 2 delta (hear only what this EQ changes). One band at a time (the
/// most recent non-zero wins); composes with `split_solo` so you can
/// hear e.g. just the transients of the soloed region.
pub const EQ_LISTEN_BASE: u32 = 400;
/// One past the last EQ param id.
pub const EQ_PARAM_COUNT: u32 = 424;

/// Canonical shape conversion — [`eq::slope::FilterShape`] owns the
/// one true ordering (append-only, documented there).
fn eq_shape_to_filter(shape: u32) -> eq::FilterType {
    eq::slope::FilterShape::from_canonical_index(shape).to_filter_type()
}

/// Param name for `(band, field)` — `b{band+1}_{used|on|freq|gain|q|shape}`.
pub fn eq_param_name(band: usize, field: usize) -> String {
    let f = ["used", "on", "freq", "gain", "q", "shape"][field];
    format!("b{}_{}", band + 1, f)
}

const EQ_EXT_FIELDS: [(&str, u32); 11] = [
    ("listen", EQ_LISTEN_BASE),
    ("slope", EQ_SLOPE_BASE),
    ("placement", EQ_PLACEMENT_BASE),
    ("stream", EQ_STREAM_BASE),
    ("spectral", EQ_SPECTRAL_BASE),
    ("dyn_range", EQ_DYN_RANGE_BASE),
    ("dyn_thr", EQ_DYN_THR_BASE),
    ("dyn_atk", EQ_DYN_ATK_BASE),
    ("dyn_rel", EQ_DYN_REL_BASE),
    ("dyn_auto", EQ_DYN_AUTO_BASE),
    ("dyn_relative", EQ_DYN_RELATIVE_BASE),
];

fn eq_param_id_of(name: &str) -> Option<u32> {
    match name {
        "output_gain" => return Some(EQ_OUTPUT_GAIN_ID),
        "gain_scale" => return Some(EQ_GAIN_SCALE_ID),
        "transient_mode" => return Some(EQ_TRANSIENT_MODE_ID),
        "split_balance" => return Some(EQ_SPLIT_BALANCE_ID),
        "split_attack" => return Some(EQ_SPLIT_ATTACK_ID),
        "split_hold" => return Some(EQ_SPLIT_HOLD_ID),
        "split_smooth" => return Some(EQ_SPLIT_SMOOTH_ID),
        "split_solo" => return Some(EQ_SPLIT_SOLO_ID),
        "transient_gain" => return Some(EQ_TRANSIENT_GAIN_ID),
        "steady_gain" => return Some(EQ_STEADY_GAIN_ID),
        _ => {}
    }
    let rest = name.strip_prefix('b')?;
    let (num, field) = rest.split_once('_')?;
    let band: usize = num.parse().ok()?;
    if band == 0 || band > EQ_BANDS {
        return None;
    }
    if let Some(fidx) = ["used", "on", "freq", "gain", "q", "shape"]
        .iter()
        .position(|f| *f == field)
    {
        return Some(((band - 1) * EQ_FIELDS + fidx) as u32);
    }
    EQ_EXT_FIELDS
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, base)| base + (band - 1) as u32)
}

/// Range/default metadata shared by `params()` and the rig tables.
pub fn eq_param_range(id: u32) -> (f64, f64, f64) {
    if id < (EQ_BANDS * EQ_FIELDS) as u32 {
        return match id as usize % EQ_FIELDS {
            0 | 1 => (0.0, 1.0, 0.0),
            2 => (10.0, 30000.0, 1000.0),
            3 => (-30.0, 30.0, 0.0),
            4 => (0.025, 40.0, 0.707),
            _ => (0.0, 12.0, 0.0),
        };
    }
    match id {
        EQ_OUTPUT_GAIN_ID => (-30.0, 30.0, 0.0),
        EQ_GAIN_SCALE_ID => (-1.0, 2.0, 1.0),
        EQ_TRANSIENT_MODE_ID => (0.0, 1.0, 0.0),
        EQ_SPLIT_BALANCE_ID => (-50.0, 50.0, 0.0),
        EQ_SPLIT_ATTACK_ID | EQ_SPLIT_HOLD_ID | EQ_SPLIT_SMOOTH_ID => (0.0, 100.0, 50.0),
        EQ_SPLIT_SOLO_ID => (0.0, 2.0, 0.0),
        EQ_TRANSIENT_GAIN_ID | EQ_STEADY_GAIN_ID => (-30.0, 30.0, 0.0),
        i if (EQ_SLOPE_BASE..EQ_SLOPE_BASE + 24).contains(&i) => (0.0, 10.0, 2.0),
        i if (EQ_DYN_RANGE_BASE..EQ_DYN_RANGE_BASE + 24).contains(&i) => (-30.0, 30.0, 0.0),
        i if (EQ_DYN_THR_BASE..EQ_DYN_THR_BASE + 24).contains(&i) => (-80.0, 0.0, -40.0),
        i if (EQ_DYN_ATK_BASE..EQ_DYN_ATK_BASE + 24).contains(&i) => (0.0, 100.0, 50.0),
        i if (EQ_DYN_REL_BASE..EQ_DYN_REL_BASE + 24).contains(&i) => (0.0, 100.0, 50.0),
        i if (EQ_DYN_AUTO_BASE..EQ_DYN_AUTO_BASE + 24).contains(&i) => (0.0, 1.0, 1.0),
        i if (EQ_DYN_RELATIVE_BASE..EQ_DYN_RELATIVE_BASE + 24).contains(&i) => (0.0, 1.0, 0.0),
        i if (EQ_PLACEMENT_BASE..EQ_PLACEMENT_BASE + 24).contains(&i) => (0.0, 4.0, 0.0),
        i if (EQ_STREAM_BASE..EQ_STREAM_BASE + 24).contains(&i) => (0.0, 2.0, 0.0),
        i if (EQ_SPECTRAL_BASE..EQ_SPECTRAL_BASE + 24).contains(&i) => (0.0, 1.0, 0.0),
        i if (EQ_LISTEN_BASE..EQ_LISTEN_BASE + 24).contains(&i) => (0.0, 2.0, 0.0),
        _ => (0.0, 1.0, 0.0),
    }
}

pub fn eq_param_name_of(id: u32) -> Option<String> {
    if id < (EQ_BANDS * EQ_FIELDS) as u32 {
        let (band, field) = (id as usize / EQ_FIELDS, id as usize % EQ_FIELDS);
        return Some(eq_param_name(band, field));
    }
    match id {
        EQ_OUTPUT_GAIN_ID => return Some("output_gain".into()),
        EQ_GAIN_SCALE_ID => return Some("gain_scale".into()),
        EQ_TRANSIENT_MODE_ID => return Some("transient_mode".into()),
        EQ_SPLIT_BALANCE_ID => return Some("split_balance".into()),
        EQ_SPLIT_ATTACK_ID => return Some("split_attack".into()),
        EQ_SPLIT_HOLD_ID => return Some("split_hold".into()),
        EQ_SPLIT_SMOOTH_ID => return Some("split_smooth".into()),
        EQ_SPLIT_SOLO_ID => return Some("split_solo".into()),
        EQ_TRANSIENT_GAIN_ID => return Some("transient_gain".into()),
        EQ_STEADY_GAIN_ID => return Some("steady_gain".into()),
        _ => {}
    }
    for (f, base) in EQ_EXT_FIELDS {
        if (base..base + 24).contains(&id) {
            return Some(format!("b{}_{}", id - base + 1, f));
        }
    }
    None
}

/// Native EQ block — the full FTS-EQ engine: 24 dynamic bands over
/// [`eq::EqChain`]'s Pro-Q ZPK pipeline, each with
/// used/on/freq/gain/q/shape (all thirteen canonical shapes) + slope,
/// plus per-band DYNAMIC EQ (Pro-Q-style range/threshold/auto) running
/// on the SVF dynamics engine, and output gain / gain-scale masters.
/// Bands start unused → transparent passthrough.
pub struct NativeEq {
    eq: eq::EqChain,
    /// Steady-stream chain (transient mode only; mirrors band configs
    /// per `b{i}_stream` — separate instance = separate filter state).
    eq_b: eq::EqChain,
    splitter: eq::transient::PeakSteadySplitter,
    spectral: eq::spectral::SpectralEngine,
    spectral_regions: Vec<eq::spectral::SpectralRegion>,
    dyn_bands: Vec<eq::dynamics::DynBand>,
    /// (used, on) per band — a band renders only when both are set.
    state: [(bool, bool); EQ_BANDS],
    /// Canonical shape + slope index per band (needed for routing and
    /// effective-order resolution).
    shapes: [u32; EQ_BANDS],
    slopes: [u32; EQ_BANDS],
    placements: [u32; EQ_BANDS],
    streams: [u32; EQ_BANDS],
    spectral_on: [bool; EQ_BANDS],
    transient_mode: bool,
    split_solo: u32,
    transient_gain_db: f64,
    steady_gain_db: f64,
    /// Active listen: (band, mode 1 solo / 2 delta).
    listen: Option<(usize, u32)>,
    solo_filter: eq::dynamics::Svf,
    /// Dry ring for delta listening, latency-aligned with the spectral
    /// engine (max 2048 covers every supported block size).
    dry_ring: [Vec<f64>; 2],
    dry_pos: usize,
    /// Raw dynamic params per band: (range, thr, atk, rel, auto, relative).
    dyn_cfg: [(f64, f64, f64, f64, bool, bool); EQ_BANDS],
    /// Whether the band currently routes through the dynamic engine.
    dyn_active: [bool; EQ_BANDS],
    /// Every param value by id, for host readback.
    values: Vec<f64>,
    output_gain_db: f64,
    gain_scale: f64,
    sample_rate: f64,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    /// Transient-mode stream buffers (steady L/R) — the main scratch
    /// carries the transient stream in place.
    scratch_sl: Vec<f64>,
    scratch_sr: Vec<f64>,
}

impl NativeEq {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let mk_chain = || {
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
            chain
        };
        let chain = mk_chain();
        let chain_b = mk_chain();
        let mut values = vec![0.0; EQ_PARAM_COUNT as usize];
        for id in 0..EQ_PARAM_COUNT {
            values[id as usize] = eq_param_range(id).2;
        }
        Self {
            eq: chain,
            eq_b: chain_b,
            splitter: eq::transient::PeakSteadySplitter::new(sample_rate),
            spectral: eq::spectral::SpectralEngine::new(sample_rate, 1024),
            spectral_regions: Vec::with_capacity(EQ_BANDS),
            dyn_bands: (0..EQ_BANDS)
                .map(|_| {
                    let mut d = eq::dynamics::DynBand::new(sample_rate);
                    d.params.enabled = false;
                    d
                })
                .collect(),
            state: [(false, false); EQ_BANDS],
            shapes: [0; EQ_BANDS],
            slopes: [2; EQ_BANDS],
            placements: [0; EQ_BANDS],
            streams: [0; EQ_BANDS],
            spectral_on: [false; EQ_BANDS],
            transient_mode: false,
            split_solo: 0,
            transient_gain_db: 0.0,
            steady_gain_db: 0.0,
            listen: None,
            solo_filter: eq::dynamics::Svf::new(sample_rate),
            dry_ring: [vec![0.0; 2048], vec![0.0; 2048]],
            dry_pos: 0,
            dyn_cfg: [(0.0, -40.0, 50.0, 50.0, true, false); EQ_BANDS],
            dyn_active: [false; EQ_BANDS],
            values,
            output_gain_db: 0.0,
            gain_scale: 1.0,
            sample_rate,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            scratch_sl: Vec::new(),
            scratch_sr: Vec::new(),
        }
    }

    /// Route + configure one band after any of its params changed.
    fn sync_band(&mut self, band: usize) {
        let (used, on) = self.state[band];
        let enabled = used && on;
        let shape = eq::slope::FilterShape::from_canonical_index(self.shapes[band]);
        let (range, thr, atk, rel, auto, relative) = self.dyn_cfg[band];
        // A band goes dynamic when it has a range and a dynamics-capable
        // shape (Bell/shelves — same rule as Pro-Q).
        let dyn_shape = match shape {
            eq::slope::FilterShape::Bell => Some(eq::dynamics::DynShape::Bell),
            eq::slope::FilterShape::LowShelf => Some(eq::dynamics::DynShape::LowShelf),
            eq::slope::FilterShape::HighShelf => Some(eq::dynamics::DynShape::HighShelf),
            _ => None,
        };
        let spectral = self.spectral_on[band] && range.abs() > 1.0e-3;
        let go_dynamic = enabled && !spectral && range.abs() > 1.0e-3 && dyn_shape.is_some();
        self.dyn_active[band] = go_dynamic;

        let freq = self.values[band * EQ_FIELDS + 2].clamp(10.0, 30000.0);
        let gain = self.values[band * EQ_FIELDS + 3].clamp(-30.0, 30.0) * self.gain_scale;
        let q = self.values[band * EQ_FIELDS + 4].clamp(0.025, 40.0);

        // Stream routing (transient mode): 0 Both, 1 Transient (chain
        // A), 2 Steady (chain B). Outside transient mode chain A takes
        // everything and chain B idles.
        let stream = self.streams[band];
        let in_a = !self.transient_mode || stream != 2;
        let in_b = self.transient_mode && stream != 1;
        for (chain, present) in [(&mut self.eq, in_a), (&mut self.eq_b, in_b)] {
            if let Some(b) = chain.band_mut(band) {
                b.enabled = enabled && !go_dynamic && present;
                b.freq_hz = freq;
                b.gain_db = gain;
                b.q = q;
                b.filter_type = eq_shape_to_filter(self.shapes[band]);
                // effective_order 0 = a 0 dB/oct cut = true bypass.
                let order = shape.effective_order(self.slopes[band] as usize);
                b.order = order.max(1);
                b.enabled = b.enabled && order > 0;
                b.placement = eq::band::Placement::from_index(self.placements[band]);
            }
            chain.update_band(band);
        }
        self.sync_spectral_regions();
        self.sync_listen();

        let d = &mut self.dyn_bands[band];
        d.params.enabled = go_dynamic;
        if go_dynamic {
            d.params.shape = dyn_shape.unwrap_or(eq::dynamics::DynShape::Bell);
            d.params.freq_hz = freq;
            d.params.q = q;
            d.params.base_gain_db = gain;
            d.params.range_db = range * self.gain_scale;
            d.params.placement = eq::band::Placement::from_index(self.placements[band]);
            d.detector.params.threshold_db = if auto { 0.0 } else { thr };
            d.detector.params.auto = auto;
            d.detector.params.relative = relative;
            // Percent knobs around frequency-dependent auto ballistics
            // (lower bands ride slower — Pro-Q's published behavior).
            let base_atk = (5000.0 / freq).clamp(2.0, 120.0);
            let base_rel = base_atk * 5.0;
            d.detector.params.attack_ms =
                base_atk * 8.0f64.powf((atk.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.detector.params.release_ms =
                base_rel * 8.0f64.powf((rel.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.update(self.sample_rate);
        }
    }

    /// Rebuild the shared spectral engine's band-region set from every
    /// enabled band with `spectral` on and a non-zero dynamic range.
    fn sync_spectral_regions(&mut self) {
        self.spectral_regions.clear();
        for band in 0..EQ_BANDS {
            let (used, on) = self.state[band];
            if !(used && on && self.spectral_on[band]) {
                continue;
            }
            let (range, thr, _, _, auto, _) = self.dyn_cfg[band];
            if range.abs() <= 1.0e-3 {
                continue;
            }
            let freq = self.values[band * EQ_FIELDS + 2].clamp(10.0, 30000.0);
            let q = self.values[band * EQ_FIELDS + 4].clamp(0.025, 40.0);
            let bw = freq / q.max(0.1);
            self.spectral_regions.push(eq::spectral::SpectralRegion {
                lo_hz: (freq - bw * 0.5).max(20.0),
                hi_hz: (freq + bw * 0.5).min(20000.0),
                // Range −30..30 dB → depth 0..1 (cut depth; per-bin
                // expansion comes later).
                amount: (range.abs() / 30.0).clamp(0.0, 1.0),
                // Auto keeps the relative default (+4 dB prominence);
                // manual threshold maps the −80..0 knob into a 0..12 dB
                // prominence window.
                threshold_db: if auto { 4.0 } else { (thr + 80.0) * 12.0 / 80.0 },
            });
        }
        self.spectral.set_regions(&self.spectral_regions);
    }

    /// Whether the spectral engine is currently in the signal path.
    pub fn spectral_engaged(&self) -> bool {
        self.spectral.has_regions()
    }

    /// Configure the solo filter for the active listen band: the
    /// region you hear follows the band's shape — bells/notches solo a
    /// bandpass at freq/Q, shelves and cuts solo everything they reach.
    fn sync_listen(&mut self) {
        let Some((band, mode)) = self.listen else {
            return;
        };
        if mode != 1 {
            return;
        }
        let freq = self.values[band * EQ_FIELDS + 2].clamp(10.0, 30000.0);
        let q = self.values[band * EQ_FIELDS + 4].clamp(0.025, 40.0);
        use eq::dynamics::SvfShape;
        use eq::slope::FilterShape as F;
        let (shape, sf, sq) = match F::from_canonical_index(self.shapes[band]) {
            F::LowShelf | F::LowCut => (SvfShape::Lowpass, freq, 0.707),
            F::HighShelf | F::HighCut => (SvfShape::Highpass, freq, 0.707),
            // Bells, notches, bandpasses, tilts: hear the band region.
            _ => (SvfShape::Bandpass, freq, q.max(0.5)),
        };
        self.solo_filter.set(shape, sf, sq, 0.0);
    }

    fn set(&mut self, id: u32, v: f64) {
        if id >= EQ_PARAM_COUNT {
            return;
        }
        self.values[id as usize] = v;
        if id == EQ_OUTPUT_GAIN_ID {
            self.output_gain_db = v.clamp(-30.0, 30.0);
            return;
        }
        if id == EQ_GAIN_SCALE_ID {
            self.gain_scale = v.clamp(-1.0, 2.0);
            for b in 0..EQ_BANDS {
                self.sync_band(b);
            }
            return;
        }
        match id {
            EQ_TRANSIENT_MODE_ID => {
                self.transient_mode = v >= 0.5;
                for b in 0..EQ_BANDS {
                    self.sync_band(b);
                }
                return;
            }
            EQ_SPLIT_BALANCE_ID => {
                self.splitter.params.balance = v.clamp(-50.0, 50.0);
                self.splitter.update(self.sample_rate);
                return;
            }
            EQ_SPLIT_ATTACK_ID => {
                self.splitter.params.attack = v.clamp(0.0, 100.0);
                self.splitter.update(self.sample_rate);
                return;
            }
            EQ_SPLIT_HOLD_ID => {
                self.splitter.params.hold = v.clamp(0.0, 100.0);
                self.splitter.update(self.sample_rate);
                return;
            }
            EQ_SPLIT_SMOOTH_ID => {
                self.splitter.params.smooth = v.clamp(0.0, 100.0);
                self.splitter.update(self.sample_rate);
                return;
            }
            EQ_SPLIT_SOLO_ID => {
                self.split_solo = v as u32;
                return;
            }
            EQ_TRANSIENT_GAIN_ID => {
                self.transient_gain_db = v.clamp(-30.0, 30.0);
                return;
            }
            EQ_STEADY_GAIN_ID => {
                self.steady_gain_db = v.clamp(-30.0, 30.0);
                return;
            }
            _ => {}
        }
        let band = if id < (EQ_BANDS * EQ_FIELDS) as u32 {
            let (band, field) = (id as usize / EQ_FIELDS, id as usize % EQ_FIELDS);
            match field {
                0 => self.state[band].0 = v >= 0.5,
                1 => self.state[band].1 = v >= 0.5,
                5 => self.shapes[band] = v as u32,
                _ => {}
            }
            band
        } else {
            let (base, field): (u32, usize) = match id {
                i if (EQ_SLOPE_BASE..EQ_SLOPE_BASE + 24).contains(&i) => (EQ_SLOPE_BASE, 0),
                i if (EQ_DYN_RANGE_BASE..EQ_DYN_RANGE_BASE + 24).contains(&i) => {
                    (EQ_DYN_RANGE_BASE, 1)
                }
                i if (EQ_DYN_THR_BASE..EQ_DYN_THR_BASE + 24).contains(&i) => (EQ_DYN_THR_BASE, 2),
                i if (EQ_DYN_ATK_BASE..EQ_DYN_ATK_BASE + 24).contains(&i) => (EQ_DYN_ATK_BASE, 3),
                i if (EQ_DYN_REL_BASE..EQ_DYN_REL_BASE + 24).contains(&i) => (EQ_DYN_REL_BASE, 4),
                i if (EQ_DYN_AUTO_BASE..EQ_DYN_AUTO_BASE + 24).contains(&i) => (EQ_DYN_AUTO_BASE, 5),
                i if (EQ_DYN_RELATIVE_BASE..EQ_DYN_RELATIVE_BASE + 24).contains(&i) => {
                    (EQ_DYN_RELATIVE_BASE, 6)
                }
                i if (EQ_PLACEMENT_BASE..EQ_PLACEMENT_BASE + 24).contains(&i) => {
                    (EQ_PLACEMENT_BASE, 7)
                }
                i if (EQ_STREAM_BASE..EQ_STREAM_BASE + 24).contains(&i) => (EQ_STREAM_BASE, 8),
                i if (EQ_SPECTRAL_BASE..EQ_SPECTRAL_BASE + 24).contains(&i) => {
                    (EQ_SPECTRAL_BASE, 9)
                }
                i if (EQ_LISTEN_BASE..EQ_LISTEN_BASE + 24).contains(&i) => (EQ_LISTEN_BASE, 10),
                _ => return,
            };
            let band = (id - base) as usize;
            match field {
                0 => self.slopes[band] = v as u32,
                7 => self.placements[band] = v as u32,
                8 => self.streams[band] = v as u32,
                9 => self.spectral_on[band] = v >= 0.5,
                10 => {
                    let mode = v as u32;
                    if mode == 0 {
                        if self.listen.is_some_and(|(b, _)| b == band) {
                            self.listen = None;
                        }
                    } else {
                        self.listen = Some((band, mode.min(2)));
                        self.solo_filter.reset();
                    }
                }
                1 => self.dyn_cfg[band].0 = v,
                2 => self.dyn_cfg[band].1 = v,
                3 => self.dyn_cfg[band].2 = v,
                4 => self.dyn_cfg[band].3 = v,
                5 => self.dyn_cfg[band].4 = v >= 0.5,
                _ => self.dyn_cfg[band].5 = v >= 0.5,
            }
            band
        };
        self.sync_band(band);
    }

    /// Apply a parameter by name (`b{i}_...`, `output_gain`, `gain_scale`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = eq_param_id_of(name) {
            self.set(id, value);
        }
    }

    /// Live dynamic gain of a band in dB (the yellow bar).
    pub fn live_dyn_gain_db(&self, band: usize) -> Option<f64> {
        (band < EQ_BANDS && self.dyn_active[band])
            .then(|| self.dyn_bands[band].live_gain_db())
    }
}

impl PluginInstance for NativeEq {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.eq", "EQ")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        (0..EQ_PARAM_COUNT)
            .filter_map(|id| {
                let name = eq_param_name_of(id)?;
                let (min, max, default) = eq_param_range(id);
                Some(PluginParamInfo {
                    id,
                    name,
                    min,
                    max,
                    default,
                })
            })
            .collect()
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.values.get(id as usize).copied()
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        let name = eq_param_name_of(id)?;
        if name.ends_with("_freq") {
            Some(if value >= 1000.0 {
                format!("{:.2} kHz", value / 1000.0)
            } else {
                format!("{value:.0} Hz")
            })
        } else if name.ends_with("_gain") || name.ends_with("dyn_range") || name == "output_gain" {
            Some(format!("{value:+.1} dB"))
        } else if name.ends_with("_slope") {
            let s = eq::slope::Slope::from_param_index(value as usize);
            Some(match s {
                eq::slope::Slope::Brickwall => "Brickwall".into(),
                s => format!("{:.0} dB/oct", s.db_per_octave()),
            })
        } else {
            None
        }
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        // Spectral bands put the STFT in the path; everything else is
        // zero-latency.
        if self.spectral.has_regions() {
            self.spectral.latency() as u32
        } else {
            0
        }
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0);
        self.eq.set_sample_rate(self.sample_rate);
        self.eq.reset();
        self.eq_b.set_sample_rate(self.sample_rate);
        self.eq_b.reset();
        self.splitter.update(self.sample_rate);
        self.spectral = eq::spectral::SpectralEngine::new(self.sample_rate, 1024);
        self.sync_spectral_regions();
        for b in 0..EQ_BANDS {
            self.dyn_bands[b].reset();
            self.sync_band(b);
        }
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.scratch_sl = vec![0.0; block_size.max(1) as usize];
        self.scratch_sr = vec![0.0; block_size.max(1) as usize];
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
        // Fully-idle block (no active bands, no dynamics, no spectral,
        // no transient split, unity output): straight copy, zero DSP.
        let any_dyn = self.dyn_active.iter().any(|&a| a);
        if self.listen.is_none()
            && !self.transient_mode
            && !any_dyn
            && !self.spectral.has_regions()
            && !self.eq.has_active_bands()
            && self.output_gain_db.abs() < 1.0e-9
        {
            let n = in_l.len().min(out_l.len());
            out_l[..n].copy_from_slice(&in_l[..n]);
            out_r[..n].copy_from_slice(&in_r[..n]);
            return Ok(());
        }
        let eq = &mut self.eq;
        let eq_b = &mut self.eq_b;
        let splitter = &mut self.splitter;
        let spectral = &mut self.spectral;
        let dyn_bands = &mut self.dyn_bands;
        let dyn_active = &self.dyn_active;
        let transient_mode = self.transient_mode;
        let split_solo = self.split_solo;
        let tg = audiocore_dsp::db::db_to_linear(self.transient_gain_db);
        let sg = audiocore_dsp::db::db_to_linear(self.steady_gain_db);
        let out_gain = audiocore_dsp::db::db_to_linear(self.output_gain_db);
        let scratch_sl = &mut self.scratch_sl;
        let scratch_sr = &mut self.scratch_sr;
        let listen = self.listen;
        let solo_filter = &mut self.solo_filter;
        let dry_ring = &mut self.dry_ring;
        let dry_pos = &mut self.dry_pos;
        // Delta listening compares against the dry signal delayed by
        // the current path latency (spectral engaged → block-1).
        let dry_delay = if spectral.has_regions() {
            spectral.latency()
        } else {
            0
        };
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| {
                // Record dry for delta listening (cheap ring write,
                // only while a delta listen is active).
                if matches!(listen, Some((_, 2))) {
                    let ring = dry_ring[0].len();
                    let mut p = *dry_pos;
                    for i in 0..l.len() {
                        dry_ring[0][p] = l[i];
                        dry_ring[1][p] = r[i];
                        p = (p + 1) % ring;
                    }
                }
                if transient_mode {
                    // Split the whole block, run each stream's chain
                    // block-wise (l/r become the transient stream, the
                    // steady stream rides the dedicated scratch), then
                    // recombine. Complementary split keeps flat
                    // settings a null.
                    let n = l.len();
                    for i in 0..n {
                        let mask = splitter.tick_mask(0.5 * (l[i] + r[i]));
                        let tl = l[i] * mask;
                        let tr = r[i] * mask;
                        scratch_sl[i] = l[i] - tl;
                        scratch_sr[i] = r[i] - tr;
                        l[i] = tl;
                        r[i] = tr;
                    }
                    eq.process(l, r);
                    eq_b.process(&mut scratch_sl[..n], &mut scratch_sr[..n]);
                    for i in 0..n {
                        let (ol, or) = match split_solo {
                            1 => (l[i] * tg, r[i] * tg),
                            2 => (scratch_sl[i] * sg, scratch_sr[i] * sg),
                            _ => (
                                l[i] * tg + scratch_sl[i] * sg,
                                r[i] * tg + scratch_sr[i] * sg,
                            ),
                        };
                        l[i] = ol;
                        r[i] = or;
                    }
                } else {
                    eq.process(l, r);
                }
                for (bi, d) in dyn_bands.iter_mut().enumerate() {
                    if !dyn_active[bi] {
                        continue;
                    }
                    for i in 0..l.len() {
                        let side = 0.5 * (l[i] + r[i]);
                        d.tick(&mut l[i], &mut r[i], side);
                    }
                }
                // Per-band spectral dynamics (engaged only while at
                // least one band has its spectral toggle on).
                if spectral.has_regions() {
                    for i in 0..l.len() {
                        let (sl, sr) = spectral.tick(l[i], r[i]);
                        l[i] = sl;
                        r[i] = sr;
                    }
                }
                if (out_gain - 1.0).abs() > 1.0e-9 {
                    for i in 0..l.len() {
                        l[i] *= out_gain;
                        r[i] *= out_gain;
                    }
                }
                // ── Listen: solo the band's region, or hear only the
                // delta this EQ creates. Composes with split_solo (the
                // stream solo already happened upstream), so
                // "transients of the soloed band" is stream solo +
                // band solo together.
                if let Some((_, mode)) = listen {
                    match mode {
                        1 => {
                            for i in 0..l.len() {
                                l[i] = solo_filter.tick(0, l[i]);
                                r[i] = solo_filter.tick(1, r[i]);
                            }
                        }
                        _ => {
                            let ring = dry_ring[0].len();
                            for i in 0..l.len() {
                                let read = (*dry_pos + ring - dry_delay) % ring;
                                l[i] -= dry_ring[0][read];
                                r[i] -= dry_ring[1][read];
                                *dry_pos = (*dry_pos + 1) % ring;
                            }
                        }
                    }
                }
            },
        );
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}



// ── Class-A Preamp ─────────────────────────────────────────────────────────

const PREAMP_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "drive", min: 0.0, max: 24.0, default: 6.0 },
    ParamSpec { id: 1, name: "q_point", min: -1.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 2, name: "pos_shaper", min: 0.0, max: 5.0, default: 3.0 },
    ParamSpec { id: 3, name: "neg_shaper", min: 0.0, max: 5.0, default: 3.0 },
    ParamSpec { id: 4, name: "mix", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 5, name: "output", min: -24.0, max: 24.0, default: 0.0 },
];

/// Harmonic readback ids: `h1`..`h8` (ids 100..107) return the
/// measured harmonic magnitudes of the CURRENT settings, normalized to
/// H1 — the UI's harmonic visualization polls these. The transfer
/// curve for the indicative sine display comes from
/// `saturate_dsp::preamp::analysis::transfer_curve` on a mirror.
pub const PREAMP_HARMONIC_BASE: u32 = 100;
pub const PREAMP_HARMONICS: usize = 8;

/// Native Class-A preamp — asymmetric saturation with a Q-point bias,
/// independent per-side shapers, and a 1073-style output DC blocker.
pub struct NativePreamp {
    pre: [saturate_dsp::preamp::ClassAPreamp; 2],
    harmonics: [f32; PREAMP_HARMONICS],
    harmonics_dirty: bool,
    prepared: bool,
}

impl NativePreamp {
    pub fn new(sample_rate: f64) -> Self {
        let mk = || saturate_dsp::preamp::ClassAPreamp::new(sample_rate.max(1.0) as f32);
        let mut p = Self {
            pre: [mk(), mk()],
            harmonics: [0.0; PREAMP_HARMONICS],
            harmonics_dirty: true,
            prepared: false,
        };
        for spec in PREAMP_PARAMS {
            p.set(spec.id, spec.default);
        }
        p
    }

    fn set(&mut self, id: u32, v: f64) {
        let v = v as f32;
        for pre in &mut self.pre {
            match id {
                0 => pre.drive = 10.0f32.powf(v.clamp(0.0, 24.0) / 20.0),
                1 => pre.q_point = v.clamp(-1.0, 1.0),
                2 => pre.positive = saturate_dsp::preamp::SideShaper::from_index(v as u32),
                3 => pre.negative = saturate_dsp::preamp::SideShaper::from_index(v as u32),
                4 => pre.mix = v.clamp(0.0, 1.0),
                5 => pre.output_gain = 10.0f32.powf(v.clamp(-24.0, 24.0) / 20.0),
                _ => {}
            }
        }
        if id <= 3 {
            self.harmonics_dirty = true;
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(PREAMP_PARAMS, name) {
            self.set(id, value);
        }
    }

    /// Current measured harmonic spectrum (H1..H8, linear re H1).
    pub fn harmonics(&mut self) -> [f32; PREAMP_HARMONICS] {
        if self.harmonics_dirty {
            saturate_dsp::preamp::analysis::harmonic_spectrum(&self.pre[0], &mut self.harmonics);
            self.harmonics_dirty = false;
        }
        self.harmonics
    }
}

impl PluginInstance for NativePreamp {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.preamp", "Preamp")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(PREAMP_PARAMS)
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        if (PREAMP_HARMONIC_BASE..PREAMP_HARMONIC_BASE + PREAMP_HARMONICS as u32).contains(&id) {
            let h = self.harmonics();
            return Some(f64::from(h[(id - PREAMP_HARMONIC_BASE) as usize]));
        }
        None
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        let shaper = |v: f64| -> &'static str {
            match v as u32 {
                1 => "Op-Amp",
                2 => "Tube",
                3 => "Transformer",
                4 => "Diode",
                5 => "Hard",
                _ => "Clean",
            }
        };
        match id {
            0 | 5 => Some(format!("{value:+.1} dB")),
            2 | 3 => Some(shaper(value).into()),
            _ => None,
        }
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
        for pre in &mut self.pre {
            pre.set_sample_rate(sample_rate.max(1.0) as f32);
            pre.reset();
        }
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
        for i in 0..in_l.len().min(out_l.len()) {
            out_l[i] = self.pre[0].process(0, in_l[i]);
            out_r[i] = self.pre[1].process(1, in_r[i]);
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}


// ── FTS-Saturate: the full distortion engine ──────────────────────────────

const SATURATE_PARAMS: &[ParamSpec] = &[
    ParamSpec { id: 0, name: "drive", min: 0.0, max: 36.0, default: 6.0 },
    ParamSpec { id: 1, name: "q_point", min: -1.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 2, name: "pos_shaper", min: 0.0, max: 5.0, default: 3.0 },
    ParamSpec { id: 3, name: "neg_shaper", min: 0.0, max: 5.0, default: 3.0 },
    // Pre-emphasis tilt (dB at the top vs bottom of the spectrum),
    // inverted exactly on the way out — tape/console style: drive the
    // highs harder without changing the tone balance.
    ParamSpec { id: 4, name: "emphasis", min: -12.0, max: 12.0, default: 0.0 },
    // LF protection: highpass BEFORE the shaper (kills bass-driven
    // intermodulation), re-summed after so the low end stays intact.
    ParamSpec { id: 5, name: "lf_protect", min: 0.0, max: 500.0, default: 0.0 },
    // 0 = 1x, 1 = 2x, 2 = 4x, 3 = 8x.
    ParamSpec { id: 6, name: "oversample", min: 0.0, max: 3.0, default: 1.0 },
    ParamSpec { id: 7, name: "auto_gain", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 8, name: "mix", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 9, name: "output", min: -24.0, max: 24.0, default: 0.0 },
    // 0 off / 1 delta (hear only the added distortion).
    ParamSpec { id: 10, name: "listen", min: 0.0, max: 1.0, default: 0.0 },
    // Top-level algorithm: 0 Custom, 1 Preamp (class-A), 2 Tube,
    // 3 Tape, 4 Transformer, 5 Console, 6 Fuzz. Selecting a model
    // configures shapers/bias/sag/voicing; further tweaks = Custom.
    ParamSpec { id: 11, name: "model", min: 0.0, max: 6.0, default: 1.0 },
    // Bias sag (tube bloom): program level pulls the Q point.
    ParamSpec { id: 12, name: "sag", min: 0.0, max: 1.0, default: 0.0 },
];

/// Harmonic readback ids `h1`..`h8` — same contract as the preamp.
pub const SATURATE_HARMONIC_BASE: u32 = 100;
/// Emphasis-EQ band params: `eq_b{i}_{used|on|freq|gain|q|shape}` at
/// id 200 + (band*6+field) — a full FTS-EQ band surface whose curve
/// shapes what the saturator drives; the SAME curve, gain-inverted,
/// runs on the way out so the net tone stays balanced (drive the
/// highs harder, keep the mix tonally identical).
pub const SATURATE_EQ_BASE: u32 = 200;

/// FTS-Saturate — the distortion engine: Class-A asymmetric core
/// (Q point + per-side shapers) inside a mastering-grade chain:
/// oversampled shaping (2x default, to 8x), pre/de-emphasis tilt,
/// LF-protected drive, RMS-matched auto gain, delta listen, and the
/// measured harmonic readback for the visualization.
use audiocore_dsp::biquad::{Biquad as SatBiquad, FilterType as SatFilterType};

pub struct NativeSaturate {
    pre: [saturate_dsp::preamp::ClassAPreamp; 2],
    os: audiocore_dsp::oversampling::Oversampler,
    emph_lo: SatBiquad,
    emph_hi: SatBiquad,
    deemph_lo: SatBiquad,
    deemph_hi: SatBiquad,
    /// The emphasis EQ (full FTS-EQ band engine) + its inverse mirror.
    emph_eq: eq::EqChain,
    deemph_eq: eq::EqChain,
    eq_state: [(bool, bool); EQ_BANDS],
    lf_split: SatBiquad,
    lf_low: SatBiquad,
    emphasis_db: f64,
    lf_hz: f64,
    auto_gain: bool,
    mix: f64,
    output_gain: f64,
    listen_delta: bool,
    /// Model voicing filters: tape head bump + HF loss, transformer
    /// LF-weighted drive tilt (engaged per model, identity otherwise).
    voice_bump: SatBiquad,
    voice_hf: SatBiquad,
    voice_on: (bool, bool),
    // RMS trackers for auto gain (in vs post-shaper).
    in_ms: f64,
    out_ms: f64,
    agc_gain: f64,
    harmonics: [f32; PREAMP_HARMONICS],
    harmonics_dirty: bool,
    sample_rate: f64,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    dry_l: Vec<f64>,
    dry_r: Vec<f64>,
}

impl NativeSaturate {
    pub fn new(sample_rate: f64) -> Self {
        use audiocore_dsp::oversampling::{OversampleQuality, OversampleRate};
        let sr = sample_rate.max(1.0);
        let mk = || saturate_dsp::preamp::ClassAPreamp::new(sr as f32);
        let mut sat = Self {
            pre: [mk(), mk()],
            os: audiocore_dsp::oversampling::Oversampler::new(
                OversampleRate::X2,
                OversampleQuality::Medium,
            ),
            emph_lo: SatBiquad::new(),
            emph_hi: SatBiquad::new(),
            deemph_lo: SatBiquad::new(),
            deemph_hi: SatBiquad::new(),
            emph_eq: {
                let mut c = eq::EqChain::new();
                c.set_sample_rate(sr);
                for _ in 0..EQ_BANDS {
                    let i = c.add_band();
                    if let Some(b) = c.band_mut(i) {
                        b.enabled = false;
                    }
                    c.update_band(i);
                }
                c
            },
            deemph_eq: {
                let mut c = eq::EqChain::new();
                c.set_sample_rate(sr);
                for _ in 0..EQ_BANDS {
                    let i = c.add_band();
                    if let Some(b) = c.band_mut(i) {
                        b.enabled = false;
                    }
                    c.update_band(i);
                }
                c
            },
            eq_state: [(false, false); EQ_BANDS],
            lf_split: SatBiquad::new(),
            lf_low: SatBiquad::new(),
            emphasis_db: 0.0,
            lf_hz: 0.0,
            auto_gain: true,
            mix: 1.0,
            output_gain: 1.0,
            listen_delta: false,
            voice_bump: SatBiquad::new(),
            voice_hf: SatBiquad::new(),
            voice_on: (false, false),
            in_ms: 0.0,
            out_ms: 0.0,
            agc_gain: 1.0,
            harmonics: [0.0; PREAMP_HARMONICS],
            harmonics_dirty: true,
            sample_rate: sr,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            dry_l: Vec::new(),
            dry_r: Vec::new(),
        };
        sat.os.update(sr);
        for spec in SATURATE_PARAMS {
            sat.set(spec.id, spec.default);
        }
        sat
    }

    fn update_filters(&mut self) {
        let sr = self.sample_rate;
        let e = self.emphasis_db;
        // Tilt via complementary shelves at 700 Hz, mirrored on exit.
        self.emph_lo.set(SatFilterType::LowShelf { gain_db: -e * 0.5 }, 700.0, 0.5, sr);
        self.emph_hi.set(SatFilterType::HighShelf { gain_db: e * 0.5 }, 700.0, 0.5, sr);
        self.deemph_lo.set(SatFilterType::LowShelf { gain_db: e * 0.5 }, 700.0, 0.5, sr);
        self.deemph_hi.set(SatFilterType::HighShelf { gain_db: -e * 0.5 }, 700.0, 0.5, sr);
        if self.lf_hz > 1.0 {
            self.lf_split.set(SatFilterType::Highpass, self.lf_hz, 0.707, sr);
            self.lf_low.set(SatFilterType::Lowpass, self.lf_hz, 0.707, sr);
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        use audiocore_dsp::oversampling::OversampleRate;
        match id {
            0 => {
                let g = 10.0f32.powf((v.clamp(0.0, 36.0) as f32) / 20.0);
                for p in &mut self.pre {
                    p.drive = g;
                }
            }
            1 => {
                for p in &mut self.pre {
                    p.q_point = v.clamp(-1.0, 1.0) as f32;
                }
            }
            2 => {
                for p in &mut self.pre {
                    p.positive = saturate_dsp::preamp::SideShaper::from_index(v as u32);
                }
            }
            3 => {
                for p in &mut self.pre {
                    p.negative = saturate_dsp::preamp::SideShaper::from_index(v as u32);
                }
            }
            4 => {
                self.emphasis_db = v.clamp(-12.0, 12.0);
                self.update_filters();
            }
            5 => {
                self.lf_hz = v.clamp(0.0, 500.0);
                self.update_filters();
            }
            6 => {
                self.os.set_rate(match v as u32 {
                    0 => OversampleRate::X1,
                    2 => OversampleRate::X4,
                    3 => OversampleRate::X8,
                    _ => OversampleRate::X2,
                });
                self.os.update(self.sample_rate);
                let os_sr = (self.sample_rate * self.os.rate().ratio() as f64) as f32;
                for p in &mut self.pre {
                    p.set_sample_rate(os_sr);
                }
            }
            7 => self.auto_gain = v >= 0.5,
            8 => self.mix = v.clamp(0.0, 1.0),
            9 => self.output_gain = audiocore_dsp::db::db_to_linear(v.clamp(-24.0, 24.0)),
            10 => self.listen_delta = v >= 0.5,
            11 => self.apply_model(v as u32),
            12 => {
                for p in &mut self.pre {
                    p.sag = v.clamp(0.0, 1.0) as f32;
                }
                self.harmonics_dirty = true;
            }
            i if (SATURATE_EQ_BASE..SATURATE_EQ_BASE + (EQ_BANDS * EQ_FIELDS) as u32)
                .contains(&i) =>
            {
                let rel = (i - SATURATE_EQ_BASE) as usize;
                let (band, field) = (rel / EQ_FIELDS, rel % EQ_FIELDS);
                match field {
                    0 => self.eq_state[band].0 = v >= 0.5,
                    1 => self.eq_state[band].1 = v >= 0.5,
                    _ => {}
                }
                let (used, on) = self.eq_state[band];
                // Same band in both chains; gain INVERTED in the mirror
                // (gain-less shapes — cuts/notches — mirror as-is and
                // are documented as uncompensated pre-filters).
                for (chain, invert) in
                    [(&mut self.emph_eq, false), (&mut self.deemph_eq, true)]
                {
                    if let Some(b) = chain.band_mut(band) {
                        match field {
                            0 | 1 => b.enabled = used && on,
                            2 => b.freq_hz = v.clamp(10.0, 30000.0),
                            3 => {
                                let g = v.clamp(-30.0, 30.0);
                                b.gain_db = if invert { -g } else { g };
                            }
                            4 => b.q = v.clamp(0.025, 40.0),
                            _ => b.filter_type = eq_shape_to_filter(v as u32),
                        }
                    }
                    chain.update_band(band);
                }
            }
            _ => {}
        }
        if id <= 3 {
            self.harmonics_dirty = true;
        }
    }

    /// Voice the chain as one of the named algorithms. Shapers, bias,
    /// sag, tilt, and the model filters all move; every one of them
    /// remains individually overridable afterwards (= Custom).
    fn apply_model(&mut self, model: u32) {
        use saturate_dsp::preamp::SideShaper as S;
        let sr = self.sample_rate;
        self.voice_on = (false, false);
        let (pos, neg, q, sag, tilt) = match model {
            // Preamp: the class-A story — transformer iron, biased.
            1 => (S::Transformer, S::Transformer, 0.25, 0.1, 0.0),
            // Tube: soft triode both sides, strong sag = bloom.
            2 => (S::Tube, S::Tube, 0.15, 0.6, 0.0),
            // Tape: tanh-family compression, head bump + HF loss.
            3 => {
                self.voice_bump.set(
                    SatFilterType::Peak { gain_db: 2.5 },
                    60.0,
                    0.8,
                    sr,
                );
                self.voice_hf.set(SatFilterType::Lowpass, 14_000.0, 0.707, sr);
                self.voice_on = (true, true);
                (S::Transformer, S::Transformer, 0.0, 0.2, 0.0)
            }
            // Transformer: iron knee, lows driven into the core first
            // (negative tilt = drive lows harder, mirrored out).
            4 => (S::Transformer, S::Transformer, 0.0, 0.0, -6.0),
            // Console: firm symmetric class-AB op-amp rails.
            5 => (S::OpAmp, S::OpAmp, 0.0, 0.0, 0.0),
            // Fuzz: hard + diode asymmetric mayhem.
            6 => (S::Hard, S::Diode, 0.4, 0.3, 0.0),
            // Custom: leave everything as-is.
            _ => return,
        };
        for p in &mut self.pre {
            p.positive = pos;
            p.negative = neg;
            p.q_point = q as f32;
            p.sag = sag as f32;
        }
        self.emphasis_db = tilt;
        self.update_filters();
        self.harmonics_dirty = true;
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(SATURATE_PARAMS, name) {
            self.set(id, value);
            return;
        }
        if let Some(rest) = name.strip_prefix("eq_") {
            if let Some(id) = eq_param_id_of(rest) {
                if id < (EQ_BANDS * EQ_FIELDS) as u32 {
                    self.set(SATURATE_EQ_BASE + id, value);
                }
            }
        }
    }

    pub fn harmonics(&mut self) -> [f32; PREAMP_HARMONICS] {
        if self.harmonics_dirty {
            saturate_dsp::preamp::analysis::harmonic_spectrum(&self.pre[0], &mut self.harmonics);
            self.harmonics_dirty = false;
        }
        self.harmonics
    }
}

impl PluginInstance for NativeSaturate {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.saturate", "Saturate")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        let mut out = param_infos(SATURATE_PARAMS);
        for i in 0..EQ_BANDS * EQ_FIELDS {
            let (band, field) = (i / EQ_FIELDS, i % EQ_FIELDS);
            let (min, max, default) = eq_param_range(i as u32);
            out.push(PluginParamInfo {
                id: SATURATE_EQ_BASE + i as u32,
                name: format!("eq_{}", eq_param_name(band, field)),
                min,
                max,
                default,
            });
        }
        out
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        if (SATURATE_HARMONIC_BASE..SATURATE_HARMONIC_BASE + PREAMP_HARMONICS as u32).contains(&id)
        {
            let h = self.harmonics();
            return Some(f64::from(h[(id - SATURATE_HARMONIC_BASE) as usize]));
        }
        None
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        match id {
            0 | 9 => Some(format!("{value:+.1} dB")),
            6 => Some(format!("{}x", 1usize << (value as u32).min(3))),
            11 => Some(
                match value as u32 {
                    1 => "Preamp",
                    2 => "Tube",
                    3 => "Tape",
                    4 => "Transformer",
                    5 => "Console",
                    6 => "Fuzz",
                    _ => "Custom",
                }
                .into(),
            ),
            _ => None,
        }
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        self.os.latency() as u32
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0);
        self.os.update(self.sample_rate);
        let os_sr = (self.sample_rate * self.os.rate().ratio() as f64) as f32;
        for p in &mut self.pre {
            p.set_sample_rate(os_sr);
            p.reset();
        }
        self.update_filters();
        self.emph_eq.set_sample_rate(self.sample_rate);
        self.emph_eq.reset();
        self.deemph_eq.set_sample_rate(self.sample_rate);
        self.deemph_eq.reset();
        let n = block_size.max(1) as usize;
        self.scratch_l = vec![0.0; n];
        self.scratch_r = vec![0.0; n];
        self.dry_l = vec![0.0; n];
        self.dry_r = vec![0.0; n];
        self.in_ms = 0.0;
        self.out_ms = 0.0;
        self.agc_gain = 1.0;
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
        let n = in_l.len().min(out_l.len()).min(self.scratch_l.len());
        for i in 0..n {
            self.scratch_l[i] = f64::from(in_l[i]);
            self.scratch_r[i] = f64::from(in_r[i]);
            self.dry_l[i] = self.scratch_l[i];
            self.dry_r[i] = self.scratch_r[i];
        }
        let (l, r) = (&mut self.scratch_l[..n], &mut self.scratch_r[..n]);
        // Emphasis EQ: the drawn curve decides what gets driven.
        if self.emph_eq.has_active_bands() {
            let (a, b) = (&mut *l, &mut *r);
            self.emph_eq.process(a, b);
        }
        let lf_on = self.lf_hz > 1.0;
        // Pre-emphasis + LF split (protected lows bypass the shaper).
        for i in 0..n {
            let mut xl = self.emph_hi.tick(self.emph_lo.tick(l[i], 0), 0);
            let mut xr = self.emph_hi.tick(self.emph_lo.tick(r[i], 1), 1);
            if lf_on {
                let low_l = self.lf_low.tick(xl, 0);
                let low_r = self.lf_low.tick(xr, 1);
                xl = self.lf_split.tick(xl, 0);
                xr = self.lf_split.tick(xr, 1);
                self.dry_l[i] = low_l; // stash protected lows
                self.dry_r[i] = low_r;
            }
            l[i] = xl;
            r[i] = xr;
        }
        // Track input loudness for AGC.
        for i in 0..n {
            let sq = 0.5 * (l[i] * l[i] + r[i] * r[i]);
            self.in_ms += (sq - self.in_ms) * 0.0005;
        }
        // Oversampled asymmetric shaping.
        let pre = &mut self.pre;
        self.os.process_stereo(l, r, |ol, or| {
            for i in 0..ol.len() {
                ol[i] = f64::from(pre[0].process(0, ol[i] as f32));
                or[i] = f64::from(pre[1].process(1, or[i] as f32));
            }
        });
        for i in 0..n {
            let sq = 0.5 * (l[i] * l[i] + r[i] * r[i]);
            self.out_ms += (sq - self.out_ms) * 0.0005;
        }
        // RMS-matching auto gain (slewed).
        if self.auto_gain {
            let target = (self.in_ms.max(1.0e-12) / self.out_ms.max(1.0e-12)).sqrt().clamp(0.1, 10.0);
            for i in 0..n {
                self.agc_gain += (target - self.agc_gain) * 0.0005;
                l[i] *= self.agc_gain;
                r[i] *= self.agc_gain;
            }
        }
        // Model voicing (tape head bump + HF loss).
        if self.voice_on.0 {
            for i in 0..n {
                l[i] = self.voice_bump.tick(l[i], 0);
                r[i] = self.voice_bump.tick(r[i], 1);
            }
        }
        if self.voice_on.1 {
            for i in 0..n {
                l[i] = self.voice_hf.tick(l[i], 0);
                r[i] = self.voice_hf.tick(r[i], 1);
            }
        }
        // Mirror EQ: the inverse curve restores the tonal balance.
        if self.deemph_eq.has_active_bands() {
            let (a, b) = (&mut *l, &mut *r);
            self.deemph_eq.process(a, b);
        }
        // De-emphasis + reassemble.
        for i in 0..n {
            let mut yl = self.deemph_hi.tick(self.deemph_lo.tick(l[i], 0), 0);
            let mut yr = self.deemph_hi.tick(self.deemph_lo.tick(r[i], 1), 1);
            if lf_on {
                yl += self.dry_l[i];
                yr += self.dry_r[i];
            }
            let (dl, dr) = (f64::from(in_l[i]), f64::from(in_r[i]));
            let (mut fl, mut fr) = (
                dl + (yl - dl) * self.mix,
                dr + (yr - dr) * self.mix,
            );
            if self.listen_delta {
                fl -= dl;
                fr -= dr;
            }
            out_l[i] = (fl * self.output_gain) as f32;
            out_r[i] = (fr * self.output_gain) as f32;
        }
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
    ParamSpec { id: 6, name: "mix_b", min: 0.0, max: 1.0, default: 0.08 },
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
    // Mod (per-voice randomization). cho_voice: 0 Tenor / 1 Baritone.
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
    // INFINITE footswitch: engage (0/1) + per-preset mode
    // (0 Freeze / 1 Infinite / 2 Off). Latch-vs-momentary is the
    // footswitch controller's concern; `freeze` covers both.
    ParamSpec { id: 44, name: "freeze", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 45, name: "inf_mode", min: 0.0, max: 2.0, default: 0.0 },
    // Chamber Color (0 Neutral / 1 Clear / 2 Smooth / 3 Crisp / 4 Deep).
    ParamSpec { id: 46, name: "chamber_color", min: 0.0, max: 4.0, default: 0.0 },
    // Magneto: head count menu (0 One / 1 Two / 2 Three / 3 Four /
    // 4 Six) + spacing (0 Even / 1 Uneven). With Magneto active the
    // pedal remaps predelay -> engine feedback and decay -> last-head
    // time; the chain handles that from the existing decay/predelay ids.
    ParamSpec { id: 47, name: "mag_heads", min: 0.0, max: 4.0, default: 3.0 },
    ParamSpec { id: 48, name: "mag_spacing", min: 0.0, max: 1.0, default: 0.0 },
    // NonLinear Shape (manual order: 0 Swoosh / 1 Reverse / 2 Ramp /
    // 3 Gate / 4 Gauss / 5 Bounce). With NonLinear active the pedal
    // remaps decay -> nonlinear-window time and predelay -> generator
    // feedback; the chain handles both from the existing ids.
    ParamSpec { id: 49, name: "nl_shape", min: 0.0, max: 5.0, default: 0.0 },
    // Spring: Dwell drive stage (0 Clean / 1 Combo / 2 Tube /
    // 3 Overdrive) + Number of Springs (0 One / 1 Two / 2 Three).
    ParamSpec { id: 50, name: "spring_dwell", min: 0.0, max: 3.0, default: 0.0 },
    ParamSpec { id: 51, name: "spring_num", min: 0.0, max: 2.0, default: 1.0 },
    // Common per-reverb params the MX menu carries on most engines:
    // Diffusion (ER softening/density) and Low End (low-frequency
    // content + decay profile; 0.5 = neutral, above = lows ring longer
    // / "larger spaces", below = lows tamed).
    ParamSpec { id: 52, name: "diffusion", min: 0.0, max: 1.0, default: 0.5 },
    ParamSpec { id: 53, name: "low_end", min: 0.0, max: 1.0, default: 0.5 },
    // Chorale: vowel program (0 AAHHOO / 1 AAHH / 2 AAHHOH / 3 OH /
    // 4 OOOHOH / 5 OOO / 6 Random) + Resonance (0 Mild / 1 Medium /
    // 2 High).
    ParamSpec { id: 54, name: "cho_vowel", min: 0.0, max: 6.0, default: 1.0 },
    ParamSpec { id: 55, name: "cho_resonance", min: 0.0, max: 2.0, default: 0.0 },
    // Impulse Decay EQ: frequency-dependent decay shaping (end-of-tail
    // band gains in dB at ~250 Hz / ~4 kHz; negative shortens that
    // band's decay, positive stretches it). FTS extra beyond the MX
    // surface.
    ParamSpec { id: 56, name: "imp_decay_lo", min: -24.0, max: 12.0, default: 0.0 },
    ParamSpec { id: 57, name: "imp_decay_hi", min: -24.0, max: 12.0, default: 0.0 },
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
    /// Impulse re-prepare workers (native only), one per chain: re-bake
    /// the IR off the audio thread when imp_* shaping params change and
    /// take the displaced buffers for off-thread deallocation. Without
    /// them the Impulse live params would mark slots dirty and never
    /// re-prepare.
    #[cfg(not(target_arch = "wasm32"))]
    reshaper_a: Option<reverb::ir::ImpulseReshaper>,
    #[cfg(not(target_arch = "wasm32"))]
    reshaper_b: Option<reverb::ir::ImpulseReshaper>,
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
            #[cfg(not(target_arch = "wasm32"))]
            reshaper_a: None,
            #[cfg(not(target_arch = "wasm32"))]
            reshaper_b: None,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        // Ids < 100: chain A + the dual block. Ids 100+: the same
        // chain-scoped param on chain B (`r2_*` names, id − 100).
        match id {
            3 => {
                self.rev.routing =
                    reverb::DualRouting::from_index(v.round().max(0.0) as usize)
            }
            // Legacy dual block (kept for preset compat; equivalent to
            // r2_algorithm / r2_decay / r2_mix / r2_pan).
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
            6 => self.rev.b.mix = v.clamp(0.0, 1.0),
            8 => self.rev.b.pan = v.clamp(-1.0, 1.0),
            _ if id >= 100 => Self::set_chain(&mut self.rev.b, id - 100, v),
            _ => Self::set_chain(&mut self.rev.a, id, v),
        }
    }

    /// Apply a chain-scoped param (everything in `REVERB_PARAMS` except
    /// the dual block) to one `ReverbChain`. Chain A hears these at
    /// their base ids, chain B at id + 100 (`r2_*`).
    fn set_chain(c: &mut reverb::ReverbChain, id: u32, v: f64) {
        match id {
            0 => c.mix = v.clamp(0.0, 1.0),
            1 => {
                c.params.decay = v;
                c.update_params();
            }
            2 => {
                c.params.size = v;
                c.update_params();
            }
            7 => c.pan = v.clamp(-1.0, 1.0),
            9 => c.trem_rate_hz = v,
            10 => c.trem_depth = v.clamp(0.0, 1.0),
            11 => {
                c.impulse.decay = v.clamp(0.01, 1.0);
                c.update_params();
            }
            12 => {
                c.impulse.tail = if v >= 0.5 {
                    reverb::ImpulseTail::Gate
                } else {
                    reverb::ImpulseTail::Envelope
                };
                c.update_params();
            }
            13 => {
                c.impulse.attack = v.clamp(0.0, 1.0);
                c.update_params();
            }
            14 => {
                c.impulse.stretch = v.clamp(0.25, 4.0);
                c.update_params();
            }
            15 => {
                c.impulse.direction = if v >= 0.5 {
                    reverb::ImpulseDirection::Reverse
                } else {
                    reverb::ImpulseDirection::Forward
                };
                c.update_params();
            }
            16 => {
                c.impulse.feedback = v.clamp(0.0, 1.0);
                c.update_params();
            }
            17 => {
                c.shimmer.shift1_semitones = Some(v.clamp(-12.0, 12.0));
                c.update_params();
            }
            18 => {
                c.shimmer.shift2_semitones = Some(v.clamp(-12.0, 12.0));
                c.update_params();
            }
            19 => {
                c.shimmer.voice2 = v >= 0.5;
                c.update_params();
            }
            20 => {
                c.shimmer.amount = Some(v.clamp(0.0, 1.0));
                c.update_params();
            }
            21 => {
                c.shimmer.feedback_mode =
                    reverb::ShimmerFeedbackMode::from_index(v.round().max(0.0) as usize);
                c.update_params();
            }
            22 => {
                c.magneto.ping_pong = v >= 0.5;
                c.update_params();
            }
            23 => {
                c.nonlinear.chop_rate_hz = v.clamp(0.1, 15.0);
                c.update_params();
            }
            24 => {
                c.nonlinear.chop_depth = v.clamp(0.0, 1.0);
                c.update_params();
            }
            25 => {
                c.nonlinear.gate_speed = v.clamp(0.0, 1.0);
                c.update_params();
            }
            26 => {
                c.nonlinear.late_speed = v.clamp(0.0, 1.0);
                c.update_params();
            }
            27 => {
                c.nonlinear.late_decay = v.clamp(0.0, 1.0);
                c.update_params();
            }
            28 => {
                c.nonlinear.late_level = v.clamp(0.0, 1.0);
                c.update_params();
            }
            29 => {
                c.cloud.ensemble = v.clamp(0.0, 1.0);
                c.update_params();
            }
            30 => {
                c.bloom.harmonics = v.clamp(0.0, 1.0);
                c.update_params();
            }
            31 => {
                c.chorale.choir_level = Some(v.clamp(0.0, 1.0));
                c.update_params();
            }
            32 => {
                c.chorale.voice = if v >= 0.5 {
                    reverb::ChoirVoice::Baritone
                } else {
                    reverb::ChoirVoice::Tenor
                };
                c.update_params();
            }
            33 => {
                c.chorale.mod_amount = v.clamp(0.0, 1.0);
                c.update_params();
            }
            34 => {
                c.voice = if v >= 0.5 {
                    reverb::ReverbVoice::Classic
                } else {
                    reverb::ReverbVoice::Mx
                };
                c.update_params();
            }
            35 => {
                c.hall.mid_db = v.clamp(-6.0, 6.0);
                c.update_params();
            }
            36 => {
                c.hall.swell_rise = v.clamp(0.0, 1.0);
                c.update_params();
            }
            37 => {
                c.hall.swell_type = if v >= 0.5 {
                    reverb::SwellType::WetPlusDry
                } else {
                    reverb::SwellType::Wet
                };
                c.update_params();
            }
            38 => {
                c.set_size_index(v.round().max(0.0) as usize);
            }
            39 => {
                c.set_algorithm(reverb::AlgorithmType::from_index(v.round().max(0.0) as usize));
                c.update_params();
            }
            40 => {
                c.params.modulation = v;
                c.update_params();
            }
            41 => {
                c.params.damping = v;
                c.update_params();
            }
            42 => {
                c.params.tone = v;
                c.update_params();
            }
            43 => c.predelay_ms = v,
            44 => c.freeze = v > 0.5,
            45 => {
                c.infinite_mode = match v.round().max(0.0) as usize {
                    1 => reverb::InfiniteMode::Infinite,
                    2 => reverb::InfiniteMode::Off,
                    _ => reverb::InfiniteMode::Freeze,
                }
            }
            46 => {
                c.chamber.color = reverb::ChamberColor::from_index(v.round().max(0.0) as usize);
                c.update_params();
            }
            47 => {
                c.magneto.heads = reverb::MagnetoHeads::from_index(v.round().max(0.0) as usize);
                c.update_params();
            }
            48 => {
                c.magneto.spacing = if v > 0.5 {
                    reverb::MagnetoSpacing::Uneven
                } else {
                    reverb::MagnetoSpacing::Even
                };
                c.update_params();
            }
            49 => {
                c.nonlinear.shape = Some(reverb::NlShape::from_index(v.round().max(0.0) as usize));
                c.update_params();
            }
            50 => {
                c.spring.dwell = reverb::SpringDwell::from_index(v.round().max(0.0) as usize);
                c.update_params();
            }
            51 => {
                c.spring.springs = v.round().max(0.0) as u8 + 1;
                c.update_params();
            }
            52 => {
                c.params.diffusion = v.clamp(0.0, 1.0);
                c.update_params();
            }
            54 => {
                c.chorale.vowel =
                    Some(reverb::ChoraleVowel::from_index(v.round().max(0.0) as usize));
                c.update_params();
            }
            55 => {
                c.chorale.resonance =
                    reverb::ChoraleResonance::from_index(v.round().max(0.0) as usize);
                c.update_params();
            }
            56 => {
                c.impulse.decay_lo_db = v.clamp(-24.0, 12.0);
                c.update_params();
            }
            57 => {
                c.impulse.decay_hi_db = v.clamp(-24.0, 12.0);
                c.update_params();
            }
            53 => {
                // 0 -> lows tamed (0.5x), 0.5 -> neutral, 1 -> lows
                // bloom (1.6x) — the "impression of larger spaces".
                let v = v.clamp(0.0, 1.0);
                c.params.low_decay_mult = if v < 0.5 {
                    0.5 + v
                } else {
                    1.0 + (v - 0.5) * 1.2
                };
                c.update_params();
            }
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (see `REVERB_PARAMS`).
    /// `r2_<name>` addresses the same chain-scoped param on reverb 2.
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(base) = name.strip_prefix("r2_") {
            if let Some(id) = param_id(REVERB_PARAMS, base) {
                self.set(id + 100, value);
            }
            return;
        }
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
        let mut infos = param_infos(REVERB_PARAMS);
        // Mirror every chain-scoped param for reverb 2 at id + 100
        // (`r2_*`), so a dual preset can run two full MX engines. The
        // dual block (routing + the legacy b params) stays single.
        infos.extend(
            REVERB_PARAMS
                .iter()
                .filter(|s| !matches!(s.id, 3..=6 | 8))
                .map(|s| PluginParamInfo {
                    id: s.id + 100,
                    name: format!("r2_{}", s.name),
                    min: s.min,
                    max: s.max,
                    default: s.default,
                }),
        );
        infos
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
        // Impulse live-param pipeline, one worker per chain (the r2_*
        // mirror block can host a second Impulse engine): workers
        // re-prepare shaped IRs and dispose swap garbage off the audio
        // thread.
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.reshaper_a.is_none() {
                let (reshaper, rx) = reverb::ir::ImpulseReshaper::new();
                self.rev.a.set_prepared_ir_receiver(rx);
                self.rev.a.set_reshape_sender(reshaper.sender());
                self.rev.a.set_ir_trash_sender(reshaper.trash_sender());
                self.reshaper_a = Some(reshaper);
            }
            if self.reshaper_b.is_none() {
                let (reshaper, rx) = reverb::ir::ImpulseReshaper::new();
                self.rev.b.set_prepared_ir_receiver(rx);
                self.rev.b.set_reshape_sender(reshaper.sender());
                self.rev.b.set_ir_trash_sender(reshaper.trash_sender());
                self.reshaper_b = Some(reshaper);
            }
        }
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
    // 2 ms floor: the Lo-Fi machine is spec'd down to 2 ms (chorus/flange/
    // realtime-lofi use); every style re-clamps to its own range anyway.
    ParamSpec { id: 1, name: "time", min: 2.0, max: 2500.0, default: 400.0 },
    ParamSpec { id: 2, name: "feedback", min: 0.0, max: 0.95, default: 0.30 },
    // TimeLine MX parity params (style index: see delay::DelayStyle).
    ParamSpec { id: 3, name: "style", min: 0.0, max: 13.0, default: 1.0 },
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
    ParamSpec { id: 18, name: "style_b", min: 0.0, max: 13.0, default: 1.0 },
    ParamSpec { id: 19, name: "time_b", min: 2.0, max: 2500.0, default: 300.0 },
    ParamSpec { id: 20, name: "feedback_b", min: 0.0, max: 0.95, default: 0.30 },
    ParamSpec { id: 21, name: "mix_b", min: 0.0, max: 1.0, default: 0.08 },
    // Spectral machine (grain_shape: 0 Soft/1 Swell/2 SoftPluck/
    // 3 Pluck/4 Bounce; direction: 0 Fwd/1 Rev/2 Both; density = the
    // MX 15-step synced menu index (CC 0-14, 1/1 .. 1/32 of the repeat
    // incl. off-grid ratios); density_ms >= 6 switches to free
    // 6-250 ms).
    ParamSpec { id: 22, name: "grain_shape", min: 0.0, max: 4.0, default: 0.0 },
    ParamSpec { id: 23, name: "direction", min: 0.0, max: 2.0, default: 0.0 },
    ParamSpec { id: 24, name: "density", min: 0.0, max: 14.0, default: 9.0 },
    ParamSpec { id: 25, name: "density_ms", min: 0.0, max: 250.0, default: 0.0 },
    ParamSpec { id: 26, name: "spread", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 27, name: "stretch", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 28, name: "octave", min: 0.0, max: 1.0, default: 0.0 },
    // Lo-Fi machine (filter_shape: 0 Off .. 8 Intercom; grit rides the
    // shared "drive" engine field via id 29). sample_rate is the MX
    // 21-step menu index: 0 = 750 Hz ... 20 = 96 kHz, geometrically
    // spaced (the manual gives endpoints + step count), converted to a
    // divisor against the host rate.
    ParamSpec { id: 29, name: "grit", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 30, name: "bit_depth", min: 4.0, max: 32.0, default: 12.0 },
    ParamSpec { id: 31, name: "sample_rate", min: 0.0, max: 20.0, default: 11.0 },
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
    // Common Mod Speed/Depth (TimeLine PARAM-menu mod): one knob pair
    // drives whichever machine is active (each engine keeps per-machine
    // fields; the Reverb machine routes these to its wet tremolo).
    ParamSpec { id: 38, name: "mod_rate", min: 0.05, max: 8.0, default: 0.6 },
    ParamSpec { id: 39, name: "mod_depth", min: 0.0, max: 1.0, default: 0.0 },
    // Reverse machine: Smear (diffusion on the reversed audio).
    ParamSpec { id: 40, name: "rev_smear", min: 0.0, max: 1.0, default: 0.0 },
    // Digital Classic voice: morphing FILTER (0 full-bw -> 1 tape).
    ParamSpec { id: 41, name: "dig_morph", min: 0.0, max: 1.0, default: 0.0 },
    // Ducking (TimeLine Duck Sens 0-18, Duck Release 0.05-1.00 s).
    ParamSpec { id: 42, name: "duck_sens", min: 0.0, max: 18.0, default: 0.0 },
    ParamSpec { id: 43, name: "duck_release", min: 0.05, max: 1.0, default: 0.2 },
    // Drum machine: head spacing (0 Even / 1 Triplet / 2 Golden /
    // 3 Silver) + Lo Cut.
    ParamSpec { id: 44, name: "drum_spacing", min: 0.0, max: 3.0, default: 2.0 },
    ParamSpec { id: 45, name: "drum_locut", min: 0.0, max: 1.0, default: 0.2 },
    // Oil Can: head mode (0 Long / 1 Short / 2 Both).
    ParamSpec { id: 46, name: "oilcan_heads", min: 0.0, max: 2.0, default: 0.0 },
    // Per-delay wet output level (TimeLine Output Level).
    ParamSpec { id: 47, name: "output_level", min: 0.0, max: 1.0, default: 1.0 },
    // Filter machine (swept filter + trem on repeats).
    ParamSpec { id: 48, name: "flt_shape", min: 0.0, max: 10.0, default: 0.0 },
    ParamSpec { id: 49, name: "flt_speed", min: 0.03125, max: 32.0, default: 1.0 },
    ParamSpec { id: 50, name: "flt_depth", min: 0.0, max: 1.0, default: 0.5 },
    ParamSpec { id: 51, name: "flt_center", min: 100.0, max: 8000.0, default: 1200.0 },
    ParamSpec { id: 52, name: "flt_q", min: 0.5, max: 10.0, default: 2.0 },
    ParamSpec { id: 53, name: "flt_location", min: 0.0, max: 1.0, default: 1.0 },
    ParamSpec { id: 54, name: "flt_trem_depth", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 55, name: "flt_trem_speed", min: 0.03125, max: 32.0, default: 1.0 },
    // MultiTap: Classic pattern recall (0 = custom, 1-16 = Classic n),
    // feedback topology (0 Input / 1 Parallel), step grid
    // (0 16th / 1 Triplet / 2 Off-256).
    ParamSpec { id: 56, name: "mtap_pattern", min: 0.0, max: 16.0, default: 0.0 },
    ParamSpec { id: 57, name: "mtap_fb_mode", min: 0.0, max: 1.0, default: 0.0 },
    ParamSpec { id: 58, name: "mtap_grid", min: 0.0, max: 2.0, default: 0.0 },
    // Filter-machine tremolo waveform (manual list: 0 Triangle /
    // 1 Square / 2 Sine / 3 Ramp / 4 Saw).
    ParamSpec { id: 59, name: "flt_trem_shape", min: 0.0, max: 4.0, default: 2.0 },
    // Spectral post-granular diffusion (FTS voicing extra beyond the
    // hardware surface — Clouds allpass-chain crossfade on the cloud).
    ParamSpec { id: 60, name: "spec_diffusion", min: 0.0, max: 1.0, default: 0.5 },
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
            0 => a.mix = v.clamp(0.0, 1.0),
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
            21 => self.dly.b.mix = v.clamp(0.0, 1.0),
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
                // MX synced-density menu: 15 steps from 1/1 down to
                // 1/32 of the repeat time, including the off-grid
                // ratios the walkthrough demos (2/3, 3/8, ...).
                // // interpretation: the manual gives only the
                // endpoints + step count; intermediate ratios are a
                // musical fill to be dialed in against hardware.
                const SYNCED_STEPS: [f64; 15] = [
                    1.0,
                    3.0 / 4.0,
                    2.0 / 3.0,
                    1.0 / 2.0,
                    3.0 / 8.0,
                    1.0 / 3.0,
                    1.0 / 4.0,
                    3.0 / 16.0,
                    1.0 / 6.0,
                    1.0 / 8.0,
                    1.0 / 12.0,
                    1.0 / 16.0,
                    1.0 / 20.0,
                    1.0 / 24.0,
                    1.0 / 32.0,
                ];
                let i = (v.round().max(0.0) as usize).min(SYNCED_STEPS.len() - 1);
                let d = delay::DensityMode::Synced(SYNCED_STEPS[i]);
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
                // Step index -> absolute Hz (750 Hz .. 96 kHz geometric)
                // -> hold divisor at the host rate. Steps at or above
                // the host rate mean "no reduction" (divisor 1).
                let step = v.clamp(0.0, 20.0);
                let hz = 750.0 * (96000.0f64 / 750.0).powf(step / 20.0);
                let div = (self.sample_rate / hz).max(1.0);
                a.delay_l.lofi_sr_div = div;
                a.delay_r.lofi_sr_div = div;
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
            38 => {
                for e in [&mut a.delay_l, &mut a.delay_r] {
                    e.digital_mod_rate = v;
                    e.reverse_mod_rate = v;
                    e.lofi_mod_rate = v;
                    e.pitch_mod_rate = v;
                    e.bbd_mod_rate = v;
                    e.oilcan_mod_rate = v;
                    e.multitap_mod_rate_hz = v;
                    e.reverb_trem_rate = v;
                }
            }
            39 => {
                let d = v.clamp(0.0, 1.0);
                for e in [&mut a.delay_l, &mut a.delay_r] {
                    e.digital_mod_depth = d;
                    e.reverse_mod_depth = d;
                    e.lofi_mod_depth = d;
                    e.pitch_mod_depth = d;
                    e.bbd_mod_depth = d;
                    e.multitap_mod_depth = d;
                    e.reverb_trem_depth = d;
                }
            }
            40 => {
                a.delay_l.reverse_smear = v;
                a.delay_r.reverse_smear = v;
            }
            41 => {
                a.delay_l.digital_morph = v;
                a.delay_r.digital_morph = v;
            }
            42 => {
                a.ducking_enabled = v > 0.05;
                a.ducker.amount = (v / 18.0).clamp(0.0, 1.0);
            }
            43 => a.ducker.release_ms = v.clamp(0.05, 1.0) * 1000.0,
            44 => {
                let spacing = match v.round().max(0.0) as usize {
                    0 => delay::DrumSpacing::Even,
                    1 => delay::DrumSpacing::Triplet,
                    3 => delay::DrumSpacing::Silver,
                    _ => delay::DrumSpacing::Golden,
                };
                a.delay_l.set_drum_spacing(spacing);
                a.delay_r.set_drum_spacing(spacing);
            }
            45 => {
                a.delay_l.drum_lo_cut = v;
                a.delay_r.drum_lo_cut = v;
            }
            46 => {
                let heads = match v.round().max(0.0) as usize {
                    1 => delay::OilCanHeads::Short,
                    2 => delay::OilCanHeads::Both,
                    _ => delay::OilCanHeads::Long,
                };
                a.delay_l.oilcan_heads = heads;
                a.delay_r.oilcan_heads = heads;
            }
            47 => a.output_level = v.clamp(0.0, 1.0),
            48 => {
                let shape = delay::FilterLfoShape::from_index(v.round().max(0.0) as usize);
                a.delay_l.filter_lfo_shape = shape;
                a.delay_r.filter_lfo_shape = shape;
            }
            49 => {
                a.delay_l.filter_lfo_speed = v;
                a.delay_r.filter_lfo_speed = v;
            }
            50 => {
                a.delay_l.filter_depth = v;
                a.delay_r.filter_depth = v;
            }
            51 => {
                a.delay_l.filter_center = v;
                a.delay_r.filter_center = v;
            }
            52 => {
                a.delay_l.filter_q = v;
                a.delay_r.filter_q = v;
            }
            53 => {
                let loc = if v > 0.5 {
                    delay::FilterLocation::Post
                } else {
                    delay::FilterLocation::Pre
                };
                a.delay_l.filter_location = loc;
                a.delay_r.filter_location = loc;
            }
            54 => {
                a.delay_l.filter_trem_depth = v;
                a.delay_r.filter_trem_depth = v;
            }
            55 => {
                a.delay_l.filter_trem_speed = v;
                a.delay_r.filter_trem_speed = v;
            }
            56 => {
                let n = v.round().max(0.0) as u8;
                if n >= 1 {
                    a.delay_l.apply_multitap_classic(n);
                    a.delay_r.apply_multitap_classic(n);
                }
            }
            57 => {
                let mode = if v > 0.5 {
                    delay::FeedbackMode::Parallel
                } else {
                    delay::FeedbackMode::Input
                };
                a.delay_l.multitap_feedback_mode = mode;
                a.delay_r.multitap_feedback_mode = mode;
            }
            58 => {
                let grid = match v.round().max(0.0) as usize {
                    1 => delay::TapGrid::Triplet,
                    2 => delay::TapGrid::Off,
                    _ => delay::TapGrid::Sixteenth,
                };
                a.delay_l.multitap_grid = grid;
                a.delay_r.multitap_grid = grid;
            }
            60 => {
                a.delay_l.spectral_diffusion = v.clamp(0.0, 1.0);
                a.delay_r.spectral_diffusion = v.clamp(0.0, 1.0);
            }
            59 => {
                let shape = match v.round().max(0.0) as usize {
                    0 => delay::FilterLfoShape::TrianglePos,
                    1 => delay::FilterLfoShape::SquarePos,
                    3 => delay::FilterLfoShape::Ramp,
                    4 => delay::FilterLfoShape::Saw,
                    _ => delay::FilterLfoShape::SinePos,
                };
                a.delay_l.filter_trem_shape = shape;
                a.delay_r.filter_trem_shape = shape;
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

    /// The `r2_*` mirror block must land chain-scoped params on chain B
    /// and leave chain A untouched (and vice versa).
    #[test]
    fn reverb_r2_params_reach_chain_b() {
        let mut r = NativeReverb::new(48000.0);
        r.set_named("cloud_ensemble", 0.8);
        r.set_named("r2_cloud_ensemble", 0.3);
        assert!((r.rev.a.cloud.ensemble - 0.8).abs() < 1e-9);
        assert!((r.rev.b.cloud.ensemble - 0.3).abs() < 1e-9);

        r.set_named("r2_mix", 0.77);
        assert!((r.rev.b.mix - 0.77).abs() < 1e-9);

        r.set_named("r2_algorithm", 6.0); // Shimmer
        assert_eq!(r.rev.b.algorithm_type(), reverb::AlgorithmType::Shimmer);
        assert_ne!(r.rev.a.algorithm_type(), reverb::AlgorithmType::Shimmer);

        // The advertised param list contains the mirrors exactly once.
        let infos = r.params();
        let r2: Vec<_> = infos.iter().filter(|i| i.name.starts_with("r2_")).collect();
        assert_eq!(r2.len(), REVERB_PARAMS.len() - 5);
    }
}
