//! Gain curve — time-indexed gain envelope for offline analysis.
//!
//! All offline analyzers (gate, compressor, limiter, rider) produce a
//! [`GainCurve`] as their output. The curve stores (time, gain_db) points
//! that can be:
//! - Applied directly to audio via [`GainCurve::apply`]
//! - Thinned to reduce point count via [`GainCurve::thin`]
//! - Shifted in time for lookahead via [`GainCurve::shift`]
//! - Written to DAW automation envelopes
//! - Exported as CSV/JSON for external use
//!
//! This design mirrors EUGEN27771's envelope-based compressor concept:
//! the DSP runs offline with full lookahead, producing an envelope that
//! the DAW plays back as volume automation.

/// A single point in the gain curve.
#[derive(Debug, Clone, Copy)]
pub struct GainPoint {
    /// Time in seconds from the start of the audio.
    pub time: f64,
    /// Gain in dB (0.0 = unity, positive = boost, negative = cut).
    pub gain_db: f64,
}

// r[impl offline.automation.envelope-write]
// r[impl offline.analysis.preview]
/// Time-indexed gain envelope produced by offline analysis.
///
/// The curve is a series of [`GainPoint`]s sorted by time. It can represent
/// the output of any dynamics processor: compressor gain reduction, gate
/// open/close, limiter attenuation, or rider level adjustment.
#[derive(Debug, Clone)]
pub struct GainCurve {
    /// Sorted gain points.
    pub points: Vec<GainPoint>,
    /// Sample rate the curve was computed at.
    pub sample_rate: f64,
}

impl GainCurve {
    /// Create an empty gain curve.
    pub fn new(sample_rate: f64) -> Self {
        Self {
            points: Vec::new(),
            sample_rate,
        }
    }

    /// Create from per-sample gain values (one gain per sample).
    ///
    /// Automatically thins to the specified interval to avoid excessive
    /// point counts. An interval of 0.0 keeps all samples.
    pub fn from_samples(gains_db: &[f64], sample_rate: f64, interval_ms: f64) -> Self {
        let interval_samples = if interval_ms > 0.0 {
            (interval_ms * 0.001 * sample_rate).max(1.0) as usize
        } else {
            1
        };

        let mut points = Vec::with_capacity(gains_db.len() / interval_samples + 2);

        // Always include first point
        if !gains_db.is_empty() {
            points.push(GainPoint {
                time: 0.0,
                gain_db: gains_db[0],
            });
        }

        let mut last_added = 0;
        for i in (interval_samples..gains_db.len()).step_by(interval_samples) {
            points.push(GainPoint {
                time: i as f64 / sample_rate,
                gain_db: gains_db[i],
            });
            last_added = i;
        }

        // Always include last point
        let last = gains_db.len().saturating_sub(1);
        if last > last_added && !gains_db.is_empty() {
            points.push(GainPoint {
                time: last as f64 / sample_rate,
                gain_db: gains_db[last],
            });
        }

        Self {
            points,
            sample_rate,
        }
    }

    /// Number of points in the curve.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether the curve is empty.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Duration in seconds.
    pub fn duration(&self) -> f64 {
        self.points.last().map_or(0.0, |p| p.time)
    }

    // ── Transformations ─────────────────────────────────────────────

    /// Shift all points by a time offset (negative = lookahead / pre-comp).
    ///
    /// Points that would end up before time 0 are clamped to 0.
    pub fn shift(&mut self, offset_seconds: f64) {
        for p in &mut self.points {
            p.time = (p.time + offset_seconds).max(0.0);
        }
    }

    /// Apply an output gain offset in dB to all points.
    pub fn apply_gain_offset(&mut self, offset_db: f64) {
        for p in &mut self.points {
            p.gain_db += offset_db;
        }
    }

