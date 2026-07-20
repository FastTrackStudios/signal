//! trigger-dsp — drum-trigger transient-detection engine.
//!
//! Audio in → hit events out: a fast peak envelope follower feeds an onset
//! detector that fires when the signal crosses an absolute threshold AND
//! jumps a *dynamics margin* above a slow "recent floor" follower (so a
//! drum's ring-out never retriggers — only a fresh transient can outrun the
//! floor). A retrigger-guard window plus arm/disarm hysteresis suppress
//! double-fires; a short capture window after each onset measures the hit's
//! true peak, which maps through a configurable dB range (with optional
//! gamma curve) to MIDI velocity 1..=127.
//!
//! Hits are reported as `(sample_offset_in_block, velocity)` so a plugin
//! shell can emit sample-accurate note events. The report arrives at the
//! *end* of the capture window (~1.5 ms after onset); [`Hit::latency_samples`]
//! is the distance back to the onset sample.
//!
//! Processing-core rules (platform targets): no allocation on the hot path,
//! no threads, no platform I/O. All coefficients are pre-computed in the
//! setters; `process_sample()` is pure arithmetic and branch-light.

#![no_std]

// Tests use std float math (`sin`/`exp`) to synthesize kicks; the engine
// itself never does.
#[cfg(test)]
extern crate std;

/// A detected hit, reported when its velocity-capture window completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// MIDI velocity, 1..=127.
    pub velocity: u8,
    /// Samples elapsed between the onset and this report (the capture-window
    /// length minus one). Subtract from the current sample index to place the
    /// note at the onset.
    pub latency_samples: u32,
}

/// A hit positioned inside a block (see [`TriggerDetector::process_block`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHit {
    /// Onset offset in samples from the start of the block. Clamped to 0 when
    /// the onset happened near the end of the previous block.
    pub offset: u32,
    /// MIDI velocity, 1..=127.
    pub velocity: u8,
}

/// Maximum hits [`TriggerDetector::process_block`] reports per call — with
/// the minimum 5 ms retrigger guard this covers blocks up to ~150 ms.
pub const MAX_BLOCK_HITS: usize = 32;

// ── Fixed detector time constants ─────────────────────────────────────────
/// Fast peak-follower attack (near-instant, keeps the transient edge).
const ATTACK_MS: f32 = 0.1;
/// Fast peak-follower release — slow enough to bridge waveform zero
/// crossings, fast enough to expose the next hit.
const RELEASE_MS: f32 = 20.0;
/// The "recent floor" follower — a lagged copy of the fast envelope. During
/// ring-out fast ≈ floor; only a fresh transient can exceed floor × margin.
const FLOOR_MS: f32 = 40.0;
/// Velocity capture window after onset.
const CAPTURE_MS: f32 = 1.5;

/// Mono drum-trigger detector. Feed it the (mono-summed) input one sample at
/// a time; it returns a [`Hit`] when one completes its capture window.
#[derive(Debug, Clone)]
pub struct TriggerDetector {
    sample_rate: f32,

    // ── user config (kept in user units for sample-rate changes) ──
    threshold_db: f32,
    dynamics_db: f32,
    retrigger_ms: f32,
    vel_min_db: f32,
    vel_max_db: f32,
    /// Velocity curve exponent (1.0 = linear in dB).
    gamma: f32,

    // ── derived (recomputed in setters) ──
    threshold_lin: f32,
    /// Linear ratio the fast envelope must exceed the floor by.
    margin_lin: f32,
    /// Re-arm ratio (hysteresis): half the margin in dB.
    rearm_lin: f32,
    retrigger_samples: u32,
    capture_len: u32,
    attack_coef: f32,
    release_coef: f32,
    floor_coef: f32,

    // ── state ──
    /// Fast peak envelope.
    fast: f32,
    /// Slow "recent floor" (lagged fast envelope).
    floor: f32,
    /// Hysteresis arm flag — cleared on fire, set again once the envelope
    /// settles back toward the floor (or under the threshold).
    armed: bool,
    /// Retrigger-guard countdown; onsets are ignored while non-zero.
    guard: u32,
    /// Samples left in the current velocity-capture window (0 = idle).
    capture_remaining: u32,
    capture_peak: f32,
}

impl TriggerDetector {
    pub fn new(sample_rate: f32) -> Self {
        let mut d = Self {
            sample_rate: sample_rate.max(1.0),
            threshold_db: -30.0,
            dynamics_db: 6.0,
            retrigger_ms: 40.0,
            vel_min_db: -40.0,
            vel_max_db: -6.0,
            gamma: 1.0,
            threshold_lin: 0.0,
            margin_lin: 1.0,
            rearm_lin: 1.0,
            retrigger_samples: 0,
            capture_len: 1,
            attack_coef: 1.0,
            release_coef: 0.0,
            floor_coef: 0.0,
            fast: 0.0,
            floor: 0.0,
            armed: true,
            guard: 0,
            capture_remaining: 0,
            capture_peak: 0.0,
        };
        d.recompute();
        d
    }

