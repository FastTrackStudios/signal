//! Dual-delay composition — `TimeLine` MX "1+2" routing.
//!
//! Two full [`DelayChain`]s (each its own machine, params, tap division
//! incl. `Free`) composed per [`DualRouting`]:
//!
//! - `Series12` / `Series21`: one chain's full stereo output (including
//!   its dry/wet mix) feeds the other — the swappable series order from
//!   the MX ("the dTape's outputs are getting flanged").
//! - `Parallel`: both chains process the same input; outputs sum with
//!   each chain's own mix law (`out = a + b − dry`, so the two mix
//!   knobs behave like the hardware's independent per-delay mixes).
//! - `Split` / `SplitSwapped`: delay 1 → left, delay 2 → right, each
//!   mono-summed so the split is total.
//!
//! Scratch buffers are sized in `update()` from `max_buffer_size`;
//! larger `process` calls are handled in chunks — no allocation on the
//! audio thread.

use audiocore_dsp::{AudioConfig, Processor};

use crate::chain::DelayChain;
use crate::engine::DelayStyle;

/// Routing for the two delays in a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DualRouting {
    /// Only delay A runs (single-delay preset).
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

    #[must_use]
    pub const fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Series12,
            2 => Self::Series21,
            3 => Self::Parallel,
            4 => Self::Split,
            5 => Self::SplitSwapped,
            _ => Self::Single,
        }
    }

    #[must_use]
    pub const fn to_index(self) -> usize {
        match self {
            Self::Single => 0,
            Self::Series12 => 1,
            Self::Series21 => 2,
            Self::Parallel => 3,
            Self::Split => 4,
            Self::SplitSwapped => 5,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
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

/// Two delay chains + routing (`TimeLine` MX dual-delay presets).
///
/// Chain B is built only when a routing plays it: a delay is mostly
/// `Single`, and a rig prepares one in every patch — B's two engines, each
/// seconds of audio, were half of every delay's memory, never heard. Its
/// settings are kept until then ([`set_b_style`](Self::set_b_style) and
/// friends) and applied as it is built.
pub struct DualDelay {
    pub a: DelayChain,
    b: Option<Box<DelayChain>>,
    b_style: Option<DelayStyle>,
    b_time_ms: Option<f64>,
    b_feedback: Option<f64>,
    b_mix: Option<f64>,
    /// What `update` last prepared for: a B built after it is prepared too.
    config: Option<AudioConfig>,
    /// Set through [`set_routing`](Self::set_routing) so B is there to
    /// play; a routing set here with no B plays A alone.
    pub routing: DualRouting,

    // Pre-allocated scratch: the dry input copy and B's working buffers.
    dry_l: Vec<f64>,
    dry_r: Vec<f64>,
    b_l: Vec<f64>,
    b_r: Vec<f64>,
}

impl DualDelay {
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: DelayChain::new(),
            b: None,
            b_style: None,
            b_time_ms: None,
            b_feedback: None,
            b_mix: None,
            config: None,
            routing: DualRouting::Single,
            dry_l: vec![0.0; 512],
            dry_r: vec![0.0; 512],
            b_l: vec![0.0; 512],
            b_r: vec![0.0; 512],
        }
    }

    /// Play `routing`, building chain B if it plays it — an allocation:
    /// a param write's, never the audio callback's.
    pub fn set_routing(&mut self, routing: DualRouting) {
        self.routing = routing;
        if routing != DualRouting::Single {
            self.b_mut();
        }
    }

    /// Chain B, built (with its kept settings, and prepared if this is) if
    /// it was not — an allocation.
    pub fn b_mut(&mut self) -> &mut DelayChain {
        let (style, time, feedback, mix, config) =
            (self.b_style, self.b_time_ms, self.b_feedback, self.b_mix, self.config);
        self.b.get_or_insert_with(|| {
            let mut c = Box::new(DelayChain::new());
            if let Some(s) = style {
                c.set_style(s);
            }
            if let Some(t) = time {
                c.delay_l.time_ms = t;
                c.delay_r.time_ms = t;
            }
            if let Some(f) = feedback {
                c.delay_l.feedback = f;
                c.delay_r.feedback = f;
            }
            if let Some(m) = mix {
                c.mix = m;
            }
            if let Some(cfg) = config {
                c.update(cfg);
            }
            c
        })
    }

    /// Chain B, if it has been built.
    #[must_use]
    pub fn b(&self) -> Option<&DelayChain> {
        self.b.as_deref()
    }

    /// B's style: kept, and set on B if it is built (a new engine there —
    /// an allocation, as for A).
    pub fn set_b_style(&mut self, style: DelayStyle) {
        self.b_style = Some(style);
        if let Some(b) = self.b.as_deref_mut() {
            b.set_style(style);
        }
    }

    /// B's time (both lines), kept and set on B if it is built.
    pub fn set_b_time_ms(&mut self, ms: f64) {
        self.b_time_ms = Some(ms);
        if let Some(b) = self.b.as_deref_mut() {
            b.delay_l.time_ms = ms;
            b.delay_r.time_ms = ms;
        }
    }

    /// B's feedback (both lines), kept and set on B if it is built.
    pub fn set_b_feedback(&mut self, feedback: f64) {
        self.b_feedback = Some(feedback);
        if let Some(b) = self.b.as_deref_mut() {
            b.delay_l.feedback = feedback;
            b.delay_r.feedback = feedback;
        }
    }

    /// B's mix, kept and set on B if it is built.
    pub fn set_b_mix(&mut self, mix: f64) {
        self.b_mix = Some(mix);
        if let Some(b) = self.b.as_deref_mut() {
            b.mix = mix;
        }
    }

    /// Max samples per inner chunk (scratch capacity).
    const fn chunk_capacity(&self) -> usize {
        self.dry_l.len()
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "scratch buffers are pre-sized in update() to hold max_buffer_size, so slicing [..left.len()] is safe"
    )]
    fn process_chunk(&mut self, left: &mut [f64], right: &mut [f64]) {
        // No B (a routing set without `set_routing`): A alone — B is never
        // built here.
        let Some(b) = self.b.as_deref_mut() else {
            self.a.process(left, right);
            return;
        };
        match self.routing {
            DualRouting::Single => {
                self.a.process(left, right);
            }
            DualRouting::Series12 => {
                self.a.process(left, right);
                b.process(left, right);
            }
            DualRouting::Series21 => {
                b.process(left, right);
                self.a.process(left, right);
            }
            DualRouting::Parallel => {
                // Copy left/right to scratch buffers using iterators
                for (d, s) in self.dry_l.iter_mut().zip(left.iter()) {
                    *d = *s;
                }
                for (d, s) in self.dry_r.iter_mut().zip(right.iter()) {
                    *d = *s;
                }
                for (d, s) in self.b_l.iter_mut().zip(left.iter()) {
                    *d = *s;
                }
                for (d, s) in self.b_r.iter_mut().zip(right.iter()) {
                    *d = *s;
                }

                self.a.process(left, right);
                b.process(&mut self.b_l[..left.len()], &mut self.b_r[..left.len()]);

                // Sum of both chains' mix laws, dry counted once:
                // out = dry·(1 − mixA − mixB) + wetA·mixA + wetB·mixB.
                for (l, (r, (dl, (dr, (bl, br))))) in left.iter_mut().zip(
                    right.iter_mut().zip(
                        self.dry_l
                            .iter()
                            .zip(self.dry_r.iter().zip(self.b_l.iter().zip(self.b_r.iter()))),
                    ),
                ) {
                    *l += bl - dl;
                    *r += br - dr;
                }
            }
            DualRouting::Split | DualRouting::SplitSwapped => {
                // Copy left/right to scratch buffers using iterators
                for (d, s) in self.b_l.iter_mut().zip(left.iter()) {
                    *d = *s;
                }
                for (d, s) in self.b_r.iter_mut().zip(right.iter()) {
                    *d = *s;
                }

                self.a.process(left, right);
                b.process(&mut self.b_l[..left.len()], &mut self.b_r[..left.len()]);

                let swapped = self.routing == DualRouting::SplitSwapped;
                for (((l, r), bl), br) in left
                    .iter_mut()
                    .zip(right.iter_mut())
                    .zip(self.b_l.iter())
                    .zip(self.b_r.iter())
                {
                    let a_mono = (*l + *r) * 0.5;
                    let b_mono = (*bl + *br) * 0.5;
                    if swapped {
                        *l = b_mono;
                        *r = a_mono;
                    } else {
                        *l = a_mono;
                        *r = b_mono;
                    }
                }
            }
        }
    }
}

