//! Direct Note Access (DNA) kernel — polyphonic note *separation* and resynth.
//!
//! Transcription (Basic Pitch, our YIN) tells you *what* notes are present. DNA
//! needs more: it must pull each note's audio *apart* so a single note inside a
//! chord can be retuned/retimed/muted independently and the rest left untouched.
//! That is the piece Melodyne patents and no transcription library gives you.
//!
//! The approach here is classical harmonic-mask source separation over an STFT —
//! pure Rust, no model, so we own and can iterate it toward the full DNA arc
//! (later swapping the multipitch stage for a Basic-Pitch CNN, and the masks for
//! harmonic-NMF):
//!
//! 1. **STFT** — Hann-windowed, 75%-overlap analysis via [`realfft`].
//! 2. **Multipitch** — an iterative harmonic-sieve estimator picks up to `max`
//!    fundamentals per frame (dominant f0 → suppress its harmonics → repeat).
//! 3. **Harmonic masks** — each detected note claims spectral energy near its
//!    harmonics (Gaussian weight in log-frequency); masks are soft-normalised so
//!    overlapping partials are split proportionally instead of double-counted.
//! 4. **Per-note resynth** — inverse-STFT each masked spectrum with COLA overlap
//!    add → one isolated audio stream per note, independently editable.
//!
//! This is the *kernel*; a note-tracking layer (linking per-frame pitches into
//! sustained note objects across time) and per-note pitch/time editing build on
//! top. Everything is `f64`, allocation confined to construction.

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

use crate::detect::midi_to_hz;

/// STFT geometry + separation tuning.
#[derive(Clone, Copy, Debug)]
pub struct DnaConfig {
    /// FFT / analysis-window length in samples (power of two recommended).
    pub window: usize,
    /// Hop between frames in samples (window / 4 → 75% overlap).
    pub hop: usize,
    /// Max simultaneous notes the multipitch stage extracts per frame.
    pub max_notes: usize,
    /// Harmonics summed when scoring / masking a candidate f0.
    pub harmonics: usize,
    /// Mask width around each harmonic, in semitones (Gaussian sigma).
    pub mask_sigma_semitones: f64,
    /// Salience floor (relative to the frame's peak) below which no further
    /// note is accepted — stops the sieve inventing notes out of noise.
    pub salience_floor: f64,
}

impl Default for DnaConfig {
    fn default() -> Self {
        Self {
            window: 4096,
            hop: 1024,
            max_notes: 6,
            harmonics: 12,
            mask_sigma_semitones: 0.6,
            salience_floor: 0.08,
        }
    }
}

/// A separated note: its (frame-median) pitch and its isolated audio, aligned
/// sample-for-sample with the input buffer.
#[derive(Clone, Debug)]
pub struct SeparatedNote {
    /// Estimated fundamental, Hz (mean over the frames the note was active).
    pub f0_hz: f64,
    /// Isolated, resynthesised audio for this note (input length).
    pub audio: Vec<f64>,
}

/// A note laid on the STFT frame grid: active from `start_frame` for
/// `f0.len()` frames, with a per-frame fundamental (Hz). Produced by the note
/// tracker ([`crate::tracker`]) and consumed by [`DnaEngine::separate_spans`].
#[derive(Clone, Debug)]
pub struct NoteSpan {
    /// First active frame index.
    pub start_frame: usize,
    /// Per-frame fundamental in Hz (length = number of active frames).
    pub f0: Vec<f64>,
}

/// The DNA engine. Holds the FFT plans + window tables; reusable across buffers.
pub struct DnaEngine {
    cfg: DnaConfig,
    sample_rate: f64,
    fwd: Arc<dyn RealToComplex<f64>>,
    inv: Arc<dyn ComplexToReal<f64>>,
    window: Vec<f64>,
    /// COLA normalisation denominator (Σ window² over overlapping frames).
    n_bins: usize,
}

