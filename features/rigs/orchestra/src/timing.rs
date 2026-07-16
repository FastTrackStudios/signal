//! Legato TIMING verification harness — click overlay, deterministic
//! arrival math, showcase corpus, and an onset-detection cross-check.
//!
//! The verification is layered (strongest first):
//!
//! 1. **Deterministic arrival (primary)** — no audio analysis. The annotated
//!    schedule + per-zone sample markers already encode when every note is
//!    HEARD: a legato transition zone's measured `lead_in_ms` is the
//!    in-sample arrival marker (time from sample start until the pitch
//!    leaves the source; the loop region beyond it is the settled
//!    destination note), and the engine's prefire alignment skips
//!    `start_offset` so that `fire_frame + (lead_in − offset)/rate` — the
//!    heard arrival — lands exactly on the note's grid tick. The engine
//!    reports that prediction per transition
//!    ([`signal_sampler::LegatoFireEvent::arrival`]); tests assert
//!    `arrival == grid` for EVERY note, sample-accurately (± a couple of
//!    frames of ms→frame rounding). Shorts carry the same contract as a
//!    constant: the schedule pre-rolls them by `short_note_timing.
//!    pre_delay_ms`, the marker distance to the rhythmic peak.
//! 2. **Audio onset cross-check** — spectral flux over the rendered audio
//!    validates the markers themselves (a wrong/missing `lead_in_ms` in the
//!    pack data would make prediction and acoustics disagree).
//! 3. **Ears** — the same tempo map drives a session-guide click render
//!    mixed over the music, so the owner can HEAR the grid.

use signal_sampler::SamplerRig;
use signal_sampler::document::{DocCc, DocNote, TempoPoint, TrackDocument, qn_to_sec};

// ── Tempo-map helpers ────────────────────────────────────────────────────────

/// Inverse of [`qn_to_sec`]: QN position at `sec`, integrating the
/// piecewise-constant tempo map (empty map = 120 BPM).
pub fn sec_to_qn(tempo: &[TempoPoint], sec: f64) -> f64 {
    let mut bpm = tempo.first().map(|t| t.bpm).unwrap_or(120.0);
    let mut seg_sec = 0.0;
    let mut seg_qn = 0.0;
    for t in tempo {
        if t.qn > seg_qn {
            let end_sec = seg_sec + (t.qn - seg_qn) * 60.0 / bpm;
            if end_sec > sec {
                break;
            }
            seg_sec = end_sec;
            seg_qn = t.qn;
        }
        bpm = t.bpm;
    }
    seg_qn + (sec - seg_sec) * bpm / 60.0
}

/// Tempo (BPM) in effect at `sec`.
pub fn bpm_at_sec(tempo: &[TempoPoint], sec: f64) -> f64 {
    let qn = sec_to_qn(tempo, sec);
    let mut bpm = tempo.first().map(|t| t.bpm).unwrap_or(120.0);
    for t in tempo {
        if t.qn > qn {
            break;
        }
        bpm = t.bpm;
    }
    bpm
}

// ── Click render (session-guide, same tempo map) ─────────────────────────────

/// Synthesized click: a short sine burst with exponential decay.
fn click_sample(freq: f64, dur_sec: f64, amp: f32, sr: u32) -> session_guide::AudioSample {
    let n = (dur_sec * sr as f64).round() as usize;
    let samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f64 / sr as f64;
            let env = (-t * 10.0 / dur_sec).exp() as f32;
            // Cosine phase: sample 0 is the peak, so the burst's very first
            // sample marks the beat (a sine's first sample would be 0).
            (2.0 * std::f64::consts::PI * freq * t).cos() as f32 * amp * env
        })
        .collect();
    session_guide::AudioSample::mono(samples, sr)
}

/// Optional count-in: `beats` count cues starting at `start_qn` (one per
/// quarter note) — schedule the music to start at `start_qn + beats`.
#[derive(Debug, Clone, Copy)]
pub struct CountIn {
    pub start_qn: f64,
    pub beats: u32,
}

