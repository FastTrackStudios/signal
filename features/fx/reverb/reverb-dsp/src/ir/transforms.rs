//! Pure-function IR transforms — trim, stretch, reverse, envelope
//! shaping, predelay, gain — applied to an [`IrAsset`] to produce a
//! stereo pair ready for convolution.
//!
//! Inspired by REEV-R (<https://github.com/tiagolr/reevr>): stretch,
//! trim, reverse, attack and decay, plus IR predelay.

use super::asset::IrAsset;

/// How to reconcile the source IR's channel count with the convolver's
/// stereo input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLayout {
    /// Use channel 0 for both L and R.
    MonoToBoth,
    /// Channel 0 → L, channel 1 → R (or duplicate when source is mono).
    Stereo,
    /// Swap L/R.
    StereoSwapped,
    /// L = ch0, R = -ch0 (extreme M/S widening of a mono IR).
    MonoToWide,
}

#[derive(Debug, Clone, Copy)]
pub struct IrTransforms {
    /// Drop this many seconds from the start.
    pub trim_start_s: f64,
    /// Drop this many seconds from the end.
    pub trim_end_s: f64,
    /// Reverse playback (gated reverb).
    pub reverse: bool,
    /// Time-stretch factor. > 1 stretches longer, < 1 shrinks. 1.0 = none.
    pub stretch: f64,
    /// Linear attack ramp in seconds applied at the head of the IR.
    pub attack_s: f64,
    /// Exponential decay tail in seconds (-60 dB time).
    /// 0 disables; otherwise multiplies an exp(-3·t/decay_s) envelope.
    pub decay_s: f64,
    /// Insert N seconds of silence before the IR.
    pub predelay_s: f64,
    /// Output gain in dB.
    pub gain_db: f64,
    /// Channel reconciliation.
    pub layout: ChannelLayout,
    /// BigSky MX Impulse "Decay": fraction of the IR that plays
    /// (0.01..1.0, 1.0 = whole file, no-op). Applied BEFORE `reverse`,
    /// so a reversed IR is the decay-shortened head played backwards.
    pub decay_frac: f64,
    /// Frequency-dependent decay shaping ("Decay EQ"): per band
    /// `(center_hz, end_gain_db)` — the band's gain ramps linearly in
    /// dB from 0 at the IR head to `end_gain_db` at the tail, so
    /// negative values shorten that band's decay and positive values
    /// stretch it. Applied with time-varying low/high shelves
    /// (band 0 = low shelf, band 1 = high shelf).
    pub decay_eq: [(f64, f64); 2],
    /// How `decay_frac` < 1.0 shortens: `false` = decreasing ramp
    /// (Envelope), `true` = abrupt truncation (Gate).
    pub tail_gate: bool,
    /// BigSky MX Impulse "Attack": relative onset fade-in over the
    /// final buffer's head (0..1 maps to 0..25% of the shaped length).
    /// Unlike `attack_s` this is length-relative and applied after
    /// reverse/stretch, i.e. on what actually plays first.
    pub attack_frac: f64,
}

impl Default for IrTransforms {
    fn default() -> Self {
        Self {
            decay_eq: [(250.0, 0.0), (4000.0, 0.0)],
            trim_start_s: 0.0,
            trim_end_s: 0.0,
            reverse: false,
            stretch: 1.0,
            attack_s: 0.0,
            decay_s: 0.0,
            predelay_s: 0.0,
            gain_db: 0.0,
            layout: ChannelLayout::Stereo,
            decay_frac: 1.0,
            tail_gate: false,
            attack_frac: 0.0,
        }
    }
}

impl IrTransforms {
    /// Map the shaping subset of [`crate::algorithm::ImpulseParams`]
    /// (decay/tail/attack/stretch/direction) onto a transform set.
    /// The single source of truth for the Impulse-param → transform
    /// mapping, used by both the synchronous and background reshape
    /// paths.
    pub fn from_impulse(p: &crate::algorithm::ImpulseParams) -> Self {
        Self {
            decay_frac: p.decay.clamp(0.01, 1.0),
            tail_gate: p.tail == crate::algorithm::ImpulseTail::Gate,
            attack_frac: p.attack.clamp(0.0, 1.0),
            stretch: p.stretch.clamp(0.25, 4.0),
            reverse: p.direction == crate::algorithm::ImpulseDirection::Reverse,
            decay_eq: [(250.0, p.decay_lo_db), (4000.0, p.decay_hi_db)],
            ..Default::default()
        }
    }

