//! The FTS EQ engine — one engine, driven by every front end.
//!
//! There used to be two. `eq_dsp::EqChain` was a bank of static filters, which
//! is what the FTS-EQ plugin played through; `signal_fx::NativeEq` wrapped that
//! same `EqChain` and added everything else — per-band dynamics, the spectral
//! engine, transient/steady splitting, stereo placement, solo and delta
//! listening — for the rig. So they were never rival implementations, just one
//! core with the interesting half built in the wrong crate, reachable only
//! through the rig's parameter ids.
//!
//! That split had a cost you could hear: **131 of the 171 Pro-Q 4 factory
//! presets use dynamic bands and 42 use spectral bands**, and loading one in
//! the plugin recalled its static curve and dropped the rest. Same library,
//! same names, different sound depending on which EQ happened to be reading
//! it.
//!
//! [`FtsEq`] is that engine, moved down here whole and given a parameter
//! surface of its own instead of the host's numbering. `EqChain` stays as the
//! static filter bank it always was — this is what is built on top of it.
//!
//! # Driving it
//!
//! Set a band with [`FtsEq::set_band`] and [`FtsEq::set_band_dynamics`], then
//! [`FtsEq::process`] a block of `f64` in place. Nothing here knows about
//! parameter ids, automation events or sample formats; hosts own those.

use crate::band::Placement;

/// Bands the engine carries. Pro-Q 4's count, because the translated presets
/// are written against it.
pub const EQ_BANDS: usize = 24;

/// One band's static configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandConfig {
    /// The band exists (Pro-Q's "Used"). A band that is not used renders
    /// nothing regardless of `enabled`.
    pub used: bool,
    /// The band is switched on rather than bypassed.
    pub enabled: bool,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    /// Canonical shape index (see [`crate::slope::FilterShape`]).
    pub shape: u32,
    /// Slope, in Pro-Q's units: **continuous**, `slope * 6` dB/oct up to 36,
    /// then the 48 / 72 / 96 / Brickwall steps. The integer part picks the
    /// filter order and the remainder is realized as a pole-zero ladder, so a
    /// band can genuinely sit at 7.5 or 15.25 dB/oct — 137 bands in the
    /// factory library do.
    pub slope: f64,
    pub placement: Placement,
    /// Transient-mode routing: 0 both streams, 1 transient only, 2 steady
    /// only. Ignored outside transient mode.
    pub stream: u32,
}

impl Default for BandConfig {
    fn default() -> Self {
        Self {
            used: false,
            enabled: true,
            freq_hz: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            shape: 0,
            slope: 2.0,
            placement: Placement::Stereo,
            stream: 0,
        }
    }
}

/// One band's dynamics.
///
/// A range of zero is a static band — that is the test, not `enabled`, because
/// Pro-Q leaves its dynamics section switched on for bands that never use it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandDynamics {
    /// Target minus base, in dB. Negative compresses, positive expands.
    pub range_db: f64,
    /// Threshold in dB; ignored while `auto` is set.
    pub threshold_db: f64,
    /// Attack as a percentage, 0..100 — not milliseconds. Pro-Q reports this
    /// control as a percent and scales the real time constant with the band's
    /// frequency, so the mapping belongs to the engine.
    pub attack_pct: f64,
    /// Release as a percentage, 0..100.
    pub release_pct: f64,
    /// Learn the threshold from the programme rather than taking it.
    pub auto: bool,
    /// Detect prominence over the programme instead of absolute level.
    pub relative: bool,
    /// Act per FFT bin rather than as one gain ride over the band.
    pub spectral: bool,
    /// Per-bin selectivity, 0..100. Low is broad and gentle, high is a
    /// surgical notch. Per band, not per instance — Pro-Q sets it that way and
    /// the factory library uses 25 distinct values across its spectral bands.
    pub spectral_density: f64,
    /// Judge this band's prominence against a -3 dB/oct pink expectation
    /// rather than a flat one.
    pub spectral_tilt: bool,
    /// Listen to a custom frequency range instead of the band's own region.
    ///
    /// Off, the detector hears a bandpass at the band's own freq/Q, which is
    /// what makes a dynamic band self-triggering. On, it hears
    /// `side_lo_hz .. side_hi_hz` — so a band can duck one region because a
    /// different one got loud.
    pub side_filtered: bool,
    pub side_lo_hz: f64,
    pub side_hi_hz: f64,
}

impl Default for BandDynamics {
    fn default() -> Self {
        Self {
            range_db: 0.0,
            threshold_db: -18.0,
            attack_pct: 50.0,
            release_pct: 50.0,
            auto: true,
            relative: false,
            spectral: false,
            spectral_density: 50.0,
            spectral_tilt: false,
            side_filtered: false,
            side_lo_hz: 20.0,
            side_hi_hz: 20_000.0,
        }
    }
}

fn eq_shape_to_filter(shape: u32) -> crate::FilterType {
    crate::slope::FilterShape::from_canonical_index(shape).to_filter_type()
}

