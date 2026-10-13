//! Time-stretch and pitch-shift: tempo and pitch changed independently,
//! in real time.
//!
//! Three things a DAW asks of this, all one engine:
//!
//! - **Tempo without pitch** — play at 80 % and the key stays put
//!   (`ratio` < 1, `transpose` 1).
//! - **Pitch without tempo** — an item up a fifth, same length
//!   (`ratio` 1, `transpose` 1.498).
//! - **Both** — any mix.
//!
//! (Varispeed — speed and pitch together, like tape — needs none of this:
//! it is resampling, and the host does it.)
//!
//! # How
//!
//! A phase vocoder: the signal is cut into overlapping windowed frames,
//! each frame's spectrum is re-phased so its partials run on smoothly at
//! the output's pace, and the frames are overlap-added back.
//!
//! - **Pull, not push.** A DAW's sources can be read anywhere, so each
//!   output frame reads its input straight from the [`Source`] at the
//!   position the time ratio puts it — no input buffering, and a
//!   [`Stretcher::seek`] primes the frames that overlap the first output
//!   sample from the source itself: output sample 0 *is* input frame
//!   `position`. No latency to compensate.
//! - **Phase advance measured, not estimated.** Each frame is analysed
//!   twice, one output hop apart in the *input*; the phase difference is
//!   exactly how far each partial turns in one output hop, whatever the
//!   stretch — so nothing accumulates (the approach of Signalsmith
//!   Stretch). Unwrapped against each bin's centre, it scales with the
//!   transposition.
//! - **Phase locking.** Bins around a spectral peak keep their phases
//!   relative to it (Laroche & Dolson's identity locking), which is most of
//!   the difference between a clear stretch and a phasey one.
//! - **Pitch in the spectrum.** Output bin `k` takes the input at `k / α`
//!   (`α` the transposition factor), with the phase advance scaled by `α`.
//! - **Stereo stays stereo.** Phases are driven from the mid (L + R); each
//!   channel keeps its input phase offset from the mid, so the image does
//!   not smear.
//! - **Transients stay sharp.** A jump in spectral energy (an onset) resets
//!   the phases to the input's for that frame.
//!
//! Allocation happens in [`Stretcher::new`] only.

use std::sync::Arc;

use dsp_core::num::{count_to_f32, count_to_f64, f32_to_index, f64_to_index, trunc_to_i64};
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Where the stretcher reads its input: any stretch of frames, by index —
/// zeros wherever the source has nothing (before its start, past its end).
pub trait Source {
    /// Fill `left` and `right` with frames `start .. start + left.len()`.
    fn read(&mut self, start: i64, left: &mut [f32], right: &mut [f32]);
}

/// Frame size and overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Frame (FFT) length, samples. About 85 ms: long enough to resolve
    /// low notes, short enough not to smear rhythm.
    pub frame: usize,
    /// Frames per frame length (4 = 75 % overlap).
    pub overlap: usize,
}

impl Config {
    /// The frame for a sample rate: the power of two nearest ~85 ms.
    #[must_use]
    pub fn for_sample_rate(sample_rate: f64) -> Self {
        let target = f64_to_index(sample_rate * 0.085 / core::f64::consts::SQRT_2);
        Self { frame: target.next_power_of_two().max(256), overlap: 4 }
    }
}

/// How far an onset must lift the spectrum (the share of the frame's
/// energy that is new) before phases are reset to the input's.
const ONSET: f32 = 0.35;

/// Below this (relative to the frame's loudest bin) a bin is not a peak.
const PEAK_FLOOR: f32 = 1e-4;

/// One analysis bin: the mid's phase and its advance over one hop, each
/// channel's offset from the mid, and the magnitudes.
#[derive(Clone, Copy, Default)]
struct In {
    phase: f32,
    advance: f32,
    offset: [f32; 2],
    mag: [f32; 2],
    mid_mag: f32,
}

