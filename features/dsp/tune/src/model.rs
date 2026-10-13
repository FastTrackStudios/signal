//! The note-blob document — the Melodyne insight as a data model.
//!
//! Each detected note's pitch contour decomposes into three
//! independently editable terms:
//!
//! ```text
//! f0_midi(t) = center + drift(t)·drift_amount + modulation(t)·modulation_amount
//! ```
//!
//! - **center** — the note's pitch center (median MIDI). Dragging a
//!   blob edits ONLY this; snapping quantizes ONLY this.
//! - **drift** — the slow deviation of the contour from center (the
//!   singer sliding flat over a long note). Zero-phase low-pass of the
//!   residual, ≲ ~3 Hz.
//! - **modulation** — the vibrato residual (contour − center − drift).
//!
//! Melodyne's "pitch drift" and "pitch modulation" tools are exactly
//! the two `*_amount` scalars (1 = as sung, 0 = flattened).
//!
//! Timing edits use the melonix marker model (MIT — mika314/melonix):
//! sample-anchored [`WarpMarker`]s define a piecewise-linear time-warp
//! and a pitch-bend overlay, interpolated between markers and decaying
//! to identity outside them.

use crate::detect::{hz_to_midi, midi_to_hz};
use crate::tracker::TrackedNote;

/// Editable pitch blob for one note.
#[derive(Clone, Debug)]
pub struct NoteBlob {
    /// First analysis frame (inclusive).
    pub start_frame: usize,
    /// Last analysis frame (inclusive).
    pub end_frame: usize,
    /// Pitch center in MIDI (float, cents-accurate). The handle.
    pub center_midi: f64,
    /// The center as analyzed (so edits serialize as deltas and a
    /// re-analysis can preserve them).
    pub analyzed_center_midi: f64,
    /// Slow deviation from center, semitones per frame.
    pub drift: Vec<f64>,
    /// Vibrato residual, semitones per frame.
    pub modulation: Vec<f64>,
    /// 0..1 — scale the sung drift (0 = perfectly flat approach).
    pub drift_amount: f64,
    /// 0..1+ — scale the sung vibrato (0 = robotic, >1 = exaggerated).
    pub modulation_amount: f64,
    /// Per-note formant shift in semitones (0 = as sung).
    pub formant_shift: f64,
    /// Per-note gain trim in dB.
    pub gain_db: f64,
    /// Onset transition: seconds to glide from the previous pitch
    /// into this note's target (0 = hard-tune jump).
    pub retune_s: f64,
    /// Mean frame RMS from analysis (display weighting).
    pub rms: f64,
}

impl NoteBlob {
    /// Frame count.
    #[allow(clippy::len_without_is_empty)] // a blob is never empty
    pub fn len(&self) -> usize {
        self.end_frame - self.start_frame + 1
    }

    /// The edited target contour at `frame` (absolute frame index),
    /// in MIDI. `None` outside the blob.
    pub fn target_midi(&self, frame: usize) -> Option<f64> {
        if frame < self.start_frame || frame > self.end_frame {
            return None;
        }
        let i = frame - self.start_frame;
        Some(
            self.center_midi
                + self.drift[i] * self.drift_amount
                + self.modulation[i] * self.modulation_amount,
        )
    }

    /// Transpose the blob by `semitones` (drag).
    pub fn transpose(&mut self, semitones: f64) {
        self.center_midi += semitones;
    }

    /// The user's pitch edit relative to analysis (for delta storage).
    pub fn center_delta(&self) -> f64 {
        self.center_midi - self.analyzed_center_midi
    }
}

