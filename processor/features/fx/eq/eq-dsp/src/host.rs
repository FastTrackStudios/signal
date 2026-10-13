//! Canonical preset/host encoding adapters. New applications use typed configuration.

use crate::Placement;
/// One band's static configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanonicalBandConfig {
    /// The band exists (Pro-Q's "Used"). A band that is not used renders
    /// nothing regardless of `enabled`.
    pub used: bool,
    /// The band is switched on rather than bypassed.
    pub enabled: bool,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    /// Canonical shape index (see [`crate::design::slope::FilterShape`]).
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

impl Default for CanonicalBandConfig {
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent switch on the plugin panel — used, enabled, auto, relative, spectral, tilt, side-filtered and so on. Grouping them into a config struct would break every call site and tell a reader nothing the field names do not already say"
)]
pub struct CanonicalBandDynamics {
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

impl Default for CanonicalBandDynamics {
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

/// Convert a typed configuration into the established host encoding.
/// This is the only adapter that needs the canonical shape and slope tables.
///
/// # Errors
/// Returns `InvalidSlope` if the filter cannot be represented by canonical slope settings.
pub fn encode_band(
    band: crate::BandConfig,
) -> Result<(CanonicalBandConfig, CanonicalBandDynamics), crate::Error> {
    use crate::{Ballistics, DetectorSource, DynamicsMode, FilterType, Threshold};
    let (shape, freq_hz, gain_db, q, order, fraction) = band.filter.parts()?;
    let shape = match shape {
        FilterType::Peak => 0,
        FilterType::LowShelf => 1,
        FilterType::HighShelf => 2,
        FilterType::Highpass => 3,
        FilterType::Lowpass => 4,
        FilterType::Notch => 5,
        FilterType::Bandpass => 6,
        FilterType::TiltShelf => 7,
        FilterType::FlatTilt => 8,
        FilterType::Allpass => 9,
        FilterType::BandShelf => 10,
        FilterType::ShelfAlt => 11,
        FilterType::BandPassVariant => 12,
    };
    let slope = match order {
        0..=6 => dsp_core::num::count_to_f64(order) + fraction,
        8 => 7.0,
        12 => 8.0,
        16 => 9.0,
        crate::design::slope::BRICKWALL_ORDER => 10.0,
        _ => return Err(crate::Error::InvalidSlope),
    };
    let (attack_pct, release_pct) = match band.dynamics.ballistics {
        Ballistics::ProQPercent { attack, release } => (attack, release),
        Ballistics::Milliseconds { .. } => (50.0, 50.0),
    };
    let (side_filtered, side_lo_hz, side_hi_hz) = match band.dynamics.detector {
        DetectorSource::Band => (false, 20.0, 20_000.0),
        DetectorSource::FrequencyRange { low_hz, high_hz } => (true, low_hz, high_hz),
    };
    Ok((
        CanonicalBandConfig {
            used: true,
            enabled: band.enabled,
            freq_hz,
            gain_db,
            q,
            shape,
            slope,
            placement: band.placement,
            stream: crate::prepared::stream_index(band.stream),
        },
        CanonicalBandDynamics {
            range_db: if band.dynamics.mode == DynamicsMode::Static {
                0.0
            } else {
                band.dynamics.range_db
            },
            threshold_db: match band.dynamics.threshold {
                Threshold::FixedDb(db) => db,
                Threshold::Auto => -18.0,
            },
            auto: band.dynamics.threshold == Threshold::Auto,
            attack_pct,
            release_pct,
            relative: band.dynamics.relative,
            spectral: band.dynamics.mode == DynamicsMode::Spectral,
            spectral_density: band.dynamics.density * 100.0,
            spectral_tilt: band.dynamics.spectral_tilt,
            side_filtered,
            side_lo_hz,
            side_hi_hz,
        },
    ))
}

/// Engine adapter for persisted canonical host parameters.
pub use crate::engine::FtsEq as CanonicalEq;

/// Resolve continuous canonical slope control at the host boundary.
pub(crate) fn resolve_slope(
    shape: crate::design::slope::FilterShape,
    raw: f64,
) -> crate::engine::ResolvedSlope {
    use crate::design::slope::FilterShape;
    let raw = raw.max(0.0);
    let laddered = matches!(shape, FilterShape::LowCut | FilterShape::HighCut) && raw < 6.0;
    let (index, fraction) = if laddered {
        (dsp_core::num::f64_to_index(raw.floor()), raw.fract())
    } else {
        (dsp_core::num::f64_to_index(raw.round()), 0.0)
    };
    crate::engine::ResolvedSlope {
        order: shape.effective_order(index),
        fraction,
    }
}

/// Persisted shape and slope mapping shared by importers and host adapters.
pub use crate::design::slope::{FilterShape, Slope};
