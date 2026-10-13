//! Tempo maps: samples ↔ musical beats.
//!
//! Two variants:
//!
//! - [`StaticTempoMap`] — single BPM for the whole timeline. What
//!   `daw-proto`'s `Transport.tempo` currently expresses, and the
//!   only variant wired into [`TransportEngine`](super::engine) today.
//!
//! - [`DynamicTempoMap`] — keyframed BPM, **stepped** (not ramped)
//!   between keyframes, with pre-computed cumulative seconds for
//!   O(log n) lookup. Modeled after Firewheel's `DynamicTransport`.
//!   Wired in as soon as the `daw-proto` tempo-map surface lands a
//!   keyframe representation.
//!
//! Both maps are immutable after construction; mutation = build a new
//! one and swap via `Arc`.

use super::clock::{InstantMusical, InstantSamples, InstantSeconds, SampleClock};

#[inline]
fn seconds_per_beat(bpm: f64, speed: f64) -> f64 {
    60.0 / (bpm * speed)
}

#[inline]
fn beats_per_second(bpm: f64, speed: f64) -> f64 {
    bpm * speed * (1.0 / 60.0)
}

// ── Static ────────────────────────────────────────────────────────────

/// Single-tempo map.
#[derive(Debug, Clone, Copy)]
pub struct StaticTempoMap {
    pub bpm: f64,
}

impl StaticTempoMap {
    #[inline]
    pub fn new(bpm: f64) -> Self {
        debug_assert!(bpm > 0.0);
        Self { bpm }
    }

    #[inline]
    pub fn samples_to_musical(
        &self,
        samples: InstantSamples,
        speed_multiplier: f64,
        clock: &SampleClock,
    ) -> InstantMusical {
        let secs = clock.samples_to_seconds(samples).0;
        InstantMusical(secs * beats_per_second(self.bpm, speed_multiplier))
    }

    #[inline]
    pub fn musical_to_samples(
        &self,
        musical: InstantMusical,
        speed_multiplier: f64,
        clock: &SampleClock,
    ) -> InstantSamples {
        let secs = musical.0 * seconds_per_beat(self.bpm, speed_multiplier);
        clock.seconds_to_samples(InstantSeconds(secs))
    }
}

// ── Dynamic (keyframed, stepped) ──────────────────────────────────────

/// One tempo keyframe.
#[derive(Debug, Clone, Copy)]
pub struct TempoKeyframe {
    /// Musical instant where this tempo takes effect.
    pub at_musical: InstantMusical,
    /// BPM from this keyframe until the next.
    pub bpm: f64,
}

/// Cumulative seconds at the start of each keyframe; built from the
/// keyframe list at construction so sample↔musical lookups are O(log n).
#[derive(Debug, Clone, Copy)]
struct KeyframeCache {
    start_seconds: f64,
}

/// Keyframed tempo map. Tempo is **stepped** — there is no ramp
/// between keyframes; BPM jumps at each boundary.
#[derive(Debug, Clone)]
pub struct DynamicTempoMap {
    keyframes: alloc::vec::Vec<TempoKeyframe>,
    cache: alloc::vec::Vec<KeyframeCache>,
}

#[derive(Debug)]
pub enum TempoMapError {
    Empty,
    FirstKeyframeNotAtZero,
    OutOfOrder,
    NonPositiveBpm,
}

impl DynamicTempoMap {
    pub fn new(keyframes: alloc::vec::Vec<TempoKeyframe>) -> Result<Self, TempoMapError> {
        use TempoMapError::*;
        if keyframes.is_empty() {
            return Err(Empty);
        }
        if keyframes[0].at_musical.0 != 0.0 {
            return Err(FirstKeyframeNotAtZero);
        }

        let mut cache = alloc::vec::Vec::with_capacity(keyframes.len());
        let mut start_seconds = 0.0;
        cache.push(KeyframeCache { start_seconds });

        for i in 1..keyframes.len() {
            let prev = &keyframes[i - 1];
            let cur = &keyframes[i];
            if cur.at_musical.0 <= prev.at_musical.0 {
                return Err(OutOfOrder);
            }
            if prev.bpm <= 0.0 {
                return Err(NonPositiveBpm);
            }
            let span_beats = cur.at_musical.0 - prev.at_musical.0;
            start_seconds += span_beats * seconds_per_beat(prev.bpm, 1.0);
            cache.push(KeyframeCache { start_seconds });
        }
        if keyframes.last().unwrap().bpm <= 0.0 {
            return Err(NonPositiveBpm);
        }

        Ok(Self { keyframes, cache })
    }

    /// Samples → musical, honoring `speed_multiplier`.
    pub fn samples_to_musical(
        &self,
        samples: InstantSamples,
        speed_multiplier: f64,
        clock: &SampleClock,
    ) -> InstantMusical {
        let secs = clock.samples_to_seconds(samples).0 * speed_multiplier;
        let idx = self.find_keyframe_seconds(secs);
        let kf = &self.keyframes[idx];
        let kc = &self.cache[idx];
        InstantMusical(kf.at_musical.0 + (secs - kc.start_seconds) * beats_per_second(kf.bpm, 1.0))
    }

    /// Musical → samples, honoring `speed_multiplier`.
    pub fn musical_to_samples(
        &self,
        musical: InstantMusical,
        speed_multiplier: f64,
        clock: &SampleClock,
    ) -> InstantSamples {
        let idx = self.find_keyframe_musical(musical);
        let kf = &self.keyframes[idx];
        let kc = &self.cache[idx];
        let secs = kc.start_seconds + (musical.0 - kf.at_musical.0) * seconds_per_beat(kf.bpm, 1.0);
        clock.seconds_to_samples(InstantSeconds(secs / speed_multiplier))
    }

