//! A block's bypass, carried by the block itself.
//!
//! The rig used to bypass a block by telling the host to skip its slot. That
//! flag belonged to the slot, not the chain, so a patch switch had to clear
//! every flag and re-apply the new patch's afterwards — a window in which
//! the new patch played its bypassed tremolo and spare drives — and skipping
//! a delay or reverb cut its tail dead.
//!
//! A [`BlockGate`] wraps each block and holds its bypass in a [`GateCtl`]
//! the rig keeps per chain, so a chain arrives with its bypass already set
//! and a switch never touches it. Two kinds:
//!
//! - [`GateMode::Hard`] — anything that is not a time effect. Bypassed, it
//!   passes its input straight through without running the block, exactly as
//!   the host's skip did (the dual-amp and time stages rely on a skipped
//!   block doing nothing); engaging or bypassing crossfades over a few ms
//!   rather than stepping.
//! - [`GateMode::Trails`] — delays and reverbs. Bypassing mutes the block's
//!   *input*, not its output: the repeats and the tail already in it ring
//!   out, and the dry passes (at `pass`: 1, or 0 inside a time stage's second
//!   block, where the stage adds the dry itself). Once the tail has died the
//!   block stops being run at all.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginInstance, PluginParamInfo,
};

/// Ramp length for engaging / bypassing.
const RAMP_SECS: f32 = 0.005;
/// A bypassed time effect quieter than this (−90 dBFS) …
const QUIET: f32 = 3.2e-5;
/// … for this long has finished its tail and stops being run.
const QUIET_HOLD_SECS: f32 = 2.0;

/// A block's bypass, shared between the rig (control) and the block's
/// [`BlockGate`] (audio). Lock-free.
#[derive(Debug, Default)]
pub struct GateCtl {
    bypass: AtomicBool,
    /// Take the new state at once, without the ramp — for a chain that is
    /// not playing, so the next time it does it starts in its state.
    snap: AtomicBool,
    /// Trails: ramp the block's input in from nothing — a delay or reverb
    /// just cleared and switched in, so its first repeat or reflection
    /// arrives faded in rather than as a step.
    fade_in: AtomicBool,
}

impl GateCtl {
    #[must_use]
    pub fn new(bypassed: bool) -> Arc<Self> {
        Arc::new(Self {
            bypass: AtomicBool::new(bypassed),
            snap: AtomicBool::new(true),
            fade_in: AtomicBool::new(false),
        })
    }

    /// A time effect armed empty: ramp its input in (over the gate's ramp)
    /// instead of starting it at full level. Does nothing to a bypassed or
    /// a [`GateMode::Hard`] block.
    pub fn fade_in(&self) {
        self.fade_in.store(true, Ordering::Release);
    }

    /// Bypass (or engage) the block, ramped.
    pub fn set(&self, bypassed: bool) {
        self.bypass.store(bypassed, Ordering::Relaxed);
    }

