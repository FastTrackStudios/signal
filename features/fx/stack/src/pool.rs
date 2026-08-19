//! The pooled stack — `Stack` for param-driven topology.
//!
//! A plugin's host parameters own the topology (`fx.stack.params`): each of a
//! fixed pool of `N` stages carries "am I in use" and "which lane am I on" as
//! parameters, and those change on the audio thread between blocks. The
//! Vec-of-lanes [`crate::Stack`] would need reallocation to follow; this pool
//! never moves a stage — topology is just the `in_use` / `lane_of` arrays,
//! re-read every block. Everything is fixed-size at construction, so
//! processing is allocation-free from the first block
//! (`fx.stack.process`).
//!
//! Semantics match [`crate::Stack`]: within a lane, stages run serially in
//! pool order; lanes run the same input in parallel and are summed by the
//! [`SumMode`] law over the active lanes; short lanes are delay-compensated
//! to the longest (`fx.stack.latency`); stage enable/disable crossfades
//! (`FADE_SAMPLES`).

use alloc::vec;
use alloc::vec::Vec;

use crate::{Stage, SumMode, FADE_SAMPLES};

/// Per-lane mix controls.
#[derive(Clone, Copy, Debug)]
pub struct LaneCtl {
    /// Linear gain (1.0 = 0 dB).
    pub gain: f64,
    pub mute: bool,
    pub solo: bool,
}

impl Default for LaneCtl {
    fn default() -> Self {
        Self {
            gain: 1.0,
            mute: false,
            solo: false,
        }
    }
}

struct PoolSlot<S> {
    stage: S,
    /// Host-side enable. Off = the stage is not part of the topology at all
    /// (an unused pool slot); distinct from a bypassed-but-present stage.
    in_use: bool,
    /// Stage bypass (present in the topology, crossfaded to identity).
    enabled: bool,
    lane: usize,
    /// 0..=1 bypass crossfade (1 = fully processing).
    fade: f64,
}

struct PoolLane {
    ctl: LaneCtl,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    delay: LaneAlign,
}

/// Plain stereo delay for lane alignment (same as the Vec stack's).
struct LaneAlign {
    buf_l: Vec<f64>,
    buf_r: Vec<f64>,
    pos: usize,
    delay: usize,
}