/// Render a stereo (interleaved) click track of `total_frames` from `tempo`
/// — THE SAME map the document render uses — via the session-guide engine:
/// accented downbeats (4/4), plain quarter clicks elsewhere, plus an
/// optional count-in on its own voice. Sample-accurate: the guide engine
/// places each hit from the `BlockClock` beat grid this function derives
/// from the map (see `features/guide/tests/guide_engine.rs`).
pub fn render_click(
    tempo: &[TempoPoint],
    total_frames: usize,
    sr: u32,
    count_in: Option<CountIn>,
) -> Vec<f32> {
    let mut engine = session_guide::GuideEngine::new(session_guide::GuideConfig {
        enable_beat: true,
        enable_eighth: false,
        enable_sixteenth: false,
        enable_triplet: false,
        enable_measure_accent: true,
        enable_count: count_in.is_some(),
        enable_guide: false,
        ..Default::default()
    });
    engine.bank_mut().beat = Some(click_sample(1000.0, 0.045, 0.7, sr));
    engine.bank_mut().measure_accent = Some(click_sample(1600.0, 0.055, 0.85, sr));
    if let Some(ci) = count_in {
        let mut schedule = session_guide::CueSchedule::default();
        for i in 0..ci.beats.min(8) {
            engine.bank_mut().counts[i as usize] = Some(click_sample(760.0, 0.09, 0.8, sr));
            schedule.cues.push(session_guide::ScheduledCue {
                time_seconds: qn_to_sec(tempo, ci.start_qn + f64::from(i)),
                event: session_guide::CueEvent::Count { index: i as usize },
            });
        }
        engine.set_schedule(schedule);
    }

    const BLOCK: usize = 256;
    let mut out = vec![0.0f32; total_frames * 2];
    let mut cl = vec![0.0f32; BLOCK];
    let mut cr = vec![0.0f32; BLOCK];
    let mut nl = vec![0.0f32; BLOCK];
    let mut nr = vec![0.0f32; BLOCK];
    let mut gl = vec![0.0f32; BLOCK];
    let mut gr = vec![0.0f32; BLOCK];
    let mut start = 0usize;
    while start < total_frames {
        let n = BLOCK.min(total_frames - start);
        for b in [&mut cl, &mut cr, &mut nl, &mut nr, &mut gl, &mut gr] {
            b[..n].fill(0.0);
        }
        let pos_seconds = start as f64 / sr as f64;
        let clock = session_guide::BlockClock {
            playing: true,
            pos_seconds,
            pos_beats: sec_to_qn(tempo, pos_seconds),
            tempo_bpm: bpm_at_sec(tempo, pos_seconds),
            time_sig_num: 4,
            time_sig_den: 4,
            sample_rate: sr as f64,
        };
        let mut buses = session_guide::GuideBuses {
            click_l: &mut cl[..n],
            click_r: &mut cr[..n],
            count_l: &mut nl[..n],
            count_r: &mut nr[..n],
            guide_l: &mut gl[..n],
            guide_r: &mut gr[..n],
        };
        engine.render(&mut buses, &clock);
        for i in 0..n {
            out[(start + i) * 2] += cl[i] + nl[i];
            out[(start + i) * 2 + 1] += cr[i] + nr[i];
        }
        start += n;
    }
    out
}

/// Mix a click track over music (both interleaved stereo; output length =
/// the longer input). The click is gain-scaled and panned toward the right
/// (`pan` 0.5 = center, 1.0 = hard right) so it reads clearly against the
/// music.
pub fn mix_click(music: &[f32], click: &[f32], click_gain: f32, pan: f32) -> Vec<f32> {
    let frames = (music.len().max(click.len()) + 1) / 2;
    let (gl, gr) = (
        click_gain * (1.0 - pan).max(0.0) * 2.0,
        click_gain * pan.min(1.0) * 2.0,
    );
    let mut out = vec![0.0f32; frames * 2];
    for f in 0..frames {
        let (ml, mr) = (
            music.get(f * 2).copied().unwrap_or(0.0),
            music.get(f * 2 + 1).copied().unwrap_or(0.0),
        );
        let (cl, cr) = (
            click.get(f * 2).copied().unwrap_or(0.0),
            click.get(f * 2 + 1).copied().unwrap_or(0.0),
        );
        out[f * 2] = ml + cl * gl;
        out[f * 2 + 1] = mr + cr * gr;
    }
    out
}

// ── Onset detection (spectral flux) — the acoustic cross-check ───────────────

/// Spectral-flux curve over a mono mix-down of interleaved stereo audio.
///
/// STFT: 1024-sample Hann window, 128-sample hop (≈2.7 ms at 48 kHz).
/// `flux[i] = Σ_k max(0, |X_i[k]| − |X_{i-1}[k]|)` — energy AND pitch
/// changes both register (a legato transition moves energy between
/// harmonic bins even when the broadband level is continuous, which is why
/// plain energy-rise misses bowed joins). Frame `i` is stamped at the
/// window CENTER; the constant detector latency this leaves is calibrated
/// out by the caller against material with a known sharp attack (shorts).
pub struct FluxCurve {
    pub hop_sec: f64,
    /// Time (sec) of flux frame 0.
    pub t0: f64,
    pub v: Vec<f32>,
}

const FFT_N: usize = 1024;
const HOP: usize = 128;

fn fft_inplace(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (vr, vi) = (
                    re[i + k + len / 2] * cr - im[i + k + len / 2] * ci,
                    re[i + k + len / 2] * ci + im[i + k + len / 2] * cr,
                );
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Compute the spectral-flux curve of interleaved stereo `audio`.
pub fn spectral_flux(audio: &[f32], sr: u32) -> FluxCurve {
    let frames = audio.len() / 2;
    let mono: Vec<f64> = (0..frames)
        .map(|f| (audio[f * 2] as f64 + audio[f * 2 + 1] as f64) * 0.5)
        .collect();
    let hann: Vec<f64> = (0..FFT_N)
        .map(|i| {
            0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (FFT_N - 1) as f64).cos())
        })
        .collect();
    let n_hops = if frames >= FFT_N {
        (frames - FFT_N) / HOP + 1
    } else {
        0
    };
    let mut prev_mag = vec![0.0f64; FFT_N / 2];
    let mut v = Vec::with_capacity(n_hops);
    let mut re = vec![0.0f64; FFT_N];
    let mut im = vec![0.0f64; FFT_N];
    for h in 0..n_hops {
        let off = h * HOP;
        for i in 0..FFT_N {
            re[i] = mono[off + i] * hann[i];
            im[i] = 0.0;
        }
        fft_inplace(&mut re, &mut im);
        let mut flux = 0.0f64;
        for k in 1..FFT_N / 2 {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            flux += (mag - prev_mag[k]).max(0.0);
            prev_mag[k] = mag;
        }
        v.push(flux as f32);
    }
    FluxCurve {
        hop_sec: HOP as f64 / sr as f64,
        t0: (FFT_N / 2) as f64 / sr as f64,
        v,
    }
}