    /// Bypass (or engage) the block at once — only for a chain nobody hears.
    pub fn set_now(&self, bypassed: bool) {
        self.bypass.store(bypassed, Ordering::Relaxed);
        self.snap.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn bypassed(&self) -> bool {
        self.bypass.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateMode {
    Hard,
    Trails,
}

/// A block wrapped with its bypass. See the module docs.
pub struct BlockGate {
    inner: Box<dyn PluginInstance>,
    ctl: Arc<GateCtl>,
    mode: GateMode,
    /// Trails: the dry passed while bypassed.
    pass: f32,
    /// 1 = engaged, 0 = bypassed; ramps between.
    g: f32,
    step: f32,
    /// Trails: samples the bypassed block has been quiet for.
    quiet: usize,
    quiet_hold: usize,
    /// Trails, bypassed: the tail has died; the block is not run.
    idle: bool,
    // Reserved once (no audio-thread allocation).
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
}

impl BlockGate {
    #[must_use]
    pub fn new(
        inner: Box<dyn PluginInstance>,
        ctl: Arc<GateCtl>,
        mode: GateMode,
        pass: f32,
        max_block: usize,
    ) -> Self {
        let mut gate = Self {
            inner,
            ctl,
            mode,
            pass,
            g: 1.0,
            step: 1.0,
            quiet: 0,
            quiet_hold: usize::MAX,
            idle: false,
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            out_l: vec![0.0; max_block],
            out_r: vec![0.0; max_block],
        };
        gate.set_rate(48_000.0);
        gate
    }

    fn set_rate(&mut self, sample_rate: f64) {
        let sr = sample_rate.max(1.0) as f32;
        self.step = 1.0 / (RAMP_SECS * sr).max(1.0);
        self.quiet_hold = (QUIET_HOLD_SECS * sr) as usize;
    }

    /// Run the block on `in` into the gate's own output scratch, for a block
    /// whose output is mixed rather than passed on as is.
    fn run_into_scratch(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        n: usize,
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        self.inner.process_block(
            in_l,
            in_r,
            &mut self.out_l[..n],
            &mut self.out_r[..n],
            events,
        )
    }
}

impl PluginInstance for BlockGate {
    fn descriptor(&self) -> PluginDescriptor {
        self.inner.descriptor()
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        self.inner.params()
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.inner.param_value(id)
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        self.inner.value_to_text(id, value)
    }
    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        self.inner.text_to_value(id, text)
    }
    fn latency(&mut self) -> u32 {
        self.inner.latency()
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.set_rate(sample_rate);
        self.quiet = 0;
        self.idle = false;
        self.inner.prepare(sample_rate, block_size)
    }
    fn is_prepared(&self) -> bool {
        self.inner.is_prepared()
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.inner.load_state(state)
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.inner.save_state()
    }
    fn deactivate(&mut self) {
        self.inner.deactivate();
    }
    // The block's own concrete type, so a downcast (a NAM's trims) still
    // reaches it through the gate.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        self.inner.as_any_mut()
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        let bypassed = self.ctl.bypass.load(Ordering::Relaxed);
        let target = if bypassed { 0.0 } else { 1.0 };
        if self.ctl.snap.swap(false, Ordering::Acquire) {
            self.g = target;
            self.quiet = 0;
            self.idle = false;
        }
        if self.ctl.fade_in.swap(false, Ordering::Acquire)
            && self.mode == GateMode::Trails
            && !bypassed
        {
            self.g = 0.0;
        }
        if !bypassed {
            self.idle = false;
            self.quiet = 0;
        }
        let settled = (self.g - target).abs() < f32::EPSILON;

        // Engaged and settled: the block, untouched.
        if settled && !bypassed {
            return self.inner.process_block(in_l, in_r, out_l, out_r, events);
        }
        if n > self.in_l.len() {
            // Larger than the scratch: no ramp, no trails — the old skip.
            self.g = target;
            return if bypassed {
                out_l[..n].copy_from_slice(&in_l[..n]);
                out_r[..n].copy_from_slice(&in_r[..n]);
                Ok(())
            } else {
                self.inner.process_block(in_l, in_r, out_l, out_r, events)
            };
        }

        match self.mode {
            GateMode::Hard => {
                if settled {
                    // Bypassed: the input through; the block only hears its
                    // param writes (so a knob turned while it is off holds).
                    if !events.params.is_empty() {
                        let _ = self.run_into_scratch(in_l, in_r, n, events);
                    }
                    out_l[..n].copy_from_slice(&in_l[..n]);
                    out_r[..n].copy_from_slice(&in_r[..n]);
                    return Ok(());
                }
                let result = self.run_into_scratch(in_l, in_r, n, events);
                for i in 0..n {
                    self.g = step_toward(self.g, target, self.step);
                    out_l[i] = self.g * self.out_l[i] + (1.0 - self.g) * in_l[i];
                    out_r[i] = self.g * self.out_r[i] + (1.0 - self.g) * in_r[i];
                }
                result
            }
            GateMode::Trails => {
                if self.idle {
                    if !events.params.is_empty() {
                        self.in_l[..n].fill(0.0);
                        self.in_r[..n].fill(0.0);
                        let (il, ir) = (std::mem::take(&mut self.in_l), std::mem::take(&mut self.in_r));
                        let _ = self.run_into_scratch(&il[..n], &ir[..n], n, events);
                        self.in_l = il;
                        self.in_r = ir;
                    }
                    for i in 0..n {
                        out_l[i] = self.pass * in_l[i];
                        out_r[i] = self.pass * in_r[i];
                    }
                    return Ok(());
                }
                // The block hears g·x; the dry the block no longer passes is
                // made up here at `pass`.
                let g0 = self.g;
                let mut g = g0;
                for i in 0..n {
                    g = step_toward(g, target, self.step);
                    self.in_l[i] = g * in_l[i];
                    self.in_r[i] = g * in_r[i];
                }
                let (il, ir) = (std::mem::take(&mut self.in_l), std::mem::take(&mut self.in_r));
                let result = self.inner.process_block(&il[..n], &ir[..n], out_l, out_r, events);
                self.in_l = il;
                self.in_r = ir;
                let mut peak = 0.0f32;
                g = g0;
                for i in 0..n {
                    g = step_toward(g, target, self.step);
                    peak = peak.max(out_l[i].abs()).max(out_r[i].abs());
                    let made_up = (1.0 - g) * self.pass;
                    out_l[i] += made_up * in_l[i];
                    out_r[i] += made_up * in_r[i];
                }
                self.g = g;
                if bypassed && self.g <= 0.0 {
                    if peak < QUIET {
                        self.quiet = self.quiet.saturating_add(n);
                        if self.quiet >= self.quiet_hold {
                            self.idle = true;
                        }
                    } else {
                        self.quiet = 0;
                    }
                }
                result
            }
        }
    }
}

fn step_toward(g: f32, target: f32, step: f32) -> f32 {
    if g < target {
        (g + step).min(target)
    } else {
        (g - step).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginFormat;

    /// The dry plus a slow-decaying "tail" (a leaky integrator of the
    /// input): `y = x + e`, `e ← 0.999·e + 0.001·x`.
    struct Echo {
        e: f32,
    }
    impl PluginInstance for Echo {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "echo".into(),
                name: "echo".into(),
                vendor: String::new(),
                version: String::new(),
                format: PluginFormat::Clap,
            }
        }
        fn params(&mut self) -> Vec<PluginParamInfo> {
            Vec::new()
        }
        fn param_value(&mut self, _: u32) -> Option<f64> {
            None
        }
        fn value_to_text(&mut self, _: u32, _: f64) -> Option<String> {
            None
        }
        fn text_to_value(&mut self, _: u32, _: &str) -> Option<f64> {
            None
        }
        fn latency(&mut self) -> u32 {
            0
        }
        fn prepare(&mut self, _: f64, _: u32) -> Result<(), PluginError> {
            Ok(())
        }
        fn is_prepared(&self) -> bool {
            true
        }
        fn process_block(
            &mut self,
            in_l: &[f32],
            _in_r: &[f32],
            out_l: &mut [f32],
            out_r: &mut [f32],
            _: &PluginEvents<'_>,
        ) -> Result<(), PluginError> {
            for i in 0..in_l.len() {
                let y = in_l[i] + self.e;
                self.e = 0.999 * self.e + 0.001 * in_l[i];
                out_l[i] = y;
                out_r[i] = y;
            }
            Ok(())
        }
        fn deactivate(&mut self) {}
    }

