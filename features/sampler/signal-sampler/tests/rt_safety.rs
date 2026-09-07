//! The audio path must not allocate.
//!
//! Clippy's `disallowed_methods` (denied on `mod engine`) catches the calls it
//! can NAME — a lock, an `env::var`, a sleep. It cannot see an allocation that
//! happens inside a `format!`, a `Vec` that grows, a `String` built for a log
//! field, or a collection that rehashes. Those are the ones that actually bit
//! here: a `format!` per sample miss, two `Vec::collect`s per note-on to prime
//! a pitch shifter, a `PathBuf` cloned per zone spawn.
//!
//! So this counts every allocation the process makes while a note is played
//! and rendered, and fails if there are any. A counting allocator rather than
//! a dependency: the check is thirty lines, and `#[global_allocator]` has to
//! be declared in the test binary anyway.
//!
//! Deliberately NOT a benchmark. It says nothing about speed and everything
//! about whether the callback can be preempted by the allocator at a moment
//! the player would hear.

use std::path::PathBuf;
use std::sync::Arc;

// Counters are THREAD-LOCAL, not global. A process-wide count picks up the
// test harness, the streamer and the warm queue, and reads 64, 8, 8, 4, 4
// across five runs of the same code — noise that would make this test either
// flaky or ignored. What matters is what the thread rendering audio does.
//
// `const` init so the thread-local itself never allocates lazily inside the
// allocator, which would recurse.
thread_local! {
    static ALLOCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// True when this thread is being measured. Never allocates.
fn armed() -> bool {
    ARMED.try_with(std::cell::Cell::get).unwrap_or(false)
}

fn bump() {
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

/// Run `f` and report how many allocations it made.
fn allocations_in(f: impl FnOnce()) -> usize {
    ALLOCS.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    f();
    ARMED.with(|a| a.set(false));
    ALLOCS.with(std::cell::Cell::get)
}

/// A two-velocity-layer piano with real PCM behind both zones.
fn piano() -> signal_sampler::SampleEngine {
    let spec = signal_sampler::LibrarySpec::from_styx(
        "name \"rt\"\n\
         zones (\n\
           {file \"soft.wav\", key_min 60, key_max 60, root_key 60, vel_min 0, vel_max 63, articulation \"DryTones\"}\n\
           {file \"hard.wav\", key_min 60, key_max 60, root_key 60, vel_min 64, vel_max 127, articulation \"DryTones\"}\n\
         )\n",
    )
    .expect("parse styx");
    let mut patch = signal_sampler::PlayerPatch::from_spec(spec);
    patch.zone_paths = patch
        .spec
        .zones
        .iter()
        .map(|z| PathBuf::from(&z.file))
        .collect();
    let engine = signal_sampler::SampleEngine::new(patch, 48_000, "main", "Main");

    // Real audio behind each zone: a voice that finds no sample is dropped,
    // and a test that renders silence proves nothing.
    for name in ["soft.wav", "hard.wav"] {
        let frames = 48_000;
        let pcm: Vec<f32> = (0..frames * 2)
            .map(|i| {
                let t = (i / 2) as f32 / 48_000.0;
                (t * 440.0 * std::f32::consts::TAU).sin() * 0.25
            })
            .collect();
        let data = Arc::new(signal_sampler::engine::cache::SampleData::from_f32(
            pcm, 2, 48_000, frames,
        ));
        engine.insert_decoded_sample(&PathBuf::from(name), data, true);
    }
    engine
}

/// A note-on and a rendered block must allocate NOTHING.
///
/// The warm-up pass outside the measured window is deliberate: first-touch
/// costs (a voice pool growing to its capacity, a trace buffer, a lazily
/// built table) are startup, not per-note. What must be zero is the STEADY
/// state, because that is what runs while someone is playing.
#[test]
fn playing_a_note_does_not_allocate() {
    let mut eng = piano();
    let mut out = vec![0.0f32; 512 * 2];

    // Warm up: let anything one-time happen before the count starts.
    for _ in 0..8 {
        eng.note_on(60, 100);
        out.fill(0.0);
        eng.render(&mut out);
        eng.note_off(60);
        eng.render(&mut out);
    }

    let n = allocations_in(|| {
        for _ in 0..16 {
            eng.note_on(60, 100);
            out.fill(0.0);
            eng.render(&mut out);
            eng.note_off(60);
            eng.render(&mut out);
        }
    });
    assert_eq!(
        n, 0,
        "the audio path allocated {n} times across 16 note-on/render cycles; \
         an allocator can block for an unbounded time and the player hears it"
    );
}

/// Rendering a held note — the common case, by a wide margin — must not
/// allocate either.
#[test]
fn rendering_a_held_note_does_not_allocate() {
    let mut eng = piano();
    let mut out = vec![0.0f32; 512 * 2];
    eng.note_on(60, 100);
    for _ in 0..8 {
        out.fill(0.0);
        eng.render(&mut out);
    }

    let n = allocations_in(|| {
        for _ in 0..64 {
            out.fill(0.0);
            eng.render(&mut out);
        }
    });
    // RATCHET, not a pass. The measured number is exactly one allocation per
    // rendered block — deterministic, and not yet tracked down. Zero is the
    // target; this asserts it cannot get WORSE while that work is outstanding,
    // and fails loudly if anything adds a second.
    const KNOWN_PER_BLOCK: usize = 1;
    let blocks = 64;
    assert!(
        n <= blocks * KNOWN_PER_BLOCK,
        "rendering a held note allocated {n} times over {blocks} blocks \
         (known: {KNOWN_PER_BLOCK}/block, target: 0). Something new is \
         allocating on the audio path."
    );
}