impl FluxCurve {
    /// Time (sec) of the strongest flux peak within `±search` of
    /// `expected`, refined by parabolic interpolation around the peak bin.
    /// `None` when the window is empty or flat (silence).
    pub fn onset_near(&self, expected: f64, search: f64) -> Option<f64> {
        let lo = (((expected - search - self.t0) / self.hop_sec).floor().max(0.0)) as usize;
        let hi =
            ((((expected + search - self.t0) / self.hop_sec).ceil()) as usize).min(self.v.len());
        if lo + 1 >= hi {
            return None;
        }
        let (mut best, mut best_v) = (lo, f32::MIN);
        for i in lo..hi {
            if self.v[i] > best_v {
                best_v = self.v[i];
                best = i;
            }
        }
        if best_v <= 0.0 {
            return None;
        }
        // Parabolic refinement.
        let mut idx = best as f64;
        if best > 0 && best + 1 < self.v.len() {
            let (a, b, c) = (
                self.v[best - 1] as f64,
                self.v[best] as f64,
                self.v[best + 1] as f64,
            );
            let den = a - 2.0 * b + c;
            if den.abs() > 1e-12 {
                idx += 0.5 * (a - c) / den;
            }
        }
        Some(self.t0 + idx * self.hop_sec)
    }
}

impl FluxCurve {
    /// Leading-edge onset: the first time in `[expected − pre, expected +
    /// post]` where the flux crosses 25% of the window's peak (linear
    /// interpolation across the crossing). The right measure for BLOOMING
    /// attacks (fresh arco sustains, re-bows), whose flux PEAK sits deep in
    /// the swell: the edge marks where the note starts speaking.
    pub fn leading_edge(&self, expected: f64, pre: f64, post: f64) -> Option<f64> {
        let lo = (((expected - pre - self.t0) / self.hop_sec).floor().max(0.0)) as usize;
        let hi =
            ((((expected + post - self.t0) / self.hop_sec).ceil()) as usize).min(self.v.len());
        if lo + 1 >= hi {
            return None;
        }
        let peak = self.v[lo..hi].iter().cloned().fold(f32::MIN, f32::max);
        if peak <= 0.0 {
            return None;
        }
        let thresh = peak * 0.25;
        for i in lo..hi {
            if self.v[i] >= thresh {
                let frac = if i > lo && self.v[i] > self.v[i - 1] {
                    ((thresh - self.v[i - 1]) / (self.v[i] - self.v[i - 1])).clamp(0.0, 1.0)
                        as f64
                        - 1.0
                } else {
                    0.0
                };
                return Some(self.t0 + (i as f64 + frac) * self.hop_sec);
            }
        }
        None
    }
}

// ── Pitch-crossing arrival (legato) ──────────────────────────────────────────

fn midi_to_hz(m: u8) -> f64 {
    440.0 * 2f64.powf((f64::from(m) - 69.0) / 12.0)
}

/// Goertzel power of `freq` over `n` mono samples at `off` (Hann-windowed).
fn goertzel(mono: &[f64], off: usize, n: usize, sr: u32, freq: f64) -> f64 {
    if off + n > mono.len() {
        return 0.0;
    }
    let w = 2.0 * std::f64::consts::PI * freq / f64::from(sr);
    let coeff = 2.0 * w.cos();
    let (mut s0, mut s1, mut s2) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let hann =
            0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos());
        s0 = mono[off + i] * hann + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

