//! Finding hits with spectral flux and placing them with the envelope.
//!
//! The two detectors in this crate each fail at the other's job.
//!
//! [`crate::gate`] — two envelope followers racing — is sample-accurate,
//! which is what quantizing needs, but its thresholds are absolute. Set
//! them for a verse and the choruses smear; set them for a chorus and
//! the verse goes missing. Calibrating it against the album's drum MIDI
//! showed the cost: at the shipped settings it found about a fifth of
//! what the drummer played, and no single sensitivity worked across the
//! six projects.
//!
//! [`crate::onsets`] — spectral flux against a moving median — has no
//! such dial. It asks whether *this* frame changed more than its
//! neighbourhood usually does, so it follows the material instead of
//! being told about it. But an STFT answers to the nearest hop: 5.3 ms
//! at 48 kHz with the default 256-sample hop. Quantizing to that
//! replaces one timing error with another the same size, which is why
//! flux was kept out of the quantize path.
//!
//! Neither objection applies to the two together. Flux decides **that**
//! a hit happened, at whatever resolution it likes; the envelope then
//! decides **when**, by hunting the steepest rise in a window around it.
//! The window is deliberately smaller than the gap between two hits, so
//! refinement can only sharpen a hit's position, never move it onto a
//! neighbour.
//!
//! This is the standard shape in the onset-detection literature: a
//! detection function that is robust but coarse, followed by a
//! localisation step. See Böck & Widmer, *Maximum Filter Vibrato
//! Suppression for Onset Detection* (DAFx-13) for the peak-picking side
//! of it — the adaptive-median formulation `onsets` already uses.

use crate::detect::Transient;
use crate::gate::Hit;
use crate::group_detect::refine_onset;
use crate::onsets::{self, OnsetConfig};

/// How the two stages are put together.
#[derive(Clone, Copy, Debug)]
pub struct HybridConfig {
    /// The flux stage — what decides a hit happened.
    pub onsets: OnsetConfig,
    /// How far either side of the flux frame to hunt for the attack.
    ///
    /// About twice the hop, so the true onset is inside it, and far
    /// less than the gap between two hits, so it cannot reach the
    /// neighbour. Widening this does not find more hits — flux has
    /// already decided how many there are — it only risks moving one
    /// onto the wrong attack.
    pub refine_window_secs: f64,
    /// Shortest gap between two hits after refinement.
    ///
    /// Applied again here because refinement moves hits, and two that
    /// flux placed a hop apart can land on the same attack.
    pub min_spacing_secs: f64,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            onsets: OnsetConfig {
                // Half the module's own default hop. The refinement
                // stage decides a hit's position either way, but a
                // finer hop separates hits that fall in one frame, and
                // the sweep prefers it at every threshold tried.
                hop: 128,
                ..OnsetConfig::default()
            },
            refine_window_secs: 0.012,
            min_spacing_secs: 0.030,
        }
    }
}

/// Peak and RMS over a short window from `start`, for ranking a hit.
fn measure(samples: &[f64], start: usize, len: usize) -> (f64, f64) {
    let end = (start + len).min(samples.len());
    if end <= start {
        return (0.0, 0.0);
    }
    let slice = &samples[start..end];
    let peak = slice.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let rms = (slice.iter().map(|v| v * v).sum::<f64>() / slice.len() as f64).sqrt();
    (peak, rms)
}

