//! What a parameter's normalized position *means*.
//!
//! A [`ParameterValue`](crate::ParameterValue) is a knob position: 0..=1, with
//! no units and no opinion. That is the right thing to automate, modulate,
//! learn a MIDI CC onto and store — every one of those wants a single
//! dimensionless number.
//!
//! It is the wrong thing to *show* a player, and the wrong thing to hand a
//! DSP. A filter does not want 0.85, it wants 5500 Hz. A [`ParameterRange`]
//! is the missing half: the mapping between the two, plus the unit to print.
//!
//! This is how real plugin parameters work, and for the same reasons. The
//! host sees one normalized value; the plugin knows what it means.
//!
//! # Taper is not decoration
//!
//! A frequency knob with a linear taper is unusable: half its travel sits
//! above 10 kHz, and the octave a guitarist actually reaches for is crammed
//! into the first few degrees. Frequency and time are heard as ratios, so
//! they want [`Taper::Logarithmic`] — where the midpoint is the *geometric*
//! mean, and equal movement is equal musical distance. Gain in dB is already
//! logarithmic in the ear, so it takes [`Taper::Linear`].

use facet::Facet;
use serde::{Deserialize, Serialize};

/// How normalized position maps onto value across the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Taper {
    /// Even across the range. Gain in dB, a mix, a pan, a ratio.
    #[default]
    Linear,
    /// Even in ratio — equal travel is equal multiplication. Frequency, time,
    /// and anything else the ear hears logarithmically.
    ///
    /// Requires a positive minimum; a range that starts at zero has no
    /// logarithm and falls back to linear rather than producing infinities on
    /// the audio thread.
    Logarithmic,
    /// Whole steps only — a mode, an algorithm, a band count.
    Stepped { steps: u32 },
}

/// What a value is measured in — for display, never for maths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum Unit {
    #[default]
    None,
    Hz,
    Decibels,
    Seconds,
    Milliseconds,
    Percent,
    Ratio,
    Semitones,
    Degrees,
    Beats,
}

impl Unit {
    /// The suffix as it is printed, including the leading space where one
    /// reads better. Percent and degrees hug their number; the rest do not.
    #[must_use]
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Hz => " Hz",
            Self::Decibels => " dB",
            Self::Seconds => " s",
            Self::Milliseconds => " ms",
            Self::Percent => "%",
            Self::Ratio => ":1",
            Self::Semitones => " st",
            Self::Degrees => "°",
            Self::Beats => " beats",
        }
    }
}

/// The span a parameter covers, and how it is traversed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Facet)]
pub struct ParameterRange {
    pub min: f32,
    pub max: f32,
    pub taper: Taper,
    pub unit: Unit,
}

impl Default for ParameterRange {
    /// The unit interval — what a parameter means when nobody has said.
    /// Normalized in equals normalized out, so an unranged parameter behaves
    /// exactly as it did before ranges existed.
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 1.0,
            taper: Taper::Linear,
            unit: Unit::None,
        }
    }
}

impl ParameterRange {
    #[must_use]
    pub const fn linear(min: f32, max: f32, unit: Unit) -> Self {
        Self {
            min,
            max,
            taper: Taper::Linear,
            unit,
        }
    }

    /// A range traversed by ratio — frequency, time.
    #[must_use]
    pub const fn logarithmic(min: f32, max: f32, unit: Unit) -> Self {
        Self {
            min,
            max,
            taper: Taper::Logarithmic,
            unit,
        }
    }

    /// `steps` discrete positions, inclusive of both ends.
    #[must_use]
    pub const fn stepped(min: f32, max: f32, steps: u32, unit: Unit) -> Self {
        Self {
            min,
            max,
            taper: Taper::Stepped { steps },
            unit,
        }
    }

    /// Whether a logarithmic taper is usable here.
    ///
    /// It needs both ends positive. A range crossing or touching zero has no
    /// logarithm, and the fallback is linear — silently, because the
    /// alternative is an infinity reaching the audio thread.
    #[must_use]
    fn log_usable(&self) -> bool {
        self.min > 0.0 && self.max > 0.0
    }

    /// Normalized position (0..=1) to the value the DSP wants.
    #[must_use]
    pub fn denormalize(&self, normalized: f32) -> f32 {
        let t = normalized.clamp(0.0, 1.0);
        match self.taper {
            Taper::Logarithmic if self.log_usable() => {
                let (lo, hi) = (self.min.ln(), self.max.ln());
                (hi - lo).mul_add(t, lo).exp()
            }
            Taper::Stepped { steps } if steps > 1 => {
                // Snap to the nearest step, ends inclusive. `steps > 1` is
                // guarded above, so the subtraction cannot wrap.
                let last = f32::from(u16::try_from(steps.saturating_sub(1)).unwrap_or(u16::MAX));
                let index = (t * last).round();
                (self.max - self.min).mul_add(index / last, self.min)
            }
            // A stepped range of 0 or 1 steps has one position: its minimum.
            Taper::Stepped { .. } => self.min,
            Taper::Linear | Taper::Logarithmic => (self.max - self.min).mul_add(t, self.min),
        }
    }

    /// The value the DSP wants back to a normalized position.
    ///
    /// The inverse of [`denormalize`](Self::denormalize) — a value outside
    /// the range clamps to an end rather than escaping 0..=1.
    #[must_use]
    pub fn normalize(&self, value: f32) -> f32 {
        let v = value.clamp(self.min.min(self.max), self.max.max(self.min));
        match self.taper {
            Taper::Logarithmic if self.log_usable() => {
                let (lo, hi) = (self.min.ln(), self.max.ln());
                if (hi - lo).abs() < f32::EPSILON {
                    0.0
                } else {
                    ((v.ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
                }
            }
            _ => {
                let span = self.max - self.min;
                if span.abs() < f32::EPSILON {
                    0.0
                } else {
                    ((v - self.min) / span).clamp(0.0, 1.0)
                }
            }
        }
    }

    /// The value at a normalized position, printed the way a player reads it.
    ///
    /// Frequencies above a kilohertz print in kHz, because "5.50 kHz" is what
    /// the number means and "5500.0 Hz" is what it is stored as.
    #[must_use]
    pub fn display(&self, normalized: f32) -> String {
        let value = self.denormalize(normalized);
        if self.unit == Unit::Hz && value >= 1000.0 {
            return format!("{:.2} kHz", value / 1000.0);
        }
        let decimals = if value.abs() >= 100.0 {
            0
        } else if value.abs() >= 10.0 {
            1
        } else {
            2
        };
        format!("{value:.decimals$}{}", self.unit.suffix())
    }
}