    /// Reset all runtime state (envelopes, guard, pending capture).
    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.floor = 0.0;
        self.armed = true;
        self.guard = 0;
        self.capture_remaining = 0;
        self.capture_peak = 0.0;
    }

    // ── configuration ────────────────────────────────────────────────────

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.recompute();
    }

    /// Absolute onset threshold in dBFS (typ. -60..0).
    pub fn set_threshold_db(&mut self, db: f32) {
        self.threshold_db = db;
        self.recompute();
    }

    /// Dynamics margin in dB the transient must exceed the recent floor by
    /// (typ. 0..24; higher = less sensitive to ring-out).
    pub fn set_dynamics_db(&mut self, db: f32) {
        self.dynamics_db = db.max(0.0);
        self.recompute();
    }

    /// Retrigger-guard window in ms during which new onsets are ignored.
    pub fn set_retrigger_ms(&mut self, ms: f32) {
        self.retrigger_ms = ms.max(0.0);
        self.recompute();
    }

    /// Peak dB range mapped onto velocity 1..=127.
    pub fn set_velocity_range_db(&mut self, min_db: f32, max_db: f32) {
        self.vel_min_db = min_db;
        self.vel_max_db = max_db;
    }

    /// Velocity curve exponent: <1 lifts soft hits, >1 compresses them.
    pub fn set_velocity_gamma(&mut self, gamma: f32) {
        self.gamma = gamma.clamp(0.1, 10.0);
    }

    fn recompute(&mut self) {
        self.threshold_lin = db_to_gain(self.threshold_db);
        self.margin_lin = db_to_gain(self.dynamics_db);
        // Hysteresis: re-arm once the envelope is back within half the
        // margin of the floor.
        self.rearm_lin = db_to_gain(self.dynamics_db * 0.5);
        self.retrigger_samples =
            (self.retrigger_ms * 0.001 * self.sample_rate) as u32;
        self.capture_len =
            ((CAPTURE_MS * 0.001 * self.sample_rate) as u32).max(1);
        self.attack_coef = one_pole_coef(ATTACK_MS, self.sample_rate);
        self.release_coef = one_pole_coef(RELEASE_MS, self.sample_rate);
        self.floor_coef = one_pole_coef(FLOOR_MS, self.sample_rate);
    }

    // ── processing ───────────────────────────────────────────────────────

    /// Advance one (mono) sample. Returns a [`Hit`] when a capture window
    /// completes; place the note `latency_samples` back from *this* sample.
    #[inline]
    pub fn process_sample(&mut self, x: f32) -> Option<Hit> {
        let a = x.abs();

        // Fast peak follower: near-instant attack, musical release.
        let coef = if a > self.fast { self.attack_coef } else { self.release_coef };
        self.fast += (a - self.fast) * coef;
        // Slow floor chases the fast envelope both ways — during steady
        // ring-out fast ≈ floor, so the margin test only passes on a fresh
        // transient the floor hasn't caught up to yet.
        self.floor += (self.fast - self.floor) * self.floor_coef;

        if self.guard > 0 {
            self.guard -= 1;
        }

        let mut hit = None;
        if self.capture_remaining > 0 {
            // Velocity capture in progress.
            if a > self.capture_peak {
                self.capture_peak = a;
            }
            self.capture_remaining -= 1;
            if self.capture_remaining == 0 {
                hit = Some(Hit {
                    velocity: self.velocity_for(self.capture_peak),
                    latency_samples: self.capture_len - 1,
                });
            }
        } else if self.armed
            && self.guard == 0
            && self.fast > self.threshold_lin
            && self.fast > self.floor * self.margin_lin
        {
            // Onset: disarm, start the guard and the capture window.
            self.armed = false;
            self.guard = self.retrigger_samples;
            self.capture_peak = a;
            self.capture_remaining = self.capture_len - 1;
            if self.capture_remaining == 0 {
                hit = Some(Hit {
                    velocity: self.velocity_for(self.capture_peak),
                    latency_samples: 0,
                });
            }
        }

        // Hysteresis re-arm: only once the guard expired AND the envelope
        // settled back toward the floor (or fell well under the threshold).
        if !self.armed
            && self.guard == 0
            && self.capture_remaining == 0
            && (self.fast <= self.floor * self.rearm_lin
                || self.fast < self.threshold_lin * 0.5)
        {
            self.armed = true;
        }

        hit
    }

    /// Block convenience: run `input` through the detector and write onset
    /// positions into `hits` (a caller-owned slice, e.g.
    /// `[BlockHit; MAX_BLOCK_HITS]`). Returns the number of hits written.
    /// Onsets whose capture window started in the previous block clamp to
    /// offset 0.
    pub fn process_block(&mut self, input: &[f32], hits: &mut [BlockHit]) -> usize {
        let mut n = 0;
        for (i, &x) in input.iter().enumerate() {
            if let Some(h) = self.process_sample(x) {
                if n < hits.len() {
                    hits[n] = BlockHit {
                        offset: (i as u32).saturating_sub(h.latency_samples),
                        velocity: h.velocity,
                    };
                    n += 1;
                }
            }
        }
        n
    }

    /// Current fast-envelope level (for metering / a future UI).
    pub fn envelope(&self) -> f32 {
        self.fast
    }

    fn velocity_for(&self, peak: f32) -> u8 {
        let db = gain_to_db(peak);
        let span = (self.vel_max_db - self.vel_min_db).max(0.001);
        let mut t = ((db - self.vel_min_db) / span).clamp(0.0, 1.0);
        if self.gamma != 1.0 && t > 0.0 {
            t = pow_approx(t, self.gamma);
        }
        1 + (t * 126.0 + 0.5) as u8
    }
}