/// Detect hits: spectral flux to find them, the envelope to place them.
// r[impl drums.detect.hybrid]
pub fn detect(samples: &[f64], sample_rate: f64, cfg: &HybridConfig) -> Vec<Transient> {
    if samples.is_empty() || sample_rate <= 0.0 {
        return Vec::new();
    }
    // A 10 ms window is past the attack of any struck sound and short
    // enough that a fast pattern never measures its neighbour.
    let measure_len = (sample_rate * 0.010).max(1.0) as usize;

    let mut out: Vec<Transient> = Vec::new();
    for onset in onsets::detect(samples, sample_rate, cfg.onsets) {
        let coarse = onsets::onset_seconds(&onset, sample_rate, &cfg.onsets);
        let at = refine_onset(samples, sample_rate, coarse, cfg.refine_window_secs);
        let sample = (at * sample_rate).max(0.0) as usize;
        let (peak, rms) = measure(samples, sample, measure_len);
        // Crest is how struck the sound is: a transient has a peak well
        // above its own energy, a swell does not.
        let crest_db = if rms > 1e-12 {
            20.0 * (peak / rms).log10()
        } else {
            0.0
        };
        out.push(Transient {
            at,
            loudness: peak,
            crest_db,
            hit: Hit {
                sample,
                peak,
                rms,
                crest_db,
            },
        });
    }

    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    // Refinement can pull two flux frames onto one attack; keep the
    // louder, which is the one that actually carries the hit.
    let mut deduped: Vec<Transient> = Vec::with_capacity(out.len());
    for t in out {
        match deduped.last_mut() {
            Some(last) if t.at - last.at < cfg.min_spacing_secs => {
                if t.loudness > last.loudness {
                    *last = t;
                }
            }
            _ => deduped.push(t),
        }
    }

    // Loudness is a rank, not a level: normalised to the take so the
    // panel's sensitivity means the same thing on a quiet song.
    let loudest = deduped.iter().fold(0.0f64, |m, t| m.max(t.loudness));
    if loudest > 0.0 {
        for t in &mut deduped {
            t.loudness /= loudest;
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// A strike at `at` seconds: broadband noise under an exponential
    /// decay, which is what a drum is to within what a detector cares
    /// about.
    ///
    /// Noise rather than summed sinusoids, and a decay long enough to
    /// fade rather than stop. A first attempt used `sin(i * 0.7)`, whose
    /// frequency is far above Nyquist and so aliases into something with
    /// no sensible spectrum, and cut the tail off abruptly at 80 ms. Both
    /// gave every strike a second spectral event and the detector duly
    /// reported twice as many hits as there were — a fault in the
    /// fixture that reads exactly like a fault in the detector.
    fn strike(buf: &mut [f64], at: f64, amp: f64) {
        let start = (at * SR) as usize;
        let len = (SR * 0.25) as usize;
        // A deterministic noise source: the test must not depend on
        // which numbers a generator happened to produce.
        let mut seed = 0x2545_F491_4F6C_DD1Du64 ^ start as u64;
        for i in 0..len {
            if start + i >= buf.len() {
                break;
            }
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            let env = (-(i as f64 / SR) * 25.0).exp();
            buf[start + i] += amp * env * noise;
        }
    }

    fn take(hits: &[(f64, f64)], secs: f64) -> Vec<f64> {
        let mut buf = vec![0.0; (SR * secs) as usize];
        for &(at, amp) in hits {
            strike(&mut buf, at, amp);
        }
        buf
    }

    // r[verify drums.detect.hybrid]
    #[test]
    fn every_strike_is_found() {
        let times: Vec<f64> = (0..16).map(|i| 0.2 + i as f64 * 0.25).collect();
        let buf = take(
            &times.iter().map(|&t| (t, 0.8)).collect::<Vec<_>>(),
            times.last().unwrap() + 0.5,
        );
        let hits = detect(&buf, SR, &HybridConfig::default());
        assert_eq!(hits.len(), times.len(), "got {:?}", hits.len());
    }

    // r[verify drums.detect.hybrid]
    #[test]
    fn hits_land_within_a_couple_of_milliseconds() {
        // The whole reason for the refinement stage. Flux alone answers
        // to its hop — 5.3 ms at these settings — which is the same size
        // as the timing error quantizing is meant to remove.
        let times: Vec<f64> = (0..8).map(|i| 0.3 + i as f64 * 0.4).collect();
        let buf = take(
            &times.iter().map(|&t| (t, 0.8)).collect::<Vec<_>>(),
            times.last().unwrap() + 0.5,
        );
        let hits = detect(&buf, SR, &HybridConfig::default());
        assert_eq!(hits.len(), times.len());
        for (h, want) in hits.iter().zip(&times) {
            let err = (h.at - want).abs();
            assert!(
                err < 0.003,
                "hit at {:.4}s wanted {want:.4}s — {:.1} ms out",
                h.at,
                err * 1000.0
            );
        }
    }

    // r[verify drums.detect.hybrid]
    #[test]
    fn a_quiet_passage_is_not_missed_because_a_loud_one_exists() {
        // The failure the moving median exists to prevent, and the one
        // an absolute threshold cannot avoid: the same part played
        // softly must still be found.
        let mut hits: Vec<(f64, f64)> = (0..8).map(|i| (0.3 + i as f64 * 0.3, 0.9)).collect();
        hits.extend((0..8).map(|i| (3.0 + i as f64 * 0.3, 0.05)));
        let buf = take(&hits, 6.0);
        let found = detect(&buf, SR, &HybridConfig::default());
        let quiet = found.iter().filter(|t| t.at > 2.9).count();
        assert!(
            quiet >= 7,
            "found only {quiet} of the 8 quiet strikes — an absolute \
             threshold set by the loud passage would do exactly this"
        );
    }

    #[test]
    fn silence_yields_nothing() {
        assert!(detect(&vec![0.0; 48_000], SR, &HybridConfig::default()).is_empty());
    }

    #[test]
    fn refinement_cannot_move_a_hit_onto_its_neighbour() {
        // The window is smaller than the gap, so a hit stays its own.
        let cfg = HybridConfig::default();
        let gap = 0.06;
        assert!(
            cfg.refine_window_secs < gap / 2.0,
            "the refine window must not reach the next hit"
        );
        let buf = take(&[(0.5, 0.3), (0.5 + gap, 0.9)], 1.5);
        let hits = detect(&buf, SR, &cfg);
        assert_eq!(hits.len(), 2, "the two strikes merged: {hits:?}");
        assert!((hits[0].at - 0.5).abs() < 0.005);
        assert!((hits[1].at - (0.5 + gap)).abs() < 0.005);
    }
}