/// Zero-phase one-pole low-pass (forward + backward pass) over a
/// frame-rate signal — extracts the drift term without phase lag so
/// the vibrato residual stays time-aligned.
fn zero_phase_lowpass(x: &[f64], cutoff_hz: f64, frame_rate: f64) -> Vec<f64> {
    if x.is_empty() {
        return Vec::new();
    }
    let coeff = 1.0 - (-core::f64::consts::TAU * cutoff_hz / frame_rate).exp();
    let mut fwd = Vec::with_capacity(x.len());
    let mut state = x[0];
    for &v in x {
        state += (v - state) * coeff;
        fwd.push(state);
    }
    let mut state = *fwd.last().unwrap();
    let mut out = vec![0.0; x.len()];
    for i in (0..x.len()).rev() {
        state += (fwd[i] - state) * coeff;
        out[i] = state;
    }
    out
}

/// Drift cutoff: below this is "sliding", above is "vibrato".
/// Typical vibrato sits at 4–7 Hz; drift well under 2 Hz.
const DRIFT_CUTOFF_HZ: f64 = 3.0;

/// Decompose a tracked note into an editable blob.
///
/// `hop`/`sample_rate` give the analysis frame rate for the
/// drift/vibrato split.
pub fn decompose(note: &TrackedNote, hop: usize, sample_rate: f64) -> NoteBlob {
    let center = note.median_midi;
    let residual: Vec<f64> = note.f0.iter().map(|&hz| hz_to_midi(hz) - center).collect();
    let frame_rate = sample_rate / hop.max(1) as f64;
    let drift = zero_phase_lowpass(&residual, DRIFT_CUTOFF_HZ, frame_rate);
    let modulation: Vec<f64> = residual.iter().zip(&drift).map(|(r, d)| r - d).collect();
    NoteBlob {
        start_frame: note.start_frame,
        end_frame: note.end_frame,
        center_midi: center,
        analyzed_center_midi: center,
        drift,
        modulation,
        drift_amount: 1.0,
        modulation_amount: 1.0,
        formant_shift: 0.0,
        gain_db: 0.0,
        retune_s: 0.0,
        rms: 0.0,
    }
}

/// Melonix-style warp/bend marker (sample-anchored).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpMarker {
    /// Anchor position in ORIGINAL samples.
    pub sample: f64,
    /// Time offset in samples applied at the anchor (piecewise-linear
    /// between markers, 0 outside the marker range).
    pub d_time: f64,
    /// Extra pitch bend in semitones at the anchor (same interpolation).
    pub pitch_bend: f64,
}

/// The per-clip pitch document: blobs + warp markers.
#[derive(Clone, Debug, Default)]
pub struct PitchDoc {
    pub blobs: Vec<NoteBlob>,
    /// Sorted by `sample`.
    pub markers: Vec<WarpMarker>,
    /// Analysis hop (samples) — target curves are frame-indexed.
    pub hop: usize,
    pub sample_rate: f64,
}

impl PitchDoc {
    /// Build from tracked notes.
    pub fn from_notes(notes: &[TrackedNote], hop: usize, sample_rate: f64) -> Self {
        Self {
            blobs: notes
                .iter()
                .map(|n| decompose(n, hop, sample_rate))
                .collect(),
            markers: Vec::new(),
            hop,
            sample_rate,
        }
    }

    /// Interpolate the marker pitch-bend curve at an original-sample
    /// position (semitones; 0 outside the marker range).
    pub fn bend_at(&self, sample: f64) -> f64 {
        interp_markers(&self.markers, sample, |m| m.pitch_bend)
    }

    /// Warped time offset at an original-sample position (samples).
    pub fn warp_at(&self, sample: f64) -> f64 {
        interp_markers(&self.markers, sample, |m| m.d_time)
    }

    /// Insert a marker keeping `markers` sorted.
    pub fn add_marker(&mut self, m: WarpMarker) {
        let idx = self.markers.partition_point(|x| x.sample < m.sample);
        self.markers.insert(idx, m);
    }

