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
use daw::plugin::{
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
        scratch_l[i] = f64::from(in_l[i]);
        scratch_r[i] = f64::from(in_r[i]);
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

/// First parameter id of the reverb's Decay Rate EQ (six bands of four
/// fields: shape / freq / rate / q).
pub const DECAY_BAND_BASE: u32 = 60;
/// Decay Rate EQ bands, mirroring `reverb::DECAY_BANDS`.
pub const DECAY_BANDS: usize = 8;
/// Per-band fields, in wire order: used, on, freq, gain, q, shape.
pub const EQ_FIELDS: usize = 6;

// Appended param blocks (ids are APPEND-ONLY — the 0..143 layout is
// frozen by saved rig patches):
/// `b{i}_slope` — canonical slope index 0..10 (`eq_dsp::Slope` table).
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
/// `auto_gain` 0/1 — Pro-Q's Auto Gain: hold the broadband level steady
/// against whatever curve the bands draw.
pub const EQ_AUTO_GAIN_ID: u32 = 370;
/// `character` — Pro-Q's Character mode: 0 Clean, 1 Subtle, 2 Warm.
pub const EQ_CHARACTER_ID: u32 = 371;
/// `output_pan` -1..1 and `output_pan_mode` 0 stereo / 1 mid-side.
pub const EQ_OUTPUT_PAN_ID: u32 = 372;
pub const EQ_OUTPUT_PAN_MODE_ID: u32 = 373;
/// `b{i}_spectral` 0/1 — the band's dynamics act per-bin (Pro-Q
/// Spectral): its freq/Q footprint + dyn range/threshold drive the
/// shared spectral engine instead of the whole-band gain ride.
pub const EQ_SPECTRAL_BASE: u32 = 376;
/// `b{i}_listen` — 0 off, 1 solo the band's frequency region, 2 delta (hear only what this EQ changes).
///
/// One band at a time (the most recent non-zero wins); composes with `split_solo` so you can
/// hear e.g. just the transients of the soloed region.
pub const EQ_LISTEN_BASE: u32 = 400;
/// One past the last EQ param id.
/// Per-bin selectivity for a spectral band, `b{i}_spectral_density`.
pub const EQ_SPECTRAL_DENSITY_BASE: u32 = 424;
/// Pink-noise tilt on a spectral band's trigger, `b{i}_spectral_tilt`.
pub const EQ_SPECTRAL_TILT_BASE: u32 = 448;
/// Side-chain listens to a custom range, `b{i}_dyn_side`.
pub const EQ_DYN_SIDE_BASE: u32 = 472;
/// That range, `b{i}_dyn_side_lo` / `b{i}_dyn_side_hi`.
pub const EQ_DYN_SIDE_LO_BASE: u32 = 496;
pub const EQ_DYN_SIDE_HI_BASE: u32 = 520;
pub const EQ_PARAM_COUNT: u32 = 544;

/// Canonical shape conversion — [`eq::host::FilterShape`] owns the
/// one true ordering (append-only, documented there).
fn eq_shape_to_filter(shape: u32) -> eq::FilterType {
    eq::host::FilterShape::from_canonical_index(shape).to_filter_type()
}

/// Param name for `(band, field)` — `b{band+1}_{used|on|freq|gain|q|shape}`.
#[must_use]
pub fn eq_param_name(band: usize, field: usize) -> String {
    let f = ["used", "on", "freq", "gain", "q", "shape"][field];
    format!("b{}_{}", band + 1, f)
}

const EQ_EXT_FIELDS: [(&str, u32); 16] = [
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
    ("spectral_density", EQ_SPECTRAL_DENSITY_BASE),
    ("spectral_tilt", EQ_SPECTRAL_TILT_BASE),
    ("dyn_side", EQ_DYN_SIDE_BASE),
    ("dyn_side_lo", EQ_DYN_SIDE_LO_BASE),
    ("dyn_side_hi", EQ_DYN_SIDE_HI_BASE),
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
        "auto_gain" => return Some(EQ_AUTO_GAIN_ID),
        "character" => return Some(EQ_CHARACTER_ID),
        "output_pan" => return Some(EQ_OUTPUT_PAN_ID),
        "output_pan_mode" => return Some(EQ_OUTPUT_PAN_MODE_ID),
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
#[must_use]
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
        EQ_GAIN_SCALE_ID => (-1.0, 2.0, 1.0),
        EQ_SPLIT_BALANCE_ID => (-50.0, 50.0, 0.0),
        EQ_SPLIT_ATTACK_ID | EQ_SPLIT_HOLD_ID | EQ_SPLIT_SMOOTH_ID => (0.0, 100.0, 50.0),
        EQ_OUTPUT_PAN_ID => (-1.0, 1.0, 0.0),
        EQ_OUTPUT_GAIN_ID | EQ_TRANSIENT_GAIN_ID | EQ_STEADY_GAIN_ID => (-30.0, 30.0, 0.0),
        EQ_TRANSIENT_MODE_ID | EQ_AUTO_GAIN_ID | EQ_OUTPUT_PAN_MODE_ID => (0.0, 1.0, 0.0),
        EQ_SPLIT_SOLO_ID | EQ_CHARACTER_ID => (0.0, 2.0, 0.0),
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
        i if (EQ_SPECTRAL_DENSITY_BASE..EQ_SPECTRAL_DENSITY_BASE + 24).contains(&i) => {
            (0.0, 100.0, 50.0)
        }
        i if (EQ_SPECTRAL_TILT_BASE..EQ_SPECTRAL_TILT_BASE + 24).contains(&i) => (0.0, 1.0, 0.0),
        i if (EQ_DYN_SIDE_BASE..EQ_DYN_SIDE_BASE + 24).contains(&i) => (0.0, 1.0, 0.0),
        i if (EQ_DYN_SIDE_LO_BASE..EQ_DYN_SIDE_LO_BASE + 24).contains(&i) => (20.0, 20_000.0, 20.0),
        i if (EQ_DYN_SIDE_HI_BASE..EQ_DYN_SIDE_HI_BASE + 24).contains(&i) => {
            (20.0, 20_000.0, 20_000.0)
        }
        i if (EQ_LISTEN_BASE..EQ_LISTEN_BASE + 24).contains(&i) => (0.0, 2.0, 0.0),
        _ => (0.0, 1.0, 0.0),
    }
}

#[must_use]
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
        EQ_AUTO_GAIN_ID => return Some("auto_gain".into()),
        EQ_CHARACTER_ID => return Some("character".into()),
        EQ_OUTPUT_PAN_ID => return Some("output_pan".into()),
        EQ_OUTPUT_PAN_MODE_ID => return Some("output_pan_mode".into()),
        _ => {}
    }
    for (f, base) in EQ_EXT_FIELDS {
        if (base..base + 24).contains(&id) {
            return Some(format!("b{}_{}", id - base + 1, f));
        }
    }
    None
}

/// Native EQ block — the full FTS-EQ engine: 24 dynamic bands over [`eq::runtime::chain::EqChain`]'s
/// Pro-Q ZPK pipeline.
///
/// Each band has used/on/freq/gain/q/shape (all thirteen canonical shapes) + slope, plus
/// per-band DYNAMIC EQ (Pro-Q-style range/threshold/auto) running on the SVF dynamics engine,
/// and output gain / gain-scale masters. Bands start unused → transparent passthrough.
/// The rig's front end for the one EQ engine.
///
/// Everything that makes an EQ — the filter bank, per-band dynamics, the spectral engine,
/// transient splitting, listening — lives in [`eq::host::CanonicalEq`]. This is the parameter
/// surface the rig drives it through: an id-indexed value table, the automation-event pump,
/// and the f32 buffers a host hands over.
///
/// It used to be the engine itself, which is why the FTS-EQ plugin could not
/// play a dynamic band: the capability was reachable only through these ids.
pub struct NativeEq {
    engine: eq::host::CanonicalEq,
    /// Every param value by id, for host readback.
    values: Vec<f64>,
    /// Mirrors of the fields the id table writes, so a `sync` can rebuild a
    /// whole band from partial updates.
    bands: [eq::host::CanonicalBandConfig; EQ_BANDS],
    dynamics: [eq::host::CanonicalBandDynamics; EQ_BANDS],
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeEq {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut values = vec![0.0; EQ_PARAM_COUNT as usize];
        for id in 0..EQ_PARAM_COUNT {
            values[id as usize] = eq_param_range(id).2;
        }
        let mut me = Self {
            engine: eq::host::CanonicalEq::new(sample_rate),
            values,
            bands: [eq::host::CanonicalBandConfig::default(); EQ_BANDS],
            dynamics: [eq::host::CanonicalBandDynamics::default(); EQ_BANDS],
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        };
        // The defaults in the id table are the truth about what a fresh EQ is;
        // push them through so the engine agrees with `values` from the start.
        for b in 0..EQ_BANDS {
            me.bands[b].freq_hz = me.values[b * EQ_FIELDS + 2];
            me.bands[b].gain_db = me.values[b * EQ_FIELDS + 3];
            me.bands[b].q = me.values[b * EQ_FIELDS + 4];
            me.push_band(b);
        }
        me
    }

    /// Send one band's mirrored config and dynamics to the engine.
    fn push_band(&mut self, band: usize) {
        self.engine
            .set_canonical_band(band, self.bands[band], self.dynamics[band]);
    }

    #[must_use]
    pub fn spectral_engaged(&self) -> bool {
        self.engine.spectral_engaged()
    }

    fn set(&mut self, id: u32, v: f64) {
        if id >= EQ_PARAM_COUNT {
            return;
        }
        self.values[id as usize] = v;
        if id == EQ_OUTPUT_GAIN_ID {
            self.engine.set_output_gain_db(v.clamp(-30.0, 30.0));
            return;
        }
        if id == EQ_GAIN_SCALE_ID {
            self.engine.set_gain_scale(v.clamp(-1.0, 2.0));
            return;
        }
        match id {
            EQ_TRANSIENT_MODE_ID => {
                self.engine.set_transient_mode(v >= 0.5);
                return;
            }
            EQ_SPLIT_BALANCE_ID => {
                self.engine.set_split_balance(v.clamp(-50.0, 50.0));
                return;
            }
            EQ_SPLIT_ATTACK_ID => {
                self.engine.set_split_attack(v.clamp(0.0, 100.0));
                return;
            }
            EQ_SPLIT_HOLD_ID => {
                self.engine.set_split_hold(v.clamp(0.0, 100.0));
                return;
            }
            EQ_SPLIT_SMOOTH_ID => {
                self.engine.set_split_smooth(v.clamp(0.0, 100.0));
                return;
            }
            EQ_SPLIT_SOLO_ID => {
                self.engine.set_split_solo(v as u32);
                return;
            }
            EQ_TRANSIENT_GAIN_ID => {
                self.engine.set_transient_gain_db(v.clamp(-30.0, 30.0));
                return;
            }
            EQ_STEADY_GAIN_ID => {
                self.engine.set_steady_gain_db(v.clamp(-30.0, 30.0));
                return;
            }
            EQ_AUTO_GAIN_ID => {
                self.engine.set_auto_gain(v > 0.5);
                return;
            }
            EQ_CHARACTER_ID => {
                self.engine.set_character(v.clamp(0.0, 2.0).round() as u32);
                return;
            }
            EQ_OUTPUT_PAN_ID => {
                let ms = self.engine.output_pan_mid_side();
                self.engine.set_output_pan(v.clamp(-1.0, 1.0), ms);
                return;
            }
            EQ_OUTPUT_PAN_MODE_ID => {
                let p = self.engine.output_pan();
                self.engine.set_output_pan(p, v > 0.5);
                return;
            }
            _ => {}
        }
        let band = if id < (EQ_BANDS * EQ_FIELDS) as u32 {
            let (band, field) = (id as usize / EQ_FIELDS, id as usize % EQ_FIELDS);
            match field {
                0 => self.bands[band].used = v >= 0.5,
                1 => self.bands[band].enabled = v >= 0.5,
                2 => self.bands[band].freq_hz = v,
                3 => self.bands[band].gain_db = v,
                4 => self.bands[band].q = v,
                _ => self.bands[band].shape = v as u32,
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
                i if (EQ_DYN_AUTO_BASE..EQ_DYN_AUTO_BASE + 24).contains(&i) => {
                    (EQ_DYN_AUTO_BASE, 5)
                }
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
                i if (EQ_SPECTRAL_DENSITY_BASE..EQ_SPECTRAL_DENSITY_BASE + 24).contains(&i) => {
                    (EQ_SPECTRAL_DENSITY_BASE, 11)
                }
                i if (EQ_SPECTRAL_TILT_BASE..EQ_SPECTRAL_TILT_BASE + 24).contains(&i) => {
                    (EQ_SPECTRAL_TILT_BASE, 12)
                }
                i if (EQ_DYN_SIDE_BASE..EQ_DYN_SIDE_BASE + 24).contains(&i) => {
                    (EQ_DYN_SIDE_BASE, 13)
                }
                i if (EQ_DYN_SIDE_LO_BASE..EQ_DYN_SIDE_LO_BASE + 24).contains(&i) => {
                    (EQ_DYN_SIDE_LO_BASE, 14)
                }
                i if (EQ_DYN_SIDE_HI_BASE..EQ_DYN_SIDE_HI_BASE + 24).contains(&i) => {
                    (EQ_DYN_SIDE_HI_BASE, 15)
                }
                _ => return,
            };
            let band = (id - base) as usize;
            match field {
                0 => self.bands[band].slope = v,
                7 => {
                    self.bands[band].placement = eq::Placement::from_index(v as u32);
                }
                8 => self.bands[band].stream = v as u32,
                9 => self.dynamics[band].spectral = v >= 0.5,
                10 => {
                    let mode = v as u32;
                    if mode == 0 {
                        if self.engine.listen().is_some_and(|(b, _)| b == band) {
                            self.engine.set_listen(None);
                        }
                    } else {
                        self.engine.set_listen(Some((band, mode.min(2))));
                    }
                }
                1 => self.dynamics[band].range_db = v,
                2 => self.dynamics[band].threshold_db = v,
                3 => self.dynamics[band].attack_pct = v,
                4 => self.dynamics[band].release_pct = v,
                5 => self.dynamics[band].auto = v >= 0.5,
                6 => self.dynamics[band].relative = v >= 0.5,
                11 => self.dynamics[band].spectral_density = v,
                12 => self.dynamics[band].spectral_tilt = v >= 0.5,
                13 => self.dynamics[band].side_filtered = v >= 0.5,
                14 => self.dynamics[band].side_lo_hz = v,
                _ => self.dynamics[band].side_hi_hz = v,
            }
            band
        };
        self.push_band(band);
    }

    /// Apply a parameter by name (`b{i}_...`, `output_gain`, `gain_scale`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = eq_param_id_of(name) {
            self.set(id, value);
        }
    }

    /// The static chain's magnitude at `hz`, in dB — what Auto Gain reads.
    #[must_use]
    pub fn static_magnitude_db(&self, hz: f64) -> f64 {
        self.engine.static_magnitude_db(hz)
    }

    /// The compensation Auto Gain is applying, in dB.
    #[must_use]
    pub fn auto_gain_db(&self) -> f64 {
        self.engine.auto_gain_db()
    }

    /// Live dynamic gain of a band in dB (the yellow bar).
    #[must_use]
    pub fn live_dyn_gain_db(&self, band: usize) -> Option<f64> {
        self.engine.live_dyn_gain_db(band)
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
            let s = eq::host::Slope::from_param_index(value as usize);
            Some(match s {
                eq::host::Slope::Brickwall => "Brickwall".into(),
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
        self.engine.latency()
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.engine.prepare(sample_rate, block_size);
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
        let engine = &mut self.engine;
        process_f64_inplace(
            &mut self.scratch_l,
            &mut self.scratch_r,
            in_l,
            in_r,
            out_l,
            out_r,
            |l, r| engine.process(l, r),
        );
        Ok(())
    }

    fn deactivate(&mut self) {
        self.engine.deactivate();
        self.prepared = false;
    }
}

// ── Class-A Preamp ─────────────────────────────────────────────────────────

const PREAMP_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "drive",
        min: 0.0,
        max: 24.0,
        default: 6.0,
    },
    ParamSpec {
        id: 1,
        name: "q_point",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 2,
        name: "pos_shaper",
        min: 0.0,
        max: 5.0,
        default: 3.0,
    },
    ParamSpec {
        id: 3,
        name: "neg_shaper",
        min: 0.0,
        max: 5.0,
        default: 3.0,
    },
    ParamSpec {
        id: 4,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 5,
        name: "output",
        min: -24.0,
        max: 24.0,
        default: 0.0,
    },
];