impl Default for DualDelay {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for DualDelay {
    fn reset(&mut self) {
        self.a.reset();
        if let Some(b) = self.b.as_deref_mut() {
            b.reset();
        }
    }

    fn update(&mut self, config: AudioConfig) {
        self.config = Some(config);
        self.a.update(config);
        if self.routing != DualRouting::Single {
            self.b_mut();
        }
        if let Some(b) = self.b.as_deref_mut() {
            b.update(config);
        }

        let cap = config.max_buffer_size.max(64);
        if self.dry_l.len() < cap {
            self.dry_l.resize(cap, 0.0);
            self.dry_r.resize(cap, 0.0);
            self.b_l.resize(cap, 0.0);
            self.b_r.resize(cap, 0.0);
        }
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "end is computed as min(pos + cap, n) where pos < n, so slicing [pos..end] is always in bounds"
    )]
    fn process(&mut self, left: &mut [f64], right: &mut [f64]) {
        let n = left.len().min(right.len());
        let cap = self.chunk_capacity();
        let mut pos = 0;
        while pos < n {
            let end = pos.saturating_add(cap).min(n);
            let (l, r) = (&mut left[pos..end], &mut right[pos..end]);
            self.process_chunk(l, r);
            pos = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DelayStyle;
    use dsp_core::num;

    const SR: f64 = 48000.0;

    fn config() -> AudioConfig {
        AudioConfig {
            sample_rate: SR,
            max_buffer_size: 512,
        }
    }

    /// A/B chains: Clean, no feedback, full wet, no LR offset — pure
    /// delayed impulses at distinct times (A = 100 ms, B = 150 ms).
    fn make_dual(routing: DualRouting) -> DualDelay {
        let mut d = DualDelay::new();
        d.set_routing(routing);
        let set = |chain: &mut DelayChain, ms: f64| {
            chain.set_style(DelayStyle::Clean);
            chain.delay_l.time_ms = ms;
            chain.delay_r.time_ms = ms;
            chain.delay_l.feedback = 0.0;
            chain.delay_r.feedback = 0.0;
            chain.mix = 1.0;
            chain.lr_offset_ms = 0.0;
        };
        set(&mut d.a, 100.0);
        set(d.b_mut(), 150.0);
        d.update(config());
        d
    }

    fn render(d: &mut DualDelay, n: usize) -> (Vec<f64>, Vec<f64>) {
        let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        (l, r)
    }

    fn window_energy(buf: &[f64], center_ms: f64, half_ms: f64) -> f64 {
        let c = num::f64_to_index(center_ms * SR / 1000.0);
        let h = num::f64_to_index(half_ms * SR / 1000.0);
        buf[c.saturating_sub(h)..(c.saturating_add(h)).min(buf.len())]
            .iter()
            .map(|x| x * x)
            .sum()
    }

    #[test]
    fn series_stacks_delays_parallel_does_not() {
        // Series 1→2: A's 100 ms repeat gets delayed again by B → energy
        // at 250 ms. Parallel: events at 100 and 150 only.
        let (l_series, _) = render(&mut make_dual(DualRouting::Series12), 24000);
        let (l_par, _) = render(&mut make_dual(DualRouting::Parallel), 24000);

        let series_250 = window_energy(&l_series, 250.0, 10.0);
        let par_250 = window_energy(&l_par, 250.0, 10.0);
        assert!(
            series_250 > 0.1,
            "series should produce the stacked 250 ms event: {series_250}"
        );
        assert!(
            par_250 < series_250 * 0.01,
            "parallel must not stack: {par_250} vs {series_250}"
        );
        // Parallel has both direct events.
        assert!(window_energy(&l_par, 100.0, 10.0) > 0.1);
        assert!(window_energy(&l_par, 150.0, 10.0) > 0.1);
    }

    #[test]
    fn series_order_swaps() {
        // Both orders stack to 250 ms, but 2→1 passes B's event through
        // A — the first-arrival pattern differs: 1→2 has A's dry-through
        // event at 100 ms only via B's dry path, order still yields the
        // same event times; verify both produce the stack and remain
        // finite (order equivalence for LTI chains at zero feedback).
        let (l12, _) = render(&mut make_dual(DualRouting::Series12), 24000);
        let (l21, _) = render(&mut make_dual(DualRouting::Series21), 24000);
        assert!(window_energy(&l12, 250.0, 10.0) > 0.1);
        assert!(window_energy(&l21, 250.0, 10.0) > 0.1);
        for v in l12.iter().chain(l21.iter()) {
            assert!(v.is_finite());
        }
    }

    #[test]
    fn split_isolates_channels() {
        let (l, r) = render(&mut make_dual(DualRouting::Split), 24000);

        // A (100 ms) only on the left, B (150 ms) only on the right.
        let l_at_a = window_energy(&l, 100.0, 10.0);
        let r_at_a = window_energy(&r, 100.0, 10.0);
        let l_at_b = window_energy(&l, 150.0, 10.0);
        let r_at_b = window_energy(&r, 150.0, 10.0);

        assert!(l_at_a > 0.1, "A event on L: {l_at_a}");
        assert!(r_at_b > 0.1, "B event on R: {r_at_b}");
        // -60 dB isolation.
        assert!(
            r_at_a < l_at_a * 1e-6,
            "A must not bleed right: {r_at_a} vs {l_at_a}"
        );
        assert!(
            l_at_b < r_at_b * 1e-6,
            "B must not bleed left: {l_at_b} vs {r_at_b}"
        );

        // Swapped mirror.
        let (l2, r2) = render(&mut make_dual(DualRouting::SplitSwapped), 24000);
        assert!(window_energy(&r2, 100.0, 10.0) > 0.1);
        assert!(window_energy(&l2, 150.0, 10.0) > 0.1);
    }

    #[test]
    fn single_matches_lone_chain() {
        let mut dual = make_dual(DualRouting::Single);
        let (l_dual, _) = render(&mut dual, 24000);

        let mut lone = DelayChain::new();
        lone.set_style(DelayStyle::Clean);
        lone.delay_l.time_ms = 100.0;
        lone.delay_r.time_ms = 100.0;
        lone.delay_l.feedback = 0.0;
        lone.delay_r.feedback = 0.0;
        lone.mix = 1.0;
        lone.lr_offset_ms = 0.0;
        lone.update(config());
        let mut l: Vec<f64> = (0..24000).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        lone.process(&mut l, &mut r);

        for (a, b) in l_dual.iter().zip(l.iter()) {
            assert!((a - b).abs() < 1e-12, "Single must equal a lone chain");
        }
    }

    #[test]
    fn chunked_processing_matches_capacity() {
        // Buffers larger than the scratch capacity must be handled.
        let mut d = make_dual(DualRouting::Parallel);
        let n = 24000; // single process() call, 47× the 512 scratch
        let mut l: Vec<f64> = (0..n).map(|i| if i == 0 { 1.0 } else { 0.0 }).collect();
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        for v in l.iter().chain(r.iter()) {
            assert!(v.is_finite());
        }
        assert!(window_energy(&l, 100.0, 10.0) > 0.05);
    }
}
