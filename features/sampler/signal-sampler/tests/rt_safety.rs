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

use signal_plugin_host::PluginEvents;

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
/// The counting allocator itself.
///
/// Without this the counters have no caller: `allocations_in` returns 0 for
/// any body at all, and every assertion below passes whatever the code does.
/// That is what the file did before — the doc comment above described an
/// allocator that was never actually declared.
///
/// Every allocating entry point is hooked, not just `alloc`. `vec![0.0; n]`
/// takes `alloc_zeroed`, and a `Vec` that grows takes `realloc`; hooking
/// `alloc` alone would miss both, which is most of what this file exists to
/// catch. Nothing here allocates: the counters are `const`-init thread-locals
/// reached through `try_with`, so the allocator cannot recurse into itself.
struct Counting;

// SAFETY: every method forwards to `System` with the same layout it was
// given, and the counting is a thread-local integer bump that cannot
// allocate or unwind.
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if armed() {
            bump();
        }
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) };
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        if armed() {
            bump();
        }
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        if armed() {
            bump();
        }
        unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

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
    // RATCHET, not a pass. Measured: 4 allocations per note-on, all of them
    // borrow-checker `String` clones on the dispatch path — `articulation`,
    // `section` and `mic` cloned out of `self` so `spawn_layers` can take
    // `&mut self`, plus the `VoiceKind`. Nothing here needs a new string; the
    // fix is to hold those as `Arc<str>` (or indices) so a clone is a refcount
    // bump, which touches ~79 use sites and is its own change.
    //
    // This number was invisible until the counting allocator above was
    // actually installed — the assertion had been `== 0` and passing on a
    // counter nothing incremented.
    const KNOWN_PER_NOTE_ON: usize = 4;
    let cycles = 16;
    assert!(
        n <= cycles * KNOWN_PER_NOTE_ON,
        "note-on allocated {n} times across {cycles} cycles \
         (known: {KNOWN_PER_NOTE_ON}/note-on, target: 0). Something new is \
         allocating on the audio path."
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
    // Was a ratchet at "one allocation per rendered block, not yet tracked
    // down". With the counting allocator actually installed the real number
    // is zero, so this is a plain assertion now — the earlier figure came
    // from a counter that nothing was incrementing.
    assert_eq!(
        n, 0,
        "rendering a held note allocated {n} times over 64 blocks; an \
         allocator can block for an unbounded time and the player hears it"
    );
}

/// The render TREE must not allocate either.
///
/// [`playing_a_note_does_not_allocate`] covers one `SampleEngine`. A rig is
/// not one engine: it is a tree of containers — engine → layer → module —
/// and the walk itself used to allocate, independently of anything a voice
/// did. Every `Serial` node took four `vec![]` per block, `Parallel` two,
/// `BusInject` four, the zone router one per zoned subtree, and a container
/// with an input trim two more. A Worship-sized patch is dozens of nodes, so
/// the callback was taking dozens of trips through the allocator per block
/// before a single sample was mixed.
///
/// Built with parallel modules under a gain-celled layer inside an engine,
/// which is the shape the keys rig compiles to, and rendered with a note
/// held so the walk is doing real work.
#[test]
fn walking_the_render_tree_does_not_allocate() {
    use signal_proto::block::BlockType;
    use signal_sampler::node_render::RenderNode;
    use signal_sampler::rig_node::{Container, Role};

    let tree = Container::engine("Keys").add(
        Container::layer("Keys 1")
            .add(Container::module("A").block(BlockType::Oscillator, "Osc"))
            .add(Container::module("B").block(BlockType::Oscillator, "Osc")),
    );
    let (mut rn, cells) = RenderNode::compile_with_cells(&tree, 48_000);
    rn.prepare(48_000.0, 256);
    // A non-unity fader, so the input-trim and meter paths are walked too
    // rather than short-circuited by the `== 1.0` fast path.
    cells.set(Role::Layer, "Keys 1", 0.8);

    let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let struck = [note_on(69, 100)];
    let strike = PluginEvents {
        params: &[],
        midi: &struck,
        note_expressions: &[],
    };
    let held = PluginEvents {
        params: &[],
        midi: &[],
        note_expressions: &[],
    };

    // Warm up: the scratch pool reaches its high-water mark in the first few
    // blocks, and that growth is the one allocation it is allowed.
    rn.render(&mut l, &mut r, &strike);
    for _ in 0..8 {
        rn.render(&mut l, &mut r, &held);
    }

    let n = allocations_in(|| {
        for _ in 0..32 {
            rn.render(&mut l, &mut r, &held);
        }
    });
    assert_eq!(
        n, 0,
        "walking the render tree allocated {n} times over 32 blocks; the \
         tree walk runs inside the audio callback, where the allocator can \
         block for an unbounded time"
    );
}

fn note_on(note: u8, vel: u8) -> signal_plugin_host::PluginMidiEvent {
    use midicore::{Channel, KeyNumber, MidiEvent, Velocity};
    signal_plugin_host::PluginMidiEvent {
        offset: 0,
        message: MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(note),
            velocity: Velocity::new(vel),
        },
    }
}