    /// The edited TARGET pitch contour per analysis frame across the
    /// whole clip: blob targets + marker bend; `None` where no blob is
    /// active (unvoiced — leave untouched). This is what both the
    /// realtime preview shifter and the offline renderer consume.
    pub fn target_curve(&self, n_frames: usize) -> Vec<Option<f64>> {
        let mut out = vec![None; n_frames];
        for blob in &self.blobs {
            let end = blob.end_frame.min(n_frames.saturating_sub(1));
            for (frame, slot) in out
                .iter_mut()
                .enumerate()
                .take(end + 1)
                .skip(blob.start_frame)
            {
                let sample = frame as f64 * self.hop as f64;
                *slot = blob.target_midi(frame).map(|m| m + self.bend_at(sample));
            }
        }
        out
    }

    /// Per-frame shift ratios versus the analyzed contour — feed to a
    /// pitch shifter for preview (`1.0` where inactive/unvoiced).
    pub fn shift_ratios(&self, analyzed_f0_hz: &[Option<f64>]) -> Vec<f64> {
        let targets = self.target_curve(analyzed_f0_hz.len());
        analyzed_f0_hz
            .iter()
            .zip(&targets)
            .map(|(f0, t)| match (f0, t) {
                (Some(hz), Some(target_midi)) if *hz > 0.0 => midi_to_hz(*target_midi) / hz,
                _ => 1.0,
            })
            .collect()
    }
}