    /// Thin the curve by removing points that change less than `tolerance_db`
    /// from their neighbors. Uses a simple Ramer-Douglas-Peucker-like approach.
    ///
    /// This is equivalent to the EEL script's "Interval" parameter — it
    /// controls the density of automation points written to the DAW.
    pub fn thin(&mut self, tolerance_db: f64) {
        if self.points.len() <= 2 {
            return;
        }

        let mut keep = vec![false; self.points.len()];
        keep[0] = true;
        keep[self.points.len() - 1] = true;

        Self::rdp_thin(
            &self.points,
            0,
            self.points.len() - 1,
            tolerance_db,
            &mut keep,
        );

        let mut i = 0;
        self.points.retain(|_| {
            let k = keep[i];
            i += 1;
            k
        });
    }

    fn rdp_thin(points: &[GainPoint], start: usize, end: usize, tol: f64, keep: &mut [bool]) {
        if end <= start + 1 {
            return;
        }

        let t0 = points[start].time;
        let t1 = points[end].time;
        let g0 = points[start].gain_db;
        let g1 = points[end].gain_db;
        let dt = t1 - t0;

        let mut max_dist = 0.0;
        let mut max_idx = start + 1;

        #[allow(clippy::needless_range_loop)] // parallel index math over points
        for i in (start + 1)..end {
            let t = if dt > 0.0 {
                (points[i].time - t0) / dt
            } else {
                0.0
            };
            let interp = g0 + t * (g1 - g0);
            let dist = (points[i].gain_db - interp).abs();
            if dist > max_dist {
                max_dist = dist;
                max_idx = i;
            }
        }

        if max_dist > tol {
            keep[max_idx] = true;
            Self::rdp_thin(points, start, max_idx, tol, keep);
            Self::rdp_thin(points, max_idx, end, tol, keep);
        }
    }

    // ── Application ─────────────────────────────────────────────────

    /// Get the interpolated gain in dB at a given time.
    pub fn gain_at(&self, time: f64) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        if time <= self.points[0].time {
            return self.points[0].gain_db;
        }
        if time >= self.points.last().unwrap().time {
            return self.points.last().unwrap().gain_db;
        }

        // Binary search for the surrounding points
        let idx = self
            .points
            .partition_point(|p| p.time <= time)
            .saturating_sub(1);
        let p0 = &self.points[idx];
        let p1 = &self.points[(idx + 1).min(self.points.len() - 1)];

        let dt = p1.time - p0.time;
        if dt <= 0.0 {
            return p0.gain_db;
        }