/// Acoustic PITCH-ARRIVAL time of a legato transition: the moment the
/// destination pitch takes over from the source. Tracks Goertzel energy on
/// the first few harmonics of `from` vs `to` (collision-pruned, so octaves
/// work) over `expected ± search` and returns the interpolated time where
/// the destination's share first crosses 50% and holds. This measures
/// exactly what the schedule promises ("the pitch change lands ON the
/// tick"), independent of bow noise or the pre-bow swell.
pub fn pitch_arrival(
    audio: &[f32],
    sr: u32,
    expected: f64,
    from: u8,
    to: u8,
    search: f64,
) -> Option<f64> {
    if from == to {
        return None; // re-bow: no pitch change to detect
    }
    let frames = audio.len() / 2;
    let mono: Vec<f64> = (0..frames)
        .map(|f| (audio[f * 2] as f64 + audio[f * 2 + 1] as f64) * 0.5)
        .collect();
    let (f_from, f_to) = (midi_to_hz(from), midi_to_hz(to));
    // Harmonics of each side, pruned where they collide with any harmonic
    // 1..=8 of the other side (within 3%) — e.g. the octave's fundamental
    // equals the source's 2nd harmonic. For SEMITONE/WHOLE-TONE moves the
    // fundamentals are only ~6-12% apart — inside one bin of the 43 ms
    // analysis window (≈23 Hz at 48 kHz) — so both smear into both sides
    // and dilute the share toward the louder (source) side, reading the
    // arrival tens of ms late; start at the 2nd harmonic there, where the
    // spacing doubles and the bins resolve. (A whole tone is ~2× a
    // semitone's spacing and already resolves — only the semitone loses
    // its fundamental.)
    let h_lo: u32 = if from.abs_diff(to) <= 1 { 2 } else { 1 };
    let harmonics = |f: f64, other: f64| -> Vec<f64> {
        (h_lo..=h_lo + 3)
            .map(|h| f * f64::from(h))
            .filter(|hf| {
                !(1..=8).any(|oh| {
                    let of = other * f64::from(oh);
                    (hf - of).abs() / of < 0.03
                })
            })
            .collect()
    };
    // A perfect harmonic relation (octave/fifth) can prune a side empty —
    // fall back to that side's fundamental: the destination's energy there
    // still jumps at arrival even if the other side leaks into the bin.
    let mut hf_from = harmonics(f_from, f_to);
    let mut hf_to = harmonics(f_to, f_from);
    if hf_from.is_empty() {
        hf_from = vec![f_from];
    }
    if hf_to.is_empty() {
        hf_to = vec![f_to];
    }

    const N: usize = 2048; // 43 ms window at 48 kHz
    const HOP_PA: usize = 128;
    let center0 = expected - search;
    let n_hops = (2.0 * search * f64::from(sr) / HOP_PA as f64) as usize;
    let mut share = Vec::with_capacity(n_hops);
    for h in 0..n_hops {
        let center = center0 + h as f64 * HOP_PA as f64 / f64::from(sr);
        let off = ((center * f64::from(sr)) as isize - (N as isize) / 2).max(0) as usize;
        let e_from: f64 = hf_from.iter().map(|&f| goertzel(&mono, off, N, sr, f)).sum();
        let e_to: f64 = hf_to.iter().map(|&f| goertzel(&mono, off, N, sr, f)).sum();
        share.push(if e_from + e_to > 0.0 {
            e_to / (e_from + e_to)
        } else {
            0.0
        });
    }
    // First sustained (3 hops) crossing of 50%, linearly interpolated. NO
    // post-crossing validation here: in render context (retiring source
    // voice + vibrato under the join) any hold/mean grading rejects genuine
    // crossings and reports phantom late arrivals. The MARKER measurement
    // (`examples/measure_arrivals.rs`) does validate its crossings on the
    // raw samples, where the grading is meaningful; the one case a plain
    // first-crossing misreads in renders — octaves, whose destination bins
    // are fed by source-harmonic leak-through — is handled by the caller
    // (octave joins carry a loose gate / are skipped when undetectable).
    for i in 1..share.len().saturating_sub(2) {
        if share[i] >= 0.5 && share[i + 1] >= 0.5 && share[i + 2] >= 0.5 && share[i - 1] < 0.5 {
            let frac = (0.5 - share[i - 1]) / (share[i] - share[i - 1]).max(1e-9);
            let t = center0 + ((i - 1) as f64 + frac) * HOP_PA as f64 / f64::from(sr);
            return Some(t);
        }
    }
    // Already arrived at window start, or never arrives.
    if share.first().copied().unwrap_or(0.0) >= 0.5 {
        return Some(center0);
    }
    None
}

/// Destination-vs-source harmonic share over `[t0, t1]` (sec) of interleaved
/// stereo `audio` — the raw curve behind [`pitch_arrival`], exported for the
/// offline arrival-measurement tool (`examples/measure_arrivals.rs`), which
/// needs the whole curve to grade its own confidence (source dominant before
/// the crossing, destination settled after) instead of just the crossing.
/// Same analysis: Goertzel power on collision-pruned harmonics 1..=4 of each
/// side, 2048-sample Hann window, 128-sample hop.
pub struct PitchShareCurve {
    /// Time (sec) of curve sample 0 (window CENTER).
    pub t0: f64,
    pub hop_sec: f64,
    /// Destination share `e_to / (e_from + e_to)` per hop, 0 when silent.
    pub v: Vec<f32>,
}

