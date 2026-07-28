//! Keith Barr-style single-loop reverb — the MidiVerb/FV-1 "ring".
//!
//! Clean-room implementation from the publicly documented topology (the
//! structure Barr described for his FV-1 ROM reverbs; no code
//! consulted): ONE big loop of four sections, each
//! `delay → ×feedback-gain → allpasses (one of them slowly modulated)
//! → one-pole damping`, fed through a short input allpass chain, with
//! eight output taps (two per section) mixed to stereo at descending
//! gains. Density comes from the ring re-passing every section each
//! trip; the single modulated allpass keeps the tail alive.
//!
//! This is the *Classic voice* late core: sparser and ringier than an
//! FDN, with the early-'80s single-loop character.

use super::one_pole::Lp1;
use audiocore_dsp::delay_line::DelayLine;

/// Section delay lengths in samples at 32768 Hz (the FV-1-era base
/// rate), rescaled to the host rate. Mutually prime-ish.
const SECTION_LEN_32K: [f64; 4] = [1187.0, 1583.0, 2089.0, 2557.0];
/// Allpass lengths per section (two per section), 32768 Hz base.
const AP_LEN_32K: [[f64; 2]; 4] = [
    [239.0, 331.0],
    [283.0, 397.0],
    [353.0, 431.0],
    [409.0, 467.0],
];
/// Input allpass chain lengths (32768 Hz base).
const INPUT_AP_32K: [f64; 4] = [113.0, 157.0, 197.0, 251.0];
/// Output tap gains, cycling per tap (the documented descending mix).
const TAP_GAINS: [f64; 4] = [1.5, 1.2, 1.0, 0.8];
const AP_COEFF: f64 = 0.55;

struct Ap {
    line: DelayLine,
    len: f64,
}

impl Ap {
    fn new(len: f64) -> Self {
        Self {
            line: DelayLine::new(len as usize + 8),
            len,
        }
    }

    #[inline]
    fn tick(&mut self, x: f64, len: f64) -> f64 {
        let delayed = self.line.read_linear(len.clamp(1.0, self.line.len() as f64 - 4.0));
        let v = x - AP_COEFF * delayed;
        self.line.write(v);
        delayed + AP_COEFF * v
    }
}

struct Section {
    delay: DelayLine,
    len: usize,
    ap: [Ap; 2],
    damp: Lp1,
}

pub struct BarrLoop {
    sections: [Section; 4],
    input_aps: [Ap; 4],
    /// Loop feedback gain per section hop (T60 control).
    gain: f64,
    /// Slow sine on ONE allpass (section 2's first AP), FV-1 style.
    mod_phase: f64,
    mod_inc: f64,
    mod_depth: f64,
    /// Ring state carried between samples.
    ring: f64,
    sample_rate: f64,
}

impl BarrLoop {
    pub fn new(sample_rate: f64) -> Self {
        let k = sample_rate / 32_768.0;
        let sections = core::array::from_fn(|i| Section {
            delay: DelayLine::new((SECTION_LEN_32K[i] * k) as usize + 8),
            len: (SECTION_LEN_32K[i] * k) as usize,
            ap: [
                Ap::new(AP_LEN_32K[i][0] * k),
                Ap::new(AP_LEN_32K[i][1] * k),
            ],
            damp: Lp1::new(),
        });
        let mut this = Self {
            sections,
            input_aps: core::array::from_fn(|i| Ap::new(INPUT_AP_32K[i] * k)),
            gain: 0.6,
            mod_phase: 0.0,
            mod_inc: 0.5 / sample_rate,
            mod_depth: 9.0 * k,
            ring: 0.0,
            sample_rate,
        };
        this.set_damping(6000.0);
        this
    }

    /// Per-hop loop gain from a target T60: the ring passes 4 sections
    /// per trip, total trip length Σ section delays.
    pub fn set_t60(&mut self, t60_s: f64) {
        let trip: usize = self.sections.iter().map(|s| s.len).sum();
        let trip_s = trip as f64 / self.sample_rate;
        // Per-SECTION gain g with 4 applications per trip:
        // g^4 = 10^(−3·trip_s/t60) → uniform dB/s decay.
        let g4 = 10.0f64.powf(-3.0 * trip_s / t60_s.max(0.05));
        self.gain = g4.powf(0.25).min(0.997);
    }

