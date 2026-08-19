//! The FX stack core — spec `docs/spec/fx/stack.md` (`fx.stack.*`).
//!
//! One topology for every FTS effect kind: a **stack** is parallel **lanes**,
//! each lane a serial chain of **stages**; a stage is one *style* of the kind
//! (a compressor flavour, an EQ model, a delay engine) with its own complete
//! parameter set. A single-style plugin is a stack of one lane with one stage
//! — there is no separate non-stack mode.
//!
//! This crate is the processing shape only: generic over the kind's stage
//! processor, `no_std + alloc`, allocation-free after [`Stack::prepare`], no
//! threads, no I/O (the platform-targets rules for processing cores). The
//! per-kind stage types, the parameter layout (`fx.stack.params`) and the UI
//! (`fx.stack.strip`, `fx.stack.visualize`) live with each kind; the signal
//! chain adoption reuses this unchanged (`fx.stack.signal-chain`).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

pub mod pool;
pub mod verify;

pub use pool::{LaneCtl, StagePool};

// ── Stage ─────────────────────────────────────────────────────────────────

/// One style of a kind, processing stereo f64 in place.
///
/// The stage owns its complete parameter state; the stack never reaches into
/// it (`fx.stack.params-share`). Latency is reported in samples and may
/// change when a latency-affecting setting changes (`fx.stack.latency`).
pub trait Stage {
    /// Process one block in place. `l` and `r` are the same length, at most
    /// the `max_block` given to [`Stack::prepare`].
    fn process(&mut self, l: &mut [f64], r: &mut [f64]);

    /// This stage's current latency, in samples.
    fn latency(&self) -> usize {
        0
    }

    /// Clear internal state (tails, envelopes) without touching parameters.
    fn reset(&mut self) {}
}

/// A stage slot: the processor plus its enable state. Enable transitions are
/// crossfaded by the stack over [`FADE_SAMPLES`] so bypass is click-free
/// (`fx.stack.process`, `fx.gain-comp.continuity`).
pub struct StageSlot<S> {
    pub stage: S,
    pub enabled: bool,
    /// 0..=1 bypass crossfade position (1 = fully processing).
    fade: f64,
}

impl<S> StageSlot<S> {
    pub fn new(stage: S) -> Self {
        Self {
            stage,
            enabled: true,
            fade: 1.0,
        }
    }
}

// ── Lane ──────────────────────────────────────────────────────────────────

/// A serial chain of stages, run in parallel with the other lanes.
pub struct Lane<S> {
    pub stages: Vec<StageSlot<S>>,
    /// Lane gain, linear (1.0 = 0 dB).
    pub gain: f64,
    pub mute: bool,
    pub solo: bool,
    /// Delay line compensating this lane up to the longest lane
    /// (`fx.stack.latency`). Sized at prepare.
    comp: LaneDelay,
    /// This lane's working copy of the input. Sized at prepare.
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    /// Pre-stage snapshot for the bypass crossfade. Sized at prepare.
    fade_l: Vec<f64>,
    fade_r: Vec<f64>,
}

impl<S: Stage> Lane<S> {
    pub fn new(stages: Vec<S>) -> Self {
        Self {
            stages: stages.into_iter().map(StageSlot::new).collect(),
            gain: 1.0,
            mute: false,
            solo: false,
            comp: LaneDelay::default(),
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            fade_l: Vec::new(),
            fade_r: Vec::new(),
        }
    }

    /// Sum of the enabled stages' latencies — serial stages add
    /// (`fx.stack.latency`).
    pub fn latency(&self) -> usize {
        self.stages
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.stage.latency())
            .sum()
    }

    /// Run this lane's serial chain over its scratch buffers.
    fn run(&mut self, frames: usize) {
        let scratch_l = &mut self.scratch_l[..frames];
        let scratch_r = &mut self.scratch_r[..frames];
        for slot in &mut self.stages {
            let target = if slot.enabled { 1.0 } else { 0.0 };
            if slot.fade == target {
                if slot.enabled {
                    slot.stage.process(scratch_l, scratch_r);
                }
                // Fully bypassed: identity, DSP skipped.
                continue;
            }
            // Transitioning: crossfade pre-stage ↔ processed over
            // FADE_SAMPLES, then reset a fully-bypassed stage so re-enabling
            // starts clean instead of replaying a stale tail.
            self.fade_l[..frames].copy_from_slice(scratch_l);
            self.fade_r[..frames].copy_from_slice(scratch_r);
            slot.stage.process(scratch_l, scratch_r);
            let step = 1.0 / FADE_SAMPLES;
            let dir = if slot.enabled { step } else { -step };
            let mut fade = slot.fade;
            for i in 0..frames {
                fade = (fade + dir).clamp(0.0, 1.0);
                scratch_l[i] = scratch_l[i] * fade + self.fade_l[i] * (1.0 - fade);
                scratch_r[i] = scratch_r[i] * fade + self.fade_r[i] * (1.0 - fade);
            }
            slot.fade = fade;
            if slot.fade == 0.0 {
                slot.stage.reset();
            }
        }
        self.comp.process(scratch_l, scratch_r);
    }
}

