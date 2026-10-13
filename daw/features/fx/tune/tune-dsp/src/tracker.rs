//! Polyphonic note tracking — link per-frame fundamentals into notes over time.
//!
//! [`DnaEngine::analyze_pitch_frames`](crate::dna::DnaEngine::analyze_pitch_frames)
//! gives, for each STFT frame, the *set* of fundamentals present — but with no
//! identity across time: frame 40's "220 Hz" and frame 41's "221 Hz" are almost
//! certainly the same sung note, yet the analyzer reports them independently.
//! This module threads those per-frame observations into [`TrackedNote`]s: the
//! sustained note objects a Melodyne-style UI draws as blobs and the DNA
//! separator isolates across their real lifetime.
//!
//! The tracker is a greedy nearest-in-pitch matcher with gap bridging:
//! - each frame, every open track claims the closest unused pitch within a
//!   semitone tolerance and resets its gap counter;
//! - a track that finds nothing ages; once its gap exceeds `max_gap_frames` it
//!   closes;
//! - leftover pitches seed new tracks;
//! - closed tracks shorter than `min_frames` are discarded as noise.
//!
//! Pitch contours are stored per active frame (gaps held at the last value), so
//! the resulting [`NoteSpan`] follows vibrato and drift when handed back to the
//! separator.

use crate::detect::hz_to_midi;
use crate::dna::NoteSpan;

/// Tracker tuning.
#[derive(Clone, Copy, Debug)]
pub struct TrackConfig {
    /// Pitch deviation (semitones) tolerated when matching a frame pitch to an
    /// open track.
    pub tolerance_semitones: f64,
    /// Frames a track may go unmatched before it closes.
    pub max_gap_frames: usize,
    /// Minimum retained note length in frames.
    pub min_frames: usize,
}

impl Default for TrackConfig {
    fn default() -> Self {
        Self {
            tolerance_semitones: 0.7,
            max_gap_frames: 3,
            min_frames: 3,
        }
    }
}

/// A sustained note recovered from the per-frame pitch stream.
#[derive(Clone, Debug)]
pub struct TrackedNote {
    /// First active frame (inclusive).
    pub start_frame: usize,
    /// Last active frame (inclusive).
    pub end_frame: usize,
    /// Per-frame fundamental in Hz over `[start_frame, end_frame]` (gaps held).
    pub f0: Vec<f64>,
    /// Median pitch across the note, MIDI (cents-accurate).
    pub median_midi: f64,
}

impl TrackedNote {
    /// Frame count.
    #[inline]
    pub fn len(&self) -> usize {
        self.end_frame - self.start_frame + 1
    }

    /// Whether the note is empty (never true for a well-formed note).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.end_frame < self.start_frame
    }

    /// Onset time in seconds, given the STFT hop and sample rate.
    pub fn onset_sec(&self, hop: usize, sample_rate: f64) -> f64 {
        self.start_frame as f64 * hop as f64 / sample_rate
    }

    /// Convert to a [`NoteSpan`] for [`crate::dna::DnaEngine::separate_spans`].
    pub fn to_span(&self) -> NoteSpan {
        NoteSpan {
            start_frame: self.start_frame,
            f0: self.f0.clone(),
        }
    }
}

/// Internal open-track state during the sweep.
struct Open {
    start_frame: usize,
    last_frame: usize,
    /// (frame, f0) observations; gaps are filled at close.
    obs: Vec<(usize, f64)>,
    gap: usize,
}

/// Link per-frame pitch sets into tracked notes.
///
/// `frames[i]` is the set of fundamentals (Hz) detected in frame `i`. Returns
/// notes ordered by onset frame.
pub fn track_notes(frames: &[Vec<f64>], cfg: TrackConfig) -> Vec<TrackedNote> {
    let mut open: Vec<Open> = Vec::new();
    let mut closed: Vec<Open> = Vec::new();

    for (fi, pitches) in frames.iter().enumerate() {
        let mut used = vec![false; pitches.len()];

        // Each open track grabs its nearest unused pitch within tolerance.
        for tr in &mut open {
            let cur = tr.obs.last().map(|&(_, f)| f).unwrap_or(0.0);
            let cur_midi = hz_to_midi(cur);
            let mut best: Option<usize> = None;
            let mut best_d = cfg.tolerance_semitones;
            for (pi, &p) in pitches.iter().enumerate() {
                if used[pi] {
                    continue;
                }
                let d = (hz_to_midi(p) - cur_midi).abs();
                if d <= best_d {
                    best_d = d;
                    best = Some(pi);
                }
            }
            match best {
                Some(pi) => {
                    used[pi] = true;
                    tr.obs.push((fi, pitches[pi]));
                    tr.last_frame = fi;
                    tr.gap = 0;
                }
                None => tr.gap += 1,
            }
        }

        // Retire tracks that have been silent too long.
        let mut i = 0;
        while i < open.len() {
            if open[i].gap > cfg.max_gap_frames {
                closed.push(open.swap_remove(i));
            } else {
                i += 1;
            }
        }

        // Unused pitches seed new tracks.
        for (pi, &p) in pitches.iter().enumerate() {
            if !used[pi] {
                open.push(Open {
                    start_frame: fi,
                    last_frame: fi,
                    obs: vec![(fi, p)],
                    gap: 0,
                });
            }
        }
    }
    closed.extend(open);

    let mut notes: Vec<TrackedNote> = closed
        .into_iter()
        .filter_map(|tr| finish(tr, cfg))
        .collect();
    notes.sort_by_key(|n| n.start_frame);
    notes
}

/// Convenience: track then convert straight to spans.
pub fn spans_from_frames(frames: &[Vec<f64>], cfg: TrackConfig) -> Vec<NoteSpan> {
    track_notes(frames, cfg)
        .iter()
        .map(|n| n.to_span())
        .collect()
}

/// Expand a track's sparse observations into a contiguous contour, filter by
/// length, and compute the median pitch.
fn finish(tr: Open, cfg: TrackConfig) -> Option<TrackedNote> {
    let start = tr.start_frame;
    let end = tr.last_frame;
    let len = end - start + 1;
    if len < cfg.min_frames || tr.obs.is_empty() {
        return None;
    }

    // Contiguous f0 contour, holding the last observed value across gaps.
    let mut f0 = vec![0.0f64; len];
    let mut oi = 0;
    let mut last = tr.obs[0].1;
    for (k, slot) in f0.iter_mut().enumerate() {
        let frame = start + k;
        while oi < tr.obs.len() && tr.obs[oi].0 == frame {
            last = tr.obs[oi].1;
            oi += 1;
        }
        *slot = last;
    }

    let mut midis: Vec<f64> = f0.iter().map(|&f| hz_to_midi(f)).collect();
    midis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let median_midi = midis[midis.len() / 2];

    Some(TrackedNote {
        start_frame: start,
        end_frame: end,
        f0,
        median_midi,
    })
}
