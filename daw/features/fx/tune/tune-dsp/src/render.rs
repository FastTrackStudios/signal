//! Committed (offline) render: [`crate::model::PitchDoc`] edits →
//! WORLD resynthesis.
//!
//! WORLD analyzes once (`f0[]`, spectral envelope, aperiodicity); the
//! edit render REWRITES the f0 contour from the blob targets and
//! resynthesizes — formants stay untouched by construction, which is
//! why this is the quality tier (the realtime preview shifts with
//! PSOLA instead). WORLD's frame grid (~5 ms) differs from the
//! analysis hop grid; frames map through time.

use crate::detect::midi_to_hz;
use crate::model::PitchDoc;
use world_sys::WorldAnalysis;

/// Render the edited document over the original mono audio.
///
/// Frames whose time falls inside an edited blob take the blob's
/// target contour (+ marker bend); everything else keeps the WORLD
/// analysis f0 (unvoiced consonants and unedited regions pass
/// through untouched).
pub fn render_world(doc: &PitchDoc, audio: &[f64], sample_rate: u32) -> Vec<f64> {
    let analysis = WorldAnalysis::analyze(audio, sample_rate, 5.0);
    let mut f0 = analysis.f0.clone();
    for (i, slot) in f0.iter_mut().enumerate() {
        if *slot <= 0.0 {
            continue; // unvoiced — leave it alone
        }
        let t = analysis.temporal_positions[i];
        let sample = t * sample_rate as f64;
        let frame = (sample / doc.hop.max(1) as f64) as usize;
        for blob in &doc.blobs {
            if frame >= blob.start_frame && frame <= blob.end_frame {
                if let Some(target) = blob.target_midi(frame) {
                    *slot = midi_to_hz(target + doc.bend_at(sample));
                }
                break;
            }
        }
    }
    analysis.synthesize_with_f0(&f0)
}

/// Render with the document's TIME-WARP applied (melonix marker
/// model): output frame k at time T reads the analysis frame at the
/// source position where `source_time + warp(source) = T` — WORLD
/// synthesis accepts frames at any rate, so time-stretch is pure
/// frame-index remapping (pitch and formants unaffected).
pub fn render_world_warped(doc: &PitchDoc, audio: &[f64], sample_rate: u32) -> Vec<f64> {
    let analysis = WorldAnalysis::analyze(audio, sample_rate, 5.0);
    let n_src = analysis.frames();
    if n_src == 0 {
        return Vec::new();
    }
    let sr = sample_rate as f64;
    let period_samples = 5.0 / 1000.0 * sr;

    // Edited f0 on the SOURCE grid first (same as render_world).
    let mut f0_src = analysis.f0.clone();
    for (i, slot) in f0_src.iter_mut().enumerate() {
        if *slot <= 0.0 {
            continue;
        }
        let sample = analysis.temporal_positions[i] * sr;
        let frame = (sample / doc.hop.max(1) as f64) as usize;
        for blob in &doc.blobs {
            if frame >= blob.start_frame && frame <= blob.end_frame {
                if let Some(target) = blob.target_midi(frame) {
                    *slot = midi_to_hz(target + doc.bend_at(sample));
                }
                break;
            }
        }
    }

    // Output timeline length: last source sample plus its warp offset.
    let src_end = (n_src - 1) as f64 * period_samples;
    let out_end = src_end + doc.warp_at(src_end);
    let n_out = ((out_end / period_samples) as usize).max(1);

    // Remap: for each output frame, invert the (monotonic) warp by
    // scanning — marker counts are tiny, a linear walk per frame is
    // fine at editing scale.
    let bins = analysis.bins();
    let mut f0 = Vec::with_capacity(n_out);
    let mut sp = Vec::with_capacity(n_out * bins);
    let mut ap = Vec::with_capacity(n_out * bins);
    for k in 0..n_out {
        let t_out = k as f64 * period_samples;
        // Solve src + warp(src) = t_out by bisection over [0, src_end].
        let (mut lo, mut hi) = (0.0f64, src_end);
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            if mid + doc.warp_at(mid) < t_out {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let src_sample = 0.5 * (lo + hi);
        let src_frame = ((src_sample / period_samples).round() as usize).min(n_src - 1);
        f0.push(f0_src[src_frame]);
        sp.extend_from_slice(&analysis.sp[src_frame * bins..(src_frame + 1) * bins]);
        ap.extend_from_slice(&analysis.ap[src_frame * bins..(src_frame + 1) * bins]);
    }
    let warped = WorldAnalysis {
        f0: f0.clone(),
        temporal_positions: (0..n_out).map(|k| k as f64 * period_samples / sr).collect(),
        sp,
        ap,
        fft_size: analysis.fft_size,
        frame_period_ms: analysis.frame_period_ms,
        sample_rate,
    };
    warped.synthesize_with_f0(&f0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{YinConfig, YinDetector};
    use crate::model::PitchDoc;
    use crate::tracker::TrackedNote;

    const SR: u32 = 48000;

    fn tone(buf: &[f64], freq: f64) -> f64 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &x) in buf.iter().enumerate() {
            let ph = core::f64::consts::TAU * freq * i as f64 / SR as f64;
            re += x * ph.cos();
            im += x * ph.sin();
        }
        (re * re + im * im).sqrt() / buf.len() as f64
    }

    #[test]
    fn transposed_blob_renders_at_the_new_pitch() {
        // A 220 Hz "voice" (pulse train + resonator), blob transposed
        // +3 st (→ ~261.6 Hz), rendered through WORLD.
        let n = 2 * SR as usize;
        let mut x = vec![0.0; n];
        let period = SR as f64 / 220.0;
        let mut next = 0.0f64;
        for (i, o) in x.iter_mut().enumerate() {
            if i as f64 >= next {
                *o = 1.0;
                next += period;
            }
        }
        let w = core::f64::consts::TAU * 900.0 / SR as f64;
        let (a1, a2) = (2.0 * 0.98 * w.cos(), -0.98f64 * 0.98);
        let (mut y1, mut y2) = (0.0f64, 0.0f64);
        for o in x.iter_mut() {
            let y = *o + a1 * y1 + a2 * y2;
            y2 = y1;
            y1 = y;
            *o = y * 0.05;
        }

        // Analyze with the house YIN just to build the doc (hop 512).
        let hop = 512usize;
        let mut det = YinDetector::new(SR as f64, YinConfig::default());
        let window = det.window();
        let mut f0 = Vec::new();
        let mut i = 0;
        while i + window <= n {
            f0.push(det.detect(&x[i..i + window]).f0_hz.unwrap_or(220.0));
            i += hop;
        }
        let note = TrackedNote {
            start_frame: 0,
            end_frame: f0.len() - 1,
            median_midi: 57.0, // A3 = 220 Hz
            f0,
        };
        let mut doc = PitchDoc::from_notes(&[note], hop, SR as f64);
        doc.blobs[0].transpose(3.0);
        // Robot mode so the render target is clean.
        doc.blobs[0].drift_amount = 0.0;
        doc.blobs[0].modulation_amount = 0.0;

        let y = render_world(&doc, &x, SR);
        let late = &y[SR as usize / 2..(3 * SR as usize) / 2];
        let e_new = tone(late, 261.6);
        let e_old = tone(late, 220.0);
        assert!(
            e_new > e_old * 2.0,
            "render lands on the transposed pitch: new={e_new:.5} old={e_old:.5}"
        );
        // The 900 Hz formant survives: nearest harmonic of 261.6 (3rd,
        // 785 Hz vs 4th, 1046) — envelope keeps ~900 region strong vs
        // where a resampler would move it (900·1.19 ≈ 1071).
        let e_h3 = tone(late, 261.6 * 3.0);
        let e_h5 = tone(late, 261.6 * 5.0);
        assert!(
            e_h3 > e_h5,
            "envelope still peaks near 900 Hz: h3={e_h3:.5} h5={e_h5:.5}"
        );
    }
}