    /// Apply the full pipeline to an [`IrAsset`] and produce a stereo
    /// pair at the asset's sample rate.
    pub fn apply(&self, ir: &IrAsset) -> (Vec<f64>, Vec<f64>) {
        let sr = ir.sample_rate;
        let (mut l, mut r) = extract_stereo(ir, self.layout);

        // 1. Trim
        let start = ((self.trim_start_s.max(0.0)) * sr) as usize;
        let end_drop = ((self.trim_end_s.max(0.0)) * sr) as usize;
        l = trim(l, start, end_drop);
        r = trim(r, start, end_drop);

        // 1.5. Decay window (MX Impulse Decay + Tail) — take the first
        // `decay_frac` of the file, shaped by envelope or gate. Runs
        // before `reverse` so a reversed IR is the shortened head
        // played backwards (the MX riser behavior).
        let df = self.decay_frac.clamp(0.01, 1.0);
        if df < 1.0 - 1e-9 {
            apply_decay_window(&mut l, df, self.tail_gate);
            apply_decay_window(&mut r, df, self.tail_gate);
        }

        // 1.7. Decay EQ: frequency-dependent decay via time-varying
        // shelves — recomputed every ~10 ms chunk, dB ramping linearly
        // to the band's end gain across the file.
        if self.decay_eq.iter().any(|&(_, db)| db.abs() > 0.05) {
            apply_decay_eq(&mut l, &self.decay_eq, sr);
            apply_decay_eq(&mut r, &self.decay_eq, sr);
        }

        // 2. Reverse
        if self.reverse {
            l.reverse();
            r.reverse();
        }

        // 3. Stretch
        if (self.stretch - 1.0).abs() > 1e-6 {
            l = stretch(&l, self.stretch);
            r = stretch(&r, self.stretch);
        }

        // 4. Envelope (attack then decay)
        if self.attack_s > 0.0 {
            apply_attack(&mut l, self.attack_s, sr);
            apply_attack(&mut r, self.attack_s, sr);
        }
        if self.decay_s > 0.0 {
            apply_decay(&mut l, self.decay_s, sr);
            apply_decay(&mut r, self.decay_s, sr);
        }
        // 4.5. Relative attack fade (MX Impulse Attack) — softens the
        // head of whatever now plays first (post-reverse/stretch).
        let af = self.attack_frac.clamp(0.0, 1.0);
        if af > 1e-9 {
            let n = ((l.len() as f64) * 0.25 * af) as usize;
            apply_attack_samples(&mut l, n);
            apply_attack_samples(&mut r, n);
        }

        // 5. Predelay
        let predelay = (self.predelay_s.max(0.0) * sr) as usize;
        if predelay > 0 {
            l = prepend_zeros(&l, predelay);
            r = prepend_zeros(&r, predelay);
        }

        // 6. Gain
        if (self.gain_db).abs() > 1e-6 {
            let g = 10f64.powf(self.gain_db / 20.0);
            for s in &mut l {
                *s *= g;
            }
            for s in &mut r {
                *s *= g;
            }
        }

        (l, r)
    }
}

fn extract_stereo(ir: &IrAsset, layout: ChannelLayout) -> (Vec<f64>, Vec<f64>) {
    let ch0 = ir.channels.first().cloned().unwrap_or_default();
    let ch1 = ir.channels.get(1).cloned().unwrap_or_else(|| ch0.clone());
    match layout {
        ChannelLayout::MonoToBoth => (ch0.clone(), ch0),
        ChannelLayout::Stereo => (ch0, ch1),
        ChannelLayout::StereoSwapped => (ch1, ch0),
        ChannelLayout::MonoToWide => {
            let inv: Vec<f64> = ch0.iter().map(|s| -s).collect();
            (ch0, inv)
        }
    }
}

fn trim(buf: Vec<f64>, start: usize, end_drop: usize) -> Vec<f64> {
    if start >= buf.len() {
        return Vec::new();
    }
    let end = buf.len().saturating_sub(end_drop).max(start);
    buf[start..end].to_vec()
}

