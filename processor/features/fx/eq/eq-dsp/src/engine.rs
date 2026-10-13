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

use dsp_core::num;

/// Bands the engine carries. Pro-Q 4's count, because the translated presets
/// are written against it.
pub const EQ_BANDS: usize = 24;

pub use crate::host::{CanonicalBandConfig as BandConfig, CanonicalBandDynamics as BandDynamics};

const fn eq_shape_to_filter(shape: crate::design::slope::FilterShape) -> crate::FilterType {
    shape.to_filter_type()
}

/// Everything the engine tracks for one band.
///
/// This replaced sixteen parallel `[_; EQ_BANDS]` arrays walked by a shared
/// index, three of them anonymous tuples — `dyn_cfg` was a
/// `(f64, f64, f64, f64, bool, bool)` destructured positionally at every use,
/// which is six chances to transpose two settings and no way for the compiler
/// to notice. Naming the fields is most of the value here; removing 56
/// indexing sites is the rest.
#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent switch on the plugin panel — used, enabled, auto, relative, spectral, tilt, side-filtered and so on. Grouping them into a config struct would break every call site and tell a reader nothing the field names do not already say"
)]
struct BandSlot {
    /// Pro-Q's "Used": the band exists at all.
    used: bool,
    /// Switched on rather than bypassed. A band renders only when both are set.
    enabled: bool,
    /// Canonical shape index, and the slope (needed for routing and
    /// effective-order resolution).
    shape: crate::design::slope::FilterShape,
    slope: ResolvedSlope,
    placement: crate::Placement,
    stream: crate::Stream,
    /// Frequency / gain / Q in their own units. These used to be read out of
    /// the host's id-indexed value vector, which is what tied the engine to
    /// one particular parameter numbering; they live here so any front end
    /// can drive it.
    freq_hz: f64,
    gain_db: f64,
    q: f64,
    dynamics: DynSettings,
    side: SideChain,
    spectral: SpectralSettings,
    /// Whether the band currently routes through the dynamic engine.
    dyn_active: bool,
    /// Whether the band's dynamics ride the STATIC design's gain instead —
    /// the shapes the SVF cannot build. See `DynBandParams::modulate_only`.
    dyn_modulated: bool,
    /// The gain each modulated band last had designed in, so the static
    /// cascade is only rebuilt when it has actually moved.
    dyn_modulated_gain: f64,
}

/// The raw dynamics parameters, formerly a six-tuple.
#[derive(Debug, Clone, Copy)]
struct DynSettings {
    range_db: f64,
    threshold_db: f64,
    attack_pct: f64,
    release_pct: f64,
    auto: bool,
    relative: bool,
}

/// The band's side-chain listening range, formerly a three-tuple.
#[derive(Debug, Clone, Copy)]
struct SideChain {
    filtered: bool,
    lo_hz: f64,
    hi_hz: f64,
}

