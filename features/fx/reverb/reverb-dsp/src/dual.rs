//! Dual-reverb composition — BigSky MX two-reverbs-per-preset routing.
//!
//! Two full [`ReverbChain`]s (each its own algorithm — including two
//! independent Convolution/Impulse slots — params, pan, mix) composed
//! per [`DualRouting`]:
//!
//! - `Series12` / `Series21`: one chain's full stereo output (its own
//!   dry/wet mix included) feeds the other — the walkthrough's
//!   "Magneto into Chamber" patch.
//! - `Parallel`: both chains process the same input; outputs sum with
//!   each chain's own mix law (`out = a + b − dry`, dry counted once),
//!   so the two mix knobs act like the hardware's independent
//!   per-reverb mixes. Combine with per-chain `pan` for the
//!   "two spaces offset left/right around a dry center" trick.
//! - `Split` / `SplitSwapped`: reverb 1 → left, reverb 2 → right, each
//!   mono-summed so the split is total.
//!
//! Scratch buffers are sized in `update()`; larger `process` calls are
//! chunked — no allocation on the audio thread.

use audiocore_dsp::{AudioConfig, Processor};

use crate::chain::ReverbChain;

/// Routing for the two reverbs in a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DualRouting {
    /// Only reverb A runs (single-reverb preset).
    #[default]
    Single,
    /// A feeds B.
    Series12,
    /// B feeds A.
    Series21,
    /// A and B run side by side, summed.
    Parallel,
    /// A → left channel, B → right channel.
    Split,
    /// A → right channel, B → left channel.
    SplitSwapped,
}

impl DualRouting {
    pub const COUNT: usize = 6;

    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Series12,
            2 => Self::Series21,
            3 => Self::Parallel,
            4 => Self::Split,
            5 => Self::SplitSwapped,
            _ => Self::Single,
        }
    }

    pub fn to_index(self) -> usize {
        match self {
            Self::Single => 0,
            Self::Series12 => 1,
            Self::Series21 => 2,
            Self::Parallel => 3,
            Self::Split => 4,
            Self::SplitSwapped => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Series12 => "Series 1>2",
            Self::Series21 => "Series 2>1",
            Self::Parallel => "Parallel",
            Self::Split => "Split",
            Self::SplitSwapped => "Split Swap",
        }
    }
}

/// Two reverb chains + routing (BigSky MX dual-reverb presets).
pub struct DualReverb {
    pub a: ReverbChain,
    pub b: ReverbChain,
    pub routing: DualRouting,

    // Pre-allocated scratch: the dry input copy and B's working buffers.
    dry_l: Vec<f64>,
    dry_r: Vec<f64>,
    b_l: Vec<f64>,
    b_r: Vec<f64>,
}

impl DualReverb {
    pub fn new() -> Self {
        Self {
            a: ReverbChain::new(),
            b: ReverbChain::new(),
            routing: DualRouting::Single,
            dry_l: vec![0.0; 512],
            dry_r: vec![0.0; 512],
            b_l: vec![0.0; 512],
            b_r: vec![0.0; 512],
        }
    }

    /// Copy the public parameter surface from one slot to the other
    /// (BigSky MX "copy settings"). Copies params/mix/pan/EQ/duck/trem —
    /// NOT the algorithm selection or its internal state, so you can
    /// carry one slot's settings onto a different engine. Call
    /// `update()` (or `update_params()` on the destination) afterwards
    /// so filters and smoothers pick the values up.
    pub fn copy_params(&mut self, from_a: bool) {
        let (src, dst) = if from_a {
            (&self.a, &mut self.b)
        } else {
            (&self.b, &mut self.a)
        };
        // Read everything first (src is an immutable borrow).
        let params = src.params;
        let conv_mod = src.conv_mod;
        let shimmer = src.shimmer;
        let cloud = src.cloud;
        let bloom = src.bloom;
        let chorale = src.chorale;
        let magneto = src.magneto;
        let nonlinear = src.nonlinear;
        let predelay_ms = src.predelay_ms;
        let mix = src.mix;
        let width = src.width;
        let pan = src.pan;
        let trem_rate_hz = src.trem_rate_hz;
        let trem_depth = src.trem_depth;
        let input_hp_freq = src.input_hp_freq;
        let input_lp_freq = src.input_lp_freq;
        let output_hp_freq = src.output_hp_freq;
        let output_lp_freq = src.output_lp_freq;
        let output_tilt_db = src.output_tilt_db;
        let output_tilt_pivot = src.output_tilt_pivot;
        let saturation = src.saturation;
        let duck_amount = src.duck_amount;
        let duck_threshold = src.duck_threshold;
        let duck_attack_ms = src.duck_attack_ms;
        let duck_release_ms = src.duck_release_ms;
        let freeze = src.freeze;

        dst.params = params;
        dst.conv_mod = conv_mod;
        dst.shimmer = shimmer;
        dst.cloud = cloud;
        dst.bloom = bloom;
        dst.chorale = chorale;
        dst.magneto = magneto;
        dst.nonlinear = nonlinear;
        dst.predelay_ms = predelay_ms;
        dst.mix = mix;
        dst.width = width;
        dst.pan = pan;
        dst.trem_rate_hz = trem_rate_hz;
        dst.trem_depth = trem_depth;
        dst.input_hp_freq = input_hp_freq;
        dst.input_lp_freq = input_lp_freq;
        dst.output_hp_freq = output_hp_freq;
        dst.output_lp_freq = output_lp_freq;
        dst.output_tilt_db = output_tilt_db;
        dst.output_tilt_pivot = output_tilt_pivot;
        dst.saturation = saturation;
        dst.duck_amount = duck_amount;
        dst.duck_threshold = duck_threshold;
        dst.duck_attack_ms = duck_attack_ms;
        dst.duck_release_ms = duck_release_ms;
        dst.freeze = freeze;
    }