/// Compute the [`PitchShareCurve`] for a `from → to` pitch pair.
pub fn pitch_share_curve(
    audio: &[f32],
    sr: u32,
    from: u8,
    to: u8,
    t0: f64,
    t1: f64,
) -> PitchShareCurve {
    const N: usize = 2048;
    const HOP_PA: usize = 128;
    let frames = audio.len() / 2;
    let mono: Vec<f64> = (0..frames)
        .map(|f| (audio[f * 2] as f64 + audio[f * 2 + 1] as f64) * 0.5)
        .collect();
    let (f_from, f_to) = (midi_to_hz(from), midi_to_hz(to));
    // Same harmonic policy as [`pitch_arrival`] — including the SEMITONE
    // fundamental drop (unresolvable in one 43 ms bin).
    let h_lo: u32 = if from.abs_diff(to) <= 1 { 2 } else { 1 };
    let harmonics = |f: f64, other: f64| -> Vec<f64> {
        (h_lo..=h_lo + 3)
            .map(|h| f * f64::from(h))
            .filter(|hf| {
                !(1..=8).any(|oh| {
                    let of = other * f64::from(oh);
                    (hf - of).abs() / of < 0.03
                })
            })
            .collect()
    };
    let mut hf_from = harmonics(f_from, f_to);
    let mut hf_to = harmonics(f_to, f_from);
    if hf_from.is_empty() {
        hf_from = vec![f_from];
    }
    if hf_to.is_empty() {
        hf_to = vec![f_to];
    }
    let hop_sec = HOP_PA as f64 / f64::from(sr);
    let n_hops = (((t1 - t0) / hop_sec).max(0.0)) as usize;
    let mut v = Vec::with_capacity(n_hops);
    for h in 0..n_hops {
        let center = t0 + h as f64 * hop_sec;
        let off = ((center * f64::from(sr)) as isize - (N as isize) / 2).max(0) as usize;
        let e_from: f64 = hf_from.iter().map(|&f| goertzel(&mono, off, N, sr, f)).sum();
        let e_to: f64 = hf_to.iter().map(|&f| goertzel(&mono, off, N, sr, f)).sum();
        v.push(if e_from + e_to > 0.0 {
            (e_to / (e_from + e_to)) as f32
        } else {
            0.0
        });
    }
    PitchShareCurve { t0, hop_sec, v }
}

/// Destination-band ENERGY curve over `[t0, t1]` (sec) of interleaved stereo
/// `audio`: summed Goertzel power on the destination's collision-pruned
/// harmonics (same pruning + windows as [`pitch_share_curve`]) — WITHOUT
/// normalizing against the source. The share detector reads
/// "destination-vs-source balance", which the source's own level skews
/// (retiring voices, recorded tails, collision leak); the raw destination
/// energy tracks what the ear follows — "when does the new note's pitch
/// appear and grow" — independent of everything else in the mix. Used by
/// the join analyzer (`examples/analyze_joins.rs`) as the independent
/// cross-measurement.
pub fn dest_energy_curve(
    audio: &[f32],
    sr: u32,
    from: u8,
    to: u8,
    t0: f64,
    t1: f64,
) -> PitchShareCurve {
    const N: usize = 2048;
    const HOP_PA: usize = 128;
    let frames = audio.len() / 2;
    let mono: Vec<f64> = (0..frames)
        .map(|f| (audio[f * 2] as f64 + audio[f * 2 + 1] as f64) * 0.5)
        .collect();
    let (f_from, f_to) = (midi_to_hz(from), midi_to_hz(to));
    let h_lo: u32 = if from.abs_diff(to) <= 1 { 2 } else { 1 };
    let mut hf_to: Vec<f64> = (h_lo..=h_lo + 3)
        .map(|h| f_to * f64::from(h))
        .filter(|hf| {
            !(1..=8).any(|oh| {
                let of = f_from * f64::from(oh);
                (hf - of).abs() / of < 0.03
            })
        })
        .collect();
    if hf_to.is_empty() {
        hf_to = vec![f_to];
    }
    let hop_sec = HOP_PA as f64 / f64::from(sr);
    let n_hops = (((t1 - t0) / hop_sec).max(0.0)) as usize;
    let mut v = Vec::with_capacity(n_hops);
    for h in 0..n_hops {
        let center = t0 + h as f64 * hop_sec;
        let off = ((center * f64::from(sr)) as isize - (N as isize) / 2).max(0) as usize;
        let e: f64 = hf_to.iter().map(|&f| goertzel(&mono, off, N, sr, f)).sum();
        v.push(e as f32);
    }
    PitchShareCurve { t0, hop_sec, v }
}

// ── Showcase corpus ──────────────────────────────────────────────────────────

/// What kind of arrival a corpus note expects — drives which deterministic
/// identity the tests assert and how the onset cross-check searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnsetKind {
    /// Fresh attack (first of a phrase / isolated note): the schedule
    /// triggers the sustain zone exactly ON the tick.
    PhraseStart,
    /// Legato transition: prefired so the measured arrival lands ON the tick.
    Legato,
    /// Same-pitch re-bow (Legzero): no lead-in → prefire ON the tick.
    Rebow,
    /// Short articulation: pre-rolled by `pre_delay_ms`; the rhythmic peak
    /// (the marker `pre_delay_ms` into the sample) lands ON the tick.
    Short,
}

/// One expected note arrival in a corpus case.
#[derive(Debug, Clone)]
pub struct ExpectedOnset {
    pub qn: f64,
    pub sec: f64,
    pub pitch: u8,
    pub kind: OnsetKind,
}

/// One deterministic timing-showcase case: a document plus every note's
/// expected grid arrival.
#[derive(Debug, Clone)]
pub struct TimingCase {
    pub name: String,
    pub desc: String,
    pub bpm: f64,
    pub doc: TrackDocument,
    pub expected: Vec<ExpectedOnset>,
}

/// The corpus starts after a one-bar count-in.
pub const COUNT_IN_QN: f64 = 4.0;
const CORPUS_SEED: u64 = 0x71D1_1E6A_70C0_2B05;

/// CC58 keyswitch values (mid-band, from `specs/cinematic-strings.styx`).
const KS_LOWLAT: u8 = 2;
const KS_EXPR: u8 = 8;
const KS_STACCATO: u8 = 23;