pub struct FtsEq {
    eq: crate::chain::EqChain,
    /// Steady-stream chain (transient mode only; mirrors band configs
    /// per `b{i}_stream` — separate instance = separate filter state).
    eq_b: crate::chain::EqChain,
    splitter: crate::transient::PeakSteadySplitter,
    spectral: crate::spectral::SpectralEngine,
    spectral_regions: Vec<crate::spectral::SpectralRegion>,
    dyn_bands: Vec<crate::dynamics::DynBand>,
    /// (used, on) per band — a band renders only when both are set.
    state: [(bool, bool); EQ_BANDS],
    /// Canonical shape + slope index per band (needed for routing and
    /// effective-order resolution).
    shapes: [u32; EQ_BANDS],
    slopes: [f64; EQ_BANDS],
    placements: [u32; EQ_BANDS],
    streams: [u32; EQ_BANDS],
    spectral_on: [bool; EQ_BANDS],
    spectral_density: [f64; EQ_BANDS],
    spectral_tilt: [bool; EQ_BANDS],
    transient_mode: bool,
    split_solo: u32,
    transient_gain_db: f64,
    steady_gain_db: f64,
    /// Active listen: (band, mode 1 solo / 2 delta).
    listen: Option<(usize, u32)>,
    solo_filter: crate::dynamics::Svf,
    /// Dry ring for delta listening, latency-aligned with the spectral
    /// engine.
    ///
    /// It has to be longer than the spectral engine's latency, because the
    /// read index walks back by exactly that much. Sized in `prepare`, where
    /// the analysis length is known — a fixed 2048 was enough for a
    /// 1024-point analysis and silently underflowed the read index when that
    /// became 4096.
    dry_ring: [Vec<f64>; 2],
    dry_pos: usize,
    /// Band frequency / gain / Q, in their own units. These used to be read
    /// out of the host's id-indexed value vector, which is what tied the
    /// engine to one particular parameter numbering; they live here now so any
    /// front end can drive it.
    freqs: [f64; EQ_BANDS],
    gains: [f64; EQ_BANDS],
    qs: [f64; EQ_BANDS],
    /// Per-band side-chain range: (filtered, lo, hi).
    side_cfg: [(bool, f64, f64); EQ_BANDS],
    /// Raw dynamic params per band: (range, thr, atk, rel, auto, relative).
    dyn_cfg: [(f64, f64, f64, f64, bool, bool); EQ_BANDS],
    /// Whether the band currently routes through the dynamic engine.
    dyn_active: [bool; EQ_BANDS],
    /// Whether the band's dynamics ride the STATIC design's gain instead —
    /// the shapes the SVF cannot build. See `DynBandParams::modulate_only`.
    dyn_modulated: [bool; EQ_BANDS],
    /// The gain each modulated band last had designed in, so the static
    /// cascade is only rebuilt when it has actually moved.
    dyn_modulated_gain: [f64; EQ_BANDS],
    output_gain_db: f64,
    /// Pro-Q's Character: 0 Clean, 1 Subtle, 2 Warm.
    character: u32,
    /// Output Pan, -1..1, and whether it balances mid against side rather
    /// than left against right.
    output_pan: f64,
    output_pan_mid_side: bool,
    /// Pro-Q's Auto Gain: hold the broadband level steady against the curve.
    auto_gain: bool,
    /// The compensation [`Self::auto_gain`] currently asks for, in dB.
    auto_gain_db: f64,
    /// The Auto Gain grid: frequencies, the static chain's curve on them, and
    /// each dynamic or spectral band's normalised shape on them. The static
    /// half is rebuilt on a parameter change; the rest is summed per block
    /// with whatever gain those bands are currently applying.
    auto_grid_hz: Vec<f64>,
    auto_grid_static_db: Vec<f64>,
    auto_grid_env: Vec<Vec<f64>>,
    gain_scale: f64,
    sample_rate: f64,
    prepared: bool,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    /// Transient-mode stream buffers (steady L/R) — the main scratch
    /// carries the transient stream in place.
    scratch_sl: Vec<f64>,
    scratch_sr: Vec<f64>,
    /// The block's input, mono, kept for the dynamic detectors.
    ///
    /// Every dynamic band triggers from what arrived at the EQ, not from what
    /// the bands ahead of it have already done to the signal. Feeding them the
    /// running buffer instead couples them into a chain: a band whose region
    /// has been cut by an earlier band sees silence and never engages. On
    /// "Musical Bandpass Filter" two expanding shelves above a 48 dB/oct high
    /// cut should add about 20 dB back at 8 kHz — the plugin does, and ours
    /// sat 49 dB low because those shelves were listening downstream of the
    /// cut.
    side_ref: Vec<f64>,
}

/// A band's normalised magnitude shape, 1 at its own frequency and 0 far away.
///
/// Used by the Auto Gain grid to place a dynamic or spectral band's live gain
/// on the curve: those bands are not in the static chain, so their shape has
/// to be reconstructed. Only the shape matters, not the exact skirt.
fn band_envelope(shape: crate::slope::FilterShape, f0: f64, q: f64, hz: f64) -> f64 {
    use crate::slope::FilterShape as F;
    if hz <= 0.0 || f0 <= 0.0 {
        return 0.0;
    }
    let ratio = hz / f0;
    match shape {
        F::LowShelf => 1.0 / (1.0 + ratio * ratio),
        F::HighShelf => {
            let x = ratio * ratio;
            x / (1.0 + x)
        }
        _ => {
            let qq = (q * core::f64::consts::FRAC_1_SQRT_2).max(0.02);
            let u = (ratio - 1.0 / ratio) * qq;
            1.0 / (1.0 + u * u)
        }
    }
}

