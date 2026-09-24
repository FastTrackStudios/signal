//! The switching machinery never allocates on the audio thread.
//!
//! A counting global allocator, armed only around the calls the audio
//! thread makes — `TailStage::process` and `BlockGate::process_block` —
//! through switches, fades, voices finishing and being made room for,
//! bypasses and their tails dying. Any allocation there is a potential
//! xrun, and the design rests on there being none: voices arrive whole from
//! the control thread, every scratch buffer is reserved up front.
//!
//! Its own test binary: a global allocator is per binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginParamInfo,
};
use signal_sampler::block_gate::{BlockGate, GateCtl, GateMode};
use signal_sampler::tail_stage::{InputShare, TailStage, Voice};
use signal_sampler::time_stage;

struct Counting;

// Both per thread: the tests run in parallel, and one test's allocations
// must not land in another's count.
thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    if ARMED.with(Cell::get) {
        ALLOCS.with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count();
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Run `f` as the audio thread would, counting what it allocates or frees.
fn audio<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCS.with(Cell::get);
    ARMED.with(|a| a.set(true));
    let r = f();
    ARMED.with(|a| a.set(false));
    (r, ALLOCS.with(Cell::get) - before)
}

/// A decaying echo standing in for a delay or reverb (`y = x + e`).
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
            self.e = 0.99 * self.e + 0.01 * in_l[i];
            out_l[i] = y;
            out_r[i] = y;
        }
        Ok(())
    }
    fn deactivate(&mut self) {}
}

fn echo() -> Box<dyn PluginInstance> {
    Box::new(Echo { e: 0.0 })
}

const N: usize = 128;

#[test]
fn switching_and_ringing_out_allocate_nothing() {
    let input = InputShare::new(1024);
    let mut stage = TailStage::new(input.clone(), 48_000.0, 1024);
    let x = vec![0.1f32; N];
    let (mut ol, mut or) = (vec![0.0f32; N], vec![0.0f32; N]);
    let mut finished = Vec::with_capacity(16);
    let mut total = 0;
    // Seven switches (more than the four tails the stage holds), each
    // followed by enough audio to fade, ring, finish and be collected.
    for k in 0..7u32 {
        let voice = Voice::new(k, vec![Some(echo()), Some(echo())], 1);
        // The control thread's part: handing over, collecting.
        let evicted = stage.switch(Some(voice), -3.0 + k as f32);
        drop(evicted);
        for _ in 0..(48_000 * 3 / N) {
            let ((), n) = audio(|| {
                input.write(&x, &x);
                stage.process(&x, &x, &mut ol, &mut or);
            });
            total += n;
        }
        stage.collect_finished(&mut finished);
        finished.clear();
    }
    assert_eq!(total, 0, "the output stage allocated on the audio thread");
}

#[test]
fn bypass_and_its_tail_allocate_nothing() {
    for mode in [GateMode::Trails, GateMode::Hard] {
        let ctl = GateCtl::new(false);
        let mut gate = BlockGate::new(echo(), ctl.clone(), mode, 1.0, 2048);
        gate.prepare(48_000.0, 512).unwrap();
        let x = vec![0.1f32; N];
        let (mut ol, mut or) = (vec![0.0f32; N], vec![0.0f32; N]);
        let params = [(0u32, 0.5f64)];
        let mut total = 0;
        // Engaged, bypassed (ramping, ringing, idle — with a param write
        // while idle), engaged again.
        for (bypass, blocks) in [(false, 100), (true, 48_000 * 3 / N), (false, 100)] {
            ctl.set(bypass);
            for b in 0..blocks {
                let ev = PluginEvents {
                    params: if b == blocks / 2 { &params } else { &[] },
                    ..PluginEvents::default()
                };
                let (r, n) = audio(|| gate.process_block(&x, &x, &mut ol, &mut or, &ev));
                r.unwrap();
                total += n;
            }
        }
        assert_eq!(total, 0, "{mode:?} gate allocated on the audio thread");
    }
}

#[test]
fn a_time_stage_allocates_nothing() {
    let mut boxes = vec![Some(echo()), Some(echo())];
    time_stage::wrap(&mut boxes, &["DLY 1", "DLY 2"], 1024);
    let mut boxes: Vec<Box<dyn PluginInstance>> = boxes.into_iter().flatten().collect();
    let x = vec![0.1f32; N];
    let (mut a_l, mut a_r) = (vec![0.0f32; N], vec![0.0f32; N]);
    let (mut b_l, mut b_r) = (vec![0.0f32; N], vec![0.0f32; N]);
    let mut total = 0;
    for _ in 0..200 {
        let ((), n) = audio(|| {
            let ev = PluginEvents::default();
            boxes[0].process_block(&x, &x, &mut a_l, &mut a_r, &ev).unwrap();
            boxes[1].process_block(&a_l, &a_r, &mut b_l, &mut b_r, &ev).unwrap();
        });
        total += n;
    }
    assert_eq!(total, 0, "the time stage allocated on the audio thread");
}

#[test]
fn the_counter_counts() {
    let (v, n) = audio(|| vec![1u8; 64]);
    drop(v);
    assert!(n >= 1, "an allocation inside `audio` is seen");
    let _ = Arc::new(());
}
