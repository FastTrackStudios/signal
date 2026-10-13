//! A transient gate — two envelope followers racing each other.
//!
//! The detector for quantizing. [`crate::onsets`] uses spectral flux,
//! which is the right tool for *segmenting* a take (it gives a spectral
//! centroid for free, which is what sorts a hit into a band) and the
//! wrong one for timing: an STFT reports the frame a change happened in,
//! so its answer is quantised to a hop — 5.8 ms at the usual settings.
//! Quantizing with that replaces one jitter with another of the same
//! size.
//!
//! An envelope gate has no such limit. It runs sample by sample, so the
//! moment it fires *is* the answer.
//!
//! ## How it decides
//!
//! Two peak followers on the rectified signal, differing only in their
//! time constants:
//!
//! - **fast** — attacks in about a millisecond, releases in ten.
//! - **slow** — attacks in about seven milliseconds, releases in fifteen.
//!
//! On steady material the two sit together. On an attack the fast one
//! leaps and the slow one lags, so their *ratio* spikes — and that ratio
//! is a direct measure of how struck a sound is, available at the
//! instant it happens rather than reconstructed afterwards. A hit fires
//! when the fast envelope is both **loud enough** (absolute threshold,
//! which is what rejects bleed) and **far enough ahead of the slow one**
//! (the crest test, which is what rejects swells).
//!
//! Requiring both is the point. A threshold alone passes any loud thing,
//! including a sustained note's body; a ratio alone passes the onset of
//! every quiet noise in the room.
//!
//! ### The threshold is not optional
//!
//! The crest test only means anything *above* the threshold, and that is
//! worth stating because the failure is quiet rather than obvious.
//! Coming out of digital silence the slow envelope is zero, so it gets
//! floored at the threshold — and the ratio then measures how far above
//! the threshold the signal has got, not how sharply it arrived. A slow
//! swell fading up from nothing therefore *does* trip a high crest
//! setting, if the threshold is far enough below it.
//!
//! Set the threshold near the noise floor of the material, which is what
//! it is for, and the problem disappears: by the time a swell crosses a
//! realistic threshold the slow envelope has caught up with it and the
//! ratio is honest. Leaving it at -60 dB on a clean recording means
//! every fade-in in the take looks like a hit.
//!
//! ## How it measures
//!
//! After firing, the hit is measured over a short window — long enough
//! to contain the attack, short enough not to reach the next hit. Two
//! details in there are not obvious and both matter:
//!
//! - **The peak is taken after a holdoff.** The first instants of an
//!   attack are the pick or the stick, not the drum; taking the maximum
//!   from sample zero measures the click rather than the hit.
//! - **RMS is taken with the DC average removed**, and skips the first
//!   couple of milliseconds. A close-miked kick has real DC offset, and
//!   an RMS that includes it reports a quiet hit as a loud one.

/// One detected hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Sample the gate fired on.
    pub sample: usize,
    /// Peak amplitude over the measurement window, after the holdoff.
    pub peak: f64,
    /// RMS over the measurement window, DC removed.
    pub rms: f64,
    /// Fast-to-slow envelope ratio at the moment of firing, in dB. How
    /// struck the sound was.
    pub crest_db: f64,
}

/// How the gate is tuned.
#[derive(Clone, Copy, Debug)]
pub struct GateConfig {
    /// Absolute floor on the fast envelope, dBFS. Rejects bleed.
    pub threshold_db: f64,
    /// How far the fast envelope must lead the slow one, in dB, to
    /// count as a transient. Low values catch soft, slow attacks; high
    /// values only sharp ones.
    pub crest_db: f64,
    /// Shortest gap between two hits, in seconds.
    pub retrigger_secs: f64,
    /// Fast follower, seconds.
    pub fast_attack_secs: f64,
    pub fast_release_secs: f64,
    /// Slow follower, seconds.
    pub slow_attack_secs: f64,
    pub slow_release_secs: f64,
    /// How long a hit is measured for.
    pub measure_secs: f64,
    /// How long to wait before starting to look for the peak.
    pub peak_holdoff_secs: f64,
    /// How long to wait before starting to accumulate RMS.
    pub rms_skip_secs: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            threshold_db: -60.0,
            crest_db: 3.0,
            retrigger_secs: 0.050,
            // The fast follower has to be quick enough to catch a stick
            // and slow enough not to follow individual cycles of a bass
            // note; a millisecond is about the crossover.
            fast_attack_secs: 0.001,
            fast_release_secs: 0.010,
            slow_attack_secs: 0.007,
            slow_release_secs: 0.015,
            measure_secs: 0.025,
            peak_holdoff_secs: 0.003,
            rms_skip_secs: 0.002,
        }
    }
}

/// One-pole follower coefficient for a time constant in seconds.
fn coeff(secs: f64, sample_rate: f64) -> f64 {
    if secs <= 0.0 {
        return 0.0;
    }
    (-1.0 / (sample_rate * secs)).exp()
}

