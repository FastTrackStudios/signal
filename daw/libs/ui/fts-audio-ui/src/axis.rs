//! Frequency / dB axis mapping primitives.
//!
//! These are pure math helpers shared by spectrum analyzers, EQ graphs,
//! response viewers, and any audio viz that needs a log-frequency or
//! symmetric-dB coordinate space. Provider-agnostic — the consumer brings
//! its own pixel rect; this module just handles the value↔position math.

/// Logarithmic frequency axis between two endpoints.
///
/// Maps a frequency in Hz to a normalized `[0.0, 1.0]` position (and back),
/// using `log10` so a decade occupies an equal amount of screen real
/// estate. Suitable for the X axis of every typical audio frequency
/// display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreqAxis {
    pub min_hz: f64,
    pub max_hz: f64,
    log_min: f64,
    log_span: f64,
}

impl FreqAxis {
    /// New axis from `min_hz` to `max_hz`. Both must be positive and
    /// `max > min`; debug builds assert this.
    pub fn new(min_hz: f64, max_hz: f64) -> Self {
        debug_assert!(min_hz > 0.0, "FreqAxis min must be positive");
        debug_assert!(max_hz > min_hz, "FreqAxis max must be > min");
        let log_min = min_hz.log10();
        let log_max = max_hz.log10();
        Self {
            min_hz,
            max_hz,
            log_min,
            log_span: log_max - log_min,
        }
    }

    /// Standard 20 Hz – 20 kHz audible range.
    pub fn audible() -> Self {
        Self::new(20.0, 20_000.0)
    }

    /// Convert a frequency to a `[0.0, 1.0]` position. Out-of-range values
    /// map outside that interval — clamp at the call site if you need
    /// strict bounds.
    #[inline]
    pub fn freq_to_norm(&self, freq_hz: f64) -> f64 {
        (freq_hz.log10() - self.log_min) / self.log_span
    }

    /// Convert a `[0.0, 1.0]` normalized position back to a frequency in
    /// Hz. Inverse of [`Self::freq_to_norm`].
    #[inline]
    pub fn norm_to_freq(&self, norm: f64) -> f64 {
        10.0_f64.powf(self.log_min + norm * self.log_span)
    }

    /// Map a frequency to a pixel x-coordinate within `[x0, x1]`.
    #[inline]
    pub fn freq_to_x(&self, freq_hz: f64, x0: f64, x1: f64) -> f64 {
        x0 + self.freq_to_norm(freq_hz) * (x1 - x0)
    }

    /// Inverse of [`Self::freq_to_x`].
    #[inline]
    pub fn x_to_freq(&self, x: f64, x0: f64, x1: f64) -> f64 {
        let norm = (x - x0) / (x1 - x0);
        self.norm_to_freq(norm)
    }

    /// Generate `count` log-spaced sample frequencies covering the axis,
    /// inclusive at both ends. Useful for sampling a frequency response
    /// curve at evenly-spaced screen pixels.
    pub fn log_spaced(&self, count: usize) -> Vec<f64> {
        if count <= 1 {
            return vec![self.min_hz];
        }
        let denom = (count - 1) as f64;
        (0..count)
            .map(|i| self.norm_to_freq(i as f64 / denom))
            .collect()
    }
}

/// Symmetric dB axis ranged at `±range_db` from 0 dB. Linear mapping —
/// 0 dB is the centre of the screen; positive values toward the top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DbAxis {
    pub range_db: f64,
}

impl DbAxis {
    /// New axis with `±range_db` half-span. Common values: 6, 12, 18, 24, 30.
    pub fn symmetric(range_db: f64) -> Self {
        debug_assert!(range_db > 0.0, "DbAxis range must be positive");
        Self { range_db }
    }

    /// Map a dB value to a `[0.0, 1.0]` position. **Top-origin** (0 dB
    /// at norm = 0.5; positive dB → smaller norm). Matches typical
    /// graph-viewer conventions where Y increases downward.
    #[inline]
    pub fn db_to_norm(&self, db: f64) -> f64 {
        let clamped = db.clamp(-self.range_db, self.range_db);
        0.5 - clamped / (2.0 * self.range_db)
    }

    /// Inverse of [`Self::db_to_norm`].
    #[inline]
    pub fn norm_to_db(&self, norm: f64) -> f64 {
        (0.5 - norm) * 2.0 * self.range_db
    }

    /// Map a dB value to a pixel y-coordinate within `[y0, y1]`.
    #[inline]
    pub fn db_to_y(&self, db: f64, y0: f64, y1: f64) -> f64 {
        y0 + self.db_to_norm(db) * (y1 - y0)
    }

    /// Inverse of [`Self::db_to_y`].
    #[inline]
    pub fn y_to_db(&self, y: f64, y0: f64, y1: f64) -> f64 {
        let norm = (y - y0) / (y1 - y0);
        self.norm_to_db(norm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_axis_endpoints_map_to_zero_and_one() {
        let ax = FreqAxis::audible();
        assert!((ax.freq_to_norm(20.0)).abs() < 1e-9);
        assert!((ax.freq_to_norm(20_000.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn freq_axis_round_trip() {
        let ax = FreqAxis::audible();
        for hz in [25.0, 100.0, 440.0, 1000.0, 5000.0, 15_000.0] {
            let n = ax.freq_to_norm(hz);
            let back = ax.norm_to_freq(n);
            assert!((back - hz).abs() < 1e-6, "{hz} round-trip mismatch: {back}");
        }
    }

    #[test]
    fn freq_axis_decade_evenly_spaced() {
        // 100 → 1000 should be ⅓ of the audible-range span (3 decades total).
        let ax = FreqAxis::audible();
        let span = ax.freq_to_norm(1000.0) - ax.freq_to_norm(100.0);
        assert!((span - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn freq_axis_pixel_mapping() {
        // Use 100 Hz – 10 kHz so 1 kHz lands at exactly the midpoint.
        let ax = FreqAxis::new(100.0, 10_000.0);
        let x = ax.freq_to_x(1000.0, 0.0, 300.0);
        assert!(
            (x - 150.0).abs() < 1e-6,
            "1 kHz mid-axis should map to mid-pixel, got {x}"
        );
        // Inverse round-trip.
        let f = ax.x_to_freq(150.0, 0.0, 300.0);
        assert!((f - 1000.0).abs() < 1e-6, "x_to_freq round-trip: {f}");
    }

    #[test]
    fn freq_axis_log_spaced_endpoints() {
        let ax = FreqAxis::audible();
        let pts = ax.log_spaced(5);
        assert_eq!(pts.len(), 5);
        assert!((pts[0] - 20.0).abs() < 1e-6);
        assert!((pts[4] - 20_000.0).abs() < 1e-3);
    }

    #[test]
    fn db_axis_zero_centred() {
        let ax = DbAxis::symmetric(24.0);
        assert!((ax.db_to_norm(0.0) - 0.5).abs() < 1e-12);
        assert!((ax.db_to_norm(24.0) - 0.0).abs() < 1e-12);
        assert!((ax.db_to_norm(-24.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn db_axis_clamps() {
        let ax = DbAxis::symmetric(12.0);
        assert!((ax.db_to_norm(50.0) - 0.0).abs() < 1e-12);
        assert!((ax.db_to_norm(-50.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn db_axis_round_trip() {
        let ax = DbAxis::symmetric(18.0);
        for db in [-12.0, -3.0, 0.0, 6.0, 17.0] {
            let n = ax.db_to_norm(db);
            let back = ax.norm_to_db(n);
            assert!((back - db).abs() < 1e-9, "{db} round-trip: {back}");
        }
    }
}