impl LaneAlign {
    fn new(max_delay: usize) -> Self {
        let len = max_delay.max(1);
        Self {
            buf_l: vec![0.0; len],
            buf_r: vec![0.0; len],
            pos: 0,
            delay: 0,
        }
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

/// A fixed pool of `N` stages arranged into up to `N` lanes by per-stage
/// `lane` assignments (`fx.stack.model`, `fx.stack.topology`).
// r[impl fx.stack.model]
// r[impl fx.stack.topology]
// r[impl fx.stack.limits]
pub struct StagePool<S> {
    slots: Vec<PoolSlot<S>>,
    lanes: Vec<PoolLane>,
    pub sum_mode: SumMode,
    /// Output trim, linear.
    pub output_trim: f64,
    /// Pre-stage snapshot for the bypass crossfade.
    fade_l: Vec<f64>,
    fade_r: Vec<f64>,
    max_block: usize,
    prepared: bool,
}

impl<S: Stage> StagePool<S> {
    /// A pool of `stages.len()` slots. Slot 0 starts in use on lane 0 (the
    /// single-stage plugin, `fx.stack.model`); the rest start unused.
    pub fn new(stages: Vec<S>) -> Self {
        let n = stages.len();
        let slots = stages
            .into_iter()
            .enumerate()
            .map(|(i, stage)| PoolSlot {
                stage,
                in_use: i == 0,
                enabled: true,
                lane: 0,
                fade: 1.0,
            })
            .collect();
        Self {
            slots,
            lanes: (0..n).map(|_| PoolLane {
                ctl: LaneCtl::default(),
                scratch_l: Vec::new(),
                scratch_r: Vec::new(),
                delay: LaneAlign::new(1),
            }).collect(),
            sum_mode: SumMode::default(),
            output_trim: 1.0,
            fade_l: Vec::new(),
            fade_r: Vec::new(),
            max_block: 0,
            prepared: false,
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Allocate every buffer processing will touch. `max_latency` bounds the
    /// per-lane alignment delay (the kind's worst case).
    pub fn prepare(&mut self, max_block: usize, max_latency: usize) {
        self.max_block = max_block;
        for lane in &mut self.lanes {
            lane.scratch_l = vec![0.0; max_block];
            lane.scratch_r = vec![0.0; max_block];
            lane.delay = LaneAlign::new(max_latency + 1);
        }
        self.fade_l = vec![0.0; max_block];
        self.fade_r = vec![0.0; max_block];
        self.update_alignment();
        self.prepared = true;
    }

    /// The stage in slot `i` (for parameter sync).
    pub fn stage_mut(&mut self, i: usize) -> Option<&mut S> {
        self.slots.get_mut(i).map(|s| &mut s.stage)
    }

    pub fn stage(&self, i: usize) -> Option<&S> {
        self.slots.get(i).map(|s| &s.stage)
    }

    /// Set slot `i`'s topology: in the stack or not, bypassed or not, and
    /// which lane it feeds. Call every block from the param sync — cheap.
    pub fn set_slot(&mut self, i: usize, in_use: bool, enabled: bool, lane: usize) {
        let n = self.slots.len();
        if let Some(slot) = self.slots.get_mut(i) {
            // A slot entering the topology starts clean and fully faded in —
            // it was not audible before, so there is no transition to hide.
            if in_use && !slot.in_use {
                slot.stage.reset();
                slot.fade = if enabled { 1.0 } else { 0.0 };
            }
            slot.in_use = in_use;
            slot.enabled = enabled;
            slot.lane = lane.min(n - 1);
        }
    }

    /// Set lane `l`'s mix controls. Call every block from the param sync.
    pub fn set_lane(&mut self, l: usize, ctl: LaneCtl) {
        if let Some(lane) = self.lanes.get_mut(l) {
            lane.ctl = ctl;
        }
    }

    /// Latency of lane `l`: enabled, in-use stages on it, serially
    /// (`fx.stack.latency`).
    fn lane_latency(&self, l: usize) -> usize {
        self.slots
            .iter()
            .filter(|s| s.in_use && s.enabled && s.lane == l)
            .map(|s| s.stage.latency())
            .sum()
    }

    /// Whether lane `l` is part of the topology at all.
    fn lane_populated(&self, l: usize) -> bool {
        self.slots.iter().any(|s| s.in_use && s.lane == l)
    }

    /// The stack's reported latency: the slowest populated lane.
    // r[impl fx.stack.latency]
    pub fn latency(&self) -> usize {
        (0..self.lanes.len())
            .filter(|&l| self.lane_populated(l))
            .map(|l| self.lane_latency(l))
            .max()
            .unwrap_or(0)
    }

    /// Re-derive lane alignment delays. Call after any latency-affecting
    /// change (topology or a stage's lookahead-style setting).
    pub fn update_alignment(&mut self) {
        let max = self.latency();
        for l in 0..self.lanes.len() {
            let lat = self.lane_latency(l);
            let d = max.saturating_sub(lat);
            let lane = &mut self.lanes[l];
            lane.delay.delay = d.min(lane.delay.buf_l.len().saturating_sub(1));
        }
    }

    /// Whether lane `l` currently sounds (solo beats mute across the
    /// populated lanes).
    fn lane_active(&self, l: usize) -> bool {
        if !self.lane_populated(l) {
            return false;
        }
        let any_solo = (0..self.lanes.len())
            .any(|k| self.lane_populated(k) && self.lanes[k].ctl.solo);
        if any_solo {
            self.lanes[l].ctl.solo
        } else {
            !self.lanes[l].ctl.mute
        }
    }

    /// Process one block in place — same law as [`crate::Stack::process`].
    /// A pool with no populated lane passes the input through unchanged (an
    /// empty stack is a wire, not a mute — the plugin's bypass story).
    // r[impl fx.stack.process]
    // r[impl fx.stack.sum]
    pub fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
        debug_assert!(self.prepared, "StagePool::process before prepare");
        debug_assert_eq!(l.len(), r.len());
        let frames = l.len().min(self.max_block);
        let n_lanes = self.lanes.len();

        if !(0..n_lanes).any(|k| self.lane_populated(k)) {
            return;
        }

        let n_active = (0..n_lanes).filter(|&k| self.lane_active(k)).count();
        let norm = self.sum_mode.norm(n_active) * self.output_trim;

        for lane_idx in 0..n_lanes {
            if !self.lane_populated(lane_idx) {
                continue;
            }
            // Fill the lane scratch with the input, then run its serial
            // stages in pool order.
            {
                let lane = &mut self.lanes[lane_idx];
                lane.scratch_l[..frames].copy_from_slice(&l[..frames]);
                lane.scratch_r[..frames].copy_from_slice(&r[..frames]);
            }
            for si in 0..self.slots.len() {
                if !(self.slots[si].in_use && self.slots[si].lane == lane_idx) {
                    continue;
                }
                let slot = &mut self.slots[si];
                let lane = &mut self.lanes[lane_idx];
                let scratch_l = &mut lane.scratch_l[..frames];
                let scratch_r = &mut lane.scratch_r[..frames];
                let target = if slot.enabled { 1.0 } else { 0.0 };
                if slot.fade == target {
                    if slot.enabled {
                        slot.stage.process(scratch_l, scratch_r);
                    }
                    continue;
                }
                // Bypass crossfade (`fx.gain-comp.continuity`).
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
            let lane = &mut self.lanes[lane_idx];
            lane.delay
                .process(&mut lane.scratch_l[..frames], &mut lane.scratch_r[..frames]);
        }

        // Sum the audible lanes.
        l[..frames].iter_mut().for_each(|x| *x = 0.0);
        r[..frames].iter_mut().for_each(|x| *x = 0.0);
        for lane_idx in 0..n_lanes {
            if !self.lane_active(lane_idx) {
                continue;
            }
            let lane = &self.lanes[lane_idx];
            let g = lane.ctl.gain * norm;
            for i in 0..frames {
                l[i] += lane.scratch_l[i] * g;
                r[i] += lane.scratch_r[i] * g;
            }
        }
    }

    /// Reset every stage and delay line (parameters untouched).
    pub fn reset(&mut self) {
        for slot in &mut self.slots {
            slot.stage.reset();
            slot.fade = if slot.enabled { 1.0 } else { 0.0 };
        }
        for lane in &mut self.lanes {
            lane.delay.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Gain(f64);
    impl Stage for Gain {
        fn process(&mut self, l: &mut [f64], r: &mut [f64]) {
            l.iter_mut().for_each(|x| *x *= self.0);
            r.iter_mut().for_each(|x| *x *= self.0);
        }
    }

    fn pool(gains: &[f64]) -> StagePool<Gain> {
        let mut p = StagePool::new(gains.iter().map(|&g| Gain(g)).collect());
        p.prepare(64, 16);
        p
    }

    // r[verify fx.stack.model]
    #[test]
    fn slot_zero_alone_is_the_plain_plugin() {
        let mut p = pool(&[0.5, 3.0]);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        p.process(&mut l, &mut r);
        assert!((l[8] - 0.5).abs() < 1e-12);
    }

    // r[verify fx.stack.topology]
    #[test]
    fn serial_within_a_lane_parallel_across_lanes() {
        // Slots 0,1 on lane 0 (×2·×3 = ×6); slot 2 on lane 1 (×12).
        // Coherent sum of two lanes: (6 + 12)/2 = 9.
        let mut p = pool(&[2.0, 3.0, 12.0]);
        p.set_slot(1, true, true, 0);
        p.set_slot(2, true, true, 1);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        p.process(&mut l, &mut r);
        assert!((l[8] - 9.0).abs() < 1e-12, "{}", l[8]);
    }

    // r[verify fx.stack.sum]
    #[test]
    fn lane_gain_mute_and_solo() {
        let mut p = pool(&[1.0, 3.0]);
        p.set_slot(1, true, true, 1);
        p.set_lane(1, LaneCtl { gain: 1.0, mute: false, solo: true });
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        p.process(&mut l, &mut r);
        assert!((l[8] - 3.0).abs() < 1e-12, "solo lane 1: {}", l[8]);
    }

    // r[verify fx.stack.process]
    #[test]
    fn an_empty_pool_is_a_wire() {
        let mut p = pool(&[2.0]);
        p.set_slot(0, false, true, 0);
        let mut l = vec![0.7; 16];
        let mut r = vec![0.7; 16];
        p.process(&mut l, &mut r);
        assert_eq!(l[8], 0.7);
    }

    // r[verify fx.stack.process]
    #[test]
    fn removing_a_stage_from_the_topology_does_not_leak_state() {
        // Slot 1 in use, then removed, then re-added: it must come back
        // clean (reset), fully faded per its enable.
        let mut p = pool(&[1.0, 5.0]);
        p.set_slot(1, true, true, 0);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        p.process(&mut l, &mut r);
        assert!((l[8] - 5.0).abs() < 1e-12);
        p.set_slot(1, false, true, 0);
        let mut l = vec![1.0; 16];
        let mut r = vec![1.0; 16];
        p.process(&mut l, &mut r);
        assert!((l[8] - 1.0).abs() < 1e-12);
    }
}
