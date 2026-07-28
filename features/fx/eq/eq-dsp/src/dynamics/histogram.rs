//! Decaying loudness histogram for auto-threshold ("Learn" / Pro-Q "A").
//!
//! Block loudness values (dB) accumulate into decaying bins; percentiles
//! read from the histogram give a program-adaptive threshold
//! (P50) and knee (half the P10–P90 loudness spread, floored at 5 dB).

const N_BINS: usize = 120;
const DB_MIN: f64 = -100.0;
const DB_MAX: f64 = 0.0;

#[derive(Debug, Clone)]
pub struct LoudnessHistogram {
    bins: [f64; N_BINS],
    /// Per-push decay factor (0.999 ≈ slow adaptation at typical
    /// control rates; the caller chooses the push cadence).
    pub decay: f64,
    total: f64,
}

impl LoudnessHistogram {
    pub fn new() -> Self {
        Self {
            bins: [0.0; N_BINS],
            decay: 0.999,
            total: 0.0,
        }
    }

    #[inline]
    fn bin_of(db: f64) -> usize {
        let t = (db - DB_MIN) / (DB_MAX - DB_MIN);
        ((t * N_BINS as f64) as isize).clamp(0, N_BINS as i64 as isize - 1) as usize
    }

    /// Push one loudness observation (dB). Silence below the floor is
    /// ignored so pauses don't drag the threshold down.
    pub fn push(&mut self, db: f64) {
        if db <= DB_MIN {
            return;
        }
        self.total = 0.0;
        for b in &mut self.bins {
            *b *= self.decay;
            self.total += *b;
        }
        self.bins[Self::bin_of(db)] += 1.0;
        self.total += 1.0;
    }

    /// dB value at the given percentile (0..1), or None while empty.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        if self.total <= 0.0 {
            return None;
        }
        let target = self.total * p.clamp(0.0, 1.0);
        let mut acc = 0.0;
        for (i, &b) in self.bins.iter().enumerate() {
            acc += b;
            if acc >= target {
                let frac = (i as f64 + 0.5) / N_BINS as f64;
                return Some(DB_MIN + frac * (DB_MAX - DB_MIN));
            }
        }
        Some(DB_MAX)
    }

    /// Learned (threshold, knee): threshold = P50; knee = half the
    /// P10..P90 spread, floored at 5 dB.
    pub fn learned(&self) -> Option<(f64, f64)> {
        let thr = self.percentile(0.5)?;
        let lo = self.percentile(0.1)?;
        let hi = self.percentile(0.9)?;
        Some((thr, (0.5 * (hi - lo)).max(5.0)))
    }

    pub fn reset(&mut self) {
        self.bins = [0.0; N_BINS];
        self.total = 0.0;
    }
}

impl Default for LoudnessHistogram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_to_the_program_level() {
        let mut h = LoudnessHistogram::new();
        for i in 0..2000 {
            // Program alternating around −24 dB with ±6 dB swing.
            let db = -24.0 + if i % 2 == 0 { 6.0 } else { -6.0 };
            h.push(db);
        }
        let (thr, knee) = h.learned().unwrap();
        assert!(
            (-31.0..=-17.0).contains(&thr),
            "threshold should sit within the program spread: {thr}"
        );
        assert!((5.0..=10.0).contains(&knee), "knee ≈ half spread: {knee}");
    }

    #[test]
    fn silence_is_ignored() {
        let mut h = LoudnessHistogram::new();
        for _ in 0..100 {
            h.push(-20.0);
        }
        for _ in 0..1000 {
            h.push(-200.0); // below floor — ignored
        }
        let (thr, _) = h.learned().unwrap();
        assert!(thr > -25.0, "silence must not drag the threshold: {thr}");
    }
}