struct CaseBuilder {
    bpm: f64,
    notes: Vec<DocNote>,
    ccs: Vec<DocCc>,
    expected: Vec<ExpectedOnset>,
}

impl CaseBuilder {
    fn new(bpm: f64, ks: u8, cc1: u8) -> Self {
        let ccs = vec![
            DocCc {
                qn: 0.0,
                chan: 0,
                cc: 58,
                val: ks,
            },
            DocCc {
                qn: 0.0,
                chan: 0,
                cc: 1,
                val: cc1,
            },
        ];
        Self {
            bpm,
            notes: Vec::new(),
            ccs,
            expected: Vec::new(),
        }
    }

    fn tempo(&self) -> Vec<TempoPoint> {
        vec![TempoPoint {
            qn: 0.0,
            bpm: self.bpm,
        }]
    }

    fn note(&mut self, start_qn: f64, end_qn: f64, pitch: u8, vel: u8, kind: OnsetKind) {
        self.notes.push(DocNote {
            start_qn,
            end_qn,
            chan: 0,
            pitch,
            vel,
        });
        let tempo = self.tempo();
        self.expected.push(ExpectedOnset {
            qn: start_qn,
            sec: qn_to_sec(&tempo, start_qn),
            pitch,
            kind,
        });
    }

    fn build(self, name: &str, desc: &str) -> TimingCase {
        let tempo = self.tempo();
        TimingCase {
            name: name.to_string(),
            desc: desc.to_string(),
            bpm: self.bpm,
            doc: TrackDocument {
                version: 1,
                seed: CORPUS_SEED,
                auto_divisi: false,
                notes: self.notes,
                ccs: self.ccs,
                tempo,
            },
            expected: self.expected,
        }
    }
}

/// An 8-note legato scale line (up then down), quarter notes from
/// `COUNT_IN_QN`, 20-ms-style overlaps (0.02 QN) so stage-1 annotation sees
/// legato edges.
fn legato_line(b: &mut CaseBuilder, vel: u8) {
    const LINE: [u8; 8] = [67, 69, 71, 72, 74, 72, 71, 69]; // G4 A4 B4 C5 D5 C5 B4 A4
    for (i, &p) in LINE.iter().enumerate() {
        let start = COUNT_IN_QN + i as f64;
        let end = start + if i + 1 < LINE.len() { 1.02 } else { 1.0 };
        let kind = if i == 0 {
            OnsetKind::PhraseStart
        } else {
            OnsetKind::Legato
        };
        b.note(start, end, p, vel, kind);
    }
}

