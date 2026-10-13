//! Bluebox allocates its curve tables when it is built and never again:
//! `set`, `process` and `reset` run on the audio thread.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use saturate_dsp::bluebox::{Bluebox, BlueboxParams, Cal, SatFreq};

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts calls.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the caller's contract is passed straight through.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn set_process_and_reset_never_allocate() {
    let mut fx = Bluebox::new(48_000.0);
    fx.set(BlueboxParams::default());
    let hot = BlueboxParams {
        pentode: 100.0,
        triode: 80.0,
        saturation_in: true,
        saturation: 70.0,
        saturation_freq: SatFreq::Low,
        alt_tube: true,
        air_in: true,
        air: 60.0,
        cal: Cal::Dark,
        density: 40.0,
        mix: 70.0,
        ..BlueboxParams::default()
    };
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    let mut acc = 0.0;
    for i in 0..48_000_u32 {
        if i == 12_000 {
            fx.set(hot);
        }
        if i == 36_000 {
            fx.reset();
        }
        let x = (f64::from(i) * 0.05).sin() * 2.0;
        let (l, r) = fx.process(x, -x);
        acc += l + r;
    }
    let after = ALLOCATIONS.load(Ordering::SeqCst);
    assert!(acc.is_finite());
    assert_eq!(after, before, "the audio path allocated {} times", after - before);
}