/// A plain stereo delay line for lane alignment.
#[derive(Default)]
struct LaneDelay {
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    pos: usize,
    delay: usize,
}

impl LaneDelay {
    fn prepare(&mut self, max_delay: usize) {
        let len = max_delay.max(1);
        self.buf_l = vec![0.0; len];
        self.buf_r = vec![0.0; len];
        self.pos = 0;
        self.delay = 0;
    }

    fn set_delay(&mut self, delay: usize) {
        self.delay = delay.min(self.buf_l.len().saturating_sub(1));
    }

    fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        if self.delay == 0 {
            return;
        }
        let len = self.buf_l.len();
        for i in 0..l.len() {
            let read = (self.pos + len - self.delay) % len;
            let (dl, dr) = (self.buf_l[read], self.buf_r[read]);
            self.buf_l[self.pos] = l[i];
            self.buf_r[self.pos] = r[i];
            l[i] = dl;
            r[i] = dr;
            self.pos = (self.pos + 1) % len;
        }
    }

    fn reset(&mut self) {
        self.buf_l.iter_mut().for_each(|x| *x = 0.0);
        self.buf_r.iter_mut().for_each(|x| *x = 0.0);
        self.pos = 0;
    }
}

// ── Sum law ───────────────────────────────────────────────────────────────

/// How parallel lanes are summed (`fx.stack.sum`, `fx.gain-comp.stack`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SumMode {
    /// `sum / N` over the active lanes — N copies of the same signal are as
    /// loud as one. The default: parallel styles of one source are highly
    /// correlated.
    #[default]
    Coherent,
    /// `sum / √N` — equal-power, for decorrelated lanes.
    Power,
    /// Raw sum, no normalization.
    Raw,
}

impl SumMode {
    /// The normalization factor for `n` active lanes.
    pub fn norm(self, n: usize) -> f64 {
        let n = n.max(1) as f64;
        match self {
            SumMode::Coherent => 1.0 / n,
            SumMode::Power => 1.0 / sqrt(n),
            SumMode::Raw => 1.0,
        }
    }
}

#[inline]
fn sqrt(x: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        x.sqrt()
    }
    #[cfg(not(feature = "std"))]
    {
        libm::sqrt(x)
    }
}

// ── Stack ─────────────────────────────────────────────────────────────────

/// Crossfade length for stage enable/disable, in samples (≈5 ms at 48 kHz —
/// the click-free floor of `fx.gain-comp.continuity`).
pub const FADE_SAMPLES: f64 = 240.0;

/// The stack: parallel lanes of serial stages (`fx.stack.model`,
/// `fx.stack.topology`).
// r[impl fx.stack.model]
// r[impl fx.stack.topology]
pub struct Stack<S> {
    pub lanes: Vec<Lane<S>>,
    pub sum_mode: SumMode,
    /// Output trim, linear.
    pub output_trim: f64,
    max_block: usize,
    max_latency: usize,
    prepared: bool,
}

impl<S: Stage> Stack<S> {
    /// A stack from serial chains — each inner `Vec` becomes one lane.
    pub fn new(lanes: Vec<Vec<S>>) -> Self {
        Self {
            lanes: lanes.into_iter().map(Lane::new).collect(),
            sum_mode: SumMode::default(),
            output_trim: 1.0,
            max_block: 0,
            max_latency: 0,
            prepared: false,
        }
    }

    /// Allocate every buffer the audio thread will touch
    /// (`fx.stack.process`): per-lane scratch at `max_block`, alignment
    /// delays at `max_latency` (the largest latency difference the stack
    /// will ever compensate — the kind's worst case).
    pub fn prepare(&mut self, max_block: usize, max_latency: usize) {
        self.max_block = max_block;
        self.max_latency = max_latency;
        for lane in &mut self.lanes {
            lane.scratch_l = vec![0.0; max_block];
            lane.scratch_r = vec![0.0; max_block];
            lane.fade_l = vec![0.0; max_block];
            lane.fade_r = vec![0.0; max_block];
            lane.comp.prepare(max_latency + 1);
        }
        self.update_alignment();
        self.prepared = true;
    }

    /// The stack's reported latency: the slowest lane (`fx.stack.latency`).
    // r[impl fx.stack.latency]
    pub fn latency(&self) -> usize {
        self.lanes.iter().map(Lane::latency).max().unwrap_or(0)
    }