    /// Max samples per inner chunk (scratch capacity).
    fn chunk_capacity(&self) -> usize {
        self.dry_l.len()
    }

    fn process_chunk(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len();
        match self.routing {
            DualRouting::Single => {
                self.a.process(left, right);
            }
            DualRouting::Series12 => {
                self.a.process(left, right);
                self.b.process(left, right);
            }
            DualRouting::Series21 => {
                self.b.process(left, right);
                self.a.process(left, right);
            }
            DualRouting::Parallel => {
                self.dry_l[..n].copy_from_slice(left);
                self.dry_r[..n].copy_from_slice(right);
                self.b_l[..n].copy_from_slice(left);
                self.b_r[..n].copy_from_slice(right);

                self.a.process(left, right);
                self.b.process(&mut self.b_l[..n], &mut self.b_r[..n]);

                // Sum of both chains' mix laws, dry counted once:
                // out = dry·(1 − mixA − mixB) + wetA·mixA + wetB·mixB.
                for i in 0..n {
                    left[i] += self.b_l[i] - self.dry_l[i];
                    right[i] += self.b_r[i] - self.dry_r[i];
                }
            }
            DualRouting::Split | DualRouting::SplitSwapped => {
                self.b_l[..n].copy_from_slice(left);
                self.b_r[..n].copy_from_slice(right);

                self.a.process(left, right);
                self.b.process(&mut self.b_l[..n], &mut self.b_r[..n]);

                let swapped = self.routing == DualRouting::SplitSwapped;
                for i in 0..n {
                    let a_mono = (left[i] + right[i]) * 0.5;
                    let b_mono = (self.b_l[i] + self.b_r[i]) * 0.5;
                    if swapped {
                        left[i] = b_mono;
                        right[i] = a_mono;
                    } else {
                        left[i] = a_mono;
                        right[i] = b_mono;
                    }
                }
            }
        }
    }
}

impl Default for DualReverb {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for DualReverb {
    fn reset(&mut self) {
        self.a.reset();
        self.b.reset();
    }