/// Find transients.
///
/// Returns hits in time order. `samples` should already be band-limited
/// if the caller wants it to be — this measures whatever it is given.
pub fn detect(samples: &[f64], sample_rate: f64, cfg: GateConfig) -> Vec<Hit> {
    if samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }

    let fast_a = coeff(cfg.fast_attack_secs, sample_rate);
    let fast_r = coeff(cfg.fast_release_secs, sample_rate);
    let slow_a = coeff(cfg.slow_attack_secs, sample_rate);
    let slow_r = coeff(cfg.slow_release_secs, sample_rate);

    let threshold = 10f64.powf(cfg.threshold_db / 20.0);
    let crest = 10f64.powf(cfg.crest_db / 20.0);

    let measure = ((cfg.measure_secs * sample_rate) as usize).max(1);
    let holdoff = (cfg.peak_holdoff_secs * sample_rate) as usize;
    let rms_skip = (cfg.rms_skip_secs * sample_rate) as usize;
    // The gap is measured from the *end* of a measurement, so a
    // retrigger shorter than the window cannot mean anything — two hits
    // closer than that were never measured separately in the first
    // place. Clamping here beats letting a user set a value that
    // silently does nothing.
    let retrigger = (((cfg.retrigger_secs * sample_rate) as usize).max(measure)).max(1);

    let mut env_fast = samples[0].abs();
    let mut env_slow = env_fast;

    let mut hits = Vec::new();
    // What the gate is currently doing: counting down a cooldown, or
    // measuring a hit that has already fired.
    let mut cooldown = 0usize;
    let mut measuring: Option<Measurement> = None;

    for (i, sample) in samples.iter().enumerate() {
        let x = sample.abs();

        // Followers. Rising uses the attack coefficient and falling the
        // release, which is what makes them peak followers rather than
        // averagers — an averager smooths the attack away, and the
        // attack is the entire signal here.
        env_fast = if env_fast < x {
            x + fast_a * (env_fast - x)
        } else {
            x + fast_r * (env_fast - x)
        };
        env_slow = if env_slow < x {
            x + slow_a * (env_slow - x)
        } else {
            x + slow_r * (env_slow - x)
        };

        match &mut measuring {
            Some(m) => {
                let age = i - m.at;
                if age >= holdoff && x > m.peak {
                    m.peak = x;
                }
                if age >= rms_skip {
                    // The *signed* sample, not the rectified one. DC
                    // removal is subtracting the mean, and the mean of a
                    // rectified signal is not the offset that was added
                    // to it — rectifying folds the negative half up, so
                    // `|s + dc|` is not `|s| + dc` and no amount of
                    // mean-subtraction afterwards recovers it.
                    m.sum += sample;
                    m.square_sum += sample * sample;
                    m.count += 1;
                }
                if age >= measure {
                    hits.push(m.finish());
                    measuring = None;
                    cooldown = retrigger;
                }
            }
            None => {
                if cooldown > 0 {
                    cooldown -= 1;
                    continue;
                }
                // Both conditions, and neither is redundant: the
                // threshold alone passes the body of any loud note, the
                // ratio alone passes the onset of every quiet noise in
                // the room.
                // The slow envelope is floored at the threshold before
                // the ratio is taken. Out of silence it is essentially
                // zero, so an unfloored ratio is enormous for *any*
                // rise — a swell fading up from nothing then reads as
                // infinitely struck and the crest test does nothing at
                // exactly the moment it is most needed.
                //
                // The threshold is the natural floor: below it there is
                // nothing to compare against, by the caller's own
                // definition of what counts as signal.
                let reference = env_slow.max(threshold);
                let ratio = env_fast / reference;
                if env_fast > threshold && ratio > crest {
                    measuring = Some(Measurement {
                        at: i,
                        peak: 0.0,
                        sum: 0.0,
                        square_sum: 0.0,
                        count: 0,
                        crest_db: 20.0 * ratio.max(1e-12).log10(),
                    });
                }
            }
        }
    }

    // A hit that fired near the end still happened. Measuring it over a
    // short window is better than dropping it, which would leave the
    // last note of a take unquantized.
    if let Some(m) = measuring {
        hits.push(m.finish());
    }
    hits
}

/// A hit being measured.
struct Measurement {
    at: usize,
    peak: f64,
    /// Running sum, for the DC average.
    sum: f64,
    square_sum: f64,
    count: usize,
    crest_db: f64,
}

impl Measurement {
    fn finish(&self) -> Hit {
        let rms = if self.count == 0 {
            0.0
        } else {
            let n = self.count as f64;
            let mean = self.sum / n;
            // Variance about the mean, which is RMS with the DC
            // component taken out — a close-miked kick has real offset
            // and counting it reports a quiet hit as a loud one.
            (self.square_sum / n - mean * mean).max(0.0).sqrt()
        };
        Hit {
            sample: self.at,
            peak: self.peak.min(1.0),
            rms: rms.min(1.0),
            crest_db: self.crest_db,
        }
    }
}
