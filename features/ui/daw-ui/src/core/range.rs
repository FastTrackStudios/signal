//! Extended range types for audio parameters.
//!
//! These types complement nih-plug's `FloatRange` and `IntRange` with
//! audio-specific ranges like logarithmic dB and frequency.

use super::normal::Normal;

/// Logarithmic dB range with configurable -infinity handling.
///
/// This range maps normalized values to decibels with proper handling
/// of the -infinity point (silence). The zero position determines where
/// -infinity dB is located in the normalized range.
///
/// # Example
///
/// ```
/// use audio_controls::core::range::LogDBRange;
///
/// // Standard gain range: -60dB to +6dB
/// let range = LogDBRange::gain();
/// assert!((range.unnormalize(0.0) - (-60.0)).abs() < 0.1);
/// assert!((range.unnormalize(1.0) - 6.0).abs() < 0.1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogDBRange {
    /// Minimum dB value (can be `f32::NEG_INFINITY` for true silence).
    pub min_db: f32,
    /// Maximum dB value.
    pub max_db: f32,
    /// Position (0.0-1.0) where -infinity dB is located.
    /// Set to 0.0 for standard ranges where minimum is at the bottom.
    pub zero_position: f32,
}

impl LogDBRange {
    /// Standard gain range: -60dB to +6dB.
    #[must_use]
    pub const fn gain() -> Self {
        Self {
            min_db: -60.0,
            max_db: 6.0,
            zero_position: 0.0,
        }
    }

    /// Wide gain range: -80dB to +12dB.
    #[must_use]
    pub const fn wide() -> Self {
        Self {
            min_db: -80.0,
            max_db: 12.0,
            zero_position: 0.0,
        }
    }

    /// Unity-centered range: -24dB to +24dB.
    #[must_use]
    pub const fn unity_centered() -> Self {
        Self {
            min_db: -24.0,
            max_db: 24.0,
            zero_position: 0.0,
        }
    }

    /// Normalize a dB value to [0.0, 1.0].
    #[must_use]
    pub fn normalize(&self, db: f32) -> Normal {
        if db <= self.min_db {
            return Normal::new(self.zero_position);
        }

        let range = self.max_db - self.min_db;
        let usable_range = 1.0 - self.zero_position;

        // Linear mapping within the dB range
        let normalized = ((db - self.min_db) / range) * usable_range + self.zero_position;
        Normal::new(normalized)
    }

    /// Unnormalize a value from [0.0, 1.0] to dB.
    #[must_use]
    pub fn unnormalize(&self, normal: f32) -> f32 {
        let normal = normal.clamp(0.0, 1.0);

        if normal <= self.zero_position {
            return self.min_db;
        }

        let range = self.max_db - self.min_db;
        let usable_range = 1.0 - self.zero_position;

        // Linear mapping back to dB
        let proportion = (normal - self.zero_position) / usable_range;
        self.min_db + (proportion * range)
    }

    /// Format a dB value as a string.
    #[must_use]
    pub fn format(&self, db: f32) -> String {
        if db <= self.min_db + 0.1 {
            "-inf dB".to_string()
        } else {
            format!("{db:+.1} dB")
        }
    }
}

impl Default for LogDBRange {
    fn default() -> Self {
        Self::gain()
    }
}

/// Logarithmic frequency range (octave-based).
///
/// This range maps normalized values to frequencies using a logarithmic
/// (octave-based) scale, which is how humans perceive pitch.
///
/// # Example
///
/// ```
/// use audio_controls::core::range::FreqRange;
///
/// // Standard audio spectrum
/// let range = FreqRange::audio_spectrum();
/// assert!((range.unnormalize(0.0) - 20.0).abs() < 1.0);
/// assert!((range.unnormalize(1.0) - 20000.0).abs() < 100.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreqRange {
    /// Minimum frequency in Hz.
    pub min_hz: f32,
    /// Maximum frequency in Hz.
    pub max_hz: f32,
}

impl FreqRange {
    /// Full audio spectrum: 20Hz - 20kHz.
    #[must_use]
    pub const fn audio_spectrum() -> Self {
        Self {
            min_hz: 20.0,
            max_hz: 20000.0,
        }
    }

    /// Sub-bass to bass: 20Hz - 200Hz.
    #[must_use]
    pub const fn bass() -> Self {
        Self {
            min_hz: 20.0,
            max_hz: 200.0,
        }
    }