/// The deterministic legato-timing showcase corpus. Every case is a
/// standalone [`TrackDocument`] (single channel/line) whose every note has
/// a grid-time arrival expectation.
pub fn timing_corpus() -> Vec<TimingCase> {
    let mut cases = Vec::new();

    // 1. Legato scale lines across tempos (expressive, medium zone vel 85).
    for bpm in [60.0, 90.0, 120.0, 160.0] {
        let mut b = CaseBuilder::new(bpm, KS_EXPR, 90);
        legato_line(&mut b, 85);
        cases.push(b.build(
            &format!("scale_expr_{}bpm", bpm as u32),
            &format!("legato scale, expressive medium zone (vel 85), {bpm} bpm"),
        ));
    }

    // 2. Velocity zones: slow-expressive (333 ms zone) vs fast-low-latency
    //    (100 ms zone).
    let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
    legato_line(&mut b, 40);
    cases.push(b.build(
        "zones_slow_expressive_90bpm",
        "legato scale, expressive SLOW zone (vel 40, 333 ms delay), 90 bpm",
    ));
    let mut b = CaseBuilder::new(120.0, KS_LOWLAT, 90);
    legato_line(&mut b, 110);
    cases.push(b.build(
        "zones_fast_lowlat_120bpm",
        "legato scale, low-latency FAST zone (vel 110, 100 ms delay), 120 bpm",
    ));

    // 2b. The three EXPRESSIVE transition speeds (KSP attack-velocity
    //     splits 64/100 → LT-offset bases 0/83/177): the same scale and
    //     interval material at slow / medium / fast velocity, so the ear
    //     can verify EVERY speed class lands its arrivals on the grid.
    for (vel, speed) in [(40u8, "slow"), (85, "med"), (115, "fast")] {
        let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
        legato_line(&mut b, vel);
        cases.push(b.build(
            &format!("scale_expr_90bpm_{speed}"),
            &format!("legato scale, expressive {speed} speed (vel {vel}), 90 bpm"),
        ));
        let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
        let mut qn = COUNT_IN_QN;
        for iv in [1i8, 2, 3, 4, 5, 7, 9, 12] {
            let to = (67i16 + i16::from(iv)) as u8;
            b.note(qn, qn + 2.02, 67, vel, OnsetKind::PhraseStart);
            b.note(qn + 2.0, qn + 4.0, to, vel, OnsetKind::Legato);
            qn += 6.0;
        }
        cases.push(b.build(
            &format!("intervals_up_90bpm_{speed}"),
            &format!("two-note legato slurs up, expressive {speed} speed (vel {vel}), 90 bpm"),
        ));
    }

    // 3. Intervals up and down (2nds through the octave): isolated two-note
    //    slurs (half + half) with a two-beat rest between pairs — each pair
    //    is one phrase start + one transition.
    for (name, desc, ivs) in [
        (
            "intervals_up_90bpm",
            "two-note legato slurs up: +1 +2 +3 +4 +5 +7 +9 +12 st from G4",
            [1i8, 2, 3, 4, 5, 7, 9, 12],
        ),
        (
            "intervals_down_90bpm",
            "two-note legato slurs down: -1 -2 -3 -4 -5 -7 -9 -12 st from G4",
            [-1i8, -2, -3, -4, -5, -7, -9, -12],
        ),
    ] {
        let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
        let mut qn = COUNT_IN_QN;
        for iv in ivs {
            let to = (67i16 + i16::from(iv)) as u8;
            b.note(qn, qn + 2.02, 67, 85, OnsetKind::PhraseStart);
            b.note(qn + 2.0, qn + 4.0, to, 85, OnsetKind::Legato);
            qn += 6.0; // 2-beat rest → clean phrase boundaries
        }
        cases.push(b.build(name, desc));
    }

    // 4. Repeated notes (re-bow): same pitch, abutted quarters.
    let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
    for i in 0..8 {
        let start = COUNT_IN_QN + i as f64;
        let kind = if i == 0 {
            OnsetKind::PhraseStart
        } else {
            OnsetKind::Rebow
        };
        b.note(start, start + 1.0, 67, 85, kind);
    }
    cases.push(b.build(
        "rebow_90bpm",
        "repeated G4 quarters, same-pitch re-bow (Legzero), 90 bpm",
    ));

    // 5. Phrase starts: isolated attacks (quarter note + quarter rest) — the
    //    fresh-attack baseline and the calibration anchor for sustains.
    let mut b = CaseBuilder::new(90.0, KS_EXPR, 90);
    for i in 0..8 {
        let start = COUNT_IN_QN + 2.0 * i as f64;
        b.note(start, start + 1.0, 67, 85, OnsetKind::PhraseStart);
    }
    cases.push(b.build(
        "phrase_starts_90bpm",
        "isolated G4 quarters (fresh sustain attacks), 90 bpm",
    ));

    // 6. Shorts (staccato): sharp attacks — the onset-detector CALIBRATION
    //    case (known 60 ms pre-delay compensation → peak on the grid).
    let mut b = CaseBuilder::new(120.0, KS_STACCATO, 90);
    for i in 0..8 {
        let start = COUNT_IN_QN + 2.0 * i as f64;
        b.note(start, start + 0.25, 67, 100, OnsetKind::Short);
    }
    cases.push(b.build(
        "shorts_calib_120bpm",
        "isolated staccato G4 (detector calibration; 60 ms pre-roll → peak on grid), 120 bpm",
    ));

    cases
}

// ── StrictLive replay ────────────────────────────────────────────────────────