impl DnaEngine {
    /// Build an engine for a sample rate.
    pub fn new(sample_rate: f64, cfg: DnaConfig) -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let fwd = planner.plan_fft_forward(cfg.window);
        let inv = planner.plan_fft_inverse(cfg.window);
        let denom = (cfg.window.max(2) - 1) as f64;
        let window = (0..cfg.window)
            .map(|n| 0.5 * (1.0 - (core::f64::consts::TAU * n as f64 / denom).cos()))
            .collect();
        let n_bins = cfg.window / 2 + 1;
        Self {
            cfg,
            sample_rate: sample_rate.max(1.0),
            fwd,
            inv,
            window,
            n_bins,
        }
    }

    /// Frequency (Hz) of FFT bin `k`.
    #[inline]
    fn bin_hz(&self, k: usize) -> f64 {
        k as f64 * self.sample_rate / self.cfg.window as f64
    }

    /// Estimate up to `max_notes` fundamentals from a magnitude spectrum using
    /// an iterative harmonic sieve. Returns f0s in Hz, strongest first.
    pub fn estimate_pitches(&self, mag: &[f64]) -> Vec<f64> {
        // Candidate f0 grid: one bin per ~10 cents from 55 Hz to 1 kHz.
        let f_lo = 55.0_f64;
        let f_hi = (self.sample_rate * 0.45).min(1000.0);
        let mut residual = mag.to_vec();
        let peak = mag.iter().cloned().fold(0.0_f64, f64::max).max(1e-12);
        let mut out = Vec::new();

        for _ in 0..self.cfg.max_notes {
            // Score every candidate f0 by a 1/h-weighted harmonic sum on the
            // residual. The 1/h weighting plus a hard "energy must exist at the
            // fundamental" gate rejects subharmonic phantoms (an f0 an octave or
            // fifth *below* a real note whose harmonics happen to cover it).
            let mut best_f0 = 0.0;
            let mut best_score = 0.0;
            let mut f = f_lo;
            while f <= f_hi {
                let f0_bin = (f * self.cfg.window as f64 / self.sample_rate).round() as usize;
                let fundamental = residual.get(f0_bin).copied().unwrap_or(0.0);
                // A genuine note has real energy at its fundamental.
                if fundamental >= self.cfg.salience_floor * peak {
                    let mut score = 2.0 * fundamental; // weight the fundamental
                    for h in 2..=self.cfg.harmonics {
                        let bin = (h as f64 * f * self.cfg.window as f64 / self.sample_rate).round()
                            as usize;
                        if bin < residual.len() {
                            score += residual[bin] / h as f64;
                        }
                    }
                    if score > best_score {
                        best_score = score;
                        best_f0 = f;
                    }
                }
                f *= 2f64.powf(0.1 / 12.0); // ~10-cent steps
            }

            if best_f0 <= 0.0 {
                break;
            }
            out.push(best_f0);

            // Suppress this f0's harmonics in the residual so the next pass finds
            // a different note rather than an octave of the same one.
            for h in 1..=self.cfg.harmonics {
                let center = h as f64 * best_f0;
                let bw = center * (2f64.powf(self.cfg.mask_sigma_semitones / 12.0) - 1.0);
                let lo = ((center - 2.0 * bw) * self.cfg.window as f64 / self.sample_rate)
                    .floor()
                    .max(0.0) as usize;
                let hi = (((center + 2.0 * bw) * self.cfg.window as f64 / self.sample_rate).ceil()
                    as usize)
                    .min(residual.len() - 1);
                #[allow(clippy::needless_range_loop)]
                for bin in lo..=hi {
                    residual[bin] = 0.0;
                }
            }
        }
        out
    }

    /// Harmonic-mask weight of bin `k` for a note at `f0` (0..1, pre-normalise).
    fn harmonic_weight(&self, k: usize, f0: f64) -> f64 {
        if f0 <= 0.0 {
            return 0.0;
        }
        let f = self.bin_hz(k);
        if f < f0 * 0.5 {
            return 0.0;
        }
        // Distance to the nearest harmonic, in semitones.
        let ratio = f / f0;
        let nearest_h = ratio.round().max(1.0);
        let harmonic_f = nearest_h * f0;
        if nearest_h as usize > self.cfg.harmonics {
            return 0.0;
        }
        let semis = 12.0 * (f / harmonic_f).abs().log2();
        let s = self.cfg.mask_sigma_semitones;
        (-(semis * semis) / (2.0 * s * s)).exp()
    }

    /// Separate a mono buffer into per-note audio streams.
    ///
    /// `pitches` optionally forces the note set (Hz); pass `None` to run the
    /// built-in multipitch estimator on the whole-buffer average spectrum. The
    /// returned notes are ordered to match `pitches` when supplied.
    pub fn separate(&self, signal: &[f64], pitches: Option<&[f64]>) -> Vec<SeparatedNote> {
        let n = self.cfg.window;
        let hop = self.cfg.hop.max(1);
        if signal.len() < n {
            return Vec::new();
        }

        // Resolve the note set: explicit, or estimate from the mean spectrum.
        let notes: Vec<f64> = match pitches {
            Some(p) => p.to_vec(),
            None => {
                let mag = self.mean_magnitude(signal);
                self.estimate_pitches(&mag)
            }
        };
        if notes.is_empty() {
            return Vec::new();
        }

        // Per-note output accumulators + COLA normaliser.
        let mut outs = vec![vec![0.0f64; signal.len()]; notes.len()];
        let mut norm = vec![0.0f64; signal.len()];
        // Track summed f0 energy per note for the reported median pitch.
        let mut f0_energy = vec![0.0f64; notes.len()];

        let mut in_buf = self.fwd.make_input_vec();
        let mut spectrum = self.fwd.make_output_vec();
        let mut note_spec = self.inv.make_input_vec();
        let mut synth = self.inv.make_output_vec();

        let mut pos = 0usize;
        while pos + n <= signal.len() {
            // Windowed forward transform.
            for i in 0..n {
                in_buf[i] = signal[pos + i] * self.window[i];
            }
            self.fwd.process(&mut in_buf, &mut spectrum).ok();

            // Precompute per-bin note weights and their sum (soft assignment).
            for (ni, &f0) in notes.iter().enumerate() {
                for k in 0..self.n_bins {
                    let w_this = self.harmonic_weight(k, f0);
                    if w_this <= 0.0 {
                        note_spec[k] = Complex::new(0.0, 0.0);
                        continue;
                    }
                    // Normalise against all notes' weights at this bin.
                    let mut w_sum = 0.0;
                    for &g in &notes {
                        w_sum += self.harmonic_weight(k, g);
                    }
                    let mask = if w_sum > 1e-12 { w_this / w_sum } else { 0.0 };
                    note_spec[k] = spectrum[k] * mask;
                    f0_energy[ni] += (spectrum[k] * mask).norm_sqr();
                }

                // Inverse → windowed overlap-add.
                self.inv.process(&mut note_spec, &mut synth).ok();
                let scale = 1.0 / n as f64;
                for i in 0..n {
                    outs[ni][pos + i] += synth[i] * self.window[i] * scale;
                }
            }

            // COLA denominator (same for every note; Σ window²).
            for i in 0..n {
                norm[pos + i] += self.window[i] * self.window[i];
            }
            pos += hop;
        }

        // Normalise overlap-add and package.
        notes
            .iter()
            .enumerate()
            .map(|(ni, &f0)| {
                let mut audio = std::mem::take(&mut outs[ni]);
                for (s, &d) in audio.iter_mut().zip(norm.iter()) {
                    if d > 1e-9 {
                        *s /= d;
                    }
                }
                let _ = f0_energy[ni];
                SeparatedNote { f0_hz: f0, audio }
            })
            .collect()
    }

    /// STFT geometry accessors — the tracker needs these to map frame indices
    /// to time and to lay note spans on the same grid the separator uses.
    #[inline]
    pub fn window(&self) -> usize {
        self.cfg.window
    }
    /// Hop size in samples.
    #[inline]
    pub fn hop(&self) -> usize {
        self.cfg.hop.max(1)
    }
    /// Sample rate the engine runs at.
    #[inline]
    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
    /// Number of analysis frames a buffer of `len` samples produces.
    #[inline]
    pub fn frame_count(&self, len: usize) -> usize {
        let n = self.cfg.window;
        if len < n {
            0
        } else {
            (len - n) / self.hop() + 1
        }
    }

    /// Estimate the fundamentals present in *every* analysis frame. Frame `i`
    /// starts at sample `i * hop`. This is the front end the note tracker links
    /// into sustained notes over time.
    pub fn analyze_pitch_frames(&self, signal: &[f64]) -> Vec<Vec<f64>> {
        let n = self.cfg.window;
        let hop = self.hop();
        let mut in_buf = self.fwd.make_input_vec();
        let mut spectrum = self.fwd.make_output_vec();
        let mut mag = vec![0.0f64; self.n_bins];
        let mut out = Vec::new();
        let mut pos = 0;
        while pos + n <= signal.len() {
            for i in 0..n {
                in_buf[i] = signal[pos + i] * self.window[i];
            }
            self.fwd.process(&mut in_buf, &mut spectrum).ok();
            for (m, s) in mag.iter_mut().zip(spectrum.iter()) {
                *m = s.norm();
            }
            out.push(self.estimate_pitches(&mag));
            pos += hop;
        }
        out
    }

    /// Time-varying separation driven by note spans (from the tracker).
    ///
    /// Each [`NoteSpan`] is active over a contiguous range of frames with its
    /// own per-frame f0 (so vibrato and drift are followed). Within each frame,
    /// only the spans active *there* compete for spectral energy, so a note that
    /// has ended stops claiming bins — the key improvement over the static
    /// whole-buffer [`DnaEngine::separate`].
    pub fn separate_spans(&self, signal: &[f64], spans: &[NoteSpan]) -> Vec<SeparatedNote> {
        let n = self.cfg.window;
        let hop = self.hop();
        if signal.len() < n || spans.is_empty() {
            return Vec::new();
        }

        let mut outs = vec![vec![0.0f64; signal.len()]; spans.len()];
        let mut norm = vec![0.0f64; signal.len()];

        let mut in_buf = self.fwd.make_input_vec();
        let mut spectrum = self.fwd.make_output_vec();
        let mut note_spec = self.inv.make_input_vec();
        let mut synth = self.inv.make_output_vec();

        // Reused per-frame: which spans are active + their f0 this frame.
        let mut active: Vec<(usize, f64)> = Vec::with_capacity(spans.len());

        let n_frames = self.frame_count(signal.len());
        for fi in 0..n_frames {
            let pos = fi * hop;

            active.clear();
            for (si, span) in spans.iter().enumerate() {
                if fi >= span.start_frame && fi < span.start_frame + span.f0.len() {
                    active.push((si, span.f0[fi - span.start_frame]));
                }
            }
            if active.is_empty() {
                for i in 0..n {
                    norm[pos + i] += self.window[i] * self.window[i];
                }
                continue;
            }

            for i in 0..n {
                in_buf[i] = signal[pos + i] * self.window[i];
            }
            self.fwd.process(&mut in_buf, &mut spectrum).ok();

            for &(si, f0) in &active {
                for k in 0..self.n_bins {
                    let w_this = self.harmonic_weight(k, f0);
                    if w_this <= 0.0 {
                        note_spec[k] = Complex::new(0.0, 0.0);
                        continue;
                    }
                    let mut w_sum = 0.0;
                    for &(_, g) in &active {
                        w_sum += self.harmonic_weight(k, g);
                    }
                    let mask = if w_sum > 1e-12 { w_this / w_sum } else { 0.0 };
                    note_spec[k] = spectrum[k] * mask;
                }
                self.inv.process(&mut note_spec, &mut synth).ok();
                let scale = 1.0 / n as f64;
                for i in 0..n {
                    outs[si][pos + i] += synth[i] * self.window[i] * scale;
                }
            }

            for i in 0..n {
                norm[pos + i] += self.window[i] * self.window[i];
            }
        }

        spans
            .iter()
            .enumerate()
            .map(|(si, span)| {
                let mut audio = std::mem::take(&mut outs[si]);
                for (s, &d) in audio.iter_mut().zip(norm.iter()) {
                    if d > 1e-9 {
                        *s /= d;
                    }
                }
                let mid = span.f0.len() / 2;
                SeparatedNote {
                    f0_hz: span.f0.get(mid).copied().unwrap_or(0.0),
                    audio,
                }
            })
            .collect()
    }

    /// Mean magnitude spectrum across all analysis frames (for whole-buffer
    /// pitch estimation).
    pub fn mean_magnitude(&self, signal: &[f64]) -> Vec<f64> {
        let n = self.cfg.window;
        let hop = self.cfg.hop.max(1);
        let mut in_buf = self.fwd.make_input_vec();
        let mut spectrum = self.fwd.make_output_vec();
        let mut acc = vec![0.0f64; self.n_bins];
        let mut frames = 0usize;
        let mut pos = 0;
        while pos + n <= signal.len() {
            for i in 0..n {
                in_buf[i] = signal[pos + i] * self.window[i];
            }
            self.fwd.process(&mut in_buf, &mut spectrum).ok();
            for (a, s) in acc.iter_mut().zip(spectrum.iter()) {
                *a += s.norm();
            }
            frames += 1;
            pos += hop;
        }
        if frames > 0 {
            for a in &mut acc {
                *a /= frames as f64;
            }
        }
        acc
    }

    /// Convenience: separate by MIDI note numbers instead of Hz.
    pub fn separate_midi(&self, signal: &[f64], midi: &[f64]) -> Vec<SeparatedNote> {
        let hz: Vec<f64> = midi.iter().map(|&m| midi_to_hz(m)).collect();
        self.separate(signal, Some(&hz))
    }
}