    fn run(gate: &mut BlockGate, x: f32, n: usize) -> Vec<f32> {
        let input = vec![x; n];
        let (mut ol, mut or) = (vec![0.0; n], vec![0.0; n]);
        gate.process_block(&input, &input, &mut ol, &mut or, &PluginEvents::EMPTY)
            .unwrap();
        ol
    }

    #[test]
    fn a_bypassed_delay_rings_out_and_passes_the_dry() {
        let ctl = GateCtl::new(false);
        let mut gate = BlockGate::new(Box::new(Echo { e: 0.0 }), ctl.clone(), GateMode::Trails, 1.0, 1024);
        gate.prepare(48_000.0, 512).unwrap();
        run(&mut gate, 1.0, 512);
        ctl.set(true);
        let out = run(&mut gate, 1.0, 512);
        // Past the 5 ms ramp: the dry (1.0) plus a tail still above zero.
        let late = out[400];
        assert!(late > 1.0, "tail rings over the dry: {late}");
        // Silence in: the tail alone, decaying — not cut.
        let silent = run(&mut gate, 0.0, 16);
        assert!(silent[0] > 0.0);
    }

    #[test]
    fn a_hard_gate_passes_the_input_once_settled_without_a_step() {
        let ctl = GateCtl::new(false);
        let mut gate = BlockGate::new(Box::new(Echo { e: 0.0 }), ctl.clone(), GateMode::Hard, 1.0, 1024);
        gate.prepare(48_000.0, 512).unwrap();
        let before = run(&mut gate, 0.25, 512);
        ctl.set(true);
        let out = run(&mut gate, 0.25, 512);
        let jump = (out[0] - before[511]).abs();
        assert!(jump < 0.01, "no step at the switch: {jump}");
        assert!((out[511] - 0.25).abs() < 1e-6, "settled: the input");
    }

    #[test]
    fn a_snap_lands_at_once() {
        let ctl = GateCtl::new(false);
        let mut gate = BlockGate::new(Box::new(Echo { e: 0.0 }), ctl.clone(), GateMode::Hard, 1.0, 1024);
        gate.prepare(48_000.0, 512).unwrap();
        ctl.set_now(true);
        let out = run(&mut gate, 0.5, 4);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }
}