    /// Mid frequencies: 200Hz - 5kHz.
    #[must_use]
    pub const fn mids() -> Self {
        Self {
            min_hz: 200.0,
            max_hz: 5000.0,
        }
    }

    /// High frequencies: 5kHz - 20kHz.
    #[must_use]
    pub const fn highs() -> Self {
        Self {
            min_hz: 5000.0,
            max_hz: 20000.0,
        }
    }

    /// Filter cutoff range: 20Hz - 20kHz (same as audio spectrum).
    #[must_use]
    pub const fn filter() -> Self {
        Self::audio_spectrum()
    }

    /// Normalize a frequency value to [0.0, 1.0] using logarithmic scaling.
    #[must_use]
    pub fn normalize(&self, hz: f32) -> Normal {
        let hz = hz.clamp(self.min_hz, self.max_hz);
        let log_min = self.min_hz.ln();
        let log_max = self.max_hz.ln();
        let log_hz = hz.ln();

        Normal::new((log_hz - log_min) / (log_max - log_min))
    }

    /// Unnormalize a value from [0.0, 1.0] to frequency using logarithmic scaling.
    #[must_use]
    pub fn unnormalize(&self, normal: f32) -> f32 {
        let normal = normal.clamp(0.0, 1.0);
        let log_min = self.min_hz.ln();
        let log_max = self.max_hz.ln();

        (log_min + normal * (log_max - log_min)).exp()
    }

    /// Format a frequency value as a string.
    #[must_use]
    pub fn format(&self, hz: f32) -> String {
        if hz >= 1000.0 {
            format!("{:.2} kHz", hz / 1000.0)
        } else {
            format!("{hz:.1} Hz")
        }
    }

    /// Calculate the number of octaves in this range.
    #[must_use]
    pub fn octaves(&self) -> f32 {
        (self.max_hz / self.min_hz).log2()
    }
}

impl Default for FreqRange {
    fn default() -> Self {
        Self::audio_spectrum()
    }
}

/// Trait for range types that can normalize and unnormalize values.
pub trait ParameterRange {
    /// The plain value type for this range.
    type Plain;

    /// Normalize a plain value to [0.0, 1.0].
    fn normalize(&self, plain: Self::Plain) -> Normal;

    /// Unnormalize a normalized value back to a plain value.
    fn unnormalize(&self, normal: f32) -> Self::Plain;

    /// Format a plain value as a display string.
    fn format(&self, plain: Self::Plain) -> String;
}

impl ParameterRange for LogDBRange {
    type Plain = f32;

    fn normalize(&self, plain: f32) -> Normal {
        self.normalize(plain)
    }

    fn unnormalize(&self, normal: f32) -> f32 {
        self.unnormalize(normal)
    }

    fn format(&self, plain: f32) -> String {
        self.format(plain)
    }
}

impl ParameterRange for FreqRange {
    type Plain = f32;

    fn normalize(&self, plain: f32) -> Normal {
        self.normalize(plain)
    }

    fn unnormalize(&self, normal: f32) -> f32 {
        self.unnormalize(normal)
    }

    fn format(&self, plain: f32) -> String {
        self.format(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_db_range_normalization() {
        let range = LogDBRange::gain();

        // Min should be at 0
        assert!((range.normalize(-60.0).value() - 0.0).abs() < 0.01);

        // Max should be at 1
        assert!((range.normalize(6.0).value() - 1.0).abs() < 0.01);

        // Roundtrip
        let db = -12.0;
        let normalized = range.normalize(db);
        let unnormalized = range.unnormalize(normalized.value());
        assert!((unnormalized - db).abs() < 0.1);
    }

    #[test]
    fn freq_range_normalization() {
        let range = FreqRange::audio_spectrum();

        // Min should be at 0
        assert!((range.normalize(20.0).value() - 0.0).abs() < 0.01);

        // Max should be at 1
        assert!((range.normalize(20000.0).value() - 1.0).abs() < 0.01);

        // 1kHz should be roughly in the middle (10 octaves, 1kHz is ~5.6 octaves up)
        let normalized_1k = range.normalize(1000.0).value();
        assert!(normalized_1k > 0.4 && normalized_1k < 0.6);
    }

    #[test]
    fn freq_range_octaves() {
        let range = FreqRange::audio_spectrum();
        let octaves = range.octaves();
        // 20Hz to 20kHz is about 10 octaves
        assert!((octaves - 10.0).abs() < 0.1);
    }
}
