//! The audio-thread half of every tap must not allocate: count this
//! thread's allocations around the pushes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use feed_dsp::{Aggregate, envelope, history, level, scalar, scope, spectrum};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator; only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|a| a.set(a.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|a| a.set(a.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations(f: impl FnOnce()) -> usize {
    let before = ALLOCS.with(Cell::get);
    f();
    ALLOCS.with(Cell::get) - before
}

#[test]
fn audio_thread_pushes_never_allocate() {
    let sr = 48_000.0;
    let (mut spec, mut analyzer) = spectrum(2048, sr);
    let (mut trace, mut trace_r) = history(512, sr, 60.0, Aggregate::Min, 0.0);
    let (mut meter, mut meter_r) = level(2, sr);
    let (gr, mut gr_r) = scalar(Aggregate::Min, 0.0);
    let (mut sc, mut sc_r) = scope(4096);
    let (env, env_r) = envelope(8);
    let left: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin()).collect();
    let right = left.clone();
    let inter: Vec<f32> = left.iter().flat_map(|x| [*x, -*x]).collect();

    // Sanity: the counter sees allocations on this thread.
    assert!(allocations(|| drop(std::hint::black_box(vec![1u8; 64]))) > 0);

    let n = allocations(|| {
        for block in 0..200 {
            spec.push(&left);
            spec.push_stereo(&left, &right);
            trace.push(-(block as f32 % 7.0), 512);
            trace.push_point(-1.0);
            meter.push(0, &left);
            meter.push(1, &right);
            meter.push_interleaved(&inter);
            gr.push(-2.5);
            sc.push(&left);
            env.set(block % 8, 1, 0.5, 0.7);
            env.idle((block + 3) % 8);
        }
    });
    assert_eq!(n, 0, "audio-thread pushes allocated {n} times");

    // The control side may allocate; it still reads what was pushed.
    let mut out = Vec::new();
    analyzer.analyze(0.01, &mut out);
    assert_eq!(out.len(), 64);
    let mut pts = [0.0; 16];
    trace_r.snapshot(&mut pts);
    let mut lv = [0.0; 2];
    meter_r.peak_db(&mut lv);
    assert!(lv[0] > -1.0, "{lv:?}");
    assert_eq!(gr_r.take(), -2.5);
    sc_r.snapshot(&mut out);
    env_r.read_into(&mut out);
    assert_eq!(out.len(), 24);
}