/// Replay a document through the LIVE (reactive) path: every CC / note-on /
/// note-off dispatched at its wall-clock time against `render_offline` gaps
/// — the way a live player would drive the rig, with NO lookahead. Returns
/// interleaved stereo. Live legato commits reactively (at/after the note-on
/// plus the Overlap-Delay), so arrivals are EXPECTED to sit late of the
/// grid by roughly the velocity-zone delay — rendering both paths side by
/// side makes that difference audible and measurable.
pub fn render_live_replay(rig: &SamplerRig, id: &str, doc: &TrackDocument, tail_sec: f64) -> Vec<f32> {
    let sr = 48_000u32; // matches SamplerRig::new_offline in the harness
    #[derive(Clone, Copy)]
    enum Ev {
        Cc(u8, u8),
        On(u8, u8),
        Off(u8),
    }
    let mut events: Vec<(f64, u32, Ev)> = Vec::new();
    let mut seq = 0u32;
    for c in &doc.ccs {
        events.push((qn_to_sec(&doc.tempo, c.qn), seq, Ev::Cc(c.cc, c.val)));
        seq += 1;
    }
    for n in &doc.notes {
        events.push((
            qn_to_sec(&doc.tempo, n.start_qn),
            seq,
            Ev::On(n.pitch, n.vel),
        ));
        seq += 1;
        events.push((qn_to_sec(&doc.tempo, n.end_qn), seq, Ev::Off(n.pitch)));
        seq += 1;
    }
    events.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut audio: Vec<f32> = Vec::new();
    let mut cursor_frames = 0i64;
    let render_to = |rig: &SamplerRig, audio: &mut Vec<f32>, cursor: &mut i64, sec: f64| {
        let target = (sec * sr as f64).round() as i64;
        if target > *cursor {
            let mut buf = vec![0.0f32; ((target - *cursor) as usize) * 2];
            rig.render_offline(&mut buf).expect("render_offline");
            audio.extend_from_slice(&buf);
            *cursor = target;
        }
    };
    for (sec, _, ev) in &events {
        render_to(rig, &mut audio, &mut cursor_frames, *sec);
        match ev {
            Ev::Cc(cc, val) => rig.cc(id, *cc, *val),
            Ev::On(p, v) => {
                rig.warm_note(id, *p);
                rig.note_on(id, *p, *v);
            }
            Ev::Off(p) => rig.note_off(id, *p),
        }
    }
    let end = events.last().map(|e| e.0).unwrap_or(0.0) + tail_sec;
    render_to(rig, &mut audio, &mut cursor_frames, end);
    audio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_to_qn_inverts_qn_to_sec_across_changes() {
        let tempo = vec![
            TempoPoint { qn: 0.0, bpm: 120.0 },
            TempoPoint { qn: 8.0, bpm: 60.0 },
        ];
        for qn in [0.0, 1.0, 7.9, 8.0, 8.1, 20.0] {
            let sec = qn_to_sec(&tempo, qn);
            assert!((sec_to_qn(&tempo, sec) - qn).abs() < 1e-9, "qn {qn}");
        }
        assert_eq!(bpm_at_sec(&tempo, 0.0), 120.0);
        assert_eq!(bpm_at_sec(&tempo, 10.0), 60.0);
    }

    #[test]
    fn click_lands_on_the_tempo_grid() {
        let tempo = vec![TempoPoint { qn: 0.0, bpm: 120.0 }];
        let sr = 48_000u32;
        let click = render_click(&tempo, sr as usize * 2, sr, None);
        // First non-zero sample of each burst = the beat positions 0 / 0.5 /
        // 1.0 / 1.5 s.
        let mut onsets = Vec::new();
        let mut i = 0usize;
        let frames = click.len() / 2;
        while i < frames {
            if click[i * 2].abs() > 1e-6 {
                onsets.push(i);
                i += sr as usize / 10; // skip the burst body
            } else {
                i += 1;
            }
        }
        assert_eq!(onsets, vec![0, 24_000, 48_000, 72_000]);
    }

    #[test]
    fn corpus_is_deterministic_and_grid_consistent() {
        let a = timing_corpus();
        let b = timing_corpus();
        assert_eq!(a.len(), b.len());
        for (ca, cb) in a.iter().zip(&b) {
            assert_eq!(ca.doc.notes.len(), cb.doc.notes.len());
            assert_eq!(ca.expected.len(), ca.doc.notes.len());
            for (e, n) in ca.expected.iter().zip(&ca.doc.notes) {
                assert_eq!(e.qn, n.start_qn);
                assert!((e.sec - qn_to_sec(&ca.doc.tempo, n.start_qn)).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn pitch_arrival_finds_a_crossfaded_pitch_change() {
        let sr = 48_000u32;
        let frames = sr as usize * 2;
        let mut audio = vec![0.0f32; frames * 2];
        let (f1, f2) = (midi_to_hz(67), midi_to_hz(69));
        // 30 ms linear crossfade from G4 to A4 centered at 1.0 s.
        let xfade = 0.03;
        for i in 0..frames {
            let t = i as f64 / sr as f64;
            let mix = ((t - (1.0 - xfade / 2.0)) / xfade).clamp(0.0, 1.0);
            let s = ((2.0 * std::f64::consts::PI * f1 * t).sin() * (1.0 - mix)
                + (2.0 * std::f64::consts::PI * f2 * t).sin() * mix) as f32
                * 0.5;
            audio[i * 2] = s;
            audio[i * 2 + 1] = s;
        }
        let t = pitch_arrival(&audio, sr, 1.0, 67, 69, 0.3).expect("crossing found");
        assert!((t - 1.0).abs() < 0.030, "pitch arrival at {t}, expected 1.0");
        // Octave (harmonic-colliding) case still resolves.
        let mut audio8 = vec![0.0f32; frames * 2];
        let f3 = midi_to_hz(79);
        for i in 0..frames {
            let t = i as f64 / sr as f64;
            let mix = ((t - (1.0 - xfade / 2.0)) / xfade).clamp(0.0, 1.0);
            let s = ((2.0 * std::f64::consts::PI * f1 * t).sin() * (1.0 - mix)
                + (2.0 * std::f64::consts::PI * f3 * t).sin() * mix) as f32
                * 0.5;
            audio8[i * 2] = s;
            audio8[i * 2 + 1] = s;
        }
        let t = pitch_arrival(&audio8, sr, 1.0, 67, 79, 0.3).expect("octave crossing");
        assert!((t - 1.0).abs() < 0.040, "octave arrival at {t}");
    }

    #[test]
    fn flux_finds_an_impulse_train() {
        let sr = 48_000u32;
        let mut audio = vec![0.0f32; sr as usize * 2 * 2];
        // 40 ms 2 kHz bursts at 0.5 s and 1.25 s.
        for &at in &[0.5f64, 1.25] {
            let start = (at * sr as f64) as usize;
            for i in 0..(sr as usize / 25) {
                let t = i as f64 / sr as f64;
                let s = (2.0 * std::f64::consts::PI * 2000.0 * t).sin() as f32
                    * (-t * 60.0).exp() as f32;
                audio[(start + i) * 2] = s;
                audio[(start + i) * 2 + 1] = s;
            }
        }
        let flux = spectral_flux(&audio, sr);
        for &at in &[0.5f64, 1.25] {
            let t = flux.onset_near(at, 0.1).expect("onset found");
            assert!((t - at).abs() < 0.015, "onset at {t}, expected {at}");
        }
    }
}