/// Linear-interpolated resampling — stretches duration by `factor`.
/// factor > 1 → longer (lower pitch); factor < 1 → shorter (higher pitch).
fn stretch(buf: &[f64], factor: f64) -> Vec<f64> {
    if buf.is_empty() || factor <= 0.0 {
        return Vec::new();
    }
    let new_len = ((buf.len() as f64) * factor) as usize;
    let mut out = Vec::with_capacity(new_len);
    let inv = 1.0 / factor;
    for i in 0..new_len {
        let src = i as f64 * inv;
        let idx = src.floor() as usize;
        let frac = src - idx as f64;
        let a = buf[idx.min(buf.len() - 1)];
        let b = buf[(idx + 1).min(buf.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn apply_attack(buf: &mut [f64], attack_s: f64, sr: f64) {
    let n = ((attack_s * sr) as usize).min(buf.len());
    apply_attack_samples(buf, n);
}

fn apply_attack_samples(buf: &mut [f64], n: usize) {
    let n = n.min(buf.len());
    if n == 0 {
        return;
    }
    let inv = 1.0 / n as f64;
    for (i, s) in buf.iter_mut().take(n).enumerate() {
        *s *= i as f64 * inv;
    }
}

/// Keep the first `frac` of the buffer; shape its end with a linear
/// ramp-down (envelope) or truncate hard (gate). The buffer is
/// truncated to the window so partition counts shrink with decay.
fn apply_decay_window(buf: &mut Vec<f64>, frac: f64, gate: bool) {
    let keep = (((buf.len() as f64) * frac) as usize).max(1);
    buf.truncate(keep);
    if !gate {
        // Envelope: ramp the kept portion down to zero across its
        // second half so the shortening is smooth, not a cliff.
        let ramp_start = keep / 2;
        let ramp_len = (keep - ramp_start).max(1);
        #[allow(clippy::needless_range_loop)]
        for i in ramp_start..keep {
            let g = 1.0 - (i - ramp_start) as f64 / ramp_len as f64;
            buf[i] *= g;
        }
    }
}

fn apply_decay(buf: &mut [f64], decay_s: f64, sr: f64) {
    let t60_samples = (decay_s * sr).max(1.0);
    for (i, s) in buf.iter_mut().enumerate() {
        let env = 10f64.powf(-3.0 * i as f64 / t60_samples);
        *s *= env;
    }
}

fn prepend_zeros(buf: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![0.0; n];
    out.extend_from_slice(buf);
    out
}


/// Time-varying decay EQ: low shelf (band 0) + high shelf (band 1)
/// whose gains ramp linearly in dB from 0 at the head to the band's
/// `end_gain_db` at the tail. Coefficients refresh per ~10 ms chunk;
/// one-pole shelf construction keeps it phase-benign for IR use.
fn apply_decay_eq(x: &mut [f64], bands: &[(f64, f64); 2], sample_rate: f64) {
    if x.is_empty() {
        return;
    }
    let n = x.len();
    let chunk = ((sample_rate * 0.01) as usize).max(64);
    let (lo_f, lo_db) = bands[0];
    let (hi_f, hi_db) = bands[1];
    // One-pole crossover states.
    let a_lo = (-core::f64::consts::TAU * lo_f.clamp(40.0, 2000.0) / sample_rate).exp();
    let a_hi = (-core::f64::consts::TAU * hi_f.clamp(800.0, 12_000.0) / sample_rate).exp();
    let mut lp_lo = 0.0;
    let mut lp_hi = 0.0;
    let mut i = 0;
    while i < n {
        let end = (i + chunk).min(n);
        let t = i as f64 / n as f64;
        let g_lo = 10.0f64.powf(lo_db * t / 20.0);
        let g_hi = 10.0f64.powf(hi_db * t / 20.0);
        for v in &mut x[i..end] {
            let inp = *v;
            lp_lo = (1.0 - a_lo) * inp + a_lo * lp_lo;
            lp_hi = (1.0 - a_hi) * inp + a_hi * lp_hi;
            let low = lp_lo;
            let high = inp - lp_hi;
            let mid = inp - low - high;
            *v = low * g_lo + mid + high * g_hi;
        }
        i = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_flips_signal() {
        let ir = IrAsset::from_mono(vec![1.0, 0.0, 0.0, 0.0], 1000.0);
        let t = IrTransforms {
            reverse: true,
            ..Default::default()
        };
        let (l, _r) = t.apply(&ir);
        assert!(l[3] > 0.9 && l[0].abs() < 1e-9);
    }

    #[test]
    fn predelay_inserts_silence() {
        let ir = IrAsset::from_mono(vec![1.0; 10], 1000.0);
        let t = IrTransforms {
            predelay_s: 0.005,
            ..Default::default()
        };
        let (l, _r) = t.apply(&ir);
        // 5ms @ 1kHz = 5 samples of silence prepended
        assert!(l[0..5].iter().all(|s| s.abs() < 1e-9));
        assert!(l[5] > 0.5);
    }

    #[test]
    fn stretch_doubles_length() {
        let ir = IrAsset::from_mono(vec![1.0; 100], 1000.0);
        let t = IrTransforms {
            stretch: 2.0,
            ..Default::default()
        };
        let (l, _r) = t.apply(&ir);
        assert!(l.len() >= 195 && l.len() <= 200);
    }

    #[test]
    fn trim_shortens() {
        let ir = IrAsset::from_mono((0..100).map(|i| i as f64).collect(), 1000.0);
        let t = IrTransforms {
            trim_start_s: 0.010,
            trim_end_s: 0.010,
            ..Default::default()
        };
        let (l, _r) = t.apply(&ir);
        assert_eq!(l.len(), 80);
        assert!((l[0] - 10.0).abs() < 1e-9);
    }
}