    pub fn set_damping(&mut self, cutoff_hz: f64) {
        for s in &mut self.sections {
            s.damp.set_freq(cutoff_hz.clamp(500.0, 16_000.0), self.sample_rate);
        }
    }

    /// One mono input sample → stereo out from the eight taps.
    #[inline]
    pub fn tick(&mut self, input: f64) -> (f64, f64) {
        // Input diffusion chain.
        let mut x = input;
        for ap in &mut self.input_aps {
            let len = ap.len;
            x = ap.tick(x, len);
        }

        // The slow sine for the single modulated allpass.
        self.mod_phase += self.mod_inc;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        let mod_off = (self.mod_phase * core::f64::consts::TAU).sin() * self.mod_depth;

        // One trip around the ring, injecting the input at section 0.
        let mut sig = self.ring + x;
        let mut out_l = 0.0;
        let mut out_r = 0.0;
        for i in 0..4 {
            let len = self.sections[i].len;
            self.sections[i].delay.write(sig);
            let read = self.sections[i].delay.read(len);

            // Two output taps per section at staggered offsets.
            let t1 = self.sections[i].delay.read(len / 3);
            let t2 = self.sections[i].delay.read(2 * len / 3);
            let g = TAP_GAINS[i] * 0.22;
            if i % 2 == 0 {
                out_l += t1 * g;
                out_r += t2 * g * 0.9;
            } else {
                out_r += t1 * g;
                out_l += t2 * g * 0.9;
            }

            let mut v = read * self.gain;
            // Section 1 carries the modulated allpass (FV-1 style: one
            // moving allpass keeps the whole ring alive).
            let base0 = self.sections[i].ap[0].len;
            let l0 = if i == 1 { base0 + mod_off } else { base0 };
            v = self.sections[i].ap[0].tick(v, l0);
            let l1 = self.sections[i].ap[1].len;
            v = self.sections[i].ap[1].tick(v, l1);
            sig = self.sections[i].damp.tick(v);
        }
        self.ring = sig;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        for s in &mut self.sections {
            s.delay.clear();
            s.ap[0].line.clear();
            s.ap[1].line.clear();
            s.damp.reset();
        }
        for ap in &mut self.input_aps {
            ap.line.clear();
        }
        self.ring = 0.0;
        self.mod_phase = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_decays_at_the_target_rate() {
        let mut b = BarrLoop::new(48000.0);
        b.set_t60(1.5);
        b.set_damping(16_000.0);
        let mut e_early = 0.0;
        let mut e_late = 0.0;
        for n in 0..120_000 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let (l, r) = b.tick(x);
            assert!(l.is_finite() && r.is_finite());
            if (24_000..48_000).contains(&n) {
                e_early += l * l + r * r;
            }
            if (72_000..96_000).contains(&n) {
                e_late += l * l + r * r;
            }
        }
        // 1 s apart at T60 = 1.5 s → −40 dB = 1e-4 (generous band:
        // damping and the modulated allpass shade the broadband rate).
        let ratio = e_late / e_early.max(1e-30);
        assert!(
            (1.0e-5..1.0e-2).contains(&ratio),
            "ring decay off target: {ratio:e}"
        );
    }

    #[test]
    fn stereo_taps_are_decorrelated() {
        let mut b = BarrLoop::new(48000.0);
        b.set_t60(2.0);
        let mut dot = 0.0;
        let mut el = 0.0;
        let mut er = 0.0;
        for n in 0..96_000 {
            let x = if n == 0 { 1.0 } else { 0.0 };
            let (l, r) = b.tick(x);
            if n > 4800 {
                dot += l * r;
                el += l * l;
                er += r * r;
            }
        }
        let corr = dot / (el * er).sqrt().max(1e-30);
        assert!(
            corr.abs() < 0.9,
            "taps should decorrelate the ring: corr={corr}"
        );
    }
}