/// Per-band spectral-mode settings.
#[derive(Debug, Clone, Copy)]
struct SpectralSettings {
    on: bool,
    density: f64,
    tilt: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedSlope {
    pub(crate) order: usize,
    pub(crate) fraction: f64,
}

impl BandSlot {
    const DEFAULT: Self = Self {
        used: false,
        enabled: false,
        shape: crate::design::slope::FilterShape::Bell,
        slope: ResolvedSlope {
            order: 2,
            fraction: 0.0,
        },
        placement: crate::Placement::Stereo,
        stream: crate::Stream::Both,
        freq_hz: 1000.0,
        gain_db: 0.0,
        q: 0.707,
        dynamics: DynSettings {
            range_db: 0.0,
            threshold_db: -40.0,
            attack_pct: 50.0,
            release_pct: 50.0,
            auto: true,
            relative: false,
        },
        side: SideChain {
            filtered: false,
            lo_hz: 20.0,
            hi_hz: 20_000.0,
        },
        spectral: SpectralSettings {
            on: false,
            density: 50.0,
            tilt: false,
        },
        dyn_active: false,
        dyn_modulated: false,
        dyn_modulated_gain: f64::NAN,
    };
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "engine-wide mode switches (transient split, freeze, prepared, pan mid/side, …), each an independent plugin control"
)]
pub struct FtsEq {
    eq: crate::runtime::chain::EqChain,
    /// Steady-stream chain (transient mode only; mirrors band configs
    /// per `b{i}_stream` — separate instance = separate filter state).
    eq_b: crate::runtime::chain::EqChain,
    splitter: crate::dynamics::transient::PeakSteadySplitter,
    spectral: crate::dynamics::spectral::SpectralEngine,
    spectral_regions: Vec<crate::dynamics::spectral::SpectralRegion>,
    dyn_bands: Vec<crate::dynamics::DynBand>,
    /// Per-band settings and routing flags. One struct rather than the
    /// sixteen parallel arrays this used to be.
    bands: Vec<BandSlot>,
    auto_live: Vec<f64>,
    canonical_cache: Vec<Option<(BandConfig, BandDynamics)>>,
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
    output_gain_db: f64,
    /// Pro-Q's Character: 0 Clean, 1 Subtle, 2 Warm.
    character: u32,
    character_shaper: [CharacterShaper; 2],
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
fn band_envelope(shape: crate::design::slope::FilterShape, f0: f64, q: f64, hz: f64) -> f64 {
    use crate::design::slope::FilterShape as F;
    if hz <= 0.0 || f0 <= 0.0 {
        return 0.0;
    }
    let ratio = hz / f0;
    match shape {
        F::LowShelf => 1.0 / ratio.mul_add(ratio, 1.0),
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
/// with a flat band and a tone swept from -60 to -3 dBFS the fundamental only
/// ever moves by a fixed gain, the same at every level and every frequency:
///
/// ```text
///   Clean   0.00 dB
///   Subtle +0.01
///   Warm   +0.55
/// ```
///
/// What the shaper does lands in the harmonics — see [`CharacterShaper`].
/// How much second harmonic Warm generates, as the coefficient of a squarer.
///
/// Measured by feeding a sine and reading the output at the fundamental and
/// the next four harmonics. Warm produces **only a second harmonic**, and its
/// level tracks the input one-for-one over 35 dB:
///
/// ```text
///   in dBFS     -36     -30     -24     -18     -12      -6      -1
///   2nd      -61.87  -55.88  -49.88  -43.88  -37.88  -31.88  -26.88
///   3rd     -108.84 -118.78 -123.30 -146.25 -136.31 -142.06 -145.13
/// ```
///
/// A second harmonic proportional to the square of the input, with nothing
/// above it, is a squarer and nothing else. (Subtle generates no harmonics at
/// all — it is a hundredth of a decibel of gain and no more.)
const CHARACTER_SQUARE: f64 = 0.0739;

/// The low shelf the squarer's input is driven through: gain at DC, and the
/// corner it returns to unity above.
///
/// The distortion is stronger at the bottom. Reading the second harmonic at
/// -18 dBFS across frequency, and halving it to get the drive:
///
/// ```text
///      Hz       60     100     500    3000    8000
///   Pro-Q   -39.53  -39.37  -43.88  -45.91  -45.98
///    ours   -39.12  -39.46  -43.67  -45.84  -45.92
/// ```
///
/// Flat below about 100 Hz and flat again above 3 kHz, which is a shelf. The
/// two constants were fitted together against that row; the worst miss is 0.4
/// dB on a harmonic forty decibels down.
const CHARACTER_DRIVE_DB: f64 = 3.6;
const CHARACTER_DRIVE_HZ: f64 = 280.0;

/// Warm's waveshaper: a second-harmonic generator with a low-emphasised drive.
#[derive(Debug, Clone, Copy, Default)]
struct CharacterShaper {
    /// One-pole state for the drive shelf, and its coefficient.
    lp: f64,
    lp_coeff: f64,
    /// Shelf lift, `10^(drive/20) - 1`.
    lift: f64,
    /// DC blocker — a squarer puts as much energy at zero hertz as it does at
    /// the second harmonic, and that has nowhere useful to go.
    dc_x1: f64,
    dc_y1: f64,
    dc_r: f64,
}

impl CharacterShaper {
    fn update(&mut self, sample_rate: f64) {
        self.lp_coeff = 1.0 - (-core::f64::consts::TAU * CHARACTER_DRIVE_HZ / sample_rate).exp();
        self.lift = 10.0f64.powf(CHARACTER_DRIVE_DB / 20.0) - 1.0;
        self.dc_r = 1.0 - core::f64::consts::TAU * 5.0 / sample_rate;
        self.lp = 0.0;
        self.dc_x1 = 0.0;
        self.dc_y1 = 0.0;
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        self.lp += (x - self.lp) * self.lp_coeff;
        let drive = self.lift.mul_add(self.lp, x);
        let second = CHARACTER_SQUARE * drive * drive;
        let blocked = self.dc_r.mul_add(self.dc_y1, second - self.dc_x1);
        self.dc_x1 = second;
        self.dc_y1 = blocked;
        x + blocked
    }
}

#[inline]
const fn character_gain_db(mode: u32) -> f64 {
    match mode {
        1 => 0.01,
        2 => 0.55,
        _ => 0.0,
    }
}

impl FtsEq {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        Self::with_capacity(sample_rate, EQ_BANDS)
    }

    pub(crate) fn with_capacity(sample_rate: f64, capacity: usize) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let mk_chain = || {
            let mut chain = crate::runtime::chain::EqChain::with_capacity(capacity);
            chain.set_sample_rate(sample_rate);
            for _ in 0..capacity {
                let Ok(idx) = chain.add_band() else {
                    break;
                };
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
            splitter: crate::dynamics::transient::PeakSteadySplitter::new(sample_rate),
            spectral: crate::dynamics::spectral::SpectralEngine::with_capacity(
                sample_rate,
                1024,
                capacity,
            ),
            spectral_regions: Vec::with_capacity(capacity),
            dyn_bands: (0..capacity)
                .map(|_| {
                    let mut d = crate::dynamics::DynBand::new(sample_rate);
                    d.params.enabled = false;
                    d
                })
                .collect(),
            bands: vec![BandSlot::DEFAULT; capacity],
            auto_live: vec![0.0; capacity],
            canonical_cache: vec![None; capacity],
            transient_mode: false,
            split_solo: 0,
            transient_gain_db: 0.0,
            steady_gain_db: 0.0,
            listen: None,
            solo_filter: crate::dynamics::Svf::new(sample_rate),
            dry_ring: [vec![0.0; 2048], vec![0.0; 2048]],
            dry_pos: 0,
            output_gain_db: 0.0,
            character: 0,
            character_shaper: [CharacterShaper::default(); 2],
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
            scratch_sl: Vec::new(),
            scratch_sr: Vec::new(),
            side_ref: Vec::new(),
        }
    }

    /// The static chain's magnitude at `hz`, in dB.
    #[must_use]
    pub fn static_magnitude_db(&self, hz: f64) -> f64 {
        self.eq.magnitude_db(hz, self.sample_rate)
    }

    /// Rebuild the shared spectral engine's band-region set from every
    /// enabled band with `spectral` on and a non-zero dynamic range.
    fn sync_spectral_regions(&mut self) {
        self.spectral_regions.clear();
        for slot in &self.bands {
            let (used, on) = (slot.used, slot.enabled);
            if !(used && on && slot.spectral.on) {
                continue;
            }
            let DynSettings {
                range_db: range,
                threshold_db: thr,
                auto,
                ..
            } = slot.dynamics;
            if range.abs() <= 1.0e-3 {
                continue;
            }
            let freq = slot.freq_hz.clamp(10.0, 30000.0);
            let q = slot.q.clamp(0.025, 40.0);
            let shape = slot.shape;
            self.spectral_regions
                .push(crate::dynamics::spectral::SpectralRegion {
                    freq_hz: freq,
                    q,
                    shape: match shape {
                        crate::design::slope::FilterShape::LowShelf => {
                            crate::dynamics::spectral::SpectralShape::LowShelf
                        }
                        crate::design::slope::FilterShape::HighShelf => {
                            crate::dynamics::spectral::SpectralShape::HighShelf
                        }
                        // Bell for everything else: 54 of the 74 spectral bands in
                        // the factory library are bells, and the handful that are
                        // not shelves are close enough to one that a separate
                        // curve for each would be fitting noise.
                        _ => crate::dynamics::spectral::SpectralShape::Bell,
                    },
                    // The band's range is the ceiling on how far a bin may be
                    // pulled down, not a scale factor against some other maximum.
                    max_depth_db: range.abs(),
                    // An ABSOLUTE per-bin threshold in dBFS, not a prominence.
                    // The manual knob's own range is -80..0 dB.
                    threshold_db: thr,
                    auto,
                    density: (slot.spectral.density / 100.0).clamp(0.0, 1.0),
                    tilt: slot.spectral.tilt,
                });
        }
        self.spectral.set_regions(&self.spectral_regions);
    }

    /// Whether the spectral engine is currently in the signal path.
    #[must_use]
    pub const fn spectral_engaged(&self) -> bool {
        self.spectral.has_regions()
    }

    /// Configure the solo filter for the active listen band: the
    /// region you hear follows the band's shape — bells/notches solo a
    /// bandpass at freq/Q, shelves and cuts solo everything they reach.
    fn sync_listen(&mut self) {
        use crate::design::slope::FilterShape as F;
        use crate::dynamics::SvfShape;

        let Some((band, mode)) = self.listen else {
            return;
        };
        if mode != 1 {
            return;
        }
        let Some(slot) = self.bands.get(band) else {
            return;
        };

        let freq = slot.freq_hz.clamp(10.0, 30000.0);
        let q = slot.q.clamp(0.025, 40.0);
        let (shape, sf, sq) = match slot.shape {
            F::LowShelf | F::LowCut => (SvfShape::Lowpass, freq, 0.707),
            F::HighShelf | F::HighCut => (SvfShape::Highpass, freq, 0.707),
            // Bells, notches, bandpasses, tilts: hear the band region.
            _ => (SvfShape::Bandpass, freq, q.max(0.5)),
        };
        self.solo_filter.set(shape, sf, sq, 0.0);
    }
    #[must_use]
    pub fn live_dyn_gain_db(&self, band: usize) -> Option<f64> {
        self.bands
            .get(band)
            .is_some_and(|slot| slot.dyn_active || slot.dyn_modulated)
            .then(|| {
                self.dyn_bands
                    .get(band)
                    .map(crate::dynamics::DynBand::live_gain_db)
            })
            .flatten()
    }

    /// Added latency in samples — non-zero only while a spectral band puts
    /// the STFT in the path.
    #[must_use]
    pub fn latency(&self) -> u32 {
        // Spectral bands put the STFT in the path; everything else is
        // zero-latency.
        if self.spectral.has_regions() {
            u32::try_from(self.spectral.latency()).unwrap_or(u32::MAX)
        } else {
            0
        }
    }
    pub fn prepare(&mut self, sample_rate: f64, block_size: u32) {
        self.sample_rate = sample_rate.max(1.0);
        self.auto_grid_hz.clear();
        for sh in &mut self.character_shaper {
            sh.update(self.sample_rate);
        }
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
        self.spectral = crate::dynamics::spectral::SpectralEngine::with_capacity(
            self.sample_rate,
            4096,
            self.bands.len(),
        );
        // Room for the whole delay the delta-listen read walks back over, plus
        // a block so a write and a read never collide inside one buffer.
        let ring = self
            .spectral
            .latency()
            .saturating_add(num::u32_to_index(block_size.max(1)))
            .next_power_of_two();
        self.dry_ring = [vec![0.0; ring], vec![0.0; ring]];
        self.dry_pos = 0;
        self.sync_spectral_regions();
        for band in &mut self.dyn_bands {
            band.reset();
        }
        for b in 0..self.bands.len() {
            self.sync_band(b);
        }
        self.scratch_sl = vec![0.0; num::u32_to_index(block_size.max(1))];
        self.scratch_sr = vec![0.0; num::u32_to_index(block_size.max(1))];
        self.side_ref = vec![0.0; num::u32_to_index(block_size.max(1))];
        let grid_capacity = num::f64_to_index(
            ((self.sample_rate * 0.45 / 20.0).log2() * 12.0)
                .ceil()
                .max(0.0),
        )
        .saturating_add(2);
        self.auto_grid_hz.reserve(grid_capacity);
        self.auto_grid_static_db.reserve(grid_capacity);
        self.auto_grid_env
            .resize_with(self.bands.len(), || Vec::with_capacity(grid_capacity));
        for env in &mut self.auto_grid_env {
            env.reserve(grid_capacity);
        }
        self.prepared = true;
    }

    #[must_use]
    pub const fn is_prepared(&self) -> bool {
        self.prepared
    }

    pub const fn deactivate(&mut self) {
        self.prepared = false;
    }
}

impl FtsEq {
    /// Set one band's static configuration.
    pub fn set_band(&mut self, band: usize, cfg: BandConfig) {
        if let Some(cache) = self.canonical_cache.get_mut(band) {
            *cache = None;
        }
        self.store_band(band, cfg);
        if band < self.bands.len() {
            self.sync_band(band);
        }
    }

    fn store_band(&mut self, band: usize, cfg: BandConfig) {
        let Some(slot) = self.bands.get_mut(band) else {
            return;
        };
        slot.used = cfg.used;
        slot.enabled = cfg.enabled;
        slot.freq_hz = cfg.freq_hz;
        slot.gain_db = cfg.gain_db;
        slot.q = cfg.q;
        slot.shape = crate::design::slope::FilterShape::from_canonical_index(cfg.shape);
        slot.slope = crate::host::resolve_slope(slot.shape, cfg.slope);
        slot.placement = cfg.placement;
        slot.stream = match cfg.stream {
            1 => crate::Stream::Transient,
            2 => crate::Stream::Steady,
            _ => crate::Stream::Both,
        };
    }

    /// Set one band's dynamics.
    pub fn set_band_dynamics(&mut self, band: usize, dynamics: BandDynamics) {
        if let Some(cache) = self.canonical_cache.get_mut(band) {
            *cache = None;
        }
        self.store_dynamics(band, dynamics);
        if band < self.bands.len() {
            self.sync_band(band);
        }
    }

    fn store_dynamics(&mut self, band: usize, dynamics: BandDynamics) {
        let Some(slot) = self.bands.get_mut(band) else {
            return;
        };
        slot.dynamics = DynSettings {
            range_db: dynamics.range_db,
            threshold_db: dynamics.threshold_db,
            attack_pct: dynamics.attack_pct,
            release_pct: dynamics.release_pct,
            auto: dynamics.auto,
            relative: dynamics.relative,
        };
        slot.spectral.on = dynamics.spectral;
        slot.spectral.density = dynamics.spectral_density;
        slot.spectral.tilt = dynamics.spectral_tilt;
        slot.side = SideChain {
            filtered: dynamics.side_filtered,
            lo_hz: dynamics.side_lo_hz,
            hi_hz: dynamics.side_hi_hz,
        };
    }

    /// Output trim in dB, applied after everything else.
    pub const fn set_output_gain_db(&mut self, db: f64) {
        self.output_gain_db = db;
    }

    /// A global scale on every band's gain and dynamic range — the "EQ amount"
    /// macro. 1.0 leaves the curve as written.
    pub fn set_gain_scale(&mut self, scale: f64) {
        if self.gain_scale.to_bits() == scale.to_bits() {
            return;
        }
        self.gain_scale = scale;
        for b in 0..self.bands.len() {
            self.sync_band(b);
        }
    }

    #[must_use]
    pub const fn gain_scale(&self) -> f64 {
        self.gain_scale
    }

    /// Split the signal into transient and steady streams and run a separate
    /// filter bank on each; bands choose a stream with `BandConfig::stream`.
    pub fn set_transient_mode(&mut self, on: bool) {
        if self.transient_mode == on {
            return;
        }
        self.transient_mode = on;
        for b in 0..self.bands.len() {
            self.sync_band(b);
        }
    }

    pub const fn set_transient_gain_db(&mut self, db: f64) {
        self.transient_gain_db = db;
    }

    pub const fn set_steady_gain_db(&mut self, db: f64) {
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
    pub const fn set_split_solo(&mut self, solo: u32) {
        self.split_solo = solo;
    }

    /// Listen to one band in isolation: `Some((band, 1))` solos what it
    /// touches, `Some((band, 2))` plays the difference it is making.
    pub fn set_listen(&mut self, listen: Option<(usize, u32)>) {
        self.listen = listen;
        self.sync_listen();
    }

    #[must_use]
    pub const fn listen(&self) -> Option<(usize, u32)> {
        self.listen
    }

    /// Whether any band currently routes through the whole-band dynamics.
    #[must_use]
    pub fn any_dynamic(&self) -> bool {
        self.bands
            .iter()
            .any(|slot| slot.dyn_active || slot.dyn_modulated)
    }

    #[must_use]
    pub const fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}

impl FtsEq {
    pub(crate) fn install_band(
        &mut self,
        index: usize,
        config: BandConfig,
        dynamics: BandDynamics,
        filter: &crate::PreparedFilter,
    ) {
        self.store_band(index, config);
        self.store_dynamics(index, dynamics);
        self.sync_band_prepared(index, Some(filter));
    }

    /// Apply both legacy host parameter groups with one synchronization.
    pub fn set_canonical_band(&mut self, index: usize, config: BandConfig, dynamics: BandDynamics) {
        let Some(cache) = self.canonical_cache.get_mut(index) else {
            return;
        };
        if *cache == Some((config, dynamics)) {
            return;
        }
        *cache = Some((config, dynamics));
        self.store_band(index, config);
        self.store_dynamics(index, dynamics);
        self.sync_band(index);
    }

    pub(crate) fn set_ballistics_ms(&mut self, index: usize, attack: f64, release: f64) {
        if let Some(band) = self.dyn_bands.get_mut(index) {
            band.detector.params.attack_ms = attack;
            band.detector.params.release_ms = release;
            band.detector.update(self.sample_rate);
        }
    }

    pub(crate) fn reset_band(&mut self, index: usize) {
        if let Some(b) = self.eq.band_mut(index) {
            b.reset();
        }
        if let Some(b) = self.eq_b.band_mut(index) {
            b.reset();
        }
        if let Some(b) = self.dyn_bands.get_mut(index) {
            b.reset();
        }
    }

    /// Clear audio history without coefficient design or allocation.
    pub fn reset(&mut self) {
        self.eq.reset();
        self.eq_b.reset();
        self.splitter.reset();
        self.spectral.reset();
        self.solo_filter.reset();
        for b in &mut self.dyn_bands {
            b.reset();
        }
        for ring in &mut self.dry_ring {
            ring.fill(0.0);
        }
        self.dry_pos = 0;
        for sh in &mut self.character_shaper {
            sh.lp = 0.0;
            sh.dc_x1 = 0.0;
            sh.dc_y1 = 0.0;
        }
        self.side_ref.fill(0.0);
        self.scratch_sl.fill(0.0);
        self.scratch_sr.fill(0.0);
    }
}

mod output;
mod processing;
mod routing;

impl FtsEq {
    pub(crate) fn configure_globals(&mut self, output: crate::Output, transient: crate::Transient) {
        self.output_gain_db = output.gain_db;
        self.gain_scale = output.gain_scale;
        self.auto_gain = output.auto_gain;
        self.set_character(match output.character {
            crate::Character::Clean => 0,
            crate::Character::Subtle => 1,
            crate::Character::Warm => 2,
        });
        let (pan, ms) = match output.pan {
            crate::Pan::Center => (0.0, false),
            crate::Pan::LeftRight(p) => (p, false),
            crate::Pan::MidSide(p) => (p, true),
        };
        self.set_output_pan(pan, ms);
        self.transient_mode = transient.enabled;
        self.transient_gain_db = transient.transient_gain_db;
        self.steady_gain_db = transient.steady_gain_db;
        self.split_solo = crate::prepared::stream_index(transient.solo);
        self.splitter.params.balance = transient.balance;
        self.splitter.params.attack = transient.attack_percent;
        self.splitter.params.hold = transient.hold_percent;
        self.splitter.params.smooth = transient.smooth_percent;
        self.splitter.update(self.sample_rate);
    }

    pub(crate) fn disable_unused(&mut self, first: usize) {
        for (index, slot) in self.bands.iter_mut().enumerate().skip(first) {
            slot.used = false;
            slot.dyn_active = false;
            slot.dyn_modulated = false;
            if let Some(band) = self.eq.band_mut(index) {
                band.enabled = false;
            }
            if let Some(band) = self.eq_b.band_mut(index) {
                band.enabled = false;
            }
        }
    }

    pub(crate) fn finish_update(&mut self) {
        self.sync_spectral_regions();
        self.sync_listen();
        self.refresh_auto_gain();
    }
}

impl FtsEq {
    #[must_use]
    pub fn last_design_error(&self, index: usize) -> Option<crate::Error> {
        self.eq.band(index).and_then(|b| b.last_design_error)
    }
}

impl FtsEq {
    pub(crate) fn restore_filters(&mut self, filters: &[crate::PreparedFilter]) {
        for (index, filter) in filters.iter().enumerate() {
            for chain in [&mut self.eq, &mut self.eq_b] {
                if let Some(band) = chain.band_mut(index) {
                    let enabled = band.enabled;
                    band.install(filter);
                    band.enabled = enabled;
                }
            }
        }
        for slot in &mut self.bands {
            slot.dyn_modulated_gain = f64::NAN;
        }
        self.refresh_auto_gain();
    }
}
