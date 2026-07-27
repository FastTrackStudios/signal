//! FreeVerb — Schroeder–Moorer reference architecture.
//!
//! 8 parallel lowpass-feedback comb filters → 4 series allpass filters,
//! per channel. Right channel adds a 23-sample offset to every delay
//! length for stereo decorrelation. Public-domain algorithm by Jezar at
//! Dreampoint (~2000); long-standing reference implementation.
//!
//! References:
//! - <https://github.com/sinshu/freeverb> (mirror of original sources)
//! - <https://ccrma.stanford.edu/~jos/pasp/Freeverb.html> (J. O. Smith)
//!
//! The published constants are tuned for 44.1 kHz. We scale them to the
//! actual sample rate at construction.

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::denormal::flush;

const STEREO_SPREAD: usize = 23;

const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

struct LpComb {
    buffer: Vec<f64>,
    idx: usize,
    feedback: f64,
    damp1: f64,
    damp2: f64,
    filterstore: f64,
}

impl LpComb {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            idx: 0,
            feedback: 0.84,
            damp1: 0.2,
            damp2: 0.8,
            filterstore: 0.0,
        }
    }

    fn set_feedback(&mut self, fb: f64) {
        self.feedback = fb;
    }

    fn set_damp(&mut self, damp: f64) {
        self.damp1 = damp;
        self.damp2 = 1.0 - damp;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filterstore = 0.0;
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        let out = self.buffer[self.idx];
        self.filterstore = flush(out * self.damp2 + self.filterstore * self.damp1);
        self.buffer[self.idx] = input + self.filterstore * self.feedback;
        self.idx += 1;
        if self.idx >= self.buffer.len() {
            self.idx = 0;
        }
        out
    }
}

struct AllpassF {
    buffer: Vec<f64>,
    idx: usize,
    feedback: f64,
}

impl AllpassF {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            idx: 0,
            feedback: 0.5,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
    }

    #[inline]
    fn tick(&mut self, input: f64) -> f64 {
        let bufout = self.buffer[self.idx];
        let output = -input + bufout;
        self.buffer[self.idx] = input + bufout * self.feedback;
        self.idx += 1;
        if self.idx >= self.buffer.len() {
            self.idx = 0;
        }
        output
    }
}

pub struct FreeVerb {
    // DC blockers on the comb-bank input keep incoming offset
    // out of the 16 recirculating loops.
    dc_in: DcBlocker,
    combs_l: [LpComb; 8],
    combs_r: [LpComb; 8],
    allpass_l: [AllpassF; 4],
    allpass_r: [AllpassF; 4],
    gain: f64,
}

impl FreeVerb {
    pub fn new(sample_rate: f64) -> Self {
        let _ = sample_rate;
        let scale = sample_rate / 44100.0;
        let comb_l =
            std::array::from_fn(|i| LpComb::new((COMB_TUNINGS[i] as f64 * scale) as usize));
        let comb_r = std::array::from_fn(|i| {
            LpComb::new(((COMB_TUNINGS[i] + STEREO_SPREAD) as f64 * scale) as usize)
        });
        let ap_l =
            std::array::from_fn(|i| AllpassF::new((ALLPASS_TUNINGS[i] as f64 * scale) as usize));
        let ap_r = std::array::from_fn(|i| {
            AllpassF::new(((ALLPASS_TUNINGS[i] + STEREO_SPREAD) as f64 * scale) as usize)
        });
        Self {
            dc_in: DcBlocker::new(),
            combs_l: comb_l,
            combs_r: comb_r,
            allpass_l: ap_l,
            allpass_r: ap_r,
            gain: 0.015, // Jezar's "fixed gain"
        }
    }
}

impl ReverbAlgorithm for FreeVerb {
    fn reset(&mut self) {
        self.dc_in.reset();
        for c in &mut self.combs_l {
            c.reset();
        }
        for c in &mut self.combs_r {
            c.reset();
        }
        for a in &mut self.allpass_l {
            a.reset();
        }
        for a in &mut self.allpass_r {
            a.reset();
        }
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Room size (Jezar): 0.28..1.00 mapped from size.
        let room_size = 0.7 + params.size * 0.28;
        // Damping: 0..0.4.
        let damp = params.damping * 0.4;
        // Decay multiplier
        let decay_boost = 0.7 + params.decay * 0.29; // 0.7..0.99

        let feedback = room_size * decay_boost;
        for c in &mut self.combs_l {
            c.set_feedback(feedback);
            c.set_damp(damp);
        }
        for c in &mut self.combs_r {
            c.set_feedback(feedback);
            c.set_damp(damp);
        }
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        let input = self.dc_in.tick((left + right) * self.gain);

        let mut out_l = 0.0;
        let mut out_r = 0.0;

        for i in 0..8 {
            out_l += self.combs_l[i].tick(input);
            out_r += self.combs_r[i].tick(input);
        }
        for i in 0..4 {
            out_l = self.allpass_l[i].tick(out_l);
            out_r = self.allpass_r[i].tick(out_r);
        }

        (out_l, out_r)
    }
}