/// One output bin.
#[derive(Clone, Copy, Default)]
struct Out {
    mag: [f32; 2],
    mid_mag: f32,
    /// The mid's phase this frame, and last frame's.
    phase: f32,
    last: f32,
    /// The input bin it takes its phase from (pitch mapping).
    from: u32,
    /// The peak whose phase it keeps its offset from.
    owner: u32,
}

/// One stereo voice of the stretcher.
pub struct Stretcher {
    n: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    fft_scratch: Vec<Complex32>,
    ifft_scratch: Vec<Complex32>,
    window: Vec<f32>,
    /// Time-domain frame being transformed.
    time: Vec<f32>,
    /// Frames read from the source, per channel: now, and one hop before.
    read: [Vec<f32>; 2],
    read_before: [Vec<f32>; 2],
    /// Input spectra: now, and one hop before, per channel.
    now: [Vec<Complex32>; 2],
    before: [Vec<Complex32>; 2],
    bins_in: Vec<In>,
    bins_out: Vec<Out>,
    peaks: Vec<u32>,
    spectrum: Vec<Complex32>,
    /// Overlap-add accumulator: output times [c − n/2, c + n/2) for the
    /// next frame centred at c.
    ola: [Vec<f32>; 2],
    /// Finished output waiting to be taken (one hop's worth).
    ready: [Vec<f32>; 2],
    ready_at: usize,
    /// The next frame's centre, in input frames.
    centre: f64,
    /// Phases continue from the last frame (false: take the input's).
    continuing: bool,
    /// The last frame reset its phases (an onset): not twice in a row.
    just_reset: bool,
    ratio: f64,
    transpose: f32,
}

