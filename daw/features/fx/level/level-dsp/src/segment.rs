//! Gap-bridged segmentation of a classified block stream.
//!
//! Runs of tonal blocks become [`Segment`]s. Short unvoiced gaps between two
//! tonal runs (vibrato dips, brief closures) are bridged so a single sung word
//! stays one segment instead of shattering into micro-fragments; segments below
//! a minimum length are then dropped as noise. Clean-room reimplementation of
//! the reference macro's three-pass segmenter.

use crate::classify::ClassifyConfig;

/// A contiguous tonal region, expressed in block indices `[from, to]`
/// (inclusive) with its mean level.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    /// First block index (inclusive).
    pub from: usize,
    /// Last block index (inclusive).
    pub to: usize,
    /// Mean RMS across the segment (linear).
    pub avg_rms: f64,
    /// Mean RMS in dBFS.
    pub avg_rms_db: f64,
}

impl Segment {
    /// Block count in the segment.
    #[inline]
    pub fn len(&self) -> usize {
        self.to - self.from + 1
    }

    /// Whether the segment is empty (never true for a well-formed segment).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.to < self.from
    }
}

/// Segmentation tuning.
#[derive(Clone, Copy, Debug)]
pub struct SegmentConfig {
    /// Minimum retained segment length, in milliseconds.
    pub min_seg_ms: f64,
    /// Maximum tonal gap bridged, in milliseconds (vibrato fix).
    pub max_gap_ms: f64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            min_seg_ms: 30.0,
            max_gap_ms: 100.0,
        }
    }
}

/// Build segments from a tonal mask and per-block RMS.
///
/// `is_tonal[i]` and `rms[i]` describe block `i`; both must be the same length.
/// `block_ms` is the analysis block length used to convert the ms thresholds to
/// block counts. The tonal mask is mutated in place to reflect bridged gaps so
/// downstream gain application sees the same view.
pub fn build_segments(
    is_tonal: &mut [bool],
    rms: &[f64],
    block_ms: f64,
    cfg: SegmentConfig,
    class_cfg: &ClassifyConfig,
) -> Vec<Segment> {
    let _ = class_cfg; // reserved for future centroid-aware bridging
    let n = is_tonal.len();
    if n == 0 {
        return Vec::new();
    }
    let max_gap_blks = (cfg.max_gap_ms / block_ms).ceil() as usize;
    let min_blks = ((cfg.min_seg_ms / block_ms).ceil() as usize).max(1);

    // Pass 1: raw tonal runs as (from, to) inclusive.
    let mut raw: Vec<(usize, usize)> = Vec::new();
    let mut cur: Option<(usize, usize)> = None;
    for (i, &t) in is_tonal.iter().enumerate() {
        if t {
            match cur {
                Some((from, _)) => cur = Some((from, i)),
                None => cur = Some((i, i)),
            }
        } else if let Some(run) = cur.take() {
            raw.push(run);
        }
    }
    if let Some(run) = cur.take() {
        raw.push(run);
    }

    // Pass 2: bridge short gaps, painting bridged blocks tonal.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    if !raw.is_empty() {
        let mut current = raw[0];
        for &next in &raw[1..] {
            let gap = next.0 - current.1;
            if gap <= max_gap_blks {
                #[allow(clippy::needless_range_loop)]
                for b in (current.1 + 1)..next.0 {
                    is_tonal[b] = true;
                }
                current.1 = next.1;
            } else {
                merged.push(current);
                current = next;
            }
        }
        merged.push(current);
    }

    // Pass 3: drop short segments, compute means.
    merged
        .into_iter()
        .filter(|&(from, to)| to - from + 1 >= min_blks)
        .map(|(from, to)| {
            let count = to - from + 1;
            let sum: f64 = rms[from..=to].iter().sum();
            let avg = sum / count as f64;
            Segment {
                from,
                to,
                avg_rms: avg,
                avg_rms_db: audiocore_dsp::db::linear_to_db(avg),
            }
        })
        .collect()
}

/// Mean RMS (dB) of the tonal segments — the natural auto-target for riding.
/// Returns `None` when there is no tonal material.
pub fn auto_target_db(segments: &[Segment]) -> Option<f64> {
    if segments.is_empty() {
        return None;
    }
    let (sum, count) = segments.iter().fold((0.0, 0usize), |(s, c), seg| {
        (s + seg.avg_rms * seg.len() as f64, c + seg.len())
    });
    if count == 0 {
        None
    } else {
        Some(audiocore_dsp::db::linear_to_db(sum / count as f64))
    }
}