    /// Re-derive each lane's alignment delay from the current stage
    /// latencies. Call after any latency-affecting change.
    pub fn update_alignment(&mut self) {
        let max = self.latency();
        for lane in &mut self.lanes {
            let lat = lane.latency();
            lane.comp.set_delay(max - lat);
        }
    }

    /// Lanes that currently sound: soloed lanes if any are, else the
    /// unmuted ones (`fx.stack.sum`).
    fn lane_active(&self, idx: usize) -> bool {
        let any_solo = self.lanes.iter().any(|l| l.solo);
        let lane = &self.lanes[idx];
        if any_solo { lane.solo } else { !lane.mute }
    }

    /// Process one block in place (`fx.stack.process`): every lane gets the
    /// same input, serial stages run in order in the lane's scratch, lanes
    /// are latency-aligned, summed by the sum law over the active lanes, and
    /// trimmed. Allocation-free after [`Stack::prepare`].
    ///
    /// Inactive (muted / un-soloed) lanes still run, so their stages keep
    /// their state warm and un-muting is instant; they contribute nothing to
    /// the sum. A stack with no audible lane outputs silence — muting every
    /// lane is a mute, not a bypass.
    // r[impl fx.stack.process]
    // r[impl fx.stack.sum]
    pub fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        debug_assert!(self.prepared, "Stack::process before prepare");
        debug_assert_eq!(l.len(), r.len());
        let frames = l.len().min(self.max_block);

        let n_active = (0..self.lanes.len())
            .filter(|&i| self.lane_active(i))
            .count();
        let norm = self.sum_mode.norm(n_active) * self.output_trim;
        let active: u64 = (0..self.lanes.len())
            .filter(|&i| self.lane_active(i))
            .fold(0u64, |m, i| m | (1 << i.min(63)));

        for lane in &mut self.lanes {
            lane.scratch_l[..frames].copy_from_slice(&l[..frames]);
            lane.scratch_r[..frames].copy_from_slice(&r[..frames]);
            lane.run(frames);
        }