/// The level Pro-Q's Character modes sit at, in dB.
///
/// Character introduces "vintage non-linearities and warmth", and measured
/// with a flat band and a tone swept from -60 to -3 dBFS the linear part of it
/// is a **fixed gain**, the same at every level and every frequency:
///
/// ```text
///   Clean   0.00 dB
///   Subtle +0.01
///   Warm   +0.55
/// ```
///
/// The non-linear part is real but small: on noise, which has peaks a tone
/// does not, Warm reads +0.44 dB at -18 dBFS rising to +1.44 at -3, as
/// distortion products fill in the spectrum. That is not modelled — 26 of the
/// 171 factory presets set Character, and at the level the library is measured
/// at the gain accounts for all but a tenth of a decibel of it.
#[inline]
fn character_gain_db(mode: u32) -> f64 {
    match mode {
        1 => 0.01,
        2 => 0.55,
        _ => 0.0,
    }
}

impl FtsEq {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let mk_chain = || {
            let mut chain = crate::chain::EqChain::new();
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
        Self {
            eq: chain,
            eq_b: chain_b,
            splitter: crate::transient::PeakSteadySplitter::new(sample_rate),
            spectral: crate::spectral::SpectralEngine::new(sample_rate, 1024),
            spectral_regions: Vec::with_capacity(EQ_BANDS),
            dyn_bands: (0..EQ_BANDS)
                .map(|_| {
                    let mut d = crate::dynamics::DynBand::new(sample_rate);
                    d.params.enabled = false;
                    d
                })
                .collect(),
            state: [(false, false); EQ_BANDS],
            shapes: [0; EQ_BANDS],
            slopes: [2.0; EQ_BANDS],
            placements: [0; EQ_BANDS],
            streams: [0; EQ_BANDS],
            spectral_on: [false; EQ_BANDS],
            spectral_density: [50.0; EQ_BANDS],
            spectral_tilt: [false; EQ_BANDS],
            transient_mode: false,
            split_solo: 0,
            transient_gain_db: 0.0,
            steady_gain_db: 0.0,
            listen: None,
            solo_filter: crate::dynamics::Svf::new(sample_rate),
            dry_ring: [vec![0.0; 2048], vec![0.0; 2048]],
            dry_pos: 0,
            dyn_cfg: [(0.0, -40.0, 50.0, 50.0, true, false); EQ_BANDS],
            dyn_active: [false; EQ_BANDS],
            dyn_modulated: [false; EQ_BANDS],
            dyn_modulated_gain: [f64::NAN; EQ_BANDS],
            side_cfg: [(false, 20.0, 20_000.0); EQ_BANDS],
            freqs: [1000.0; EQ_BANDS],
            gains: [0.0; EQ_BANDS],
            qs: [0.707; EQ_BANDS],
            output_gain_db: 0.0,
            character: 0,
            output_pan: 0.0,
            output_pan_mid_side: false,
            auto_gain: false,
            auto_gain_db: 0.0,
            auto_grid_hz: Vec::new(),
            auto_grid_static_db: Vec::new(),
            auto_grid_env: Vec::new(),
            gain_scale: 1.0,
            sample_rate,
            prepared: false,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            scratch_sl: Vec::new(),
            scratch_sr: Vec::new(),
            side_ref: Vec::new(),
        }
    }

