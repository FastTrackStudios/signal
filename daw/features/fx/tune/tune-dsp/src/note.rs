//! Note segmentation from a pitch track.
//!
//! Groups a sequence of [`PitchFrame`]s into [`Note`]s — the editable "blobs" a
//! Melodyne-style UI shows. A note is a run of voiced frames whose pitch stays
//! within a tolerance of a running centre; unvoiced gaps end the current note.
//! Each note carries its median pitch (robust to vibrato/overshoot) so the
//! correction stage can snap it to a scale.
//!
//! This is the monophonic foundation; polyphonic note extraction (multiple
//! simultaneous blobs) is the larger follow-up.

use crate::detect::{hz_to_midi, PitchFrame};

/// One detected note over a frame range.
#[derive(Clone, Copy, Debug)]
pub struct Note {
    /// First frame index (inclusive).
    pub start_frame: usize,
    /// Last frame index (inclusive).
    pub end_frame: usize,
    /// Median pitch across the note, MIDI (float, cents-accurate).
    pub median_midi: f64,
    /// Mean RMS of the note (linear).
    pub mean_rms: f64,
}

impl Note {
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
}

/// Note-segmentation tuning.
#[derive(Clone, Copy, Debug)]
pub struct NoteConfig {
    /// Pitch deviation (semitones) tolerated within one note.
    pub tolerance_semitones: f64,
    /// Minimum note length in frames (shorter runs are dropped).
    pub min_frames: usize,
    /// Voiced gap (frames) bridged before a note is split.
    pub max_gap_frames: usize,
}

impl Default for NoteConfig {
    fn default() -> Self {
        Self {
            tolerance_semitones: 1.5,
            min_frames: 3,
            max_gap_frames: 2,
        }
    }
}

/// Segment a pitch track into notes.
pub fn segment_notes(frames: &[PitchFrame], cfg: NoteConfig) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut cur_start: Option<usize> = None;
    let mut cur_midi: Vec<f64> = Vec::new();
    let mut cur_rms: Vec<f64> = Vec::new();
    let mut centre = 0.0;
    let mut gap = 0usize;

    let flush = |start: usize,
                 end: usize,
                 midis: &mut Vec<f64>,
                 rmss: &mut Vec<f64>,
                 out: &mut Vec<Note>| {
        if !midis.is_empty() && (end - start + 1) >= cfg.min_frames {
            let mut sorted = midis.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            let median = sorted[sorted.len() / 2];
            let mean_rms = rmss.iter().sum::<f64>() / rmss.len() as f64;
            out.push(Note {
                start_frame: start,
                end_frame: end,
                median_midi: median,
                mean_rms,
            });
        }
        midis.clear();
        rmss.clear();
    };

    let mut last_voiced = 0usize;
    for (i, f) in frames.iter().enumerate() {
        match f.f0_hz {
            Some(hz) => {
                let midi = hz_to_midi(hz);
                match cur_start {
                    None => {
                        cur_start = Some(i);
                        centre = midi;
                        cur_midi.push(midi);
                        cur_rms.push(f.rms);
                        last_voiced = i;
                        gap = 0;
                    }
                    Some(start) => {
                        if (midi - centre).abs() <= cfg.tolerance_semitones {
                            cur_midi.push(midi);
                            cur_rms.push(f.rms);
                            // Slowly track the centre.
                            centre = 0.9 * centre + 0.1 * midi;
                            last_voiced = i;
                            gap = 0;
                        } else {
                            // Pitch jumped — close this note, open a new one.
                            flush(start, last_voiced, &mut cur_midi, &mut cur_rms, &mut notes);
                            cur_start = Some(i);
                            centre = midi;
                            cur_midi.push(midi);
                            cur_rms.push(f.rms);
                            last_voiced = i;
                            gap = 0;
                        }
                    }
                }
            }
            None => {
                if cur_start.is_some() {
                    gap += 1;
                    if gap > cfg.max_gap_frames {
                        let start = cur_start.take().unwrap();
                        flush(start, last_voiced, &mut cur_midi, &mut cur_rms, &mut notes);
                        gap = 0;
                    }
                }
            }
        }
    }
    if let Some(start) = cur_start {
        flush(start, last_voiced, &mut cur_midi, &mut cur_rms, &mut notes);
    }
    notes
}