        let t = (time - p0.time) / dt;
        p0.gain_db + t * (p1.gain_db - p0.gain_db)
    }

    /// Apply the gain curve to stereo audio buffers in-place.
    ///
    /// Interpolates between points for sample-accurate application.
    pub fn apply(&self, left: &mut [f64], right: &mut [f64], start_time: f64) {
        let n = left.len().min(right.len());
        for i in 0..n {
            let time = start_time + i as f64 / self.sample_rate;
            let gain_db = self.gain_at(time);
            let gain_lin = 10.0_f64.powf(gain_db / 20.0);
            left[i] *= gain_lin;
            right[i] *= gain_lin;
        }
    }

    // ── Export ───────────────────────────────────────────────────────

    /// Export as CSV string (time_seconds, gain_db).
    pub fn to_csv(&self) -> String {
        let mut out = String::from("time_s,gain_db\n");
        for p in &self.points {
            out.push_str(&format!("{:.6},{:.4}\n", p.time, p.gain_db));
        }
        out
    }

    /// Export as JSON array of [time, gain_db] pairs.
    pub fn to_json(&self) -> String {
        let mut out = String::from("[\n");
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                out.push_str(",\n");
            }
            out.push_str(&format!("  [{:.6}, {:.4}]", p.time, p.gain_db));
        }
        out.push_str("\n]\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_samples_basic() {
        let gains = vec![0.0, -1.0, -2.0, -3.0, -4.0];
        let curve = GainCurve::from_samples(&gains, 48000.0, 0.0);
        assert_eq!(curve.len(), 5);
        assert!((curve.points[0].gain_db - 0.0).abs() < 1e-10);
        assert!((curve.points[4].gain_db - (-4.0)).abs() < 1e-10);
    }

    #[test]
    fn from_samples_with_interval() {
        let gains: Vec<f64> = (0..48000).map(|i| -(i as f64) * 0.001).collect();
        let curve = GainCurve::from_samples(&gains, 48000.0, 5.0); // 5ms interval
        // 48000 samples / (5ms * 48) = ~200 points, plus first/last
        assert!(curve.len() < 300);
        assert!(curve.len() > 100);
    }

    #[test]
    fn gain_at_interpolates() {
        let curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.0,
                    gain_db: 0.0,
                },
                GainPoint {
                    time: 1.0,
                    gain_db: -10.0,
                },
            ],
            sample_rate: 48000.0,
        };
        assert!((curve.gain_at(0.5) - (-5.0)).abs() < 0.01);
        assert!((curve.gain_at(0.0) - 0.0).abs() < 0.01);
        assert!((curve.gain_at(1.0) - (-10.0)).abs() < 0.01);
    }

    #[test]
    fn shift_moves_points() {
        let mut curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.5,
                    gain_db: -3.0,
                },
                GainPoint {
                    time: 1.0,
                    gain_db: -6.0,
                },
            ],
            sample_rate: 48000.0,
        };
        curve.shift(-0.3); // 300ms lookahead
        assert!((curve.points[0].time - 0.2).abs() < 1e-10);
        assert!((curve.points[1].time - 0.7).abs() < 1e-10);
    }

    #[test]
    fn shift_clamps_to_zero() {
        let mut curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.1,
                    gain_db: -3.0,
                },
                GainPoint {
                    time: 0.5,
                    gain_db: -6.0,
                },
            ],
            sample_rate: 48000.0,
        };
        curve.shift(-0.3);
        assert!((curve.points[0].time - 0.0).abs() < 1e-10); // clamped
        assert!((curve.points[1].time - 0.2).abs() < 1e-10);
    }

    #[test]
    fn thin_removes_colinear() {
        let mut curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.0,
                    gain_db: 0.0,
                },
                GainPoint {
                    time: 0.5,
                    gain_db: -5.0,
                }, // on the line
                GainPoint {
                    time: 1.0,
                    gain_db: -10.0,
                },
            ],
            sample_rate: 48000.0,
        };
        curve.thin(0.1);
        // Middle point is on the line from first to last → should be removed
        assert_eq!(curve.len(), 2);
    }

    #[test]
    fn thin_keeps_significant_points() {
        let mut curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.0,
                    gain_db: 0.0,
                },
                GainPoint {
                    time: 0.5,
                    gain_db: -10.0,
                }, // big deviation
                GainPoint {
                    time: 1.0,
                    gain_db: 0.0,
                },
            ],
            sample_rate: 48000.0,
        };
        curve.thin(1.0);
        // Middle point deviates 10dB from the line (0→0), keep it
        assert_eq!(curve.len(), 3);
    }

    #[test]
    fn apply_modifies_audio() {
        let curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.0,
                    gain_db: -6.0,
                },
                GainPoint {
                    time: 1.0,
                    gain_db: -6.0,
                },
            ],
            sample_rate: 48000.0,
        };
        let mut left = vec![1.0; 100];
        let mut right = vec![1.0; 100];
        curve.apply(&mut left, &mut right, 0.0);

        // -6dB ≈ 0.501
        assert!((left[0] - 0.501).abs() < 0.01);
    }

    #[test]
    fn empty_curve_is_unity() {
        let curve = GainCurve::new(48000.0);
        assert!((curve.gain_at(0.5) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn csv_export() {
        let curve = GainCurve {
            points: vec![
                GainPoint {
                    time: 0.0,
                    gain_db: 0.0,
                },
                GainPoint {
                    time: 1.0,
                    gain_db: -6.0,
                },
            ],
            sample_rate: 48000.0,
        };
        let csv = curve.to_csv();
        assert!(csv.contains("time_s,gain_db"));
        assert!(csv.contains("0.000000,0.0000"));
        assert!(csv.contains("1.000000,-6.0000"));
    }
}
