//! The audio thread's rule: rendering allocates nothing.

#![allow(
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::missing_panics_doc,
    clippy::suboptimal_flops,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::too_many_arguments,
    clippy::missing_const_for_fn
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use stretch_dsp::{Config, Source, Stretcher};

struct Counting;

static WATCHING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator, counting while watched.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if WATCHING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

struct Noise(u32);
impl Source for Noise {
    fn read(&mut self, _start: i64, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *l = (self.0 >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            *r = -*l;
        }
    }
}

#[test]
fn seeking_and_rendering_allocate_nothing() {
    let mut s = Stretcher::new(Config::for_sample_rate(48_000.0));
    let mut src = Noise(1);
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    WATCHING.store(true, Ordering::Relaxed);
    s.seek(1_000.0, 0.8, 1.2, &mut src);
    for block in 0..400 {
        let ratio = 0.8 + f64::from(block % 7) * 0.05;
        s.render(&mut l, &mut r, ratio, 1.2, &mut src);
    }
    WATCHING.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
}