fn interp_markers<F: Fn(&WarpMarker) -> f64>(markers: &[WarpMarker], sample: f64, field: F) -> f64 {
    if markers.is_empty() {
        return 0.0;
    }
    if sample <= markers[0].sample {
        return field(&markers[0]);
    }
    if sample >= markers[markers.len() - 1].sample {
        return field(&markers[markers.len() - 1]);
    }
    let idx = markers.partition_point(|m| m.sample <= sample);
    let (a, b) = (&markers[idx - 1], &markers[idx]);
    let span = (b.sample - a.sample).max(1.0e-9);
    let t = (sample - a.sample) / span;
    field(a) + (field(b) - field(a)) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::midi_to_hz;

    /// Synthesize a tracked note: center + slow drift + 5.5 Hz vibrato.
    fn synth_note(center_midi: f64, n_frames: usize, frame_rate: f64) -> TrackedNote {
        let f0: Vec<f64> = (0..n_frames)
            .map(|i| {
                let t = i as f64 / frame_rate;
                let drift = -0.4 * (t / 2.0).min(1.0); // slides 40 cents flat
                let vibrato = 0.3 * (core::f64::consts::TAU * 5.5 * t).sin();
                midi_to_hz(center_midi + drift + vibrato)
            })
            .collect();
        TrackedNote {
            start_frame: 10,
            end_frame: 10 + n_frames - 1,
            f0,
            median_midi: center_midi - 0.2, // median sits mid-drift
        }
    }

    const FRAME_RATE: f64 = 93.75; // 48k / 512

    #[test]
    fn decompose_separates_drift_from_vibrato() {
        let note = synth_note(60.0, 200, FRAME_RATE);
        let blob = decompose(&note, 512, 48_000.0);
        // Drift is slow: it should capture the slide, not the vibrato.
        let drift_range = blob.drift.iter().cloned().fold(f64::MIN, f64::max)
            - blob.drift.iter().cloned().fold(f64::MAX, f64::min);
        assert!(
            drift_range > 0.25 && drift_range < 0.6,
            "drift captures the ~0.4 st slide: {drift_range:.3}"
        );
        // Vibrato RMS ≈ 0.3/√2 ≈ 0.21 semitones.
        let vib_rms = (blob.modulation.iter().map(|m| m * m).sum::<f64>()
            / blob.modulation.len() as f64)
            .sqrt();
        assert!(
            (0.12..=0.30).contains(&vib_rms),
            "modulation carries the vibrato: rms={vib_rms:.3}"
        );
        // Reconstruction: center + drift + modulation = analyzed contour.
        for (i, &hz) in note.f0.iter().enumerate() {
            let rebuilt = blob.center_midi + blob.drift[i] + blob.modulation[i];
            assert!(
                (rebuilt - crate::detect::hz_to_midi(hz)).abs() < 1.0e-9,
                "lossless decomposition at frame {i}"
            );
        }
    }

    #[test]
    fn flattening_drift_keeps_vibrato() {
        let note = synth_note(60.0, 200, FRAME_RATE);
        let mut blob = decompose(&note, 512, 48_000.0);
        blob.drift_amount = 0.0;
        // Late in the note (slide fully applied) the target must sit at
        // center ± vibrato, i.e. within ~0.45 st of center — not 0.4 st flat.
        let late = blob.target_midi(blob.end_frame).unwrap();
        assert!(
            (late - blob.center_midi).abs() < 0.45,
            "flattened drift pulls the tail to center: {late:.3} vs {:.3}",
            blob.center_midi
        );
        // Vibrato survives: contour still oscillates.
        let n = blob.len();
        let vals: Vec<f64> = (0..n)
            .map(|i| blob.target_midi(blob.start_frame + i).unwrap())
            .collect();
        let vib_rms = {
            let mean = vals.iter().sum::<f64>() / n as f64;
            (vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n as f64).sqrt()
        };
        assert!(
            vib_rms > 0.1,
            "vibrato survives drift flattening: {vib_rms:.3}"
        );
    }

    #[test]
    fn robotic_mode_is_flat() {
        let note = synth_note(60.0, 200, FRAME_RATE);
        let mut blob = decompose(&note, 512, 48_000.0);
        blob.drift_amount = 0.0;
        blob.modulation_amount = 0.0;
        for f in blob.start_frame..=blob.end_frame {
            assert!(
                (blob.target_midi(f).unwrap() - blob.center_midi).abs() < 1.0e-9,
                "fully flattened = constant center"
            );
        }
    }

    #[test]
    fn transpose_and_delta() {
        let note = synth_note(60.0, 100, FRAME_RATE);
        let mut blob = decompose(&note, 512, 48_000.0);
        blob.transpose(2.0);
        assert!((blob.center_delta() - 2.0).abs() < 1.0e-12);
        let mid = blob.target_midi(blob.start_frame + 50).unwrap();
        assert!(mid > 60.5, "transposed contour moved up: {mid:.2}");
    }

    #[test]
    fn markers_interpolate_and_shift_ratios_apply() {
        let note = synth_note(69.0, 100, FRAME_RATE); // A4 region
        let doc = {
            let mut d = PitchDoc::from_notes(core::slice::from_ref(&note), 512, 48_000.0);
            d.add_marker(WarpMarker {
                sample: 0.0,
                d_time: 0.0,
                pitch_bend: 0.0,
            });
            d.add_marker(WarpMarker {
                sample: 100.0 * 512.0,
                d_time: 0.0,
                pitch_bend: 1.0,
            });
            d
        };
        // Bend halfway through ≈ 0.5 st.
        let mid_bend = doc.bend_at(50.0 * 512.0);
        assert!(
            (mid_bend - 0.5).abs() < 0.02,
            "linear bend interp: {mid_bend:.3}"
        );

        // Shift ratios: at a frame where analysis == target(+bend), the
        // ratio reflects only the bend.
        let analyzed: Vec<Option<f64>> = (0..120)
            .map(|i| {
                if (10..110).contains(&i) {
                    Some(note.f0[i - 10])
                } else {
                    None
                }
            })
            .collect();
        let ratios = doc.shift_ratios(&analyzed);
        assert!(
            (ratios[0] - 1.0).abs() < 1.0e-12,
            "unvoiced frames stay 1.0"
        );
        let r = ratios[60];
        assert!(
            r > 1.0 && r < 1.2,
            "mid-clip ratio carries the upward bend: {r:.4}"
        );
    }
}