#[cfg(test)]
mod warp_tests {
    use super::*;
    use crate::model::{PitchDoc, WarpMarker};
    use crate::tracker::TrackedNote;

    const SR: u32 = 48000;

    #[test]
    fn warp_markers_stretch_time_without_changing_pitch() {
        // 2 s of a 220 Hz vowel-like voice. WORLD needs a natural
        // source: D4C's voicing detector reads BOTH a pure sine and a
        // bare pulse train as aperiodic (verified — synthesis turns to
        // noise); a pulse train through formant resonators behaves
        // like the real vocals this renderer exists for.
        let n = 2 * SR as usize;
        let mut x = vec![0.0f64; n];
        let period = SR as f64 / 220.0;
        let mut next = 0.0f64;
        for (i, o) in x.iter_mut().enumerate() {
            if i as f64 >= next {
                *o = 0.6;
                next += period;
            }
        }
        for &(freq, r) in &[(700.0f64, 0.985f64), (1200.0, 0.985)] {
            let w = core::f64::consts::TAU * freq / SR as f64;
            let (a1, a2) = (2.0 * r * w.cos(), -r * r);
            let (mut y1, mut y2) = (0.0f64, 0.0f64);
            for o in x.iter_mut() {
                let y = *o + a1 * y1 + a2 * y2;
                y2 = y1;
                y1 = y;
                *o = y * 0.02;
            }
        }
        let hop = 512usize;
        let frames = n / hop;
        let note = TrackedNote {
            start_frame: 0,
            end_frame: frames - 1,
            median_midi: 57.0,
            f0: vec![220.0; frames],
        };
        let mut doc = PitchDoc::from_notes(&[note], hop, SR as f64);
        doc.blobs[0].drift_amount = 0.0;
        doc.blobs[0].modulation_amount = 0.0;
        doc.add_marker(WarpMarker {
            sample: 0.0,
            d_time: 0.0,
            pitch_bend: 0.0,
        });
        doc.add_marker(WarpMarker {
            sample: n as f64,
            d_time: n as f64 * 0.5, // +50% length at the end
            pitch_bend: 0.0,
        });

        let y = render_world_warped(&doc, &x, SR);
        // Length ≈ 3 s.
        let secs = y.len() as f64 / SR as f64;
        assert!(
            (2.7..=3.3).contains(&secs),
            "warped render is ~1.5x longer: {secs:.2} s"
        );
        // Pitch unchanged: 220 Hz fundamental dominates 330 (which a
        // naive 1.5x resample would have produced).
        let late = &y[y.len() / 3..2 * y.len() / 3];
        let tone = |freq: f64| -> f64 {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (i, &v) in late.iter().enumerate() {
                let ph = core::f64::consts::TAU * freq * i as f64 / SR as f64;
                re += v * ph.cos();
                im += v * ph.sin();
            }
            (re * re + im * im).sqrt() / late.len() as f64
        };
        let at_220 = tone(220.0);
        let at_330 = tone(330.0);
        assert!(
            at_220 > at_330 * 2.0,
            "time-stretch must not move pitch: e220={at_220:.5} e330={at_330:.5}"
        );
    }
}