impl Stretcher {
    /// A stretcher for `config` — every buffer it will need, sized now.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let n = config.frame.max(64);
        let hop = n.checked_div(config.overlap.max(2)).unwrap_or(n).max(1);
        let bins = (n >> 1).saturating_add(1);
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let fft_scratch = fft.make_scratch_vec();
        let ifft_scratch = ifft.make_scratch_vec();
        // Periodic Hann: analysis × synthesis overlap-adds to a constant
        // at 75 %.
        let window = (0..n)
            .map(|i| 0.5f32.mul_add(-(core::f32::consts::TAU * count_to_f32(i) / count_to_f32(n)).cos(), 0.5))
            .collect();
        let zeros = |len: usize| vec![0.0f32; len];
        let czeros = |len: usize| vec![Complex32::new(0.0, 0.0); len];
        Self {
            n,
            hop,
            fft,
            ifft,
            fft_scratch,
            ifft_scratch,
            window,
            time: zeros(n),
            read: [zeros(n), zeros(n)],
            read_before: [zeros(n), zeros(n)],
            now: [czeros(bins), czeros(bins)],
            before: [czeros(bins), czeros(bins)],
            bins_in: vec![In::default(); bins],
            bins_out: vec![Out::default(); bins],
            peaks: Vec::with_capacity(bins),
            spectrum: czeros(bins),
            ola: [zeros(n), zeros(n)],
            ready: [zeros(hop), zeros(hop)],
            ready_at: hop,
            centre: 0.0,
            continuing: false,
            just_reset: false,
            ratio: 1.0,
            transpose: 1.0,
        }
    }

    /// Frame length, samples.
    #[must_use]
    pub const fn frame(&self) -> usize {
        self.n
    }

    /// Start (again) so that the next output sample is input frame
    /// `position`, running at `ratio` input frames per output frame and
    /// `transpose` (frequency factor). Reads the source for the frames that
    /// overlap the first output — what makes the output line up with the
    /// input with no latency.
    pub fn seek(&mut self, position: f64, ratio: f64, transpose: f32, source: &mut impl Source) {
        self.ratio = ratio;
        self.transpose = transpose;
        for ch in &mut self.ola {
            ch.fill(0.0);
        }
        self.continuing = false;
        self.just_reset = false;
        // The frames overlapping output [0, hop) are centred at output
        // times from n/2 back by whole hops; the ones before n/2 only fill
        // the accumulator, their finished hops lying before time 0.
        let earlier = (self.n >> 1).checked_div(self.hop).unwrap_or(1).saturating_sub(1);
        let hop = count_to_f64(self.hop);
        let mut centre_out = count_to_f64(self.n >> 1) - count_to_f64(earlier.saturating_add(1)) * hop;
        for _ in 0..=earlier {
            self.centre = centre_out.mul_add(ratio, position);
            self.frame_out(source);
            centre_out += hop;
        }
        self.centre = centre_out.mul_add(ratio, position);
        // Nothing ready: the next frame yields output 0..hop.
        self.ready_at = self.hop;
    }

    /// Render `left.len()` output frames at `ratio` input frames per output
    /// frame and `transpose` (1.0 = none; 2^(semitones/12)). Changes take
    /// effect from the next frame (a hop — ~20 ms — away).
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32], ratio: f64, transpose: f32, source: &mut impl Source) {
        self.ratio = ratio;
        self.transpose = transpose;
        let mut outputs = left.iter_mut().zip(right.iter_mut());
        loop {
            if self.ready_at >= self.hop {
                self.frame_out(source);
                self.centre = count_to_f64(self.hop).mul_add(self.ratio, self.centre);
                self.ready_at = 0;
            }
            let [ready_l, ready_r] = &self.ready;
            let (Some(rl), Some(rr)) = (ready_l.get(self.ready_at..), ready_r.get(self.ready_at..)) else {
                return;
            };
            let mut taken = 0usize;
            // The ready hop first: a zip pulls from its left side before
            // its right, and an output slot pulled after the hop ran dry
            // would be skipped.
            for ((&a, &b), (l, r)) in rl.iter().zip(rr).zip(outputs.by_ref()) {
                *l = a;
                *r = b;
                taken = taken.saturating_add(1);
            }
            self.ready_at = self.ready_at.saturating_add(taken);
            if self.ready_at < self.hop {
                // The output ran out before the ready hop did.
                return;
            }
        }
    }

    /// Analyse, re-phase and overlap-add the frame centred at
    /// `self.centre`; move the hop it finishes into `ready`.
    fn frame_out(&mut self, source: &mut impl Source) {
        let onset = self.analyse_frame(source);
        self.map_bins();
        self.rephase(onset);
        self.synthesise();
    }

    /// Read and transform the frame and the one a hop before it; fill the
    /// input bins. Returns whether the frame starts an onset.
    fn analyse_frame(&mut self, source: &mut impl Source) -> bool {
        let Self { n, hop, fft, fft_scratch, window, time, read, read_before, now, before, bins_in, centre, continuing, just_reset, .. } =
            self;
        let (n, hop) = (*n, *hop);
        let half = i64::try_from(n >> 1).unwrap_or(0);
        let hop_i = i64::try_from(hop).unwrap_or(0);
        let start = trunc_to_i64(centre.round()).saturating_sub(half);
        {
            let [l, r] = read;
            source.read(start, l, r);
            let [l, r] = read_before;
            source.read(start.saturating_sub(hop_i), l, r);
        }
        for ch in 0..2 {
            let (Some(src), Some(src_before), Some(spec), Some(spec_before)) =
                (read.get(ch), read_before.get(ch), now.get_mut(ch), before.get_mut(ch))
            else {
                continue;
            };
            analyse(&**fft, fft_scratch, window, time, src, spec);
            analyse(&**fft, fft_scratch, window, time, src_before, spec_before);
        }

        // The mid: phases, advances, each channel's offset from it; and
        // how much of the frame's energy is new (an onset).
        let expected = core::f32::consts::TAU * count_to_f32(hop) / count_to_f32(n);
        let (mut flux, mut energy) = (0.0f32, 0.0f32);
        let [now_l, now_r] = &*now;
        let [before_l, before_r] = &*before;
        for (k, (bin, ((&l, &r), (&bl, &br)))) in bins_in
            .iter_mut()
            .zip(now_l.iter().zip(now_r).zip(before_l.iter().zip(before_r)))
            .enumerate()
        {
            let mid = l + r;
            let mid_before = bl + br;
            let phase = mid.arg();
            let centre_turn = expected * count_to_f32(k);
            bin.phase = phase;
            bin.advance = centre_turn + wrap(phase - mid_before.arg() - centre_turn);
            bin.offset = [wrap(l.arg() - phase), wrap(r.arg() - phase)];
            bin.mag = [l.norm(), r.norm()];
            bin.mid_mag = mid.norm();
            flux += (bin.mid_mag - mid_before.norm()).max(0.0);
            energy += bin.mid_mag;
        }
        *continuing && !*just_reset && energy > 1e-6 && flux / energy > ONSET
    }

    /// Map the input bins to output bins (the transposition), find the
    /// peaks, and give every bin its peak.
    fn map_bins(&mut self) {
        let Self { bins_in, bins_out, peaks, transpose, .. } = self;
        let alpha = *transpose;
        // Map to output bins (pitch), and find the loudest.
        let last_in = bins_in.len().saturating_sub(1);
        let mut loudest = 0.0f32;
        for (k, out) in bins_out.iter_mut().enumerate() {
            let at = count_to_f32(k) / alpha;
            let i = f32_to_index(at);
            let frac = at - count_to_f32(i);
            let (lo, hi) = (bins_in.get(i), bins_in.get(i.saturating_add(1)));
            let (m, ml, mr) = match (lo, hi) {
                (Some(lo), Some(hi)) if i < last_in => {
                    let lerp = |a: f32, b: f32| (b - a).mul_add(frac, a);
                    (lerp(lo.mid_mag, hi.mid_mag), lerp(lo.mag[0], hi.mag[0]), lerp(lo.mag[1], hi.mag[1]))
                }
                _ => (0.0, 0.0, 0.0),
            };
            out.mid_mag = m;
            out.mag = [ml, mr];
            out.from = u32::try_from(f32_to_index(at.round()).min(last_in)).unwrap_or(0);
            loudest = loudest.max(m);
        }

        // Peaks, and which peak each bin belongs to: the one on its side of
        // the lowest point between two peaks.
        peaks.clear();
        let floor = loudest * PEAK_FLOOR;
        for (k, w) in bins_out.windows(3).enumerate() {
            if let [a, b, c] = w
                && b.mid_mag > floor
                && b.mid_mag > a.mid_mag
                && b.mid_mag >= c.mid_mag
            {
                peaks.push(u32::try_from(k.saturating_add(1)).unwrap_or(0));
            }
        }
        if peaks.is_empty() {
            for (k, out) in bins_out.iter_mut().enumerate() {
                out.owner = u32::try_from(k).unwrap_or(0);
            }
        } else {
            let mut from = 0usize;
            for (w, &p) in peaks.iter().enumerate() {
                let p = usize::try_from(p).unwrap_or(0);
                let to = match peaks.get(w.saturating_add(1)) {
                    Some(&q) => {
                        let q = usize::try_from(q).unwrap_or(p);
                        bins_out
                            .get(p..=q)
                            .and_then(|span| {
                                span.iter()
                                    .enumerate()
                                    .min_by(|a, b| a.1.mid_mag.total_cmp(&b.1.mid_mag))
                                    .map(|(i, _)| p.saturating_add(i))
                            })
                            .unwrap_or(p)
                    }
                    None => bins_out.len().saturating_sub(1),
                };
                if let Some(span) = bins_out.get_mut(from..=to) {
                    let owner = u32::try_from(p).unwrap_or(0);
                    for out in span {
                        out.owner = owner;
                    }
                }
                from = to.saturating_add(1);
            }
        }

    }

    /// The mid's output phases for this frame.
    fn rephase(&mut self, onset: bool) {
        let Self { bins_in, bins_out, peaks, continuing, just_reset, transpose, .. } = self;
        let alpha = *transpose;
        // The mid's output phases: an onset or a fresh start takes the
        // input's; otherwise peaks turn by their (scaled) measured advance
        // and the bins they own keep their input offset from them.
        let input = |i: u32| bins_in.get(usize::try_from(i).unwrap_or(0)).copied().unwrap_or_default();
        if !*continuing || onset {
            for out in bins_out.iter_mut() {
                out.phase = input(out.from).phase;
            }
        } else if peaks.is_empty() {
            for out in bins_out.iter_mut() {
                out.phase = wrap(alpha.mul_add(input(out.from).advance, out.last));
            }
        } else {
            for &p in peaks.iter() {
                if let Some(out) = bins_out.get_mut(usize::try_from(p).unwrap_or(0)) {
                    out.phase = wrap(alpha.mul_add(input(out.from).advance, out.last));
                }
            }
            for k in 0..bins_out.len() {
                let Some(&out) = bins_out.get(k) else { continue };
                let owner = usize::try_from(out.owner).unwrap_or(k);
                if owner == k {
                    continue;
                }
                let Some(&peak) = bins_out.get(owner) else { continue };
                let phase = wrap(peak.phase + (input(out.from).phase - input(peak.from).phase));
                if let Some(slot) = bins_out.get_mut(k) {
                    slot.phase = phase;
                }
            }
        }
        for out in bins_out.iter_mut() {
            out.last = out.phase;
        }
        *just_reset = onset;
        *continuing = true;

    }

    /// Each channel back to time, overlap-added; the finished hop out.
    fn synthesise(&mut self) {
        let Self { n, hop, ifft, ifft_scratch, window, time, bins_in, bins_out, spectrum, ola, ready, .. } = self;
        let (n, hop) = (*n, *hop);
        let input = |i: u32| bins_in.get(usize::try_from(i).unwrap_or(0)).copied().unwrap_or_default();
        // Each channel: its magnitude with the mid's phase plus its own
        // offset; back to time, windowed, overlap-added.
        let norm = 1.0 / (count_to_f32(n) * window_gain(n, hop));
        for (ch, acc) in ola.iter_mut().enumerate() {
            for (s, out) in spectrum.iter_mut().zip(bins_out.iter()) {
                let offset = input(out.from).offset.get(ch).copied().unwrap_or(0.0);
                let mag = out.mag.get(ch).copied().unwrap_or(0.0);
                *s = Complex32::from_polar(mag, out.phase + offset);
            }
            // A real signal: DC and Nyquist carry no phase.
            if let Some(dc) = spectrum.first_mut() {
                dc.im = 0.0;
            }
            if let Some(nyquist) = spectrum.last_mut() {
                nyquist.im = 0.0;
            }
            if ifft.process_with_scratch(spectrum, time, ifft_scratch).is_err() {
                time.fill(0.0);
            }
            for (a, (&x, &w)) in acc.iter_mut().zip(time.iter().zip(window.iter())) {
                *a = (x * w).mul_add(norm, *a);
            }
        }
        // The first hop is finished: out, and the accumulator moves on.
        for (acc, done) in ola.iter_mut().zip(ready.iter_mut()) {
            if let Some(first) = acc.get(..hop) {
                done.copy_from_slice(first);
            }
            acc.copy_within(hop.., 0);
            if let Some(tail) = acc.get_mut(n.saturating_sub(hop)..) {
                tail.fill(0.0);
            }
        }
    }
}

/// What analysis × synthesis Hann windows overlap-add to at this overlap
/// (Σ w² over a period is 3/8 per frame): the output is divided by it.
fn window_gain(n: usize, hop: usize) -> f32 {
    0.375 * count_to_f32(n.checked_div(hop).unwrap_or(1))
}

fn analyse(
    fft: &dyn RealToComplex<f32>,
    scratch: &mut [Complex32],
    window: &[f32],
    time: &mut [f32],
    input: &[f32],
    out: &mut [Complex32],
) {
    for ((t, &x), &w) in time.iter_mut().zip(input).zip(window) {
        *t = x * w;
    }
    if fft.process_with_scratch(time, out, scratch).is_err() {
        out.fill(Complex32::new(0.0, 0.0));
    }
}

/// Into (−π, π].
fn wrap(x: f32) -> f32 {
    use core::f32::consts::{PI, TAU};
    let y = TAU.mul_add(-(x / TAU).round(), x);
    if y <= -PI { y + TAU } else { y }
}