    /// Route + configure one band after any of its params changed.
    fn sync_band(&mut self, band: usize) {
        let (used, on) = self.state[band];
        let enabled = used && on;
        let shape = crate::slope::FilterShape::from_canonical_index(self.shapes[band]);
        let (range, thr, atk, rel, auto, relative) = self.dyn_cfg[band];
        // A band goes dynamic when it has a range and a dynamics-capable
        // shape (Bell/shelves — same rule as Pro-Q).
        let dyn_shape = match shape {
            crate::slope::FilterShape::Bell => Some(crate::dynamics::DynShape::Bell),
            crate::slope::FilterShape::LowShelf => Some(crate::dynamics::DynShape::LowShelf),
            crate::slope::FilterShape::HighShelf => Some(crate::dynamics::DynShape::HighShelf),
            _ => None,
        };
        let spectral = self.spectral_on[band] && range.abs() > 1.0e-3;
        let go_dynamic = enabled && !spectral && range.abs() > 1.0e-3 && dyn_shape.is_some();
        self.dyn_active[band] = go_dynamic;
        // A dynamic band whose shape has no SVF equivalent — Flat Tilt is the
        // one the factory library uses — keeps its exact static design and has
        // that design's gain ridden by the detector instead. Measured against
        // the plugin, a dynamic Flat Tilt is simply the static Flat Tilt curve
        // scaled by the drive, and the static one already matches to 0.00 dB.
        let modulated = enabled && !spectral && range.abs() > 1.0e-3 && dyn_shape.is_none();
        self.dyn_modulated[band] = modulated;
        if !modulated {
            self.dyn_modulated_gain[band] = f64::NAN;
        }

        let freq = self.freqs[band].clamp(10.0, 30000.0);
        let gain = self.gains[band].clamp(-30.0, 30.0) * self.gain_scale;
        let q = self.qs[band].clamp(0.025, 40.0);

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
                // Continuous slope. A cut's slope is a one-sided roll-off, so
                // the integer part chooses the order and the remainder becomes
                // a ladder. Every other shape has a bounded transition — a
                // shelf settles at a gain, a bell returns to unity — so a
                // ladder would tilt it rather than steepen it; those take the
                // NEAREST integer, which is closer than truncating.
                //
                // Continuity only applies below 36 dB/oct: the settings above
                // it are discrete steps, not a range.
                let raw = self.slopes[band].max(0.0);
                let laddered = matches!(
                    shape,
                    crate::slope::FilterShape::LowCut | crate::slope::FilterShape::HighCut
                ) && raw < 6.0;
                let (index, fraction) = if laddered {
                    (raw.floor() as usize, raw.fract())
                } else {
                    (raw.round() as usize, 0.0)
                };
                let order = shape.effective_order(index);
                // A slope under 6 dB/oct is ALL ladder — integer order zero.
                // Forcing the order to 1 and dropping the fraction turned a
                // 1.3 dB/oct high cut into a 6 dB/oct one; on "Gentle Stereo
                // Narrowing" that took the side channel from 5 dB down at
                // 12.8 kHz to 44, a 33 dB miss that the mono harness could
                // not see because the band is on Side.
                b.order = order;
                b.fractional_order = fraction;
                b.enabled = b.enabled && (order > 0 || fraction > 1.0e-6);
                b.placement = crate::band::Placement::from_index(self.placements[band]);
            }
            chain.update_band(band);
        }
        self.sync_spectral_regions();
        self.sync_listen();
        self.refresh_auto_gain();

        let d = &mut self.dyn_bands[band];
        d.params.enabled = go_dynamic || modulated;
        d.params.modulate_only = modulated;
        if go_dynamic || modulated {
            d.params.shape = dyn_shape.unwrap_or(crate::dynamics::DynShape::Bell);
            d.params.freq_hz = freq;
            d.params.q = q;
            d.params.base_gain_db = gain;
            d.params.range_db = range * self.gain_scale;
            d.params.placement = crate::band::Placement::from_index(self.placements[band]);
            // Side-chain range: a filtered band listens to what it is told to,
            // an unfiltered one listens to itself.
            let (filtered, lo, hi) = self.side_cfg[band];
            d.params.side_mode = if filtered {
                crate::dynamics::SideMode::Free
            } else {
                // A tilt is band-linked too, even though it reshapes the whole
                // spectrum. Driven to a FIXED threshold the plugin applies its
                // full static curve at every frequency — a trigger band around
                // the tilt's own frequency left ours 4.6 dB short at 62 Hz and
                // 5.6 at 16 kHz, and a wide one matched to 0.00. On AUTO,
                // which is what all 15 of the factory library's tilt bands
                // use, the wide trigger measures worse on every preset that
                // has one: four presets moved, none improved, up to 1 dB. The
                // library is the arbiter.
                crate::dynamics::SideMode::BandLinked
            };
            d.params.side_lo_hz = lo;
            d.params.side_hi_hz = hi;
            d.detector.params.threshold_db = if auto { 0.0 } else { thr };
            d.detector.params.auto = auto;
            // Auto FOLLOWS the programme. Measured by feeding the plugin
            // unchanging noise and reading one band every second: its gain
            // walked for about seven seconds and only then held, and it held
            // *partway* to the band's target rather than at it — -7.84 dB
            // where full range is -10.75. A fixed threshold cannot do either.
            //
            // An earlier reading said the opposite, because it was taken
            // before the plugin had settled: every measurement in a sweep of
            // levels was of a threshold still moving, and the same
            // configuration measured twice in one run disagreed by 7 dB.
            d.detector.params.adaptive = auto;
            d.detector.params.relative = relative;
            // Percent knobs around the ballistics measured from the plugin.
            //
            // Attack scales with the band's frequency — a low band cannot ride
            // faster than its own period, and Pro-Q's steps a decade of
            // frequency into roughly a halving of attack time (measured at
            // 1 ms after a step: -6.3 dB at 200 Hz, -8.7 at 1 kHz, -10.0 at
            // 8 kHz on a -12 dB band).
            //
            // Release does NOT. It measures the same at 200 Hz, 1 kHz and
            // 8 kHz — about a 300 ms time constant, an order of magnitude
            // slower than the frequency-scaled figure that used to be derived
            // from the attack. That mattered far more than it looks: on a
            // steady tone the ballistics settle and the difference vanishes,
            // but programme material never settles, so a release 12x too fast
            // let every dynamic band recover between transients and apply far
            // less average reduction than the plugin.
            // Attack time constants read off the plugin's step response on a
            // -12 dB band: 1.34 ms at 200 Hz, 0.77 at 1 kHz, 0.57 at 8 kHz.
            let base_atk = (0.5 + 170.0 / freq.max(1.0)).clamp(0.3, 20.0);
            const BASE_RELEASE_MS: f64 = 300.0;
            let base_rel = BASE_RELEASE_MS;
            d.detector.params.attack_ms =
                base_atk * 8.0f64.powf((atk.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.detector.params.release_ms =
                base_rel * 8.0f64.powf((rel.clamp(0.0, 100.0) - 50.0) / 50.0);
            d.update(self.sample_rate);
        }
    }

    /// Recompute the Auto Gain compensation from the curve the chain draws.
    ///
    /// Pro-Q's Auto Gain holds the broadband level steady, and what it holds
    /// steady is measurable: on "Fast Food Notch" — a wide notch between a low
    /// cut and a high cut — the plugin's whole response sits 4.71 dB above an
    /// uncompensated render of the same preset, flat across every band. The
    /// energy-weighted mean of that preset's own curve is -4.97 dB, so the
    /// compensation is the negative of what the curve does to noise, within a
    /// quarter of a decibel.
    ///
    /// **Pink, not white.** Weighting each grid cell by its bandwidth puts
    /// most of the sum in the top octave, where a high shelf then dominates
    /// the answer. Measured against the plugin on two presets, as the error in
    /// the compensation each weighting predicts:
    ///
    /// ```text
    ///                          pink   white
    ///   Fast Food Notch        0.80    0.26
    ///   Supreme Transparency   0.42    1.79
    /// ```
    ///
    /// **From the drawn curve, not from the signal.** A live level matcher —
    /// K-weighted mean square of the output against the input, which is what
    /// "auto gain" means in most plugins — was built and measured. It fixes
    /// the preset the curve model gets most wrong (`Production Ready Vocals`,
    /// 4.66 dB to 1.07, because that preset's EQ is mostly dynamic and the
    /// static curve cannot see it) and loses everywhere else, for 4.8 dB more
    /// total error across the ten presets that set Auto Gain. It also fails
    /// outright where the two disagree in sign: on `Overheads 2` every
    /// energy-weighted measure says the curve makes the signal *louder* and
    /// the plugin compensates **upward** by 4.5 dB anyway. And a matcher
    /// launders our own broadband errors into the compensation, which makes
    /// the harness less able to see them. The exact law is still unknown.
    ///
    /// Runs only for the presets that switch Auto Gain on — ten of the 171 in
    /// the factory library.
    fn refresh_auto_gain(&mut self) {
        if !self.auto_gain {
            self.auto_gain_db = 0.0;
            self.auto_grid_hz.clear();
            return;
        }
        // 1/12-octave grid from 20 Hz up. Fine enough that a surgical notch is
        // not stepped over, cheap enough to rebuild on a parameter change.
        if self.auto_grid_hz.is_empty() {
            let step = 2.0f64.powf(1.0 / 12.0);
            let ceiling = self.sample_rate * 0.45;
            let mut hz = 20.0f64;
            while hz < ceiling {
                self.auto_grid_hz.push(hz);
                hz *= step;
            }
        }
        let n = self.auto_grid_hz.len();
        self.auto_grid_static_db.clear();
        for i in 0..n {
            let hz = self.auto_grid_hz[i];
            self.auto_grid_static_db.push(self.eq.magnitude_db(hz, self.sample_rate));
        }
        // And the shape of every band the static chain cannot see.
        self.auto_grid_env.clear();
        for band in 0..EQ_BANDS {
            let mut env = vec![0.0f64; n];
            let (used, on) = self.state[band];
            let dynamic = used && on && (self.dyn_active[band] || self.spectral_on[band]);
            if dynamic {
                let shape = crate::slope::FilterShape::from_canonical_index(self.shapes[band]);
                let f0 = self.freqs[band].clamp(10.0, 30000.0);
                let q = self.qs[band].clamp(0.025, 40.0);
                // A band that touches one side of the image only moves half
                // the signal, so it is worth half as much to a broadband
                // compensation. "Hammond Levelling" is four bands that are
                // really two, duplicated for left and right; counting both at
                // full weight doubled the compensation and cost 1.6 dB.
                let w = match crate::band::Placement::from_index(self.placements[band]) {
                    crate::band::Placement::Stereo => 1.0,
                    _ => 0.5,
                };
                for (i, e) in env.iter_mut().enumerate() {
                    *e = w * band_envelope(shape, f0, q, self.auto_grid_hz[i]);
                }
            }
            self.auto_grid_env.push(env);
        }
        self.update_auto_gain();
    }

    /// Sum the grid with whatever the dynamic and spectral bands are applying
    /// right now, and set the compensation.
    fn update_auto_gain(&mut self) {
        if !self.auto_gain || self.auto_grid_static_db.is_empty() {
            return;
        }
        let n = self.auto_grid_static_db.len();
        let mut live = [0.0f64; EQ_BANDS];
        let mut region = 0usize;
        for band in 0..EQ_BANDS {
            let (used, on) = self.state[band];
            if !(used && on) {
                continue;
            }
            if self.spectral_on[band] && self.dyn_cfg[band].0.abs() > 1.0e-3 {
                live[band] = -self.spectral.region_reduction_db(region);
                region += 1;
            } else if self.dyn_active[band] {
                // The band's live gain relative to the base the static chain
                // is NOT carrying — a dynamic band is out of that chain
                // entirely, so its whole applied gain counts here.
                live[band] = self.dyn_bands[band].live_gain_db();
            }
        }
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for i in 0..n {
            let mut db = self.auto_grid_static_db[i];
            for band in 0..EQ_BANDS {
                if live[band] != 0.0 {
                    db += live[band] * self.auto_grid_env[band][i];
                }
            }
            // Equal weight per octave — pink, not white.
            num += 10.0f64.powf(db / 10.0);
            den += 1.0;
        }
        self.auto_gain_db = if den > 0.0 && num > 0.0 {
            (-10.0 * (num / den).log10()).clamp(-30.0, 30.0)
        } else {
            0.0
        };
    }

    /// Turn Pro-Q's Auto Gain on or off.
    pub fn set_auto_gain(&mut self, on: bool) {
        self.auto_gain = on;
        self.refresh_auto_gain();
    }

    /// The compensation Auto Gain is currently applying, in dB (0 when off).
    pub fn auto_gain_db(&self) -> f64 {
        self.auto_gain_db
    }

    /// The Output Pan currently set, and its mode.
    pub fn output_pan(&self) -> f64 {
        self.output_pan
    }

    /// Whether Output Pan balances mid against side rather than left/right.
    pub fn output_pan_mid_side(&self) -> bool {
        self.output_pan_mid_side
    }

    /// Output Pan: -1..1, turning one side down and never boosting the other.
    ///
    /// Measured by writing the global straight into the plugin and reading the
    /// mid and side transfer functions back. In **Mid/Side** mode a negative
    /// value scales the side by `1 + pan` and leaves the mid alone, and a
    /// positive one scales the mid by `1 - pan` and leaves the side alone:
    ///
    /// ```text
    ///   pan     -0.80   -0.44   -0.20   +0.11   +0.50   +0.90
    ///   side   -13.96   -5.06   -1.92    0.00    0.00    0.00
    ///   mid      0.00    0.00    0.00   -0.98   -6.00  -19.98
    /// ```
    ///
    /// In Stereo mode the same law applies to left and right. Nine of the 171
    /// factory presets set the mode and five of those set a non-zero pan; all
    /// five are Mid/Side, and on "Room 01" this global alone was 2.54 dB of a
    /// 3.23 dB error.
    pub fn set_output_pan(&mut self, pan: f64, mid_side: bool) {
        self.output_pan = pan.clamp(-1.0, 1.0);
        self.output_pan_mid_side = mid_side;
    }

    /// Pro-Q's Character mode: 0 Clean, 1 Subtle, 2 Warm.
    ///
    /// Only its **gain** is modelled — see [`character_gain_db`].
    pub fn set_character(&mut self, mode: u32) {
        self.character = mode.min(2);
    }

    /// The static chain's magnitude at `hz`, in dB.
    pub fn static_magnitude_db(&self, hz: f64) -> f64 {
        self.eq.magnitude_db(hz, self.sample_rate)
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
            let freq = self.freqs[band].clamp(10.0, 30000.0);
            let q = self.qs[band].clamp(0.025, 40.0);
            let shape = crate::slope::FilterShape::from_canonical_index(self.shapes[band]);
            self.spectral_regions.push(crate::spectral::SpectralRegion {
                freq_hz: freq,
                q,
                shape: match shape {
                    crate::slope::FilterShape::LowShelf => {
                        crate::spectral::SpectralShape::LowShelf
                    }
                    crate::slope::FilterShape::HighShelf => {
                        crate::spectral::SpectralShape::HighShelf
                    }
                    // Bell for everything else: 54 of the 74 spectral bands in
                    // the factory library are bells, and the handful that are
                    // not shelves are close enough to one that a separate
                    // curve for each would be fitting noise.
                    _ => crate::spectral::SpectralShape::Bell,
                },
                // The band's range is the ceiling on how far a bin may be
                // pulled down, not a scale factor against some other maximum.
                max_depth_db: range.abs(),
                // An ABSOLUTE per-bin threshold in dBFS, not a prominence.
                // The manual knob's own range is -80..0 dB.
                threshold_db: thr,
                auto,
                density: (self.spectral_density[band] / 100.0).clamp(0.0, 1.0),
                tilt: self.spectral_tilt[band],
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
        let freq = self.freqs[band].clamp(10.0, 30000.0);
        let q = self.qs[band].clamp(0.025, 40.0);
        use crate::dynamics::SvfShape;
        use crate::slope::FilterShape as F;
        let (shape, sf, sq) = match F::from_canonical_index(self.shapes[band]) {
            F::LowShelf | F::LowCut => (SvfShape::Lowpass, freq, 0.707),
            F::HighShelf | F::HighCut => (SvfShape::Highpass, freq, 0.707),
            // Bells, notches, bandpasses, tilts: hear the band region.
            _ => (SvfShape::Bandpass, freq, q.max(0.5)),
        };
        self.solo_filter.set(shape, sf, sq, 0.0);
    }
    pub fn live_dyn_gain_db(&self, band: usize) -> Option<f64> {
        (band < EQ_BANDS && self.dyn_active[band]).then(|| self.dyn_bands[band].live_gain_db())
    }    /// Added latency in samples — non-zero only while a spectral band puts
    /// the STFT in the path.
    pub fn latency(&self) -> u32 {
        // Spectral bands put the STFT in the path; everything else is
        // zero-latency.
        if self.spectral.has_regions() {
            self.spectral.latency() as u32
        } else {
            0
        }
    }
    pub fn prepare(&mut self, sample_rate: f64, block_size: u32) {
        self.sample_rate = sample_rate.max(1.0);
        self.eq.set_sample_rate(self.sample_rate);
        self.eq.reset();
        self.eq_b.set_sample_rate(self.sample_rate);
        self.eq_b.reset();
        self.splitter.update(self.sample_rate);
        // 4096 rather than 1024. A 1024-point analysis has a main lobe about
        // 94 Hz wide at either side of a bin, so a resonance drags its
        // neighbours down whatever Density says — measured against the plugin,
        // a bin 109 Hz from a resonance came down 11 dB where Pro-Q left it
        // alone. Density can widen a neighbourhood but nothing can narrow it
        // below the resolution it is measured at. The cost is latency, which
        // a spectral band already has and which `latency()` reports.
        self.spectral = crate::spectral::SpectralEngine::new(self.sample_rate, 4096);
        // Room for the whole delay the delta-listen read walks back over, plus
        // a block so a write and a read never collide inside one buffer.
        let ring = (self.spectral.latency() + block_size.max(1) as usize).next_power_of_two();
        self.dry_ring = [vec![0.0; ring], vec![0.0; ring]];
        self.dry_pos = 0;
        self.sync_spectral_regions();
        for b in 0..EQ_BANDS {
            self.dyn_bands[b].reset();
            self.sync_band(b);
        }
        self.scratch_l = vec![0.0; block_size.max(1) as usize];
        self.scratch_r = vec![0.0; block_size.max(1) as usize];
        self.scratch_sl = vec![0.0; block_size.max(1) as usize];
        self.scratch_sr = vec![0.0; block_size.max(1) as usize];
        self.side_ref = vec![0.0; block_size.max(1) as usize];
        self.prepared = true;
    }

    pub fn is_prepared(&self) -> bool {
        self.prepared
    }
    /// Process one block in place.
    pub fn process(&mut self, buf_l: &mut [f64], buf_r: &mut [f64]) {
        // Fully-idle block (no active bands, no dynamics, no spectral,
        // no transient split, unity output): straight copy, zero DSP.
        let any_dyn = self.dyn_active.iter().any(|&a| a)
            || self.dyn_modulated.iter().any(|&a| a);
        if self.listen.is_none()
            && !self.transient_mode
            && !any_dyn
            && !self.spectral.has_regions()
            && !self.eq.has_active_bands()
            && (self.output_gain_db + self.auto_gain_db + character_gain_db(self.character)).abs()
                < 1.0e-9
            && self.output_pan.abs() < 1.0e-9
        {
            return;
        }
        // The compensation follows the curve the chain is applying right now,
        // so it is recomputed once a block — the grid itself is cached.
        if self.auto_gain {
            self.update_auto_gain();
        }
        let eq = &mut self.eq;
        let eq_b = &mut self.eq_b;
        let splitter = &mut self.splitter;
        let spectral = &mut self.spectral;
        let dyn_bands = &mut self.dyn_bands;
        let dyn_active = &self.dyn_active;
        let dyn_modulated = &self.dyn_modulated;
        let pan = self.output_pan;
        let pan_mid_side = self.output_pan_mid_side;
        let dyn_modulated_gain = &mut self.dyn_modulated_gain;
        let transient_mode = self.transient_mode;
        let split_solo = self.split_solo;
        let tg = audiocore_dsp::db::db_to_linear(self.transient_gain_db);
        let sg = audiocore_dsp::db::db_to_linear(self.steady_gain_db);
        let out_gain = audiocore_dsp::db::db_to_linear(
            self.output_gain_db + self.auto_gain_db + character_gain_db(self.character),
        );
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
        // Snapshot the input for the detectors before anything touches it.
        let n_in = buf_l.len().min(buf_r.len());
        if self.side_ref.len() < n_in {
            self.side_ref.resize(n_in, 0.0);
        }
        for i in 0..n_in {
            self.side_ref[i] = 0.5 * (buf_l[i] + buf_r[i]);
        }
        let side_ref = &self.side_ref;

        {
            let l: &mut [f64] = buf_l;
            let r: &mut [f64] = buf_r;
            {
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
                    // Bands whose dynamics ride the static design run their
                    // detectors over the block first, then the design is
                    // rebuilt at the gain they arrived at. Once per block, and
                    // only when the gain has actually moved — a redesign is
                    // not free, and a tenth of a decibel is inaudible.
                    for (bi, d) in dyn_bands.iter_mut().enumerate() {
                        if !dyn_modulated[bi] {
                            continue;
                        }
                        for i in 0..l.len() {
                            d.observe(l[i], r[i], side_ref[i]);
                        }
                        let g = d.live_gain_db();
                        if !(g - dyn_modulated_gain[bi]).abs().lt(&0.1) {
                            dyn_modulated_gain[bi] = g;
                            if let Some(b) = eq.band_mut(bi) {
                                b.gain_db = g;
                            }
                            eq.update_band(bi);
                        }
                    }
                    eq.process(l, r);
                }
                for (bi, d) in dyn_bands.iter_mut().enumerate() {
                    if !dyn_active[bi] {
                        continue;
                    }
                    for i in 0..l.len() {
                        d.tick(&mut l[i], &mut r[i], side_ref[i]);
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
                // Output Pan: turn one side down, never boost the other.
                if pan.abs() > 1.0e-9 {
                    let (a, b) = (1.0 + pan.min(0.0), 1.0 - pan.max(0.0));
                    if pan_mid_side {
                        for i in 0..l.len() {
                            let (m, sd) = (0.5 * (l[i] + r[i]), 0.5 * (l[i] - r[i]));
                            let (m, sd) = (m * b, sd * a);
                            l[i] = m + sd;
                            r[i] = m - sd;
                        }
                    } else {
                        for i in 0..l.len() {
                            l[i] *= a;
                            r[i] *= b;
                        }
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
            }
        }
    }
    pub fn deactivate(&mut self) {
        self.prepared = false;
    }
}

impl FtsEq {
    /// Set one band's static configuration.
    pub fn set_band(&mut self, band: usize, cfg: BandConfig) {
        if band >= EQ_BANDS {
            return;
        }
        self.state[band] = (cfg.used, cfg.enabled);
        self.freqs[band] = cfg.freq_hz;
        self.gains[band] = cfg.gain_db;
        self.qs[band] = cfg.q;
        self.shapes[band] = cfg.shape;
        self.slopes[band] = cfg.slope;
        self.placements[band] = cfg.placement as u32;
        self.streams[band] = cfg.stream;
        self.sync_band(band);
    }

    /// Set one band's dynamics.
    pub fn set_band_dynamics(&mut self, band: usize, dynamics: BandDynamics) {
        if band >= EQ_BANDS {
            return;
        }
        self.dyn_cfg[band] = (
            dynamics.range_db,
            dynamics.threshold_db,
            dynamics.attack_pct,
            dynamics.release_pct,
            dynamics.auto,
            dynamics.relative,
        );
        self.spectral_on[band] = dynamics.spectral;
        self.spectral_density[band] = dynamics.spectral_density;
        self.spectral_tilt[band] = dynamics.spectral_tilt;
        self.side_cfg[band] = (
            dynamics.side_filtered,
            dynamics.side_lo_hz,
            dynamics.side_hi_hz,
        );
        self.sync_band(band);
    }

    /// Output trim in dB, applied after everything else.
    pub fn set_output_gain_db(&mut self, db: f64) {
        self.output_gain_db = db;
    }

    /// A global scale on every band's gain and dynamic range — the "EQ amount"
    /// macro. 1.0 leaves the curve as written.
    pub fn set_gain_scale(&mut self, scale: f64) {
        self.gain_scale = scale;
        for b in 0..EQ_BANDS {
            self.sync_band(b);
        }
    }

    pub fn gain_scale(&self) -> f64 {
        self.gain_scale
    }

    /// Split the signal into transient and steady streams and run a separate
    /// filter bank on each; bands choose a stream with `BandConfig::stream`.
    pub fn set_transient_mode(&mut self, on: bool) {
        self.transient_mode = on;
        for b in 0..EQ_BANDS {
            self.sync_band(b);
        }
    }

    pub fn set_transient_gain_db(&mut self, db: f64) {
        self.transient_gain_db = db;
    }

    pub fn set_steady_gain_db(&mut self, db: f64) {
        self.steady_gain_db = db;
    }

    /// Transient/steady split shaping. Balance decides where the line between
    /// the two streams falls; attack, hold and smooth are the detector's
    /// ballistics, all as percentages.
    pub fn set_split_balance(&mut self, balance: f64) {
        self.splitter.params.balance = balance;
        self.splitter.update(self.sample_rate);
    }

    pub fn set_split_attack(&mut self, attack: f64) {
        self.splitter.params.attack = attack;
        self.splitter.update(self.sample_rate);
    }

    pub fn set_split_hold(&mut self, hold: f64) {
        self.splitter.params.hold = hold;
        self.splitter.update(self.sample_rate);
    }

    pub fn set_split_smooth(&mut self, smooth: f64) {
        self.splitter.params.smooth = smooth;
        self.splitter.update(self.sample_rate);
    }

    /// Solo one of the two streams: 0 both, 1 transient, 2 steady.
    pub fn set_split_solo(&mut self, solo: u32) {
        self.split_solo = solo;
    }

    /// Listen to one band in isolation: `Some((band, 1))` solos what it
    /// touches, `Some((band, 2))` plays the difference it is making.
    pub fn set_listen(&mut self, listen: Option<(usize, u32)>) {
        self.listen = listen;
        self.sync_listen();
    }

    pub fn listen(&self) -> Option<(usize, u32)> {
        self.listen
    }

    /// Whether any band currently routes through the whole-band dynamics.
    pub fn any_dynamic(&self) -> bool {
        self.dyn_active.iter().any(|&a| a)
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}