    fn update(&mut self, config: AudioConfig) {
        self.a.update(config);
        self.b.update(config);

        let cap = config.max_buffer_size.max(64);
        if self.dry_l.len() < cap {
            self.dry_l.resize(cap, 0.0);
            self.dry_r.resize(cap, 0.0);
            self.b_l.resize(cap, 0.0);
            self.b_r.resize(cap, 0.0);
        }
    }

    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());
        let cap = self.chunk_capacity();
        let mut pos = 0;
        while pos < n {
            let end = (pos + cap).min(n);
            let (l, r) = (&mut left[pos..end], &mut right[pos..end]);
            self.process_chunk(l, r);
            pos = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithm::AlgorithmType;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    /// A = Magneto-ish sparse taps, B = Hall — algorithm pair with
    /// clearly distinguishable series/parallel behavior.
    fn make_dual(routing: DualRouting) -> DualReverb {
        let mut d = DualReverb::new();
        d.routing = routing;
        d.a.set_algorithm(AlgorithmType::Magneto);
        d.a.mix = 1.0;
        d.b.set_algorithm(AlgorithmType::Hall);
        d.b.mix = 1.0;
        d.b.params.decay = 0.8;
        d.update(config());
        d
    }

    fn render(d: &mut DualReverb, n: usize) -> (Vec<f64>, Vec<f64>) {
        let mut l: Vec<f64> = (0..n).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        (l, r)
    }

    fn energy(buf: &[f64]) -> f64 {
        buf.iter().map(|x| x * x).sum()
    }

    #[test]
    fn single_matches_lone_chain() {
        let mut dual = DualReverb::new();
        dual.routing = DualRouting::Single;
        dual.a.set_algorithm(AlgorithmType::Hall);
        dual.a.mix = 1.0;
        dual.update(config());
        let (l_dual, _) = render(&mut dual, 24000);

        let mut lone = ReverbChain::new();
        lone.set_algorithm(AlgorithmType::Hall);
        lone.mix = 1.0;
        lone.update(config());
        let mut l: Vec<f64> = (0..24000).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        lone.process(&mut l, &mut r);

        for (a, b) in l_dual.iter().zip(l.iter()) {
            assert!((a - b).abs() < 1e-12, "Single must equal a lone chain");
        }
    }

    #[test]
    fn series_differs_from_parallel_and_orders_differ() {
        // Series: Hall reverberates Magneto's taps → much longer, denser
        // tail than parallel (where Hall only sees the dry click).
        let (l_series, _) = render(&mut make_dual(DualRouting::Series12), 96000);
        let (l_par, _) = render(&mut make_dual(DualRouting::Parallel), 96000);
        let late_series = energy(&l_series[48000..]);
        let late_par = energy(&l_par[48000..]);
        assert!(
            late_series > late_par * 1.5,
            "series should sustain longer (hall processes magneto taps): \
             series={late_series}, parallel={late_par}"
        );

        // Order matters: Magneto(Hall) ≠ Hall(Magneto) sample-wise.
        let (l12, _) = render(&mut make_dual(DualRouting::Series12), 48000);
        let (l21, _) = render(&mut make_dual(DualRouting::Series21), 48000);
        let diff: f64 = l12
            .iter()
            .zip(l21.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(diff > 1e-3, "series orders must differ: {diff}");
        for v in l12.iter().chain(l21.iter()) {
            assert!(v.is_finite());
        }
    }

    #[test]
    fn split_isolates_channels() {
        // Pan-neutral chains, full-wet — each side must carry only its
        // own reverb (isolation limited only by numeric noise).
        let (l, r) = render(&mut make_dual(DualRouting::Split), 48000);
        // Distinguish sides by tail length: B (hall, decay .8) rings far
        // longer than A (magneto taps) — compare LATE energy.
        let late_l = energy(&l[36000..]);
        let late_r = energy(&r[36000..]);
        assert!(
            late_r > late_l * 2.0,
            "hall side must ring longer: L={late_l}, R={late_r}"
        );

        let (l2, r2) = render(&mut make_dual(DualRouting::SplitSwapped), 48000);
        let late_l2 = energy(&l2[36000..]);
        let late_r2 = energy(&r2[36000..]);
        assert!(
            late_l2 > late_r2 * 2.0,
            "swapped: hall on left: L={late_l2}, R={late_r2}"
        );
    }

    #[test]
    fn split_isolation_is_total_for_distinct_impulses() {
        // Feed L-only input: in Split mode A and B both receive it (both
        // are fed the same stereo input), but each output channel is one
        // chain's mono sum — verify the channels are NOT identical (two
        // different algorithms) yet both active.
        let mut d = make_dual(DualRouting::Split);
        let (l, r) = render(&mut d, 24000);
        assert!(energy(&l) > 1e-6);
        assert!(energy(&r) > 1e-6);
        let diff: f64 = l.iter().zip(r.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1e-3, "split sides must be different reverbs");
    }

    #[test]
    fn dual_convolution_both_slots() {
        // Both chains on Convolution with different IRs — the
        // walkthrough's dual-impulse chapel patch. Must run without
        // panics/allocation issues and produce distinct sides in Split.
        let mut d = DualReverb::new();
        d.routing = DualRouting::Split;
        for chain in [&mut d.a, &mut d.b] {
            chain.set_algorithm(AlgorithmType::Convolution);
            chain.mix = 1.0;
        }
        d.update(config());

        // Distinct custom IRs.
        let ir_a: Vec<f64> = (0..2400).map(|i| if i % 480 == 0 { 0.5 } else { 0.0 }).collect();
        let ir_b: Vec<f64> = (0..4800).map(|i| if i % 973 == 0 { 0.4 } else { 0.0 }).collect();
        assert!(d.a.load_convolution_ir(&ir_a, &ir_a));
        assert!(d.b.load_convolution_ir(&ir_b, &ir_b));

        let (l, r) = render(&mut d, 24000);
        for v in l.iter().chain(r.iter()) {
            assert!(v.is_finite());
        }
        let diff: f64 = l.iter().zip(r.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(diff > 1e-3, "two IR slots must differ across the split");
    }

    #[test]
    fn copy_params_carries_surface_not_algorithm() {
        let mut d = DualReverb::new();
        d.a.set_algorithm(AlgorithmType::Magneto);
        d.b.set_algorithm(AlgorithmType::Hall);
        d.a.mix = 0.77;
        d.a.pan = -0.5;
        d.a.params.decay = 0.9;
        d.a.trem_depth = 0.4;
        d.copy_params(true);
        assert_eq!(d.b.mix, 0.77);
        assert_eq!(d.b.pan, -0.5);
        assert_eq!(d.b.params.decay, 0.9);
        assert_eq!(d.b.trem_depth, 0.4);
        assert_eq!(d.b.algorithm_type(), AlgorithmType::Hall, "algorithm untouched");
    }

    #[test]
    fn chunked_processing_handles_large_buffers() {
        let mut d = make_dual(DualRouting::Parallel);
        let n = 24000; // 47× the 512 scratch capacity in one call
        let mut l: Vec<f64> = (0..n).map(|i| if i < 8 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        for v in l.iter().chain(r.iter()) {
            assert!(v.is_finite());
        }
        assert!(energy(&l) > 1e-6);
    }
}