    fn find_keyframe_seconds(&self, secs: f64) -> usize {
        match self.cache.binary_search_by(|c| {
            c.start_seconds
                .partial_cmp(&secs)
                .unwrap_or(core::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }

    fn find_keyframe_musical(&self, musical: InstantMusical) -> usize {
        match self.keyframes.binary_search_by(|k| {
            k.at_musical
                .0
                .partial_cmp(&musical.0)
                .unwrap_or(core::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }
}

extern crate alloc;

/// Build a [`DynamicTempoMap`] from a list of `(time_seconds, bpm)`
/// tempo points. Integrates beats forward through each segment so the
/// resulting keyframes carry musical positions matching the proto
/// seconds-axis source.
///
/// Sorting is the caller's responsibility (`points` must be sorted by
/// time). If the first point isn't at `t = 0`, a synthetic 120 BPM
/// segment is prepended so the keyframe list is valid.
pub fn build_dynamic_from_time_bpm(
    points: &[(f64, f64)],
) -> Result<DynamicTempoMap, TempoMapError> {
    if points.is_empty() {
        return Err(TempoMapError::Empty);
    }

    let mut keyframes: alloc::vec::Vec<TempoKeyframe> =
        alloc::vec::Vec::with_capacity(points.len() + 1);

    let mut prev_secs;
    let mut prev_beats = 0.0;
    let mut prev_bpm;

    if points[0].0 > 0.0 {
        // Synthetic segment from t=0 at 120 BPM until first real point.
        keyframes.push(TempoKeyframe {
            at_musical: InstantMusical(0.0),
            bpm: 120.0,
        });
        prev_secs = 0.0;
        prev_bpm = 120.0;
    } else {
        keyframes.push(TempoKeyframe {
            at_musical: InstantMusical(0.0),
            bpm: points[0].1.max(1e-3),
        });
        prev_secs = points[0].0;
        prev_bpm = points[0].1.max(1e-3);
    }

    let start_idx = if points[0].0 > 0.0 { 0 } else { 1 };
    for &(t, bpm) in &points[start_idx..] {
        let dt = (t - prev_secs).max(0.0);
        let beats = prev_beats + dt * beats_per_second(prev_bpm, 1.0);
        keyframes.push(TempoKeyframe {
            at_musical: InstantMusical(beats),
            bpm: bpm.max(1e-3),
        });
        prev_secs = t;
        prev_beats = beats;
        prev_bpm = bpm.max(1e-3);
    }

    DynamicTempoMap::new(keyframes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_map_roundtrip() {
        let m = StaticTempoMap::new(120.0);
        let c = SampleClock::new(48_000);
        // 120 BPM, 1 beat = 0.5s = 24_000 samples
        let mu = m.samples_to_musical(InstantSamples(24_000), 1.0, &c);
        assert!((mu.0 - 1.0).abs() < 1e-9);
        let back = m.musical_to_samples(mu, 1.0, &c);
        assert_eq!(back.0, 24_000);
    }

    #[test]
    fn static_map_varispeed() {
        let m = StaticTempoMap::new(120.0);
        let c = SampleClock::new(48_000);
        // 2x speed → 1 beat passes in 12_000 samples
        let mu = m.samples_to_musical(InstantSamples(12_000), 2.0, &c);
        assert!((mu.0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn build_from_time_bpm_handles_origin_point() {
        // Point at t=0 → no synthetic prefix. Two segments: 120 BPM
        // from 0..2s, 60 BPM thereafter.
        let m = build_dynamic_from_time_bpm(&[(0.0, 120.0), (2.0, 60.0)]).unwrap();
        let c = SampleClock::new(48_000);
        // 2 s @ 120 BPM = 4 beats.
        let s_at_4 = m.musical_to_samples(InstantMusical(4.0), 1.0, &c);
        assert_eq!(s_at_4.0, 96_000);
        // 1 more beat @ 60 BPM = +1 s.
        let s_at_5 = m.musical_to_samples(InstantMusical(5.0), 1.0, &c);
        assert_eq!(s_at_5.0, 144_000);
    }

    #[test]
    fn build_from_time_bpm_prepends_synthetic_when_first_offset() {
        // First point at t=1.0 → 120 BPM from 0..1.0, then 60 BPM.
        let m = build_dynamic_from_time_bpm(&[(1.0, 60.0)]).unwrap();
        let c = SampleClock::new(48_000);
        // 1 s @ 120 = 2 beats. Then 1 beat @ 60 = +1s = 2s total.
        let s_at_3 = m.musical_to_samples(InstantMusical(3.0), 1.0, &c);
        assert_eq!(s_at_3.0, 96_000);
    }

    #[test]
    fn dynamic_map_two_segments() {
        // Beats 0..4 at 120 BPM (each = 0.5s), then 4+ at 60 BPM (each = 1.0s).
        let m = DynamicTempoMap::new(alloc::vec![
            TempoKeyframe {
                at_musical: InstantMusical(0.0),
                bpm: 120.0
            },
            TempoKeyframe {
                at_musical: InstantMusical(4.0),
                bpm: 60.0
            },
        ])
        .unwrap();
        let c = SampleClock::new(48_000);

        // Beat 4 = 2.0 s = 96_000 samples
        let s_at_4 = m.musical_to_samples(InstantMusical(4.0), 1.0, &c);
        assert_eq!(s_at_4.0, 96_000);

        // Beat 5 = 2.0s + 1.0s = 3.0s = 144_000 samples
        let s_at_5 = m.musical_to_samples(InstantMusical(5.0), 1.0, &c);
        assert_eq!(s_at_5.0, 144_000);

        // Round trip
        let mu = m.samples_to_musical(s_at_5, 1.0, &c);
        assert!((mu.0 - 5.0).abs() < 1e-9, "got {}", mu.0);
    }
}