// ── no_std math helpers (same approximations as the sibling fx cores) ─────

/// One-pole smoothing coefficient for a time constant in ms:
/// `1 - e^(-1/(tc·sr))`, via `e^x = 2^(x·log2 e)`.
#[inline]
fn one_pole_coef(tc_ms: f32, sample_rate: f32) -> f32 {
    let tc_samples = (tc_ms * 0.001 * sample_rate).max(1.0);
    1.0 - exp2_approx(-core::f32::consts::LOG2_E / tc_samples)
}

/// dB → linear gain without `std`: 10^(db/20) = 2^(db · log2(10)/20).
#[inline]
fn db_to_gain(db: f32) -> f32 {
    exp2_approx(db * 0.166_096_4)
}

/// Linear gain → dB: 20·log10(x) = 20/log2(10) · log2(x). Floors at -120 dB.
#[inline]
fn gain_to_db(x: f32) -> f32 {
    if x < 1.0e-6 {
        return -120.0;
    }
    6.020_599_9 * log2_approx(x)
}

/// `t^g` for t in (0, 1]: 2^(g · log2 t).
#[inline]
fn pow_approx(t: f32, g: f32) -> f32 {
    exp2_approx(g * log2_approx(t))
}

/// 2^x split into integer and fractional parts with a cubic fit on the
/// fraction (max err ~2e-4) — matches saturate-dsp.
#[inline]
fn exp2_approx(x: f32) -> f32 {
    // Manual floor: `f32::floor` is std/libm-only and this crate is no_std.
    let mut xi = x as i32;
    if (xi as f32) > x {
        xi -= 1;
    }
    let xf = x - xi as f32;
    let frac = 1.0 + xf * (0.695_976 + xf * (0.226_174 + xf * 0.078_024));
    frac * pow2_int(xi)
}

#[inline]
fn pow2_int(n: i32) -> f32 {
    let n = n.clamp(-126, 127);
    f32::from_bits(((127 + n) as u32) << 23)
}

