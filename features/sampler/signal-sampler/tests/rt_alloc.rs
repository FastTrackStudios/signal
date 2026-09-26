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

/// A reload committed while the rig plays: the rendering that follows —
/// the new chain crossfading in, the old one ringing out as a tail, the
/// tail finishing — allocates nothing. The commit itself (installing,
/// swapping, retiring) is control-thread work: the swap happens under the
/// renderer's lock, but on the thread that commits.
#[test]
fn rendering_through_reload_commits_allocates_nothing() {
    use signal_proto::block::BlockType;
    use signal_sampler::{GuitarRig, ProfileRig, RigBlock, RigPatch, RigProfile};

    const SR: u32 = 48_000;
    const BLOCK: usize = 128;
    let verb = |decay: f32| {
        RigBlock::effect(BlockType::Reverb, "VERB 1")
            .with_param("mix", "1")
            .with_param("level", "-6")
            .with_param("decay", decay.to_string())
    };
    let delay = |ms: f32| {
        RigBlock::effect(BlockType::Delay, "DLY 1")
            .with_param("mix", "1")
            .with_param("time", ms.to_string())
            .with_param("feedback", "0.4")
    };
    let gain = || RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "3");
    let profile = |decay: f32, ms: f32| {
        RigProfile::new("Rt")
            .with_patch(RigPatch::new("Lead").with_block(gain()).with_block(verb(decay)))
            .with_patch(RigPatch::new("Clean").with_block(gain()).with_block(delay(ms)))
    };
    let mut prig = ProfileRig::new(GuitarRig::open_offline(SR).expect("offline rig"));
    prig.set_level_match(false);
    prig.load_profile(profile(0.8, 300.0), None).expect("loads");
    let sine: Vec<f32> = (0..SR as usize)
        .map(|i| 0.2 * (std::f32::consts::TAU * 110.0 * i as f32 / SR as f32).sin())
        .collect();
    prig.rig().start_test_signal(Arc::new(sine));
    // Warm up: first-use setup (lazy statics, the first render) is not the
    // steady state being checked.
    prig.rig().render_offline(SR as usize / 2);

    // Allocations per block. daw's `ProjectRenderer::render_block` returns
    // its mix as a fresh buffer — a fixed allocation every block, live and
    // offline alike, and daw's to remove — so what is checked is that no
    // block allocates beyond that fixed floor.
    let render = |prig: &ProfileRig, blocks: usize, out: &mut Vec<usize>| {
        for _ in 0..blocks {
            let ((), a) = audio(|| prig.rig().render_offline(BLOCK));
            out.push(a);
        }
    };
    let mut baseline = Vec::new();
    render(&prig, 400, &mut baseline);
    let floor = baseline[0];
    assert!(baseline.iter().all(|&a| a == floor), "a steady floor: {baseline:?}");
    let mut through = Vec::new();
    // The playing patch rebuilt (a switch, a tail), another patch rebuilt
    // (no switch), and the playing patch rebuilt again while the first
    // tail still rings; then every tail runs out and is collected.
    // Rebuilds (a reverb's decay is structural) and retunes (a delay's
    // time, a trim: written to the running chains, the playing one
    // included), with knob writes between.
    let mut retuned = 0;
    for (k, (decay, ms)) in [(0.5, 300.0), (0.5, 450.0), (0.3, 450.0), (0.9, 200.0), (0.9, 250.0)]
        .into_iter()
        .enumerate()
    {
        let report = prig.reload_profile(profile(decay, ms), None);
        assert!(report.is_committed());
        retuned += report.retuned;
        render(&prig, 100, &mut through);
        assert!(prig.set_block_param("Trim", "gain_db", -2.0 + k as f32));
        render(&prig, 100, &mut through);
    }
    assert!(retuned >= 2, "retunes were exercised: {retuned}");
    // A retune of the playing patch itself: its trim.
    let mut retrim = profile(0.9, 250.0);
    retrim.patches[0].chain[0] = RigBlock::effect(BlockType::Volume, "Trim").with_param("gain_db", "-4");
    let report = prig.reload_profile(retrim, None);
    assert_eq!((report.built, report.retuned), (0, 1));
    render(&prig, 200, &mut through);
    render(&prig, SR as usize * 3 / BLOCK, &mut through);
    let worst = through.iter().copied().max().unwrap_or(0);
    println!(
        "{} blocks through 6 reload commits and 5 knob writes: at most {worst} allocations a block \
         (daw's render_block floor: {floor})",
        through.len()
    );
    assert!(
        worst <= floor,
        "rendering through reload commits allocated on the audio thread: {worst} > {floor}"
    );
}