/// Harmonic readback ids: `h1`..`h8` (ids 100..107) return the measured harmonic magnitudes.
///
/// The magnitudes are normalized to H1 — the UI's harmonic visualization polls these.
/// The transfer curve for the indicative sine display comes from
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
    #[must_use]
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
    ParamSpec {
        id: 0,
        name: "drive",
        min: 0.0,
        max: 36.0,
        default: 6.0,
    },
    ParamSpec {
        id: 1,
        name: "q_point",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 2,
        name: "pos_shaper",
        min: 0.0,
        max: 5.0,
        default: 3.0,
    },
    ParamSpec {
        id: 3,
        name: "neg_shaper",
        min: 0.0,
        max: 5.0,
        default: 3.0,
    },
    // Pre-emphasis tilt (dB at the top vs bottom of the spectrum),
    // inverted exactly on the way out — tape/console style: drive the
    // highs harder without changing the tone balance.
    ParamSpec {
        id: 4,
        name: "emphasis",
        min: -12.0,
        max: 12.0,
        default: 0.0,
    },
    // LF protection: highpass BEFORE the shaper (kills bass-driven
    // intermodulation), re-summed after so the low end stays intact.
    ParamSpec {
        id: 5,
        name: "lf_protect",
        min: 0.0,
        max: 500.0,
        default: 0.0,
    },
    // 0 = 1x, 1 = 2x, 2 = 4x, 3 = 8x.
    ParamSpec {
        id: 6,
        name: "oversample",
        min: 0.0,
        max: 3.0,
        default: 1.0,
    },
    ParamSpec {
        id: 7,
        name: "auto_gain",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 8,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 9,
        name: "output",
        min: -24.0,
        max: 24.0,
        default: 0.0,
    },
    // 0 off / 1 delta (hear only the added distortion).
    ParamSpec {
        id: 10,
        name: "listen",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Top-level algorithm: 0 Custom, 1 Preamp (class-A), 2 Tube,
    // 3 Tape, 4 Transformer, 5 Console, 6 Fuzz. Selecting a model
    // configures shapers/bias/sag/voicing; further tweaks = Custom.
    ParamSpec {
        id: 11,
        name: "model",
        min: 0.0,
        max: 6.0,
        default: 1.0,
    },
    // Bias sag (tube bloom): program level pulls the Q point.
    ParamSpec {
        id: 12,
        name: "sag",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
];

/// Harmonic readback ids `h1`..`h8` — same contract as the preamp.
pub const SATURATE_HARMONIC_BASE: u32 = 100;
/// Emphasis-EQ band params: `eq_b{i}_{used|on|freq|gain|q|shape}` at id 200 + (band*6+field).
///
/// A full FTS-EQ band surface whose curve shapes what the saturator drives; the SAME curve,
/// gain-inverted, runs on the way out so the net tone stays balanced (drive the highs harder,
/// keep the mix tonally identical).
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
    emph_eq: eq::runtime::chain::EqChain,
    deemph_eq: eq::runtime::chain::EqChain,
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
    #[must_use]
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
                let mut c = eq::runtime::chain::EqChain::new();
                c.set_sample_rate(sr);
                for _ in 0..EQ_BANDS {
                    let i = c
                        .add_band()
                        .expect("chain capacity reserved at construction");
                    if let Some(b) = c.band_mut(i) {
                        b.enabled = false;
                    }
                    c.update_band(i);
                }
                c
            },
            deemph_eq: {
                let mut c = eq::runtime::chain::EqChain::new();
                c.set_sample_rate(sr);
                for _ in 0..EQ_BANDS {
                    let i = c
                        .add_band()
                        .expect("chain capacity reserved at construction");
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
        self.emph_lo.set(
            SatFilterType::LowShelf { gain_db: -e * 0.5 },
            700.0,
            0.5,
            sr,
        );
        self.emph_hi.set(
            SatFilterType::HighShelf { gain_db: e * 0.5 },
            700.0,
            0.5,
            sr,
        );
        self.deemph_lo
            .set(SatFilterType::LowShelf { gain_db: e * 0.5 }, 700.0, 0.5, sr);
        self.deemph_hi.set(
            SatFilterType::HighShelf { gain_db: -e * 0.5 },
            700.0,
            0.5,
            sr,
        );
        if self.lf_hz > 1.0 {
            self.lf_split
                .set(SatFilterType::Highpass, self.lf_hz, 0.707, sr);
            self.lf_low
                .set(SatFilterType::Lowpass, self.lf_hz, 0.707, sr);
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
                for (chain, invert) in [(&mut self.emph_eq, false), (&mut self.deemph_eq, true)] {
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
                self.voice_bump
                    .set(SatFilterType::Peak { gain_db: 2.5 }, 60.0, 0.8, sr);
                self.voice_hf
                    .set(SatFilterType::Lowpass, 14_000.0, 0.707, sr);
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
            let sq = 0.5 * l[i].mul_add(l[i], r[i] * r[i]);
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
            let sq = 0.5 * l[i].mul_add(l[i], r[i] * r[i]);
            self.out_ms += (sq - self.out_ms) * 0.0005;
        }
        // RMS-matching auto gain (slewed).
        if self.auto_gain {
            let target = (self.in_ms.max(1.0e-12) / self.out_ms.max(1.0e-12))
                .sqrt()
                .clamp(0.1, 10.0);
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
                (yl - dl).mul_add(self.mix, dl),
                (yr - dr).mul_add(self.mix, dr),
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

// ── Tune (autotune / pitch correction) ────────────────────────────────────

const TUNE_PARAMS: &[ParamSpec] = &[
    // Key root pitch class (0 = C .. 11 = B).
    ParamSpec {
        id: 0,
        name: "key",
        min: 0.0,
        max: 11.0,
        default: 0.0,
    },
    // 0 Chromatic, 1 Major, 2 Minor.
    ParamSpec {
        id: 1,
        name: "scale",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // 0 = hard-tune jump; buffer-size-INDEPENDENT slew.
    ParamSpec {
        id: 2,
        name: "retune_ms",
        min: 0.0,
        max: 400.0,
        default: 40.0,
    },
    ParamSpec {
        id: 3,
        name: "strength",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // Flex-tune: only correct within this many cents of a target
    // (0 = always correct).
    ParamSpec {
        id: 4,
        name: "flex_cents",
        min: 0.0,
        max: 200.0,
        default: 0.0,
    },
    ParamSpec {
        id: 5,
        name: "a4_hz",
        min: 415.0,
        max: 466.0,
        default: 440.0,
    },
    ParamSpec {
        id: 6,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // 0 Scale, 1 MIDI latch (last note persists), 2 MIDI gate
    // (correct only while a note is held).
    ParamSpec {
        id: 7,
        name: "midi_mode",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Note-boundary hysteresis (stops target chatter).
    ParamSpec {
        id: 8,
        name: "hysteresis_cents",
        min: 0.0,
        max: 60.0,
        default: 15.0,
    },
];

/// Per-pitch-class bypass toggles (`pc_bypass_0`..`pc_bypass_11`, C..B):
/// classes left UNCORRECTED (blue notes).
pub const TUNE_PC_BYPASS_BASE: u32 = 40;
/// Readback: detected pitch in MIDI (float) and cents offset from the
/// nearest target — drives the tuner UI needle.
pub const TUNE_DETECTED_MIDI_ID: u32 = 100;
pub const TUNE_CENTS_OFF_ID: u32 = 101;

/// Native autotune block — YIN detection → keyflow-compatible scale snapping.
///
/// Hysteresis, per-PC bypass, flex-tune, A4 reference, buffer-size-independent retune slew,
/// and PSOLA shifting. MIDI target modes take notes straight from `PluginEvents.midi`.
pub struct NativeTune {
    detector: tune::detect::YinDetector,
    chain: pitch_dsp::chain::PitchChain,
    scale: tune::correct::Scale,
    key: u8,
    scale_kind: u32,
    retune_ms: f64,
    strength: f64,
    flex_cents: f64,
    a4_hz: f64,
    midi_mode: u32,
    hysteresis_cents: f64,
    bypass_mask: u16,
    /// Mono detection history (window-sized ring).
    history: Vec<f64>,
    hist_fill: usize,
    /// Current smoothed shift in semitones.
    shift_state: f64,
    /// Previous snapped target (hysteresis anchor).
    prev_target: Option<f64>,
    /// Held MIDI notes (stack; last = newest).
    held: Vec<u8>,
    /// Latched target from MIDI (mode 1).
    latched: Option<u8>,
    detected_midi: f64,
    cents_off: f64,
    sample_rate: f64,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeTune {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate.max(1.0);
        let detector = tune::detect::YinDetector::new(sr, tune::detect::YinConfig::default());
        let window = detector.window();
        let mut chain = pitch_dsp::chain::PitchChain::new();
        // PSOLA (pitch-synchronous — the right engine for voice), now
        // that its grain-boundary recentering is fixed: measured
        // +0.1/+1.3/+2.2 cents at -0.47/-2/+2 st, equal or better than
        // WSOLA (see autotune-plan.md status log).
        chain.algorithm = pitch_dsp::chain::Algorithm::Psola;
        chain.semitones = 0.0;
        chain.mix = 1.0;
        Self {
            detector,
            chain,
            scale: tune::correct::Scale::CHROMATIC,
            key: 0,
            scale_kind: 0,
            retune_ms: 40.0,
            strength: 1.0,
            flex_cents: 0.0,
            a4_hz: 440.0,
            midi_mode: 0,
            hysteresis_cents: 15.0,
            bypass_mask: 0,
            history: vec![0.0; window],
            hist_fill: 0,
            shift_state: 0.0,
            prev_target: None,
            held: Vec::with_capacity(16),
            latched: None,
            detected_midi: 0.0,
            cents_off: 0.0,
            sample_rate: sr,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn rebuild_scale(&mut self) {
        self.scale = match self.scale_kind {
            1 => tune::correct::Scale::major(self.key),
            2 => tune::correct::Scale::minor(self.key),
            _ => tune::correct::Scale {
                root_pc: self.key,
                ..tune::correct::Scale::CHROMATIC
            },
        };
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => {
                self.key = (v as u32 % 12) as u8;
                self.rebuild_scale();
            }
            1 => {
                self.scale_kind = v as u32;
                self.rebuild_scale();
            }
            2 => self.retune_ms = v.clamp(0.0, 400.0),
            3 => self.strength = v.clamp(0.0, 1.0),
            4 => self.flex_cents = v.clamp(0.0, 200.0),
            5 => self.a4_hz = v.clamp(415.0, 466.0),
            6 => self.chain.mix = v.clamp(0.0, 1.0),
            7 => self.midi_mode = v as u32,
            8 => self.hysteresis_cents = v.clamp(0.0, 60.0),
            i if (TUNE_PC_BYPASS_BASE..TUNE_PC_BYPASS_BASE + 12).contains(&i) => {
                let bit = 1u16 << (i - TUNE_PC_BYPASS_BASE);
                if v >= 0.5 {
                    self.bypass_mask |= bit;
                } else {
                    self.bypass_mask &= !bit;
                }
            }
            _ => {}
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(TUNE_PARAMS, name) {
            self.set(id, value);
            return;
        }
        if let Some(pc) = name
            .strip_prefix("pc_bypass_")
            .and_then(|s| s.parse::<u32>().ok())
        {
            if pc < 12 {
                self.set(TUNE_PC_BYPASS_BASE + pc, value);
            }
        }
    }

    /// hz → MIDI honoring the A4 reference.
    fn to_midi(&self, hz: f64) -> f64 {
        12.0f64.mul_add((hz / self.a4_hz).log2(), 69.0)
    }

    /// Choose the correction target for the detected pitch.
    fn target_for(&mut self, midi: f64) -> Option<f64> {
        match self.midi_mode {
            1 => self.latched.map(f64::from),
            2 => self.held.last().copied().map(f64::from),
            _ => self.scale.snap_hysteresis(
                midi,
                self.prev_target,
                self.hysteresis_cents,
                self.bypass_mask,
            ),
        }
    }
}

impl PluginInstance for NativeTune {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.tune", "Tune")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        let mut out = param_infos(TUNE_PARAMS);
        for pc in 0..12u32 {
            out.push(PluginParamInfo {
                id: TUNE_PC_BYPASS_BASE + pc,
                name: format!("pc_bypass_{pc}"),
                min: 0.0,
                max: 1.0,
                default: 0.0,
            });
        }
        out
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        match id {
            TUNE_DETECTED_MIDI_ID => Some(self.detected_midi),
            TUNE_CENTS_OFF_ID => Some(self.cents_off),
            _ => None,
        }
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        match id {
            0 => Some(NAMES[(value as usize) % 12].into()),
            1 => Some(
                match value as u32 {
                    1 => "Major",
                    2 => "Minor",
                    _ => "Chromatic",
                }
                .into(),
            ),
            2 => Some(if value < 0.5 {
                "Hard".into()
            } else {
                format!("{value:.0} ms")
            }),
            _ => None,
        }
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        self.chain.latency() as u32
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.sample_rate = sample_rate.max(1.0);
        self.detector =
            tune::detect::YinDetector::new(self.sample_rate, tune::detect::YinConfig::default());
        self.history = vec![0.0; self.detector.window()];
        self.hist_fill = 0;
        use audiocore_dsp::Processor;
        self.chain.update(audiocore_dsp::AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: block_size.max(1) as usize,
        });
        self.chain.reset();
        self.shift_state = 0.0;
        self.prev_target = None;
        let n = block_size.max(1) as usize;
        self.scratch_l = vec![0.0; n];
        self.scratch_r = vec![0.0; n];
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
        // MIDI target-note bookkeeping.
        for ev in events.midi {
            match &ev.message {
                daw_proto::MidiEvent::NoteOn { key, velocity, .. } => {
                    if velocity.get() > 0 {
                        let k = key.get();
                        self.held.retain(|&h| h != k);
                        if self.held.len() < 16 {
                            self.held.push(k);
                        }
                        self.latched = Some(k);
                    } else {
                        let k = key.get();
                        self.held.retain(|&h| h != k);
                    }
                }
                daw_proto::MidiEvent::NoteOff { key, .. } => {
                    let k = key.get();
                    self.held.retain(|&h| h != k);
                }
                _ => {}
            }
        }

        let n = in_l.len().min(out_l.len());
        // Slide the mono detection history.
        let window = self.history.len();
        for i in 0..n {
            let mono = 0.5 * (f64::from(in_l[i]) + f64::from(in_r[i]));
            if self.hist_fill < window {
                self.history[self.hist_fill] = mono;
                self.hist_fill += 1;
            } else {
                self.history.copy_within(1.., 0);
                self.history[window - 1] = mono;
            }
        }

        // Detect + retarget once per block.
        let mut target_shift = 0.0;
        if self.hist_fill >= window {
            let frame = self.detector.detect(&self.history);
            if let Some(hz) = frame.f0_hz {
                let midi = self.to_midi(hz);
                self.detected_midi = midi;
                if let Some(target) = self.target_for(midi) {
                    self.prev_target = Some(target);
                    self.cents_off = (midi - target) * 100.0;
                    let flex_ok = self.flex_cents <= 0.0 || self.cents_off.abs() <= self.flex_cents;
                    // MIDI gate with no note held → no correction.
                    let gated = self.midi_mode == 2 && self.held.is_empty();
                    if flex_ok && !gated {
                        target_shift = (target - midi) * self.strength;
                    }
                }
            } else {
                self.prev_target = None;
                self.cents_off = 0.0;
            }
        }

        // Buffer-size-INDEPENDENT retune slew: settle by the block's
        // real duration (retune_ms = time constant), snap when hard.
        if self.retune_ms < 0.5 {
            self.shift_state = target_shift;
        } else {
            let a = (-(n as f64) / (self.retune_ms * 0.001 * self.sample_rate)).exp();
            self.shift_state = (self.shift_state - target_shift).mul_add(a, target_shift);
        }
        self.chain.semitones = self.shift_state.clamp(-24.0, 24.0);

        // Shift.
        for i in 0..n {
            self.scratch_l[i] = f64::from(in_l[i]);
            self.scratch_r[i] = f64::from(in_r[i]);
        }
        use audiocore_dsp::Processor;
        self.chain
            .process(&mut self.scratch_l[..n], &mut self.scratch_r[..n]);
        for i in 0..n {
            out_l[i] = self.scratch_l[i] as f32;
            out_r[i] = self.scratch_r[i] as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Compressor ─────────────────────────────────────────────────────────────

/// Live compressor telemetry — the rolling waveform + GR the FTS-Comp editor shows.
///
/// A single global ring: only the *active* chain's comp processes audio, so whichever
/// instance is running owns the meters (multiband/per-instance telemetry comes with real
/// per-block channels).
pub mod comp_meter {
    //! Compressor telemetry for the compressor panels: a rolling input-peak
    //! and gain-reduction ring, and the current GR, **per meter channel**.
    //!
    //! A compressor block publishes to the channel its build-time `meter`
    //! value names (0 = none). One shared ring meant every compressor in a
    //! chain wrote the same trace — two active compressors interleaved into
    //! nonsense, and with the panel's compressor bypassed the output limiter
    //! drew the chain's final output in its place. The rig gives each of its
    //! compressor blocks its own channel, and each panel reads its own.
    use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    /// Ring length — 960 slots at ~240 Hz ≈ a 4-second window (matches the
    /// FTS-Comp editor).
    pub const WAVE_LEN: usize = 960;
    /// Samples between ring writes (~240 Hz at 48 kHz).
    pub const WAVE_INTERVAL: usize = 200;
    /// GR normalization for the ring (dB full scale).
    pub const GR_FS_DB: f32 = 30.0;
    /// Meter channels, 0 unused ("no meter"): enough for a chain's pre and
    /// post compressors and its limiter, with room.
    pub const CHANNELS: usize = 8;
    /// The channel a compressor publishes to when nothing says otherwise.
    pub const DEFAULT_CHANNEL: usize = 1;

    struct Channel {
        wave_in: [AtomicU32; WAVE_LEN],
        wave_gr: [AtomicU32; WAVE_LEN],
        pos: AtomicUsize,
        gr_db: AtomicU32,
    }

    static METERS: [Channel; CHANNELS] = [const {
        Channel {
            wave_in: [const { AtomicU32::new(0) }; WAVE_LEN],
            wave_gr: [const { AtomicU32::new(0) }; WAVE_LEN],
            pos: AtomicUsize::new(0),
            gr_db: AtomicU32::new(0),
        }
    }; CHANNELS];

    fn channel(ch: usize) -> Option<&'static Channel> {
        (ch > 0).then(|| METERS.get(ch)).flatten()
    }

    pub(crate) fn push(ch: usize, input_peak: f32, gr_norm: f32) {
        let Some(m) = channel(ch) else { return };
        let pos = m.pos.load(Ordering::Relaxed) % WAVE_LEN;
        m.wave_in[pos].store(input_peak.to_bits(), Ordering::Relaxed);
        m.wave_gr[pos].store(gr_norm.to_bits(), Ordering::Relaxed);
        m.pos.store(pos + 1, Ordering::Relaxed);
    }

    pub(crate) fn set_gr_db(ch: usize, gr: f32) {
        if let Some(m) = channel(ch) {
            m.gr_db.store(gr.to_bits(), Ordering::Relaxed);
        }
    }

    /// Clear a channel — its compressor was bypassed or removed, and a trace
    /// frozen at its last value would claim it is still working.
    pub fn clear(ch: usize) {
        let Some(m) = channel(ch) else { return };
        for (a, b) in m.wave_in.iter().zip(&m.wave_gr) {
            a.store(0, Ordering::Relaxed);
            b.store(0, Ordering::Relaxed);
        }
        m.gr_db.store(0, Ordering::Relaxed);
    }

    /// Current gain reduction on channel `ch` (dB, positive = reducing).
    #[must_use]
    pub fn gr_db_of(ch: usize) -> f32 {
        channel(ch).map_or(0.0, |m| f32::from_bits(m.gr_db.load(Ordering::Relaxed)))
    }

    /// [`gr_db_of`] the default channel.
    #[must_use]
    pub fn gr_db() -> f32 {
        gr_db_of(DEFAULT_CHANNEL)
    }

    /// Snapshot channel `ch`'s ring in time order (oldest → newest),
    /// downsampled by `stride`. Returns `(input_peaks 0..1, gr 0..1)`.
    ///
    /// Downsample groups are anchored to **absolute ring slots** (not the
    /// write position): a display column always summarises the same
    /// samples until they scroll out, so the trace crawls smoothly instead
    /// of shimmering as the head moves through a group.
    #[must_use]
    pub fn wave_snapshot_of(ch: usize, stride: usize) -> (Vec<f32>, Vec<f32>) {
        let Some(m) = channel(ch) else {
            return (Vec::new(), Vec::new());
        };
        let stride = stride.max(1);
        let n_groups = WAVE_LEN / stride;
        let pos = m.pos.load(Ordering::Relaxed) % WAVE_LEN;
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
                pi = pi.max(f32::from_bits(m.wave_in[idx].load(Ordering::Relaxed)));
                pg = pg.max(f32::from_bits(m.wave_gr[idx].load(Ordering::Relaxed)));
            }
            input.push(pi);
            gr.push(pg);
        }
        (input, gr)
    }

    /// [`wave_snapshot_of`] the default channel.
    #[must_use]
    pub fn wave_snapshot(stride: usize) -> (Vec<f32>, Vec<f32>) {
        wave_snapshot_of(DEFAULT_CHANNEL, stride)
    }
}

/// What the time and modulation effects are doing — for a remote's glow
/// behind a knob while its effect works.
///
/// Each delay, reverb, modulation or tremolo block publishes the peak of
/// what it adds to the signal (its output less its input) each block; the
/// rig reads and clears it at meter rate ([`take`]). Several blocks of a
/// kind publish their loudest.
pub mod fx_meter {
    use core::sync::atomic::{AtomicU32, Ordering};

    /// Delay, reverb, modulation, tremolo.
    pub const KINDS: usize = 4;
    pub const DELAY: usize = 0;
    pub const REVERB: usize = 1;
    pub const MODULATION: usize = 2;
    pub const TREMOLO: usize = 3;

    static PEAKS: [AtomicU32; KINDS] = [const { AtomicU32::new(0) }; KINDS];

    /// Publish a block's added peak (linear, ≥ 0). Positive `f32` bits order
    /// as the values do, so `fetch_max` on the bits keeps the loudest.
    pub fn publish(kind: usize, peak: f32) {
        if let Some(p) = PEAKS.get(kind) {
            p.fetch_max(peak.max(0.0).to_bits(), Ordering::Relaxed);
        }
    }

    /// The loudest added peak of `kind` since the last take, and clear it.
    #[must_use]
    pub fn take(kind: usize) -> f32 {
        PEAKS.get(kind).map_or(0.0, |p| f32::from_bits(p.swap(0, Ordering::Relaxed)))
    }

    /// The peak a block adds: its output less its input.
    #[must_use]
    pub fn added(in_l: &[f32], in_r: &[f32], out_l: &[f32], out_r: &[f32]) -> f32 {
        let l = in_l.iter().zip(out_l).fold(0.0f32, |m, (a, b)| m.max((b - a).abs()));
        let r = in_r.iter().zip(out_r).fold(0.0f32, |m, (a, b)| m.max((b - a).abs()));
        l.max(r)
    }
}

/// Global DI sidechain — the rig's input probe publishes the clean guitar peak per block.
///
/// The gate keys off it so gating stays tight regardless of what the tone (amp/EQ/drive)
/// does to the signal it actually gates.
pub mod sidechain {
    //! The clean-DI key a post-amp gate listens to: the rig's input probe
    //! publishes each block's DI peak, the gate reads it.
    //!
    //! **Per thread**, not per process. A chain renders on one thread — the
    //! probe at its head and the gate further down run in the same callback
    //! — so a thread-local is exactly "this chain's key". A process-wide
    //! value let any other chain rendering at the same time (an offline
    //! levelling render beside the live rig, or several side by side) key
    //! this chain's gate with its own guitar.

    use core::cell::Cell;

    std::thread_local! {
        static PEAK: Cell<Option<f32>> = const { Cell::new(None) };
    }

    /// Publish the current DI block peak (linear) for this thread's chain.
    pub fn set_peak(peak: f32) {
        PEAK.with(|p| p.set(Some(peak)));
    }

    /// The DI peak, if a probe on this thread is publishing.
    pub fn peak() -> Option<f32> {
        PEAK.with(Cell::get)
    }
}

const COMP_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "threshold",
        min: -60.0,
        max: 0.0,
        default: -18.0,
    },
    ParamSpec {
        id: 1,
        name: "ratio",
        min: 1.0,
        max: 20.0,
        default: 4.0,
    },
    ParamSpec {
        id: 2,
        name: "attack",
        min: 0.1,
        max: 200.0,
        default: 10.0,
    },
    ParamSpec {
        id: 3,
        name: "release",
        min: 5.0,
        max: 1000.0,
        default: 120.0,
    },
    ParamSpec {
        id: 4,
        name: "knee",
        min: 0.0,
        max: 24.0,
        default: 6.0,
    },
    ParamSpec {
        id: 5,
        name: "range",
        min: 0.0,
        max: 60.0,
        default: 60.0,
    },
    ParamSpec {
        id: 6,
        name: "fold",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 7,
        name: "style",
        min: 0.0,
        max: 4.0,
        default: 0.0,
    },
    // Output level after the blend (dB). On a comp in front of an amp it is
    // how hard the amp is driven, so it is part of the preset.
    ParamSpec {
        id: 8,
        name: "makeup",
        min: -24.0,
        max: 24.0,
        default: 0.0,
    },
    // Wet share of a parallel blend with the dry input (0 = dry, 1 = fully
    // compressed) — the pedal "blend" knob that keeps the pick attack under a
    // heavy squash. The compressor has no lookahead, so the two paths are
    // time-aligned and blend without combing.
    ParamSpec {
        id: 9,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
];

/// Native Compressor block — wraps [`comp::ProC3Compressor`] (ProC3-style).
/// Seeded with a musical default (−18 dB / 4:1).
pub struct NativeComp {
    comp: comp::ProC3Compressor,
    prepared: bool,
    /// Linear output gain (`makeup`, dB).
    makeup: f64,
    /// Parallel blend, 0 = dry … 1 = compressed (`mix`).
    mix: f64,
    /// The [`comp_meter`] channel this compressor draws on (0 = none) — its
    /// own panel's trace. Set by the build-time `meter` value.
    meter: usize,
    // Waveform decimation state (comp_meter ring).
    wave_counter: usize,
    wave_peak: f32,
    wave_gr_peak: f32,
}

impl NativeComp {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut comp = comp::ProC3Compressor::new(sample_rate.max(1.0));
        comp.set_threshold(-18.0);
        comp.set_ratio(4.0);
        comp.set_attack_ms(10.0);
        comp.set_release_ms(120.0);
        Self {
            comp,
            prepared: false,
            makeup: 1.0,
            mix: 1.0,
            meter: comp_meter::DEFAULT_CHANNEL,
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
            8 => self.makeup = 10f64.powf(v.clamp(-24.0, 24.0) / 20.0),
            9 => self.mix = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    /// Apply a build-time parameter by name (any `COMP_PARAMS` name, or `meter`).
    pub fn set_named(&mut self, name: &str, value: f64) {
        // Build-time only, and deliberately not a `COMP_PARAMS` entry: it is
        // wiring, not a knob a host should automate.
        if name == "meter" {
            self.meter = (value.round().max(0.0) as usize).min(comp_meter::CHANNELS - 1);
            return;
        }
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
        // Blend first, then the output level — a pedal's Level knob.
        let (wet, dry) = (self.mix * self.makeup, (1.0 - self.mix) * self.makeup);
        for i in 0..n {
            let (l, r) = (f64::from(in_l[i]), f64::from(in_r[i]));
            out_l[i] = (self.comp.process(l, 0) * wet + l * dry) as f32;
            out_r[i] = (self.comp.process(r, 1) * wet + r * dry) as f32;
            // Telemetry: rolling input peak + GR ring (see `comp_meter`).
            if self.meter == 0 {
                continue;
            }
            let in_peak = in_l[i].abs().max(in_r[i].abs());
            self.wave_peak = self.wave_peak.max(in_peak);
            self.wave_counter += 1;
            if self.wave_counter >= comp_meter::WAVE_INTERVAL {
                let gr = self.comp.gain_reduction_db() as f32;
                self.wave_gr_peak = gr / comp_meter::GR_FS_DB;
                comp_meter::push(
                    self.meter,
                    self.wave_peak.min(1.0),
                    self.wave_gr_peak.clamp(0.0, 1.0),
                );
                comp_meter::set_gr_db(self.meter, gr);
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
    ParamSpec {
        id: 0,
        name: "gate_threshold",
        min: -80.0,
        max: 0.0,
        default: -45.0,
    },
    ParamSpec {
        id: 1,
        name: "gate_range",
        min: -80.0,
        max: 0.0,
        default: -80.0,
    },
    ParamSpec {
        id: 2,
        name: "gate_attack",
        min: 0.1,
        max: 200.0,
        default: 1.0,
    },
    ParamSpec {
        id: 3,
        name: "gate_release",
        min: 5.0,
        max: 1000.0,
        default: 120.0,
    },
    // De-breath.
    ParamSpec {
        id: 4,
        name: "debreath_reduction",
        min: 0.0,
        max: 40.0,
        default: 12.0,
    },
    ParamSpec {
        id: 5,
        name: "debreath_max_level",
        min: -60.0,
        max: 0.0,
        default: -28.0,
    },
    // Rider.
    ParamSpec {
        id: 6,
        name: "ride_target",
        min: -40.0,
        max: 0.0,
        default: -18.0,
    },
    ParamSpec {
        id: 7,
        name: "ride_amount",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 8,
        name: "ride_max_gain",
        min: 0.0,
        max: 24.0,
        default: 12.0,
    },
    ParamSpec {
        id: 9,
        name: "ride_max_cut",
        min: 0.0,
        max: 24.0,
        default: 18.0,
    },
    // De-ess.
    ParamSpec {
        id: 10,
        name: "deess_freq",
        min: 2000.0,
        max: 12000.0,
        default: 6500.0,
    },
    ParamSpec {
        id: 11,
        name: "deess_threshold",
        min: -60.0,
        max: 0.0,
        default: -30.0,
    },
    ParamSpec {
        id: 12,
        name: "deess_ratio",
        min: 1.0,
        max: 20.0,
        default: 4.0,
    },
    ParamSpec {
        id: 13,
        name: "deess_range",
        min: 0.0,
        max: 40.0,
        default: 12.0,
    },
    // Shared adaptive silence floor.
    ParamSpec {
        id: 14,
        name: "silence",
        min: -90.0,
        max: -20.0,
        default: -45.0,
    },
];

/// Native Leveler block — wraps [`level_dsp::VocalLeveler`], the realtime vocal chain.
///
/// Gate → de-breath → ride → de-ess. The core is mono, so L/R each get their own leveler
/// instance (independent state). Every stage parameter is an enumerable, macro/modulation-targetable
/// [`PluginParamInfo`].
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
    #[must_use]
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

    const fn set(&mut self, id: u32, v: f64) {
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
            out_l[i] = self.left.process_sample(f64::from(in_l[i])) as f32;
            out_r[i] = self.right.process_sample(f64::from(in_r[i])) as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Reverb ─────────────────────────────────────────────────────────────────

const REVERB_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 0.08,
    },
    ParamSpec {
        id: 1,
        name: "decay",
        min: 0.0,
        max: 1.0,
        default: 0.45,
    },
    ParamSpec {
        id: 2,
        name: "size",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    // BigSky MX dual-reverb block: routing (0 Single / 1 Series 1>2 /
    // 2 Series 2>1 / 3 Parallel / 4 Split / 5 Split Swap) + reverb B.
    // Ids 0-2 keep addressing reverb A.
    ParamSpec {
        id: 3,
        name: "routing",
        min: 0.0,
        max: 5.0,
        default: 0.0,
    },
    ParamSpec {
        id: 4,
        name: "algo_b",
        min: 0.0,
        max: 14.0,
        default: 2.0,
    },
    ParamSpec {
        id: 5,
        name: "decay_b",
        min: 0.0,
        max: 1.0,
        default: 0.45,
    },
    ParamSpec {
        id: 6,
        name: "mix_b",
        min: 0.0,
        max: 1.0,
        default: 0.08,
    },
    // Per-slot wet pan (-1..+1) and wet tremolo (shared A knob set).
    ParamSpec {
        id: 7,
        name: "pan_a",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 8,
        name: "pan_b",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 9,
        name: "trem_rate",
        min: 0.1,
        max: 12.0,
        default: 4.0,
    },
    ParamSpec {
        id: 10,
        name: "trem_depth",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // BigSky MX Impulse live params (chain A; active with the
    // Convolution algorithm). tail: 0 = Envelope, 1 = Gate;
    // direction: 0 = Forward, 1 = Reverse.
    ParamSpec {
        id: 11,
        name: "imp_decay",
        min: 0.01,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 12,
        name: "imp_tail",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 13,
        name: "imp_attack",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 14,
        name: "imp_stretch",
        min: 0.25,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 15,
        name: "imp_direction",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 16,
        name: "imp_feedback",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // BigSky MX Shimmer (chain A): two shift voices in semitones,
    // shared amount, feedback mode (0 Input / 1 Regenerative /
    // 2 Input+Regen). shim_voice2: 0 = single voice, 1 = dual.
    ParamSpec {
        id: 17,
        name: "shim_shift1",
        min: -12.0,
        max: 12.0,
        default: 12.0,
    },
    ParamSpec {
        id: 18,
        name: "shim_shift2",
        min: -12.0,
        max: 12.0,
        default: 7.0,
    },
    ParamSpec {
        id: 19,
        name: "shim_voice2",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 20,
        name: "shim_amount",
        min: 0.0,
        max: 1.0,
        default: 0.35,
    },
    ParamSpec {
        id: 21,
        name: "shim_fb_mode",
        min: 0.0,
        max: 2.0,
        default: 1.0,
    },
    // BigSky MX Magneto: taps alternate hard L/R.
    ParamSpec {
        id: 22,
        name: "mag_ping_pong",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // BigSky MX NonLinear: Chop trem on the decay, explicit gate speed,
    // separate Late reverb stage.
    ParamSpec {
        id: 23,
        name: "nl_chop_rate",
        min: 0.1,
        max: 15.0,
        default: 4.0,
    },
    ParamSpec {
        id: 24,
        name: "nl_chop_depth",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 25,
        name: "nl_gate_speed",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 26,
        name: "nl_late_speed",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    ParamSpec {
        id: 27,
        name: "nl_late_decay",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    ParamSpec {
        id: 28,
        name: "nl_late_level",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // BigSky MX input-analysis generators: Cloud Ensemble (pitch-tracked
    // synthetic string layer), Bloom Harmonics (overtone generator on
    // the trail), Chorale Choir level / Voice (0 Tenor / 1 Soprano) /
    // Mod (per-voice randomization). cho_voice: 0 Tenor / 1 Baritone.
    ParamSpec {
        id: 29,
        name: "cloud_ensemble",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 30,
        name: "bloom_harmonics",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 31,
        name: "cho_choir",
        min: 0.0,
        max: 1.0,
        default: 0.3,
    },
    ParamSpec {
        id: 32,
        name: "cho_voice",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 33,
        name: "cho_mod",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // BigSky MX voices + Hall extras + named-size select (chain A).
    // voice: 0 MX / 1 Classic. hall_swell_type: 0 wet / 1 wet+dry.
    // size_sel maps named sizes (Hall Concert/Arena, Room Studio/Club,
    // else Small/Medium/Large) — applies when set.
    ParamSpec {
        id: 34,
        name: "voice",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 35,
        name: "hall_mid",
        min: -6.0,
        max: 6.0,
        default: 0.0,
    },
    ParamSpec {
        id: 36,
        name: "hall_swell_rise",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 37,
        name: "hall_swell_type",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 38,
        name: "size_sel",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Chain-A tone/space controls (kept from the control-surface work,
    // renumbered above the MX block).
    // Algorithm index (see reverb::AlgorithmType::ALL).
    ParamSpec {
        id: 39,
        name: "algorithm",
        min: 0.0,
        max: 14.0,
        default: 1.0,
    },
    ParamSpec {
        id: 40,
        name: "modulation",
        min: 0.0,
        max: 1.0,
        default: 0.2,
    },
    // High-frequency damping — the low-pass character control.
    ParamSpec {
        id: 41,
        name: "damping",
        min: 0.0,
        max: 1.0,
        default: 0.3,
    },
    // Tone tilt (−1 dark … +1 bright) — the high-pass-ish control.
    ParamSpec {
        id: 42,
        name: "tone",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 43,
        name: "predelay",
        min: 0.0,
        max: 1500.0,
        default: 0.0,
    },
    // INFINITE footswitch: engage (0/1) + per-preset mode
    // (0 Freeze / 1 Infinite / 2 Off). Latch-vs-momentary is the
    // footswitch controller's concern; `freeze` covers both.
    ParamSpec {
        id: 44,
        name: "freeze",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 45,
        name: "inf_mode",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Chamber Color (0 Neutral / 1 Clear / 2 Smooth / 3 Crisp / 4 Deep).
    ParamSpec {
        id: 46,
        name: "chamber_color",
        min: 0.0,
        max: 4.0,
        default: 0.0,
    },
    // Magneto: head count menu (0 One / 1 Two / 2 Three / 3 Four /
    // 4 Six) + spacing (0 Even / 1 Uneven). With Magneto active the
    // pedal remaps predelay -> engine feedback and decay -> last-head
    // time; the chain handles that from the existing decay/predelay ids.
    ParamSpec {
        id: 47,
        name: "mag_heads",
        min: 0.0,
        max: 4.0,
        default: 3.0,
    },
    ParamSpec {
        id: 48,
        name: "mag_spacing",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // NonLinear Shape (manual order: 0 Swoosh / 1 Reverse / 2 Ramp /
    // 3 Gate / 4 Gauss / 5 Bounce). With NonLinear active the pedal
    // remaps decay -> nonlinear-window time and predelay -> generator
    // feedback; the chain handles both from the existing ids.
    ParamSpec {
        id: 49,
        name: "nl_shape",
        min: 0.0,
        max: 5.0,
        default: 0.0,
    },
    // Spring: Dwell drive stage (0 Clean / 1 Combo / 2 Tube /
    // 3 Overdrive) + Number of Springs (0 One / 1 Two / 2 Three).
    ParamSpec {
        id: 50,
        name: "spring_dwell",
        min: 0.0,
        max: 3.0,
        default: 0.0,
    },
    ParamSpec {
        id: 51,
        name: "spring_num",
        min: 0.0,
        max: 2.0,
        default: 1.0,
    },
    // Common per-reverb params the MX menu carries on most engines:
    // Diffusion (ER softening/density) and Low End (low-frequency
    // content + decay profile; 0.5 = neutral, above = lows ring longer
    // / "larger spaces", below = lows tamed).
    ParamSpec {
        id: 52,
        name: "diffusion",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    ParamSpec {
        id: 53,
        name: "low_end",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    // Chorale: vowel program (0 AAHHOO / 1 AAHH / 2 AAHHOH / 3 OH /
    // 4 OOOHOH / 5 OOO / 6 Random) + Resonance (0 Mild / 1 Medium /
    // 2 High).
    ParamSpec {
        id: 54,
        name: "cho_vowel",
        min: 0.0,
        max: 6.0,
        default: 1.0,
    },
    ParamSpec {
        id: 55,
        name: "cho_resonance",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Impulse Decay EQ: frequency-dependent decay shaping (end-of-tail
    // band gains in dB at ~250 Hz / ~4 kHz; negative shortens that
    // band's decay, positive stretches it). FTS extra beyond the MX
    // surface.
    ParamSpec {
        id: 56,
        name: "imp_decay_lo",
        min: -24.0,
        max: 12.0,
        default: 0.0,
    },
    ParamSpec {
        id: 57,
        name: "imp_decay_hi",
        min: -24.0,
        max: 12.0,
        default: 0.0,
    },
    // Algorithm variant. Several engines ship more than one tuning
    // (Room: Medium / Chamber / Studio; Hall: Concert / Cathedral /
    // Arena; Plate: Dattorro / Lexicon / Progenitor; Spring: MX /
    // Vintage). `size_sel` and `voice` reach some of them, but Room
    // Chamber and Hall Cathedral were unreachable from the parameter
    // surface until this existed — the DSP was there, nothing selected
    // it. Out-of-range values fall back to variant 0 inside the chain.
    ParamSpec {
        id: 58,
        name: "variant",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Decay expressed as a target reverberation time in seconds, mapped
    // through the active engine's own T60 range. `decay` (id 1) spans a
    // different span of time in every algorithm, so it cannot express
    // "2.5 seconds" — this can, which is what makes a reference preset
    // translatable. Ignored by engines with no time model (the plate tank
    // and the character engines); set `decay` for those.
    ParamSpec {
        id: 59,
        name: "decay_time",
        min: 0.05,
        max: 60.0,
        default: 2.0,
    },
    // Decay Rate EQ: eight bands of decay-TIME multipliers over frequency
    // (`fx.reverb.decay-eq`), one per measured octave. `rate` 1.0 is the
    // space's natural decay, 4.0 four times longer at that band, 0.1 a tenth.
    //
    // The DSP has always been there and every Room, Hall and Random engine
    // feeds it; nothing exposed it, so a reverb could match how LONG a tail
    // lasts but not how its colour changes as it decays. `shape`: 0 Bell,
    // 1 Low Shelf, 2 High Shelf.
    ParamSpec {
        id: 60,
        name: "dband1_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 61,
        name: "dband1_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 62,
        name: "dband1_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 63,
        name: "dband1_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 64,
        name: "dband2_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 65,
        name: "dband2_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 66,
        name: "dband2_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 67,
        name: "dband2_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 68,
        name: "dband3_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 69,
        name: "dband3_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 70,
        name: "dband3_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 71,
        name: "dband3_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 72,
        name: "dband4_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 73,
        name: "dband4_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 74,
        name: "dband4_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 75,
        name: "dband4_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 76,
        name: "dband5_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 77,
        name: "dband5_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 78,
        name: "dband5_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 79,
        name: "dband5_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 80,
        name: "dband6_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 81,
        name: "dband6_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 82,
        name: "dband6_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 83,
        name: "dband6_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 84,
        name: "dband7_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 85,
        name: "dband7_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 86,
        name: "dband7_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 87,
        name: "dband7_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    ParamSpec {
        id: 88,
        name: "dband8_shape",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 89,
        name: "dband8_freq",
        min: 20.0,
        max: 20000.0,
        default: 1000.0,
    },
    ParamSpec {
        id: 90,
        name: "dband8_rate",
        min: 0.1,
        max: 4.0,
        default: 1.0,
    },
    ParamSpec {
        id: 91,
        name: "dband8_q",
        min: 0.1,
        max: 10.0,
        default: 0.707,
    },
    // Wet-bus output trim. Distinct from `mix`: mix balances wet against
    // dry, this sets how loud the wet bus itself is. A preset translated
    // from another reverb needs the second one — the balance it describes
    // is its own, and what does not survive translation is the absolute
    // level the engine puts out.
    ParamSpec {
        id: 92,
        name: "wet_gain",
        min: -36.0,
        max: 36.0,
        default: 0.0,
    },
    // The dry guitar's level through the block (1 = all of it); `mix` is how
    // much reverb is added in parallel. See the delay's `dry`.
    ParamSpec {
        id: 93,
        name: "dry",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // The reverb's level against the dry, in dB — a gain on the wet, on top
    // of `mix`. A host running the reverb fully parallel pins `mix` at 1
    // and dials this instead.
    ParamSpec {
        id: 94,
        name: "level",
        min: -60.0,
        max: 12.0,
        default: 0.0,
    },
    // The wet's band: a low cut to keep the tail out of the guitar's low
    // end (150–500 Hz on guitar) and a high cut to take the fizz off it
    // (4–8 kHz). 20 Hz / 20 kHz = open.
    ParamSpec {
        id: 95,
        name: "low_cut",
        min: 20.0,
        max: 2000.0,
        default: 20.0,
    },
    ParamSpec {
        id: 96,
        name: "high_cut",
        min: 1000.0,
        max: 20000.0,
        default: 20000.0,
    },
    // Ducking: the wet drops by `duck` (0 = none, 1 = silent) while the
    // guitar plays over `duck_threshold` (dB), and swells back over
    // `duck_release` (ms) when it stops — a big reverb that clears for the
    // notes and blooms in the gaps.
    ParamSpec {
        id: 97,
        name: "duck",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 98,
        name: "duck_threshold",
        min: -60.0,
        max: 0.0,
        default: -20.0,
    },
    ParamSpec {
        id: 99,
        name: "duck_release",
        min: 20.0,
        max: 2000.0,
        default: 120.0,
    },
];

/// Native Reverb block — wraps [`reverb::DualReverb`] (two full chains +
/// `BigSky` MX dual routing; `Single` = chain A only, bit-compatible with
/// the previous single-chain wrapper).
///
/// Defaults to a subtle Hall (low mix) so it sits under the tone rather than
/// washing it out.
/// A time effect in parallel with the dry signal: the dry passes at `dry`
/// (default all of it) and the effect's wet output is added at `wet` (the
/// block's `mix`). The engines crossfade (`dry·(1−mix) + wet·mix`), which
/// turned the guitar down by the mix whenever a delay or reverb was engaged
/// — so a patch got several dB louder when its effects were bypassed. With
/// a single chain the engine runs fully wet and the sum is made here; both
/// are zero-latency, so the paths line up.
#[derive(Clone, Copy)]
struct ParallelMix {
    /// The wet gain: `mix · level`.
    wet: f64,
    mix: f64,
    /// `level` as a linear gain.
    level: f64,
    dry: f64,
    wet_s: f64,
    dry_s: f64,
}

impl ParallelMix {
    const fn new(mix: f64) -> Self {
        Self {
            wet: mix,
            mix,
            level: 1.0,
            dry: 1.0,
            wet_s: mix,
            dry_s: 1.0,
        }
    }

    fn set_mix(&mut self, v: f64) {
        self.mix = v.clamp(0.0, 1.0);
        self.wet = self.mix * self.level;
    }

    fn set_level_db(&mut self, db: f64) {
        self.level = 10f64.powf(db.clamp(-60.0, 12.0) / 20.0);
        self.wet = self.mix * self.level;
    }

    /// `out = dry·in + wet·out`, per sample, with ~5 ms smoothing on both
    /// gains (the engine's own mix smoother is pinned at 1 here).
    fn apply(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        const K: f64 = 0.004;
        let n = out_l.len().min(out_r.len()).min(in_l.len()).min(in_r.len());
        for i in 0..n {
            self.wet_s += (self.wet - self.wet_s) * K;
            self.dry_s += (self.dry - self.dry_s) * K;
            out_l[i] = (f64::from(in_l[i]) * self.dry_s + f64::from(out_l[i]) * self.wet_s) as f32;
            out_r[i] = (f64::from(in_r[i]) * self.dry_s + f64::from(out_r[i]) * self.wet_s) as f32;
        }
    }
}

pub struct NativeReverb {
    rev: reverb::DualReverb,
    /// Writes for chain B before it is built (`DualReverb` builds B only for
    /// a routing that plays it), in order — replayed onto it as it is.
    b_log: Vec<(u32, f64)>,
    prepared: bool,
    /// Chain A in parallel with the dry (see [`ParallelMix`]).
    par: ParallelMix,
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
    #[must_use]
    pub fn new(_sample_rate: f64) -> Self {
        let mut rev = reverb::DualReverb::new();
        rev.a.set_algorithm(reverb::AlgorithmType::Hall);
        // Fully wet: single routing sums the dry itself (`ParallelMix`).
        rev.a.mix = 1.0;
        rev.a.params.decay = 0.45;
        rev.a.params.size = 0.5;
        rev.a.update_params();
        // B (a plate, see `seed_b`) is built when a routing plays it.
        Self {
            rev,
            b_log: Vec::new(),
            prepared: false,
            par: ParallelMix::new(0.08),
            #[cfg(not(target_arch = "wasm32"))]
            reshaper_a: None,
            #[cfg(not(target_arch = "wasm32"))]
            reshaper_b: None,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        // Chain A's mix and the dry are the parallel sum's (see
        // `ParallelMix`); the engine's own mix is derived in `sync_mix`.
        match id {
            0 => {
                self.par.set_mix(v);
                self.sync_mix();
                return;
            }
            94 => {
                self.par.set_level_db(v);
                self.sync_mix();
                return;
            }
            93 => {
                self.par.dry = v.clamp(0.0, 1.0);
                return;
            }
            _ => {}
        }
        self.set_engine(id, v);
        if id == 3 {
            self.sync_mix();
        }
    }

    /// With one chain the engine runs fully wet and the dry is summed in
    /// `process_block`; a dual routing keeps the engine's own crossfade.
    fn sync_mix(&mut self) {
        self.rev.a.mix = if matches!(self.rev.routing, reverb::DualRouting::Single) {
            1.0
        } else {
            self.par.wet.min(1.0)
        };
    }

    fn set_engine(&mut self, id: u32, v: f64) {
        // Ids < 100: chain A + the dual block. Ids 100+: the same
        // chain-scoped param on chain B (`r2_*` names, id − 100).
        match id {
            3 => {
                self.rev.routing = reverb::DualRouting::from_index(v.round().max(0.0) as usize);
                // A routing that plays B builds it — here, a param write.
                if self.rev.routing != reverb::DualRouting::Single {
                    self.ensure_b();
                }
            }
            4 | 5 | 6 | 8 => self.write_b(id, v),
            _ if id >= 100 => self.write_b(id, v),
            _ => Self::set_chain(&mut self.rev.a, id, v),
        }
    }

    /// A write for chain B: to it, or kept for when it is built.
    fn write_b(&mut self, id: u32, v: f64) {
        if self.rev.b().is_some() {
            Self::apply_b(self.rev.b_mut(), id, v);
            return;
        }
        // A drag on one of B's knobs is a run of writes to one param: the
        // last stands for it.
        match self.b_log.last_mut() {
            Some(last) if last.0 == id => last.1 = v,
            _ => self.b_log.push((id, v)),
        }
        // A long session of edits to a B that never plays: each param's
        // last write, in the order those were made.
        if self.b_log.len() > 1024 {
            let mut seen = std::collections::HashSet::new();
            let mut kept: Vec<(u32, f64)> =
                self.b_log.iter().rev().filter(|(i, _)| seen.insert(*i)).copied().collect();
            kept.reverse();
            self.b_log = kept;
        }
    }

    /// One of B's params, as `set_engine` addresses it: the legacy dual
    /// block (4 algorithm, 5 decay, 6 mix, 8 pan — equivalent to
    /// r2_algorithm / r2_decay / r2_mix / r2_pan) or `r2_*` (id − 100).
    fn apply_b(b: &mut reverb::ReverbChain, id: u32, v: f64) {
        match id {
            4 => b.set_algorithm(reverb::AlgorithmType::from_index(v.round().max(0.0) as usize)),
            5 => {
                b.params.decay = v;
                b.update_params();
            }
            6 => b.mix = v.clamp(0.0, 1.0),
            8 => b.pan = v.clamp(-1.0, 1.0),
            _ => Self::set_chain(b, id.saturating_sub(100), v),
        }
    }

    /// Build chain B: seeded as a plate (so engaging a dual routing is
    /// immediately audible before any params are set), then every write it
    /// was sent, in order — as if it had been there all along.
    fn ensure_b(&mut self) {
        if self.rev.b().is_some() {
            return;
        }
        let log = std::mem::take(&mut self.b_log);
        let b = self.rev.b_mut();
        b.set_algorithm(reverb::AlgorithmType::Plate);
        b.mix = 0.08;
        b.params.decay = 0.45;
        b.update_params();
        for (id, v) in log {
            Self::apply_b(b, id, v);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.prepared {
            self.wire_b_reshaper();
        }
    }

    /// B's Impulse re-prepare worker (see `reshaper_a`), once B exists.
    #[cfg(not(target_arch = "wasm32"))]
    fn wire_b_reshaper(&mut self) {
        if self.reshaper_b.is_some() || self.rev.b().is_none() {
            return;
        }
        let (reshaper, rx) = reverb::ir::ImpulseReshaper::new();
        let b = self.rev.b_mut();
        b.set_prepared_ir_receiver(rx);
        b.set_reshape_sender(reshaper.sender());
        b.set_ir_trash_sender(reshaper.trash_sender());
        self.reshaper_b = Some(reshaper);
    }

    /// Apply a chain-scoped param (everything in `REVERB_PARAMS` except
    /// the dual block) to one `ReverbChain`. Chain A hears these at
    /// their base ids, chain B at id + 100 (`r2_*`).
    fn set_chain(c: &mut reverb::ReverbChain, id: u32, v: f64) {
        match id {
            0 => c.mix = v.clamp(0.0, 1.0),
            92 => c.wet_gain_db = v.clamp(-36.0, 36.0),
            95 => {
                c.output_hp_freq = v.clamp(20.0, 2000.0);
                c.refresh_output_filters();
            }
            96 => {
                c.output_lp_freq = v.clamp(1000.0, 20000.0);
                c.refresh_output_filters();
            }
            97 => c.duck_amount = v.clamp(0.0, 1.0),
            98 => c.duck_threshold = 10f64.powf(v.clamp(-60.0, 0.0) / 20.0),
            99 => {
                c.duck_release_ms = v.clamp(20.0, 2000.0);
                c.update_params();
            }
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
                c.set_algorithm(reverb::AlgorithmType::from_index(
                    v.round().max(0.0) as usize
                ));
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
            58 => c.set_variant(v.round().max(0.0) as usize),
            59 => {
                c.set_decay_seconds(v);
            }
            // Decay Rate EQ bands, four fields each from id 60.
            i if (DECAY_BAND_BASE..DECAY_BAND_BASE + (DECAY_BANDS * 4) as u32).contains(&i) => {
                let idx = ((i - DECAY_BAND_BASE) / 4) as usize;
                let field = (i - DECAY_BAND_BASE) % 4;
                let band = &mut c.params.decay_bands[idx];
                match field {
                    0 => band.shape = v.round().clamp(0.0, 2.0) as u32,
                    1 => band.freq_hz = v.clamp(20.0, 20_000.0),
                    2 => {
                        band.rate = v.clamp(
                            reverb::algorithm::DECAY_RATE_MIN,
                            reverb::algorithm::DECAY_RATE_MAX,
                        )
                    }
                    _ => band.q = v.clamp(0.1, 10.0),
                }
                c.update_params();
            }
            53 => {
                // 0 -> lows tamed (0.5x), 0.5 -> neutral, 1 -> lows
                // bloom (1.6x) — the "impression of larger spaces".
                let v = v.clamp(0.0, 1.0);
                c.params.low_decay_mult = if v < 0.5 {
                    0.5 + v
                } else {
                    (v - 0.5).mul_add(1.2, 1.0)
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
        let Ok(ir) = fts_sample::load_planar_f32(
            std::path::Path::new(path),
            None,
            fts_sample::ResampleQuality::default(),
        ) else {
            return false;
        };
        if ir.num_frames() == 0 {
            return false;
        }
        let as_f64 = |ch: &[f32]| ch.iter().map(|&s| f64::from(s)).collect::<Vec<f64>>();
        let l = as_f64(&ir.channels[0]);
        let r = if ir.channels.len() >= 2 {
            as_f64(&ir.channels[1])
        } else {
            l.clone()
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
        if self.rev.routing != reverb::DualRouting::Single {
            self.ensure_b();
        }
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
            // B's only once B exists (a routing that plays it).
            self.wire_b_reshaper();
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
        if matches!(self.rev.routing, reverb::DualRouting::Single) {
            self.par.apply(in_l, in_r, out_l, out_r);
        }
        fx_meter::publish(fx_meter::REVERB, fx_meter::added(in_l, in_r, out_l, out_r));
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Delay ──────────────────────────────────────────────────────────────────

const DELAY_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 0.08,
    },
    // 2 ms floor: the Lo-Fi machine is spec'd down to 2 ms (chorus/flange/
    // realtime-lofi use); every style re-clamps to its own range anyway.
    ParamSpec {
        id: 1,
        name: "time",
        min: 2.0,
        max: 2500.0,
        default: 400.0,
    },
    ParamSpec {
        id: 2,
        name: "feedback",
        min: 0.0,
        max: 0.95,
        default: 0.30,
    },
    // TimeLine MX parity params (style index: see delay::DelayStyle).
    ParamSpec {
        id: 3,
        name: "style",
        min: 0.0,
        max: 13.0,
        default: 1.0,
    },
    ParamSpec {
        id: 4,
        name: "swell",
        min: 0.0,
        max: 4.0,
        default: 0.0,
    },
    ParamSpec {
        id: 5,
        name: "freeze",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 6,
        name: "tempo_bpm",
        min: 0.0,
        max: 300.0,
        default: 0.0,
    },
    ParamSpec {
        id: 7,
        name: "tap_div",
        min: 0.0,
        max: 10.0,
        default: 7.0,
    },
    ParamSpec {
        id: 8,
        name: "high_pass",
        min: 0.0,
        max: 900.0,
        default: 0.0,
    },
    ParamSpec {
        id: 9,
        name: "repeat_dyn",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Machine voice (0 = MX, 1 = Classic; Digital deep pass adds more).
    ParamSpec {
        id: 10,
        name: "voice",
        min: 0.0,
        max: 3.0,
        default: 0.0,
    },
    // dTape / dBucket character macros.
    ParamSpec {
        id: 11,
        name: "tape_age",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 12,
        name: "crinkle",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 13,
        name: "bucket_loss",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Ice (Pitch style): MX interval menu index (30 = Free), slice
    // (0 short / 1 medium / 2 long / 3 free grain), dry<->ice blend.
    ParamSpec {
        id: 14,
        name: "interval",
        min: 0.0,
        max: 30.0,
        default: 30.0,
    },
    ParamSpec {
        id: 15,
        name: "slice",
        min: 0.0,
        max: 3.0,
        default: 3.0,
    },
    ParamSpec {
        id: 16,
        name: "blend",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // Dual 1+2 (TimeLine MX): routing (0 Single / 1 Series 1>2 /
    // 2 Series 2>1 / 3 Parallel / 4 Split / 5 Split Swap) + delay B.
    // Ids 0-16 keep addressing delay A.
    ParamSpec {
        id: 17,
        name: "routing",
        min: 0.0,
        max: 5.0,
        default: 0.0,
    },
    ParamSpec {
        id: 18,
        name: "style_b",
        min: 0.0,
        max: 13.0,
        default: 1.0,
    },
    ParamSpec {
        id: 19,
        name: "time_b",
        min: 2.0,
        max: 2500.0,
        default: 300.0,
    },
    ParamSpec {
        id: 20,
        name: "feedback_b",
        min: 0.0,
        max: 0.95,
        default: 0.30,
    },
    ParamSpec {
        id: 21,
        name: "mix_b",
        min: 0.0,
        max: 1.0,
        default: 0.08,
    },
    // Spectral machine (grain_shape: 0 Soft/1 Swell/2 SoftPluck/
    // 3 Pluck/4 Bounce; direction: 0 Fwd/1 Rev/2 Both; density = the
    // MX 15-step synced menu index (CC 0-14, 1/1 .. 1/32 of the repeat
    // incl. off-grid ratios); density_ms >= 6 switches to free
    // 6-250 ms).
    ParamSpec {
        id: 22,
        name: "grain_shape",
        min: 0.0,
        max: 4.0,
        default: 0.0,
    },
    ParamSpec {
        id: 23,
        name: "direction",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    ParamSpec {
        id: 24,
        name: "density",
        min: 0.0,
        max: 14.0,
        default: 9.0,
    },
    ParamSpec {
        id: 25,
        name: "density_ms",
        min: 0.0,
        max: 250.0,
        default: 0.0,
    },
    ParamSpec {
        id: 26,
        name: "spread",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 27,
        name: "stretch",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 28,
        name: "octave",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Lo-Fi machine (filter_shape: 0 Off .. 8 Intercom; grit rides the
    // shared "drive" engine field via id 29). sample_rate is the MX
    // 21-step menu index: 0 = 750 Hz ... 20 = 96 kHz, geometrically
    // spaced (the manual gives endpoints + step count), converted to a
    // divisor against the host rate.
    ParamSpec {
        id: 29,
        name: "grit",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 30,
        name: "bit_depth",
        min: 4.0,
        max: 32.0,
        default: 12.0,
    },
    ParamSpec {
        id: 31,
        name: "sample_rate",
        min: 0.0,
        max: 20.0,
        default: 11.0,
    },
    ParamSpec {
        id: 32,
        name: "lofi_mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 33,
        name: "vinyl",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 34,
        name: "filter_shape",
        min: 0.0,
        max: 8.0,
        default: 0.0,
    },
    // Per-line tap divisions + line pan (the rig's delay surface
    // addresses these by name; id 7 "tap_div" still sets both lines at
    // once). pan: one knob places BOTH lines (-1 hard L .. +1 hard R,
    // 0 = centered); the engine's own default keeps the classic
    // hard-L/R split until the param is touched.
    ParamSpec {
        id: 35,
        name: "tap_div_l",
        min: 0.0,
        max: 10.0,
        default: 7.0,
    },
    ParamSpec {
        id: 36,
        name: "tap_div_r",
        min: 0.0,
        max: 10.0,
        default: 7.0,
    },
    ParamSpec {
        id: 37,
        name: "pan",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    // Common Mod Speed/Depth (TimeLine PARAM-menu mod): one knob pair
    // drives whichever machine is active (each engine keeps per-machine
    // fields; the Reverb machine routes these to its wet tremolo).
    ParamSpec {
        id: 38,
        name: "mod_rate",
        min: 0.05,
        max: 8.0,
        default: 0.6,
    },
    ParamSpec {
        id: 39,
        name: "mod_depth",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Reverse machine: Smear (diffusion on the reversed audio).
    ParamSpec {
        id: 40,
        name: "rev_smear",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Digital Classic voice: morphing FILTER (0 full-bw -> 1 tape).
    ParamSpec {
        id: 41,
        name: "dig_morph",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Ducking (TimeLine Duck Sens 0-18, Duck Release 0.05-1.00 s).
    ParamSpec {
        id: 42,
        name: "duck_sens",
        min: 0.0,
        max: 18.0,
        default: 0.0,
    },
    ParamSpec {
        id: 43,
        name: "duck_release",
        min: 0.05,
        max: 1.0,
        default: 0.2,
    },
    // Drum machine: head spacing (0 Even / 1 Triplet / 2 Golden /
    // 3 Silver) + Lo Cut.
    ParamSpec {
        id: 44,
        name: "drum_spacing",
        min: 0.0,
        max: 3.0,
        default: 2.0,
    },
    ParamSpec {
        id: 45,
        name: "drum_locut",
        min: 0.0,
        max: 1.0,
        default: 0.2,
    },
    // Oil Can: head mode (0 Long / 1 Short / 2 Both).
    ParamSpec {
        id: 46,
        name: "oilcan_heads",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Per-delay wet output level (TimeLine Output Level).
    ParamSpec {
        id: 47,
        name: "output_level",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // Filter machine (swept filter + trem on repeats).
    ParamSpec {
        id: 48,
        name: "flt_shape",
        min: 0.0,
        max: 10.0,
        default: 0.0,
    },
    ParamSpec {
        id: 49,
        name: "flt_speed",
        min: 0.03125,
        max: 32.0,
        default: 1.0,
    },
    ParamSpec {
        id: 50,
        name: "flt_depth",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    ParamSpec {
        id: 51,
        name: "flt_center",
        min: 100.0,
        max: 8000.0,
        default: 1200.0,
    },
    ParamSpec {
        id: 52,
        name: "flt_q",
        min: 0.5,
        max: 10.0,
        default: 2.0,
    },
    ParamSpec {
        id: 53,
        name: "flt_location",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 54,
        name: "flt_trem_depth",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 55,
        name: "flt_trem_speed",
        min: 0.03125,
        max: 32.0,
        default: 1.0,
    },
    // MultiTap: Classic pattern recall (0 = custom, 1-16 = Classic n),
    // feedback topology (0 Input / 1 Parallel), step grid
    // (0 16th / 1 Triplet / 2 Off-256).
    ParamSpec {
        id: 56,
        name: "mtap_pattern",
        min: 0.0,
        max: 16.0,
        default: 0.0,
    },
    ParamSpec {
        id: 57,
        name: "mtap_fb_mode",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 58,
        name: "mtap_grid",
        min: 0.0,
        max: 2.0,
        default: 0.0,
    },
    // Filter-machine tremolo waveform (manual list: 0 Triangle /
    // 1 Square / 2 Sine / 3 Ramp / 4 Saw).
    ParamSpec {
        id: 59,
        name: "flt_trem_shape",
        min: 0.0,
        max: 4.0,
        default: 2.0,
    },
    // Spectral post-granular diffusion (FTS voicing extra beyond the
    // hardware surface — Clouds allpass-chain crossfade on the cloud).
    ParamSpec {
        id: 60,
        name: "spec_diffusion",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    // The dry guitar's level through the block (1 = all of it). The delay
    // runs in parallel with it — `mix` is how much delay is added — so
    // engaging the block never turns the guitar down; this is how to.
    ParamSpec {
        id: 61,
        name: "dry",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // The delay's level against the dry, in dB — a gain on the wet, on top
    // of `mix`. See the reverb's `level`.
    ParamSpec {
        id: 62,
        name: "level",
        min: -60.0,
        max: 12.0,
        default: 0.0,
    },
    // A low-pass inside the feedback loop: each repeat darker than the
    // last, as an analog or tape echo (3–6 kHz on guitar). 20 kHz = open.
    ParamSpec {
        id: 63,
        name: "high_cut",
        min: 500.0,
        max: 20000.0,
        default: 8000.0,
    },
];

/// Native Delay block — wraps [`delay::DualDelay`] (two full chains +
/// `TimeLine` MX 1+2 routing; `Single` = chain A only, bit-compatible with
/// the previous single-chain wrapper).
///
/// Defaults to a subtle clean quarter-note-ish delay with modest feedback.
pub struct NativeDelay {
    dly: delay::DualDelay,
    prepared: bool,
    /// Delay A in parallel with the dry (see [`ParallelMix`]).
    par: ParallelMix,
    sample_rate: f64,
    block_size: usize,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeDelay {
    #[must_use]
    pub fn new(_sample_rate: f64) -> Self {
        let mut dly = delay::DualDelay::new();
        let chain = &mut dly.a;
        chain.set_style(delay::DelayStyle::Clean);
        chain.mix = 0.08;
        chain.delay_l.time_ms = 400.0;
        chain.delay_r.time_ms = 400.0;
        chain.delay_l.feedback = 0.30;
        chain.delay_r.feedback = 0.30;
        // B's, kept until a routing plays it (see `DualDelay`).
        dly.set_b_style(delay::DelayStyle::Clean);
        dly.set_b_mix(0.08);
        dly.set_b_feedback(0.30);
        // Fully wet: single routing sums the dry itself (`ParallelMix`).
        dly.a.mix = 1.0;
        // B seeds slightly shorter so engaging a dual routing is
        // immediately audible before any params are set.
        dly.set_b_time_ms(300.0);
        Self {
            sample_rate: 48000.0,
            block_size: 512,
            dly,
            prepared: false,
            par: ParallelMix::new(0.08),
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        // Delay A's mix and the dry are the parallel sum's (see
        // `ParallelMix`); the engine's own mix is derived in `sync_mix`.
        match id {
            0 => {
                self.par.set_mix(v);
                self.sync_mix();
                return;
            }
            62 => {
                self.par.set_level_db(v);
                self.sync_mix();
                return;
            }
            61 => {
                self.par.dry = v.clamp(0.0, 1.0);
                return;
            }
            _ => {}
        }
        self.set_engine(id, v);
        if id == 17 {
            self.sync_mix();
        }
    }

    /// With one delay the engine runs fully wet and the dry is summed in
    /// `process_block`; a dual routing keeps the engine's own crossfade.
    fn sync_mix(&mut self) {
        self.dly.a.mix = if matches!(self.dly.routing, delay::DualRouting::Single) {
            1.0
        } else {
            self.par.wet.min(1.0)
        };
    }

    fn set_engine(&mut self, id: u32, v: f64) {
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
            63 => {
                // In the loop and on the wet out: repeat n passes the
                // filter n times, the first included.
                let hz = if v >= 19_999.0 {
                    0.0
                } else {
                    v.clamp(500.0, 20_000.0)
                };
                a.delay_l.hicut_freq = hz;
                a.delay_r.hicut_freq = hz;
                a.high_cut_hz = hz;
            }
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
            // A routing that plays B builds it here (a param write).
            17 => self
                .dly
                .set_routing(delay::DualRouting::from_index(v.round().max(0.0) as usize)),
            18 => self
                .dly
                .set_b_style(delay::DelayStyle::from_index(v.round().max(0.0) as usize)),
            19 => self.dly.set_b_time_ms(v),
            20 => self.dly.set_b_feedback(v),
            21 => self.dly.set_b_mix(v.clamp(0.0, 1.0)),
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
        if matches!(self.dly.routing, delay::DualRouting::Single) {
            self.par.apply(in_l, in_r, out_l, out_r);
        }
        fx_meter::publish(fx_meter::DELAY, fx_meter::added(in_l, in_r, out_l, out_r));
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
    ParamSpec {
        id: 0,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 0.4,
    },
    ParamSpec {
        id: 1,
        name: "depth",
        min: 0.0,
        max: 1.0,
        default: 0.4,
    },
    ParamSpec {
        id: 2,
        name: "rate",
        min: 0.05,
        max: 10.0,
        default: 1.0,
    },
    // Engine (algorithm), `chorus::EngineType` order — persisted, append
    // only: 0 Cubic / 1 BBD / 2 Tape / 3 Orbit / 4 Juno / 5 CE-2 /
    // 6 Dimension / 7 Clone / 8 Tri-Chorus / 9 SCF / 10 Julia.
    ParamSpec {
        id: 3,
        name: "engine",
        min: 0.0,
        max: 10.0,
        default: 0.0,
    },
    // The engine's own colour: the wet's tone on most (0.5 = the unit as
    // built), the pre-delay on the SCF, the Lag on the Julia.
    ParamSpec {
        id: 4,
        name: "color",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    // Delay-line feedback, scaled per mode (a little in chorus, most of the
    // way in flanger) and level-compensated.
    ParamSpec {
        id: 5,
        name: "feedback",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // 0 = the engine's own mono output, 1 = its full stereo spread.
    ParamSpec {
        id: 6,
        name: "width",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
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
    #[must_use]
    pub fn chorus(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Chorus, "Chorus", 1.0)
    }
    #[must_use]
    pub fn flanger(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Flanger, "Flanger", 0.3)
    }
    #[must_use]
    pub fn vibrato(sample_rate: f64) -> Self {
        Self::new(sample_rate, EffectType::Vibrato, "Vibrato", 5.0)
    }

    fn new(_sample_rate: f64, effect: EffectType, label: &'static str, rate: f64) -> Self {
        let mut ch = ChorusChain::new();
        ch.effect_type = effect;
        // Vibrato is pitch-only (fully wet); chorus/flanger blend.
        ch.mix = if matches!(effect, EffectType::Vibrato) {
            1.0
        } else {
            0.4
        };
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
            // The chain builds an engine the first time it is picked (this
            // write), then crossfades to it.
            3 => self
                .ch
                .set_engine(modulation::chorus::engine::EngineType::from_index(
                    v.round().max(0.0) as usize,
                )),
            4 => self.ch.color = v,
            5 => self.ch.feedback = v,
            6 => self.ch.width = v,
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
        fx_meter::publish(fx_meter::MODULATION, fx_meter::added(in_l, in_r, out_l, out_r));
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
    ParamSpec {
        id: 0,
        name: "depth",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    ParamSpec {
        id: 1,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    // Free-running LFO rate (the trigger engine's free mode).
    ParamSpec {
        id: 2,
        name: "rate",
        min: 0.05,
        max: 12.0,
        default: 4.0,
    },
    // Mode (algorithm): 0 Mono / 1 Stereo / 2 Harmonic.
    ParamSpec {
        id: 3,
        name: "mode",
        min: 0.0,
        max: 2.0,
        default: 1.0,
    },
];

/// Native Tremolo block — wraps [`TremChain`] (amplitude modulation).
pub struct NativeTrem {
    tr: TremChain,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl NativeTrem {
    #[must_use]
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

    const fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.tr.set_depth(v),
            1 => self.tr.mix = v.clamp(0.0, 1.0),
            2 => {
                // Free-running rate: force the trigger engine out of sync.
                self.tr.modulator.trigger.mode =
                    modulation::trem::fts_modulation::trigger::TriggerMode::Free;
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
        fx_meter::publish(fx_meter::TREMOLO, fx_meter::added(in_l, in_r, out_l, out_r));
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Pitch ────────────────────────────────────────────────────────────────────

const PITCH_PARAMS: &[ParamSpec] = &[
    // Shift in semitones.
    ParamSpec {
        id: 0,
        name: "semitones",
        min: -24.0,
        max: 24.0,
        default: 12.0,
    },
    // Fine shift in cents.
    ParamSpec {
        id: 1,
        name: "cents",
        min: -100.0,
        max: 100.0,
        default: 0.0,
    },
    // Shifted signal vs dry (the dry is never delayed — see `NativePitch`).
    ParamSpec {
        id: 2,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 0.5,
    },
    // 0 Spectral (phase vocoder), 1 WSOLA, 2 PSOLA, 3 PolyOctave.
    ParamSpec {
        id: 3,
        name: "engine",
        min: 0.0,
        max: 3.0,
        default: 0.0,
    },
    // 1 = short frames (Spectral 1024 = 21 ms, WSOLA 256, PSOLA 512).
    ParamSpec {
        id: 4,
        name: "live",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // Voice A's level within the wet (voice A is `semitones` + `cents`) —
    // by default the octave up.
    ParamSpec {
        id: 5,
        name: "a_level",
        min: 0.0,
        max: 1.0,
        default: 0.7,
    },
    // Voice B: a second interval — by default the octave down, so the block
    // starts as a POG-style octaver (sub and octave up blended over the
    // dry); a harmony's third under its fifth otherwise. Off at level 0, and
    // then costs nothing.
    ParamSpec {
        id: 6,
        name: "b_semitones",
        min: -24.0,
        max: 24.0,
        default: -12.0,
    },
    ParamSpec {
        id: 7,
        name: "b_level",
        min: 0.0,
        max: 1.0,
        default: 0.7,
    },
    // The dry side's level within `mix` (1 = the plain dry/wet blend).
    ParamSpec {
        id: 8,
        name: "dry",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
];

/// Native Pitch block — a stereo pitch shifter over pitch-dsp's
/// [`PitchChain`](pitch_dsp::chain::PitchChain), one per channel.
///
/// Engine 0 (default) is the phase-locked phase vocoder (`Spectral`): the
/// only engine that is clean on chords and sustained notes alike —
/// pitch-dsp `examples/pitch_quality` at +12 on its default 2048-sample
/// frames (42.7 ms): 0.1 cents median, 0.2 % gross pitch errors, no grain
/// AM, non-harmonic energy −68 dB, against WSOLA's 3.3 % / −29 dB (and
/// 29 % gross at −12) and PSOLA's 44 % / −0.1 dB; ~240 ns/sample per
/// channel. `live` halves the frame (21 ms) but the low strings then fall
/// under the frequency resolution: 24 % gross errors at +12 — kept as an
/// option, not the default. PolyOctave (zero latency, octaves only) stays
/// for when feel matters more than purity.
///
/// Two voices and a dry level — `out = (1 − mix) · dry · x + mix · (a · A +
/// b · B)` — make it an octaver (A +12 and B −12 blended with the dry) or a
/// fixed-interval harmony (A a third, B a fifth). Voice B runs only while
/// its level is up.
///
/// Latency: the dry path is NOT delayed and the block reports 0 — in a live
/// rig the player hears the dry note on time and the shifted voice trails
/// it (as on a harmonizer pedal). At a different pitch the two do not comb.
pub struct NativePitch {
    chain_l: pitch_dsp::chain::PitchChain,
    chain_r: pitch_dsp::chain::PitchChain,
    /// Voice B's shifters — processed only while `b_level` > 0, and built
    /// only once it is: each chain holds only the engine it plays, and a
    /// rig prepares a pitch block in every patch, mostly with B silent.
    voice_b: Option<Box<[pitch_dsp::chain::PitchChain; 2]>>,
    /// Voice B's settings, kept for when it is built.
    algorithm: pitch_dsp::chain::Algorithm,
    live: bool,
    b_semitones: f64,
    sample_rate: f64,
    semitones: f64,
    cents: f64,
    mix: f64,
    a_level: f64,
    b_level: f64,
    dry: f64,
    /// Voice B was silent last block: reset its shifters before it speaks,
    /// so it does not start on a stale frame.
    b_idle: bool,
    prepared: bool,
    wet_l: Vec<f64>,
    wet_r: Vec<f64>,
    b_l: Vec<f64>,
    b_r: Vec<f64>,
    discard: Vec<f64>,
}

impl NativePitch {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mk = || {
            let mut c = pitch_dsp::chain::PitchChain::only(pitch_dsp::chain::Algorithm::Spectral);
            c.live = false;
            c.mix = 1.0;
            c
        };
        let mut p = Self {
            chain_l: mk(),
            chain_r: mk(),
            voice_b: None,
            algorithm: pitch_dsp::chain::Algorithm::Spectral,
            live: false,
            b_semitones: -12.0,
            sample_rate,
            semitones: 12.0,
            cents: 0.0,
            mix: 0.5,
            a_level: 0.7,
            b_level: 0.7,
            dry: 1.0,
            b_idle: true,
            prepared: false,
            wet_l: Vec::new(),
            wet_r: Vec::new(),
            b_l: Vec::new(),
            b_r: Vec::new(),
            discard: Vec::new(),
        };
        p.apply_shift();
        p
    }

    /// Voice B's two chains, built (an allocation: from a param write or
    /// `prepare`, never `process`) to match voice A's engine and mode.
    fn build_voice_b(&mut self) {
        if self.voice_b.is_some() {
            return;
        }
        let mk = || {
            let mut c = pitch_dsp::chain::PitchChain::only(self.algorithm);
            c.live = self.live;
            c.mix = 1.0;
            c.semitones = self.b_semitones;
            if self.prepared {
                c.update(AudioConfig {
                    sample_rate: self.sample_rate.max(1.0),
                    max_buffer_size: self.wet_l.len().max(1),
                });
                c.reset();
            }
            c
        };
        self.voice_b = Some(Box::new([mk(), mk()]));
        self.b_idle = true;
    }

    fn apply_shift(&mut self) {
        let st = (self.semitones + self.cents / 100.0).clamp(-24.0, 24.0);
        self.chain_l.semitones = st;
        self.chain_r.semitones = st;
    }

    fn set(&mut self, id: u32, v: f64) {
        use pitch_dsp::chain::Algorithm;
        match id {
            0 => {
                self.semitones = v.clamp(-24.0, 24.0);
                self.apply_shift();
            }
            1 => {
                self.cents = v.clamp(-100.0, 100.0);
                self.apply_shift();
            }
            2 => self.mix = v.clamp(0.0, 1.0),
            3 => {
                let algo = match v.round().max(0.0) as u32 {
                    1 => Algorithm::Wsola,
                    2 => Algorithm::Psola,
                    3 => Algorithm::PolyOctave,
                    _ => Algorithm::Spectral,
                };
                // A new engine is built here, as a reverb's new algorithm
                // is: a param write, not the audio callback.
                self.algorithm = algo;
                self.chain_l.select(algo);
                self.chain_r.select(algo);
                if let Some(b) = self.voice_b.as_deref_mut() {
                    for c in b.iter_mut() {
                        c.select(algo);
                    }
                }
            }
            4 => {
                self.live = v >= 0.5;
                for c in [&mut self.chain_l, &mut self.chain_r] {
                    c.live = self.live;
                }
                if let Some(b) = self.voice_b.as_deref_mut() {
                    for c in b.iter_mut() {
                        c.live = self.live;
                    }
                }
            }
            5 => self.a_level = v.clamp(0.0, 1.0),
            6 => {
                self.b_semitones = v.clamp(-24.0, 24.0);
                if let Some(b) = self.voice_b.as_deref_mut() {
                    for c in b.iter_mut() {
                        c.semitones = self.b_semitones;
                    }
                }
            }
            7 => {
                self.b_level = v.clamp(0.0, 1.0);
                if self.b_level > 0.0 && self.prepared {
                    self.build_voice_b();
                }
            }
            8 => self.dry = v.clamp(0.0, 1.0),
            _ => {}
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(PITCH_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativePitch {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.pitch", "Pitch")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(PITCH_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        (id == 3).then(|| {
            match value.round() as i64 {
                1 => "WSOLA",
                2 => "PSOLA",
                3 => "PolyOctave",
                _ => "Spectral",
            }
            .into()
        })
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        let cfg = AudioConfig {
            sample_rate: sample_rate.max(1.0),
            max_buffer_size: block_size.max(1) as usize,
        };
        self.sample_rate = cfg.sample_rate;
        for c in [&mut self.chain_l, &mut self.chain_r] {
            c.update(cfg);
            c.reset();
        }
        if let Some(b) = self.voice_b.as_deref_mut() {
            for c in b.iter_mut() {
                c.update(cfg);
                c.reset();
            }
        }
        self.b_idle = true;
        let n = block_size.max(1) as usize;
        self.wet_l = vec![0.0; n];
        self.wet_r = vec![0.0; n];
        self.b_l = vec![0.0; n];
        self.b_r = vec![0.0; n];
        self.discard = vec![0.0; n];
        self.prepared = true;
        if self.b_level > 0.0 {
            self.build_voice_b();
        }
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
        let n = out_l
            .len()
            .min(out_r.len())
            .min(in_l.len())
            .min(in_r.len())
            .min(self.wet_l.len());
        for i in 0..n {
            self.wet_l[i] = f64::from(in_l[i]);
            self.wet_r[i] = f64::from(in_r[i]);
        }
        // Voice B from the same input, before voice A overwrites it.
        // Built when B was turned up (a write, or `prepare`): never here.
        let b_on = self.b_level > 0.0 && self.voice_b.is_some();
        if let (true, Some(b)) = (b_on, self.voice_b.as_deref_mut()) {
            let [bl, br] = b;
            if self.b_idle {
                bl.reset();
                br.reset();
                self.b_idle = false;
            }
            self.b_l[..n].copy_from_slice(&self.wet_l[..n]);
            self.b_r[..n].copy_from_slice(&self.wet_r[..n]);
            bl.process(&mut self.b_l[..n], &mut self.discard[..n]);
            br.process(&mut self.b_r[..n], &mut self.discard[..n]);
        } else {
            self.b_idle = true;
        }
        // PitchChain is mono (it copies its result to the second buffer).
        self.chain_l
            .process(&mut self.wet_l[..n], &mut self.discard[..n]);
        self.chain_r
            .process(&mut self.wet_r[..n], &mut self.discard[..n]);
        // out = (1 − mix) · dry · x + mix · (a · A + b · B) — `mix` keeps
        // its dry↔wet meaning, `dry` trims the dry side of it.
        let (mix, a, b, dry) = (self.mix, self.a_level, if b_on { self.b_level } else { 0.0 }, self.dry);
        for i in 0..n {
            let (dl, dr) = (f64::from(in_l[i]), f64::from(in_r[i]));
            let (bl, br) = if b_on { (self.b_l[i], self.b_r[i]) } else { (0.0, 0.0) };
            out_l[i] = (dl * dry).mul_add(1.0 - mix, mix * a.mul_add(self.wet_l[i], b * bl)) as f32;
            out_r[i] = (dr * dry).mul_add(1.0 - mix, mix * a.mul_add(self.wet_r[i], b * br)) as f32;
        }
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
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        Self {
            label,
            prepared: false,
        }
    }
    pub const fn set_named(&mut self, _name: &str, _value: f64) {}
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

const GAIN_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "gain_db",
        min: -24.0,
        max: 24.0,
        default: 0.0,
    },
    // Pan, −1 (left) … +1 (right), on a −4.5 dB pan law normalised to the
    // centre: a centred guitar is exactly the old block, and panned hard to
    // one side it comes up +4.5 dB there (the far side silent) — so it
    // sounds as loud off to the side as in the middle.
    ParamSpec {
        id: 1,
        name: "pan",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
];

/// The −4.5 dB pan law, normalised so the centre is unity: each side is
/// `trig^1.5` of the pan angle (the geometric mean of the constant-power
/// and linear laws), divided by its centre value `(½)^¾`.
fn pan_law(pan: f64) -> [f64; 2] {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
    let centre = 0.5f64.powf(0.75);
    [
        theta.cos().max(0.0).powf(1.5) / centre,
        theta.sin().max(0.0).powf(1.5) / centre,
    ]
}

/// Native gain block — a clean dB trim (the "Boost" utility) with a
/// balance control. Gain and pan changes glide over ~10 ms so footswitch
/// boosts never click.
pub struct NativeGain {
    target: f64,
    current: f64,
    coeff: f64,
    prepared: bool,
    pan: f64,
    /// Per-side balance gains: target, and where the glide has got to.
    side_target: [f64; 2],
    side: [f64; 2],
}

impl NativeGain {
    #[must_use]
    pub const fn new(_sample_rate: f64) -> Self {
        Self {
            target: 1.0,
            current: 1.0,
            coeff: 0.0,
            prepared: false,
            pan: 0.0,
            side_target: [1.0, 1.0],
            side: [1.0, 1.0],
        }
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.target = 10f64.powf(v.clamp(-24.0, 24.0) / 20.0),
            1 => {
                self.pan = v.clamp(-1.0, 1.0);
                self.side_target = pan_law(self.pan);
            }
            _ => {}
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
        self.side = self.side_target;
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
        let [tl, tr] = self.side_target;
        let mut g = self.current;
        let [mut sl, mut sr] = self.side;
        for i in 0..out_l.len() {
            g = (g - t).mul_add(c, t);
            sl = (sl - tl).mul_add(c, tl);
            sr = (sr - tr).mul_add(c, tr);
            out_l[i] = in_l.get(i).copied().unwrap_or(0.0) * (g * sl) as f32;
            out_r[i] = in_r.get(i).copied().unwrap_or(0.0) * (g * sr) as f32;
        }
        self.current = g;
        self.side = [sl, sr];
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Boost pedal ────────────────────────────────────────────────────────────

/// The boost pedal's one knob, as on the pedal: 0 = unity, 1 = +18 dB.
const BOOST_MAX_DB: f64 = 18.0;

const BOOST_PARAMS: &[ParamSpec] = &[ParamSpec {
    id: 0,
    name: "drive",
    min: 0.0,
    max: 1.0,
    default: 0.5,
}];

/// Native boost pedal — the clean boost at the head of the drive board: a
/// level push into the drives and amp after it (0 dB at drive 0, +9 dB at
/// the default 0.5, +18 dB flat out), gliding over ~10 ms so a stomp or a
/// macro sweep never clicks.
pub struct NativeBoost {
    target: f64,
    current: f64,
    coeff: f64,
    prepared: bool,
}

impl NativeBoost {
    #[must_use]
    pub fn new(_sample_rate: f64) -> Self {
        let g = Self::gain(0.5);
        Self {
            target: g,
            current: g,
            coeff: 0.0,
            prepared: false,
        }
    }

    fn gain(drive: f64) -> f64 {
        10f64.powf(drive.clamp(0.0, 1.0) * BOOST_MAX_DB / 20.0)
    }

    fn set(&mut self, id: u32, v: f64) {
        if id == 0 {
            self.target = Self::gain(v);
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(BOOST_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeBoost {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.boost", "Boost")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(BOOST_PARAMS)
    }
    fn param_value(&mut self, _id: u32) -> Option<f64> {
        None
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        (id == 0).then(|| format!("+{:.1} dB", value.clamp(0.0, 1.0) * BOOST_MAX_DB))
    }
    fn text_to_value(&mut self, _id: u32, _text: &str) -> Option<f64> {
        None
    }
    fn latency(&mut self) -> u32 {
        0
    }
    fn prepare(&mut self, sample_rate: f64, _block_size: u32) -> Result<(), PluginError> {
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
            g = (g - t).mul_add(c, t);
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

#[cfg(test)]
mod boost_tests {
    use super::*;

    /// Unity at 0, +9 dB at the default, +18 dB flat out.
    #[test]
    fn boost_pedal_pushes_its_level() {
        for (drive, db) in [(0.0, 0.0), (0.5, 9.0), (1.0, 18.0)] {
            let mut b = NativeBoost::new(48_000.0);
            b.set_named("drive", drive);
            b.prepare(48_000.0, 256).unwrap();
            let x = vec![0.1f32; 256];
            let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
            b.process_block(&x, &x, &mut l, &mut r, &PluginEvents::default()).unwrap();
            let got = 20.0 * (f64::from(l[255]) / 0.1).log10();
            assert!((got - db).abs() < 0.05, "drive {drive}: {got} dB, want {db}");
        }
    }
}

// ── Gate ────────────────────────────────────────────────────────────────────

const GATE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        id: 0,
        name: "threshold",
        min: -90.0,
        max: 0.0,
        default: -50.0,
    },
    ParamSpec {
        id: 1,
        name: "attack",
        min: 0.1,
        max: 50.0,
        default: 1.0,
    },
    ParamSpec {
        id: 2,
        name: "release",
        min: 5.0,
        max: 500.0,
        default: 120.0,
    },
    // 0 = gate (closed is the range's floor), 1 = expander (below the
    // threshold the level falls 2:1, down to the floor).
    ParamSpec {
        id: 3,
        name: "mode",
        min: 0.0,
        max: 1.0,
        default: 0.0,
    },
    // How far it closes, in dB (90 = silence).
    ParamSpec {
        id: 4,
        name: "range",
        min: 0.0,
        max: 90.0,
        default: 90.0,
    },
    // How long it stays open after the level drops, in ms.
    ParamSpec {
        id: 5,
        name: "hold",
        min: 0.0,
        max: 500.0,
        default: 0.0,
    },
    // How far under the threshold the level must fall to close, in dB.
    ParamSpec {
        id: 6,
        name: "hysteresis",
        min: 0.0,
        max: 12.0,
        default: 0.0,
    },
];

/// Native noise gate — peak-follower downward gate. Opens fast (attack),
/// closes smoothly (release) to its range's floor once the level has
/// fallen the hysteresis under the threshold and the hold has run out; or,
/// as an expander, eases the level down 2:1 under the threshold.
pub struct NativeGate {
    threshold: f64,
    attack_ms: f64,
    release_ms: f64,
    expander: bool,
    floor: f64,
    hold_ms: f64,
    hysteresis: f64,
    open: bool,
    held: u32,
    env: f64,
    gain: f64,
    attack_coeff: f64,
    release_coeff: f64,
    env_coeff: f64,
    sample_rate: f64,
    prepared: bool,
}

impl NativeGate {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut g = Self {
            threshold: 10f64.powf(-50.0 / 20.0),
            attack_ms: 1.0,
            release_ms: 120.0,
            expander: false,
            floor: 10f64.powf(-90.0 / 20.0),
            hold_ms: 0.0,
            hysteresis: 1.0,
            open: false,
            held: 0,
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
            3 => self.expander = v >= 0.5,
            4 => self.floor = 10f64.powf(-v.clamp(0.0, 90.0) / 20.0),
            5 => self.hold_ms = v.clamp(0.0, 500.0),
            6 => self.hysteresis = 10f64.powf(-v.clamp(0.0, 12.0) / 20.0),
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
        let di = sidechain::peak().map(f64::from);
        for i in 0..out_l.len() {
            let l = in_l.get(i).copied().unwrap_or(0.0);
            let r = in_r.get(i).copied().unwrap_or(0.0);
            let peak = di.unwrap_or_else(|| f64::from(l.abs().max(r.abs())));
            // Peak follower: instant rise, ~30 ms fall.
            self.env = if peak > self.env {
                peak
            } else {
                (self.env - peak).mul_add(self.env_coeff, peak)
            };
            // Open at the threshold; closed once under it by the
            // hysteresis, after the hold.
            if self.env >= self.threshold {
                self.open = true;
                self.held = (self.hold_ms / 1000.0 * self.sample_rate) as u32;
            } else if self.env < self.threshold * self.hysteresis {
                if self.held > 0 {
                    self.held -= 1;
                } else {
                    self.open = false;
                }
            }
            let target = if self.open {
                1.0
            } else if self.expander {
                (self.env / self.threshold).clamp(self.floor, 1.0)
            } else {
                self.floor
            };
            let coeff = if target > self.gain {
                self.attack_coeff
            } else {
                self.release_coeff
            };
            self.gain = (self.gain - target).mul_add(coeff, target);
            out_l[i] = l * self.gain as f32;
            out_r[i] = r * self.gain as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

// ── Transient ───────────────────────────────────────────────────────────────

const TRANSIENT_PARAMS: &[ParamSpec] = &[
    // Bipolar, normalized: +1 = full attack boost, -1 = full attack cut.
    ParamSpec {
        id: 0,
        name: "attack",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 1,
        name: "sustain",
        min: -1.0,
        max: 1.0,
        default: 0.0,
    },
    ParamSpec {
        id: 2,
        name: "mix",
        min: 0.0,
        max: 1.0,
        default: 1.0,
    },
    ParamSpec {
        id: 3,
        name: "output",
        min: -24.0,
        max: 24.0,
        default: 0.0,
    },
];

/// Attack gain span at |attack| = 1 (dB). SPL-style designers ride ±15 dB.
const TRANSIENT_ATTACK_DB: f64 = 15.0;
/// Sustain gain span at |sustain| = 1 (dB).
const TRANSIENT_SUSTAIN_DB: f64 = 24.0;

/// Native transient designer — level-independent dual-envelope differential
/// (the SPL Transient Designer topology).
///
/// The attack section compares a fast follower against a slowed copy: their
/// difference in dB *is* the transient, scaled by the bipolar `attack` amount.
/// The sustain section compares a fast release against a long release: the long
/// tail hanging above the fast one *is* the sustain, scaled by `sustain`.
/// Detection is stereo-linked.
pub struct NativeTransient {
    attack: f64,
    sustain: f64,
    mix: f64,
    output_gain: f64,
    // Attack pair: same release, different attack times.
    env_fast: f64,
    env_slow: f64,
    // Sustain pair: same (instant) attack, different release times.
    env_short: f64,
    env_long: f64,
    a_fast: f64,
    a_slow: f64,
    a_rel: f64,
    r_short: f64,
    r_long: f64,
    sample_rate: f64,
    prepared: bool,
}

impl NativeTransient {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut t = Self {
            attack: 0.0,
            sustain: 0.0,
            mix: 1.0,
            output_gain: 1.0,
            env_fast: 0.0,
            env_slow: 0.0,
            env_short: 0.0,
            env_long: 0.0,
            a_fast: 0.0,
            a_slow: 0.0,
            a_rel: 0.0,
            r_short: 0.0,
            r_long: 0.0,
            sample_rate: sample_rate.max(1.0),
            prepared: false,
        };
        t.update_coeffs();
        t
    }

    fn update_coeffs(&mut self) {
        let coeff = |ms: f64| (-1.0 / (ms / 1000.0 * self.sample_rate)).exp();
        self.a_fast = coeff(0.1); // near-instant rise
        self.a_slow = coeff(20.0); // the "no transient" reference rise
        self.a_rel = coeff(60.0); // shared fall for the attack pair
        self.r_short = coeff(40.0); // fast fall — hugs the hit
        self.r_long = coeff(600.0); // long fall — carries the sustain/ring
    }

    fn set(&mut self, id: u32, v: f64) {
        match id {
            0 => self.attack = v.clamp(-1.0, 1.0),
            1 => self.sustain = v.clamp(-1.0, 1.0),
            2 => self.mix = v.clamp(0.0, 1.0),
            3 => self.output_gain = 10f64.powf(v.clamp(-24.0, 24.0) / 20.0),
            _ => {}
        }
    }

    pub fn set_named(&mut self, name: &str, value: f64) {
        if let Some(id) = param_id(TRANSIENT_PARAMS, name) {
            self.set(id, value);
        }
    }
}

impl PluginInstance for NativeTransient {
    fn descriptor(&self) -> PluginDescriptor {
        descriptor("signal.fx.transient", "Transient")
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        param_infos(TRANSIENT_PARAMS)
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
        self.env_fast = 0.0;
        self.env_slow = 0.0;
        self.env_short = 0.0;
        self.env_long = 0.0;
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
        let atk_db = self.attack * TRANSIENT_ATTACK_DB;
        let sus_db = self.sustain * TRANSIENT_SUSTAIN_DB;
        for i in 0..out_l.len() {
            let l = in_l.get(i).copied().unwrap_or(0.0);
            let r = in_r.get(i).copied().unwrap_or(0.0);
            let peak = f64::from(l.abs().max(r.abs())).max(1e-8);
            // Attack pair — different rise, shared fall.
            let up = |env: f64, a: f64| (env - peak).mul_add(a, peak);
            self.env_fast = if peak > self.env_fast {
                up(self.env_fast, self.a_fast)
            } else {
                (self.env_fast - peak).mul_add(self.a_rel, peak)
            };
            self.env_slow = if peak > self.env_slow {
                up(self.env_slow, self.a_slow)
            } else {
                (self.env_slow - peak).mul_add(self.a_rel, peak)
            };
            // Sustain pair — instant rise, different fall.
            self.env_short = if peak > self.env_short {
                peak
            } else {
                (self.env_short - peak).mul_add(self.r_short, peak)
            };
            self.env_long = if peak > self.env_long {
                peak
            } else {
                (self.env_long - peak).mul_add(self.r_long, peak)
            };
            // Ratios in dB (level-independent): >0 only during the hit /
            // the tail respectively; clamp keeps pathological ratios sane.
            let attack_amt =
                (self.env_fast / self.env_slow.max(1e-8)).ln() * (20.0 / core::f64::consts::LN_10);
            let sustain_amt =
                (self.env_long / self.env_short.max(1e-8)).ln() * (20.0 / core::f64::consts::LN_10);
            let gain_db = atk_db.mul_add(
                attack_amt.clamp(0.0, 12.0) / 12.0,
                sus_db * (sustain_amt.clamp(0.0, 24.0) / 24.0),
            );
            let wet = 10f64.powf(gain_db / 20.0) * self.output_gain;
            let g = self.mix.mul_add(wet, 1.0 - self.mix);
            out_l[i] = l * g as f32;
            out_r[i] = r * g as f32;
        }
        Ok(())
    }
    fn deactivate(&mut self) {
        self.prepared = false;
    }
}

#[cfg(test)]
mod transient_tests {
    use super::*;

    /// A percussive burst: 5 ms of full-scale noise-ish signal, then a quiet
    /// 300 ms tail. Returns (peak of first 10 ms, rms of 100–300 ms).
    fn drum_hit(t: &mut NativeTransient) -> (f32, f64) {
        let sr = 48_000usize;
        let n = sr * 3 / 10;
        let input: Vec<f32> = (0..n)
            .map(|i| {
                let x = (i as f32 * 0.7).sin();
                if i < sr * 5 / 1000 { x } else { x * 0.15 }
            })
            .collect();
        let mut out_l = vec![0.0f32; n];
        let mut out_r = vec![0.0f32; n];
        let events = PluginEvents::default();
        for (block_in, (bl, br)) in input
            .chunks(256)
            .zip(out_l.chunks_mut(256).zip(out_r.chunks_mut(256)))
        {
            t.process_block(block_in, block_in, bl, br, &events)
                .unwrap();
        }
        let head = out_l[..sr / 100]
            .iter()
            .fold(0.0f32, |a, &s| a.max(s.abs()));
        let tail = &out_l[sr / 10..];
        let rms = (tail
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum::<f64>()
            / tail.len() as f64)
            .sqrt();
        (head, rms)
    }

    #[test]
    fn attack_and_sustain_move_the_right_parts() {
        let mk = |atk: f64, sus: f64| {
            let mut t = NativeTransient::new(48_000.0);
            t.prepare(48_000.0, 256).unwrap();
            t.set_named("attack", atk);
            t.set_named("sustain", sus);
            t
        };
        let (head0, tail0) = drum_hit(&mut mk(0.0, 0.0));
        let (head_up, _) = drum_hit(&mut mk(1.0, 0.0));
        let (head_dn, _) = drum_hit(&mut mk(-1.0, 0.0));
        let (_, tail_up) = drum_hit(&mut mk(0.0, 1.0));
        let (_, tail_dn) = drum_hit(&mut mk(0.0, -1.0));
        // Neutral is a passthrough (unity ±0.1 dB on the head).
        assert!((head0 - 1.0).abs() < 0.02, "neutral head {head0}");
        assert!(head_up > head0 * 1.25, "attack boost: {head_up} vs {head0}");
        assert!(head_dn < head0 * 0.8, "attack cut: {head_dn} vs {head0}");
        assert!(
            tail_up > tail0 * 1.25,
            "sustain boost: {tail_up} vs {tail0}"
        );
        assert!(tail_dn < tail0 * 0.8, "sustain cut: {tail_dn} vs {tail0}");
        for v in [head0, head_up, head_dn] {
            assert!(v.is_finite());
        }
    }
}

#[cfg(test)]
mod pitch_tests {
    use super::*;

    fn goertzel(x: &[f32], f: f64) -> f64 {
        let w = std::f64::consts::TAU * f / 48_000.0;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s0 = f64::from(v) + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - c * s1 * s2).sqrt() * 2.0 / x.len() as f64
    }

    /// Voice A alone shifts an octave up cleanly on both channels, keeps
    /// the dry on time and mixes at the set level.
    #[test]
    fn voice_a_is_a_clean_octave_up() {
        let mut p = NativePitch::new(48_000.0);
        p.prepare(48_000.0, 256).unwrap();
        p.set_named("mix", 1.0);
        p.set_named("a_level", 1.0);
        p.set_named("b_level", 0.0);
        let n = 48_000;
        let x: Vec<f32> = (0..n)
            .map(|i| (0.5 * (std::f64::consts::TAU * 220.0 * i as f64 / 48_000.0).sin()) as f32)
            .collect();
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let ev = PluginEvents::default();
        for ((xi, lo), ro) in x.chunks(256).zip(l.chunks_mut(256)).zip(r.chunks_mut(256)) {
            p.process_block(xi, xi, lo, ro, &ev).unwrap();
        }
        for y in [&l[24_000..], &r[24_000..]] {
            let (oct, dry) = (goertzel(y, 440.0), goertzel(y, 220.0));
            assert!((oct - 0.5).abs() < 0.05, "octave level {oct}");
            assert!(dry < 1e-3, "no dry at mix 1: {dry}");
        }
        assert_eq!(p.latency(), 0);
    }

    /// An octaver: voice A an octave up, voice B an octave down, both at
    /// their levels over the dry; voice B silent (and skipped) at level 0.
    #[test]
    fn two_voices_make_an_octaver() {
        let n = 48_000;
        let x: Vec<f32> = (0..n)
            .map(|i| (0.5 * (std::f64::consts::TAU * 220.0 * i as f64 / 48_000.0).sin()) as f32)
            .collect();
        let run = |b_level: f64| {
            let mut p = NativePitch::new(48_000.0);
            p.prepare(48_000.0, 256).unwrap();
            p.set_named("mix", 0.5);
            p.set_named("a_level", 1.0);
            p.set_named("b_semitones", -12.0);
            p.set_named("b_level", b_level);
            let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
            let ev = PluginEvents::default();
            for ((xi, lo), ro) in x.chunks(256).zip(l.chunks_mut(256)).zip(r.chunks_mut(256)) {
                p.process_block(xi, xi, lo, ro, &ev).unwrap();
            }
            let y = &l[24_000..];
            (goertzel(y, 220.0), goertzel(y, 440.0), goertzel(y, 110.0))
        };
        let (dry, up, down) = run(1.0);
        assert!((dry - 0.25).abs() < 0.03, "dry {dry}");
        assert!((up - 0.25).abs() < 0.03, "octave up {up}");
        assert!((down - 0.25).abs() < 0.04, "octave down {down}");
        let (_, _, silent) = run(0.0);
        assert!(silent < 1e-3, "voice B off: {silent}");
    }

    /// Out of the box the block is a POG-style octaver: the dry with the
    /// octave down and the octave up blended in.
    #[test]
    fn default_is_a_pog_blend() {
        let n = 48_000;
        let x: Vec<f32> = (0..n)
            .map(|i| (0.5 * (std::f64::consts::TAU * 220.0 * i as f64 / 48_000.0).sin()) as f32)
            .collect();
        let mut p = NativePitch::new(48_000.0);
        p.prepare(48_000.0, 256).unwrap();
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        let ev = PluginEvents::default();
        for ((xi, lo), ro) in x.chunks(256).zip(l.chunks_mut(256)).zip(r.chunks_mut(256)) {
            p.process_block(xi, xi, lo, ro, &ev).unwrap();
        }
        let y = &l[24_000..];
        let (dry, up, down) = (goertzel(y, 220.0), goertzel(y, 440.0), goertzel(y, 110.0));
        assert!(dry > 0.2, "dry {dry}");
        assert!(up > 0.12 && down > 0.12, "octaves up {up} down {down}");
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
            ("BOOST_PARAMS", BOOST_PARAMS),
            ("GATE_PARAMS", GATE_PARAMS),
            ("TRANSIENT_PARAMS", TRANSIENT_PARAMS),
            ("PITCH_PARAMS", PITCH_PARAMS),
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
        for name in [
            "mix",
            "decay",
            "size",
            "algorithm",
            "modulation",
            "damping",
            "tone",
        ] {
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
        // B is built when a routing plays it; built now, it has every write.
        r.set_named("routing", 3.0);
        assert!((r.rev.b().unwrap().cloud.ensemble - 0.3).abs() < 1e-9);

        r.set_named("r2_mix", 0.77);
        assert!((r.rev.b().unwrap().mix - 0.77).abs() < 1e-9);

        r.set_named("r2_algorithm", 6.0); // Shimmer
        assert_eq!(r.rev.b().unwrap().algorithm_type(), reverb::AlgorithmType::Shimmer);
        assert_ne!(r.rev.a.algorithm_type(), reverb::AlgorithmType::Shimmer);

        // The advertised param list contains the mirrors exactly once.
        let infos = r.params();
        let r2: Vec<_> = infos.iter().filter(|i| i.name.starts_with("r2_")).collect();
        assert_eq!(r2.len(), REVERB_PARAMS.len() - 5);
    }
}

#[cfg(test)]
mod fx_meter_tests {
    use super::fx_meter;

    #[test]
    fn the_loudest_is_kept_until_taken() {
        fx_meter::publish(fx_meter::TREMOLO, 0.2);
        fx_meter::publish(fx_meter::TREMOLO, 0.5);
        fx_meter::publish(fx_meter::TREMOLO, 0.1);
        assert!((fx_meter::take(fx_meter::TREMOLO) - 0.5).abs() < 1e-6);
        assert_eq!(fx_meter::take(fx_meter::TREMOLO), 0.0);
    }

    #[test]
    fn what_a_block_adds() {
        let (a, b) = ([0.1f32, 0.2], [0.1f32, 0.2]);
        assert!((fx_meter::added(&a, &b, &[0.4, 0.2], &[0.1, 0.0]) - 0.3).abs() < 1e-6);
    }
}