/// log2 via exponent extraction + minimax quartic on the mantissa
/// (max err ~1e-4 over [1,2)).
#[inline]
fn log2_approx(x: f32) -> f32 {
    let bits = x.to_bits();
    let exp = ((bits >> 23) & 0xff) as i32 - 127;
    let m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000); // [1, 2)
    let poly = -1.741_793_9
        + m * (2.821_202_6 + m * (-1.469_956_8 + m * (0.447_179_55 - m * 0.056_570_85)));
    exp as f32 + poly
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn detector() -> TriggerDetector {
        // Defaults: threshold -30 dB, margin 6 dB, retrigger 40 ms,
        // velocity range -40..-6 dB.
        TriggerDetector::new(SR)
    }

    /// Write a synthetic kick: a decaying 60 Hz sine, `amp` peak, ~`decay_ms`
    /// amplitude time constant, starting at `at` samples into `buf`.
    fn add_kick(buf: &mut [f32], at: usize, amp: f32, decay_ms: f32) {
        let w = 2.0 * core::f32::consts::PI * 60.0 / SR;
        let decay = -1.0 / (decay_ms * 0.001 * SR);
        for i in at..buf.len() {
            let n = (i - at) as f32;
            // std is linked for tests; the engine itself never calls sin/exp.
            buf[i] += amp * (n * decay).exp() * (n * w).sin();
        }
    }

    fn count_hits(det: &mut TriggerDetector, buf: &[f32]) -> alloc_free::Hits {
        let mut hits = alloc_free::Hits::default();
        for (i, &x) in buf.iter().enumerate() {
            if let Some(h) = det.process_sample(x) {
                hits.push(i as u32 - h.latency_samples, h.velocity);
            }
        }
        hits
    }

    /// Tiny fixed-capacity hit list so the tests stay alloc-free too.
    mod alloc_free {
        #[derive(Default)]
        pub struct Hits {
            pub offsets: [u32; 16],
            pub velocities: [u8; 16],
            pub len: usize,
        }
        impl Hits {
            pub fn push(&mut self, offset: u32, velocity: u8) {
                assert!(self.len < 16, "too many hits");
                self.offsets[self.len] = offset;
                self.velocities[self.len] = velocity;
                self.len += 1;
            }
        }
    }

    #[test]
    fn silence_never_fires() {
        let mut det = detector();
        let buf = [0.0f32; 48_000];
        assert_eq!(count_hits(&mut det, &buf).len, 0);
    }

    #[test]
    fn sub_threshold_noise_never_fires() {
        let mut det = detector();
        // A steady tone well under the -30 dB threshold.
        let mut buf = [0.0f32; 24_000];
        for (i, s) in buf.iter_mut().enumerate() {
            *s = 0.005 * (i as f32 * 0.05).sin(); // ~-46 dBFS
        }
        assert_eq!(count_hits(&mut det, &buf).len, 0);
    }

    #[test]
    fn kick_fires_exactly_once_including_ring_out() {
        let mut det = detector();
        let mut buf = [0.0f32; 48_000]; // 1 s: burst + full ring-out
        add_kick(&mut buf, 1000, 0.8, 30.0);
        let hits = count_hits(&mut det, &buf);
        assert_eq!(hits.len, 1, "expected exactly one hit");
        // Onset placed at the burst start (within a couple ms).
        let off = hits.offsets[0] as i64;
        assert!((off - 1000).unsigned_abs() < 200, "onset at {off}");
    }

    #[test]
    fn double_hit_inside_retrigger_window_fires_once() {
        let mut det = detector();
        det.set_retrigger_ms(40.0);
        let mut buf = [0.0f32; 48_000];
        add_kick(&mut buf, 1000, 0.8, 30.0);
        // Second burst 20 ms later — inside the 40 ms guard.
        add_kick(&mut buf, 1000 + (0.020 * SR) as usize, 0.8, 30.0);
        assert_eq!(count_hits(&mut det, &buf).len, 1);
    }

    #[test]
    fn double_hit_outside_retrigger_window_fires_twice() {
        let mut det = detector();
        det.set_retrigger_ms(40.0);
        let mut buf = [0.0f32; 48_000];
        let second = 1000 + (0.100 * SR) as usize; // 100 ms later
        add_kick(&mut buf, 1000, 0.8, 30.0);
        add_kick(&mut buf, second, 0.8, 30.0);
        let hits = count_hits(&mut det, &buf);
        assert_eq!(hits.len, 2, "expected both hits");
        let off = hits.offsets[1] as i64;
        assert!((off - second as i64).unsigned_abs() < 200, "second onset at {off}");
    }

    #[test]
    fn louder_burst_maps_to_higher_velocity() {
        let mut quiet = detector();
        let mut loud = detector();
        let mut buf_q = [0.0f32; 24_000];
        let mut buf_l = [0.0f32; 24_000];
        add_kick(&mut buf_q, 1000, 0.2, 30.0);
        add_kick(&mut buf_l, 1000, 0.8, 30.0);
        let hq = count_hits(&mut quiet, &buf_q);
        let hl = count_hits(&mut loud, &buf_l);
        assert_eq!(hq.len, 1);
        assert_eq!(hl.len, 1);
        assert!(
            hl.velocities[0] > hq.velocities[0],
            "loud {} !> quiet {}",
            hl.velocities[0],
            hq.velocities[0]
        );
        assert!(hq.velocities[0] >= 1 && hl.velocities[0] <= 127);
    }

    #[test]
    fn process_block_reports_offsets() {
        let mut det = detector();
        let mut buf = [0.0f32; 24_000];
        add_kick(&mut buf, 5000, 0.8, 30.0);
        let mut hits = [BlockHit { offset: 0, velocity: 0 }; MAX_BLOCK_HITS];
        let n = det.process_block(&buf, &mut hits);
        assert_eq!(n, 1);
        let off = hits[0].offset as i64;
        assert!((off - 5000).unsigned_abs() < 200, "onset at {off}");
        assert!(hits[0].velocity >= 1);
    }

    #[test]
    fn math_helpers_are_sane() {
        use approx::assert_relative_eq;
        assert_relative_eq!(db_to_gain(0.0), 1.0, max_relative = 1e-3);
        assert_relative_eq!(db_to_gain(-20.0), 0.1, max_relative = 1e-2);
        assert_relative_eq!(gain_to_db(0.5), -6.02, max_relative = 1e-2);
        assert_relative_eq!(pow_approx(0.5, 2.0), 0.25, max_relative = 1e-2);
    }
}