        l[..frames].iter_mut().for_each(|x| *x = 0.0);
        r[..frames].iter_mut().for_each(|x| *x = 0.0);
        for (idx, lane) in self.lanes.iter().enumerate() {
            if idx < 64 && active & (1 << idx) == 0 {
                continue;
            }
            let g = lane.gain * norm;
            for i in 0..frames {
                l[i] += lane.scratch_l[i] * g;
                r[i] += lane.scratch_r[i] * g;
            }
        }
    }

    /// Reset every stage and delay line (parameters untouched).
    pub fn reset(&mut self) {
        for lane in &mut self.lanes {
            for slot in &mut lane.stages {
                slot.stage.reset();
                slot.fade = if slot.enabled { 1.0 } else { 0.0 };
            }
            lane.comp.reset();
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A gain stage: out = in × g.
    struct Gain(f64);
    impl Stage for Gain {
        fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
            l.iter_mut().for_each(|x| *x *= self.0);
            r.iter_mut().for_each(|x| *x *= self.0);
        }
    }

    /// A stage that delays by `n` samples and reports it.
    struct Delayed {
        line: Vec<f64>,
        n: usize,
    }
    impl Delayed {
        fn new(n: usize) -> Self {
            Self { line: vec![0.0; n], n }
        }
    }
    impl Stage for Delayed {
        fn process(&mut self, l: &mut [f64], _r: &mut [f64]) {
            if self.n == 0 {
                return;
            }
            for x in l.iter_mut() {
                self.line.push(*x);
                *x = self.line.remove(0);
            }
        }
        fn latency(&self) -> usize {
            self.n
        }
    }

    /// A stage that can be either, for mixed-lane tests.
    enum S {
        Gain(Gain),
        Delayed(Delayed),
    }
    impl Stage for S {
        fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
            match self {
                S::Gain(g) => g.process(l, r),
                S::Delayed(d) => d.process(l, r),
            }
        }
        fn latency(&self) -> usize {
            match self {
                S::Gain(_) => 0,
                S::Delayed(d) => d.latency(),
            }
        }
    }

    fn ramp(n: usize) -> (Vec<f64>, Vec<f64>) {
        let v: Vec<f64> = (0..n).map(|i| (i as f64 / n as f64) - 0.5).collect();
        (v.clone(), v)
    }

    // r[verify fx.stack.topology]
    #[test]
    fn serial_stages_compose_and_a_single_stage_is_the_identity_shape() {
        // One lane [×2, ×3] = ×6.
        let mut stack = Stack::new(vec![vec![Gain(2.0), Gain(3.0)]]);
        stack.prepare(64, 0);
        let (mut l, mut r) = ramp(64);
        let (dl, _) = ramp(64);
        stack.process(&mut l, &mut r);
        for i in 0..64 {
            assert!((l[i] - dl[i] * 6.0).abs() < 1e-12);
        }
    }

    // r[verify fx.stack.sum]
    // r[verify fx.gain-comp.stack]
    #[test]
    fn coherent_parallel_lanes_are_as_loud_as_one() {
        // Five identical unity lanes, coherent sum: output == input.
        let mut stack = Stack::new(vec![
            vec![Gain(1.0)],
            vec![Gain(1.0)],
            vec![Gain(1.0)],
            vec![Gain(1.0)],
            vec![Gain(1.0)],
        ]);
        stack.prepare(64, 0);
        let (mut l, mut r) = ramp(64);
        let (dl, _) = ramp(64);
        stack.process(&mut l, &mut r);
        for i in 0..64 {
            assert!((l[i] - dl[i]).abs() < 1e-12, "coherent sum not unity at {i}");
        }
    }

    // r[verify fx.stack.sum]
    #[test]
    fn power_sum_normalizes_by_sqrt_n() {
        let mut stack = Stack::new(vec![vec![Gain(1.0)], vec![Gain(1.0)]]);
        stack.sum_mode = SumMode::Power;
        stack.prepare(16, 0);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        stack.process(&mut l, &mut r);
        // 2 lanes / √2 = √2.
        assert!((l[8] - core::f64::consts::SQRT_2).abs() < 1e-12);
    }

    // r[verify fx.stack.sum]
    #[test]
    fn mute_and_solo_choose_the_audible_lanes() {
        // Lane 0 = ×1, lane 1 = ×3. Solo lane 1 → ×3 exactly (norm over the
        // soloed lane only).
        let mut stack = Stack::new(vec![vec![Gain(1.0)], vec![Gain(3.0)]]);
        stack.lanes[1].solo = true;
        stack.prepare(16, 0);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        stack.process(&mut l, &mut r);
        assert!((l[8] - 3.0).abs() < 1e-12, "solo: {}", l[8]);

        // Mute everything → silence, not dry.
        let mut stack = Stack::new(vec![vec![Gain(1.0)]]);
        stack.lanes[0].mute = true;
        stack.prepare(16, 0);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        stack.process(&mut l, &mut r);
        assert_eq!(l[8], 0.0, "muted stack must be silent");
    }

    // r[verify fx.stack.latency]
    #[test]
    fn short_lanes_are_delay_compensated_to_the_longest() {
        // Lane 0: 8-sample latency; lane 1: none. Compensated, the two unity
        // lanes stay phase-aligned: coherent sum == the delayed input.
        let mut stack = Stack::new(vec![
            vec![S::Delayed(Delayed::new(8))],
            vec![S::Gain(Gain(1.0))],
        ]);
        stack.prepare(64, 16);
        assert_eq!(stack.latency(), 8);
        let (mut l, mut r) = ramp(64);
        let (dl, _) = ramp(64);
        stack.process(&mut l, &mut r);
        for i in 8..64 {
            assert!(
                (l[i] - dl[i - 8]).abs() < 1e-12,
                "lanes not aligned at {i}: {} vs {}",
                l[i],
                dl[i - 8]
            );
        }
        // The first 8 samples are the lines' zero fill, not garbage.
        for i in 0..8 {
            assert!(l[i].abs() < 1e-12);
        }
    }

    // r[verify fx.stack.process]
    #[test]
    fn disabling_a_stage_crossfades_instead_of_clicking() {
        let mut stack = Stack::new(vec![vec![Gain(0.0)]]); // stage silences
        stack.prepare(1024, 0);
        // Run a block enabled: output is 0.
        let mut l = vec![1.0; 512];
        let mut r = vec![1.0; 512];
        stack.process(&mut l, &mut r);
        assert_eq!(l[100], 0.0);
        // Disable: the next block ramps smoothly from 0 toward dry 1.0 —
        // no sample-to-sample jump bigger than the fade step.
        stack.lanes[0].stages[0].enabled = false;
        let mut l = vec![1.0; 512];
        let mut r = vec![1.0; 512];
        stack.process(&mut l, &mut r);
        assert!(l[0] < 0.02, "fade did not start near the wet value: {}", l[0]);
        assert!(l[511] > 0.9, "fade did not reach dry: {}", l[511]);
        for w in l.windows(2) {
            assert!((w[1] - w[0]).abs() < 0.01, "click in the bypass fade");
        }
    }

    // r[verify fx.stack.model]
    #[test]
    fn one_lane_one_stage_is_the_plain_plugin() {
        let mut stack = Stack::new(vec![vec![Gain(0.5)]]);
        stack.prepare(32, 0);
        let mut l = vec![1.0; 32];
        let mut r = vec![1.0; 32];
        stack.process(&mut l, &mut r);
        assert!((l[16] - 0.5).abs() < 1e-12);
    }
}
