//! Backend-agnostic project renderer.
//!
//! `ProjectRenderer` takes a [`Standalone`] backend + project GUID
//! and produces a stereo master buffer for a given sample range.
//! It walks the routing graph each block (cheap because we snapshot
//! the state once per project revision) and honors:
//!
//! - Item playback: position, length, fade in/out (linear), volume,
//!   take start_offset, take play_rate (linear-interp resampling),
//!   take volume, item mute / track mute / solo
//! - Track sends → destination track bus (volume + pan)
//! - `parent_send_enabled` (REAPER `B_MAINSEND`) → sum to master
//! - Track volume + pan applied on the pre-send signal
//!
//! Out of scope for v0:
//! - Hardware outputs (currently sum to master same as parent send)
//! - Channel mappings beyond stereo (channel count is honored on the
//!   bus but down-mixed to stereo at the master)
//!
//! Module layout — each stage of the block pipeline lives in its own
//! file; `render_block` below only orchestrates:
//!
//! - [`snapshot`] — per-revision copy of everything the audio thread
//!   reads (tracks, items, envelopes, FX chains, master section)
//! - [`graph`] — topo processing order + solo routing mask, computed
//!   once per snapshot
//! - [`item_mix`] — item playback into the track bus
//! - [`midi`] — per-block MIDI / note-expression event collection
//! - [`envelope`] — per-frame envelope cursors
//!
//! WASM-compatible: pure math + heap, no threads, no cpal. The cpal
//! mixer in `mixer.rs` wraps this on native; an AudioWorklet shim can
//! wrap it on the web.

mod envelope;
mod graph;
mod item_mix;
mod midi;
mod parallel;
mod snapshot;

use std::sync::Arc;

use envelope::EnvelopeCursor;
use item_mix::mix_item_into_bus;
#[cfg(test)]
use midi::collect_midi_events;
use midi::{collect_midi_events_into, collect_note_expressions};
use parallel::{LaneJob, PluginPtr};
use snapshot::{RenderSnapshot, TrackSnapshot};

use crate::sync::Standalone;

/// Live hardware input feeding the renderer (project mode).
///
/// The cpal *input* callback (owned by `AudioEngine`) pushes the
/// interleaved frames it receives — all `channels` of them — into an
/// `rtrb` ring; the renderer holds the consumer and drains it once per
/// block in render **stage 0**. The ring is SPSC + lock-free on the
/// audio thread; only the consumer handle lives behind a `Mutex` (read
/// once per block, alongside the existing scratch lock).
///
/// f32 input only — `AudioEngine` opens the input stream as `f32`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct LiveInput {
    /// Ring consumer of interleaved input frames (`channels` per frame).
    pub(crate) cons: rtrb::Consumer<f32>,
    /// Number of interleaved channels the input stream delivers.
    pub(crate) channels: usize,
    /// De-ring scratch for the block (`frames * channels`), reused.
    pub(crate) scratch: Vec<f32>,
    /// Count of blocks where the ring couldn't supply a full block
    /// (input not keeping up) — the shortfall is zero-filled.
    pub(crate) underruns: u64,
    /// Count of blocks where a backlog past one spare block was dropped
    /// to keep the input → output latency at the buffer size.
    pub(crate) resyncs: u64,
}

/// Mix the live hardware input into per-track buses (render stage 0).
///
/// Drains up to `frames * channels` interleaved samples from `live`'s
/// ring into its scratch (zero-filling any shortfall and counting it as
/// an underrun), then for every track tapping a valid input channel
/// writes that channel's mono sample into BOTH L and R of the track's
/// bus and marks it dirty. The existing FX / gain / send / master
/// stages then process it unchanged.
///
/// Split out as a free function so it's unit-testable without standing
/// up a full `ProjectRenderer` (DB, transport, snapshot).
///
/// `input_channels[ti]` is `Some(ch)` for a live-input track, else
/// `None`. `passes(ti)` is the solo-routing predicate (same one stage 1
/// uses); a track that fails it contributes silence.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn mix_live_input_into_buses(
    live: &mut LiveInput,
    input_channels: &[Option<u32>],
    passes: impl Fn(usize) -> bool,
    buses: &mut [StereoBuffer],
    dirty: &mut [bool],
    frames: usize,
) {
    let channels = live.channels;
    if channels == 0 || frames == 0 {
        return;
    }
    let want = frames * channels;
    if live.scratch.len() < want {
        live.scratch.resize(want, 0.0);
    }
    // Latency guard: the ring is drained one block per output callback, so
    // anything queued beyond that — input started before output, an output
    // stall, two device clocks drifting — would otherwise stay queued (and
    // audible as delay) for the life of the stream. Keep at most one spare
    // block for callback jitter and drop the oldest frames past it.
    let backlog = live.cons.slots();
    if backlog > want * 2 {
        let skip = (backlog - want * 2) / channels * channels;
        if let Ok(chunk) = live.cons.read_chunk(skip) {
            chunk.commit_all();
            live.resyncs = live.resyncs.wrapping_add(1);
        }
    }
    // Drain interleaved input frames from the ring; zero-fill shortfall.
    let avail = live.cons.slots();
    let n = want.min(avail);
    if n < want {
        live.underruns = live.underruns.wrapping_add(1);
    }
    if n > 0
        && let Ok(chunk) = live.cons.read_chunk(n)
    {
        let (a, b) = chunk.as_slices();
        live.scratch[..a.len()].copy_from_slice(a);
        live.scratch[a.len()..a.len() + b.len()].copy_from_slice(b);
        chunk.commit_all();
    }
    for s in &mut live.scratch[n..want] {
        *s = 0.0;
    }

    for (ti, ch_opt) in input_channels.iter().enumerate() {
        let Some(ch) = *ch_opt else { continue };
        let ch = ch as usize;
        if ch >= channels {
            continue;
        }
        if !passes(ti) {
            continue;
        }
        let Some(bus) = buses.get_mut(ti) else {
            continue;
        };
        for f in 0..frames.min(bus.frames) {
            let s = live.scratch[f * channels + ch];
            bus.samples[f * 2] = s;
            bus.samples[f * 2 + 1] = s;
        }
        dirty[ti] = true;
    }
}

/// A single programmatic / live MIDI event addressed to a project track.
///
/// The MIDI analog of a live-input audio frame: pushed (lock-free) by a
/// producer the UI thread reaches through `Standalone`, drained once per
/// block by the renderer and merged into the target track's per-block
/// MIDI event list before the FX chain runs. `offset = 0` means
/// block-start — programmatic pushes don't carry sub-block timing (this
/// matches how `collect_midi_events` clamps item events to the block).
///
/// Native: the live-MIDI ring is `rtrb`-backed (a `cfg(not(wasm32))` dep),
/// mirroring [`LiveInput`]. On wasm32 the same event type is queued in a
/// plain `VecDeque` on `Standalone` (the AudioWorkletGlobalScope is
/// single-threaded, so a mutex-guarded deque is uncontended by
/// construction) — see the wasm `LiveMidiQueue` below.
#[derive(Clone, Debug)]
pub(crate) struct LiveMidiEvent {
    /// Target project track guid (resolved to a snapshot index at drain).
    pub(crate) track: String,
    /// Sample offset from block start (0..block_size). Usually 0.
    pub(crate) offset: u32,
    pub(crate) message: daw_proto::MidiEvent,
}

/// Renderer-side handle to the live-MIDI ring (consumer end).
///
/// SPSC + lock-free like [`LiveInput`]; only the consumer handle lives
/// behind a `Mutex`, drained once per block alongside the scratch lock.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct LiveMidiQueue {
    /// Ring consumer of programmatic MIDI events.
    pub(crate) cons: rtrb::Consumer<LiveMidiEvent>,
}

/// wasm32 variant: same public shape as the native queue, backed by a plain
/// `VecDeque` instead of an rtrb ring. The AudioWorkletGlobalScope is
/// single-threaded, so producer (a `WebRenderer` MIDI method) and consumer
/// (the render block) never race; the deque lives on `Standalone`
/// (`live_midi_wasm`) because the web render path constructs a fresh
/// `ProjectRenderer` per block.
#[cfg(target_arch = "wasm32")]
pub(crate) struct LiveMidiQueue {
    pub(crate) events: std::collections::VecDeque<LiveMidiEvent>,
}

/// Drain every queued live-MIDI event into a per-track scratch bucket.
///
/// Resolves each event's track guid to a snapshot index via `idx_of`
/// (built from the snapshot's `guid` fields) and appends a
/// `PluginMidiEvent` to that track's bucket. Events whose track guid
/// doesn't resolve are dropped. Split out as a free function so the
/// drain + per-track merge is unit-testable without a `ProjectRenderer`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn drain_live_midi(
    queue: &mut LiveMidiQueue,
    idx_of: impl Fn(&str) -> Option<usize>,
    buckets: &mut [Vec<crate::plugin::PluginMidiEvent>],
) {
    let avail = queue.cons.slots();
    for _ in 0..avail {
        let Ok(ev) = queue.cons.pop() else { break };
        let Some(ti) = idx_of(&ev.track) else {
            continue;
        };
        let Some(bucket) = buckets.get_mut(ti) else {
            continue;
        };
        bucket.push(crate::plugin::PluginMidiEvent {
            offset: ev.offset,
            message: ev.message,
        });
    }
}

/// wasm32 variant of [`drain_live_midi`] — identical semantics over the
/// `VecDeque`-backed queue: resolve each event's track guid via `idx_of`,
/// append to that track's bucket, drop unresolvable events.
#[cfg(target_arch = "wasm32")]
pub(crate) fn drain_live_midi(
    queue: &mut LiveMidiQueue,
    idx_of: impl Fn(&str) -> Option<usize>,
    buckets: &mut [Vec<crate::plugin::PluginMidiEvent>],
) {
    while let Some(ev) = queue.events.pop_front() {
        let Some(ti) = idx_of(&ev.track) else {
            continue;
        };
        let Some(bucket) = buckets.get_mut(ti) else {
            continue;
        };
        bucket.push(crate::plugin::PluginMidiEvent {
            offset: ev.offset,
            message: ev.message,
        });
    }
}

/// Stereo render buffer (interleaved L/R/L/R/...).
#[derive(Debug, Clone)]
pub struct StereoBuffer {
    pub samples: Vec<f32>,
    pub frames: usize,
    pub sample_rate: u32,
}

impl StereoBuffer {
    pub fn zeroed(frames: usize, sample_rate: u32) -> Self {
        Self {
            samples: vec![0.0; frames * 2],
            frames,
            sample_rate,
        }
    }

    pub fn fill(&mut self, v: f32) {
        for s in self.samples.iter_mut() {
            *s = v;
        }
    }
}

/// Pre-allocated per-block working memory, owned by the renderer and
/// reused across blocks — steady-state playback allocates nothing
/// (REAPER's model: the graph is cheap, buffers are recycled).
/// Buses are indexed by track index (== snapshot index == project
/// track index), not hashed by guid.
#[derive(Default)]
struct RenderScratch {
    /// One stereo bus per track, index == track index.
    buses: Vec<StereoBuffer>,
    /// Bus received signal this block (items / sends / children /
    /// synth FX). Clean buses skip every per-frame stage.
    dirty: Vec<bool>,
    /// De-interleave scratch for the FX stage.
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// Post-fader copy of the current track's bus, so sends / parent
    /// sums can read it while mutating destination buses.
    src: Vec<f32>,
    /// Send-tap copies (only filled when a send actually taps there).
    pre_fx_tap: Vec<f32>,
    pre_fader_tap: Vec<f32>,
    /// Each track's channels 3/4 (interleaved), what sends to its
    /// sidechain left there this block; sized on first use, so a project
    /// without sidechains carries none.
    sidechains: Vec<Vec<f32>>,
    /// Which tracks' sidechains received this block.
    keyed: Vec<bool>,
    /// Each track's plugins' outputs beyond the main pair (interleaved,
    /// [`AUX_PAIRS`] pairs one after another), for a track sending from
    /// them; sized on first use.
    aux: Vec<Vec<f32>>,
    /// Which tracks' aux outputs carry this block's audio.
    aux_live: Vec<bool>,
    /// De-interleave scratch for a sidechain.
    key_l: Vec<f32>,
    key_r: Vec<f32>,
    /// One FX's parameters this block, its automation applied
    /// ([`envelope::params_at`]): kept across blocks so it does not
    /// allocate on the audio thread.
    fx_param_events: Vec<(u32, f64)>,
    /// FX guids whose plugin panicked inside `process_block`/`prepare`.
    /// Once here they are bypassed forever (this engine run) — a plugin
    /// that panics once can't be trusted on the audio thread again.
    /// Persists across blocks (never cleared by `reset`).
    panicked_fx: std::collections::HashSet<String>,
    /// Realtime mode: the snapshot this renderer is playing (revision,
    /// snapshot) — the audio thread's own, swapped for the builder's newer
    /// one between blocks (see [`ProjectRenderer::set_realtime`]).
    current: Option<(u64, Arc<RenderSnapshot>)>,
    /// Realtime mode: the meter bank, re-read only when its lock is free.
    meters: Option<Arc<crate::metering::Meters>>,
    /// Live MIDI for this block, per track, and one track's merged events —
    /// kept across blocks so a note played does not allocate on the audio
    /// thread.
    midi_buckets: Vec<Vec<crate::plugin::PluginMidiEvent>>,
    midi_events: Vec<crate::plugin::PluginMidiEvent>,
    /// Realtime mode with a lane pool: this block's independent tracks, one
    /// job each (see [`LaneJob`]), and which tracks ran that way.
    lane_jobs: Vec<LaneJob>,
    /// Per track: something feeds its bus (a child or a send), so it waits
    /// for the topo loop.
    fed: Vec<bool>,
    /// Per track: its FX chain already ran in the parallel pass.
    chain_done: Vec<bool>,
    /// Per track: its chain's time last block, to hand out the longest
    /// lanes first.
    lane_cost: Vec<u32>,
}

impl RenderScratch {
    /// Size everything for `n` tracks × `frames` and zero the buses.
    /// Steady state (same shape as last block) is memset-only.
    fn reset(&mut self, n: usize, frames: usize, sample_rate: u32) {
        if self.buses.len() != n {
            self.buses
                .resize_with(n, || StereoBuffer::zeroed(frames, sample_rate));
        }
        for b in &mut self.buses {
            b.frames = frames;
            b.sample_rate = sample_rate;
            if b.samples.len() != frames * 2 {
                b.samples.resize(frames * 2, 0.0);
            }
            b.samples.fill(0.0);
        }
        self.dirty.clear();
        self.dirty.resize(n, false);
        self.sidechains.resize_with(n, Vec::new);
        for (sidechain, keyed) in self.sidechains.iter_mut().zip(&self.keyed) {
            if *keyed {
                sidechain.fill(0.0);
            }
        }
        self.keyed.clear();
        self.keyed.resize(n, false);
        self.aux.resize_with(n, Vec::new);
        for (aux, live) in self.aux.iter_mut().zip(&self.aux_live) {
            if *live {
                aux.fill(0.0);
            }
        }
        self.aux_live.clear();
        self.aux_live.resize(n, false);
        self.chain_done.clear();
        self.chain_done.resize(n, false);
        if self.lane_cost.len() != n {
            self.lane_cost.resize(n, 0);
        }
        for v in [
            &mut self.in_l,
            &mut self.in_r,
            &mut self.out_l,
            &mut self.out_r,
            &mut self.key_l,
            &mut self.key_r,
        ] {
            if v.len() != frames {
                v.resize(frames, 0.0);
            }
        }
    }
}

/// How many pairs past the main one a track's plugins can put out: 3/4
/// and 5/6.
pub(crate) const AUX_PAIRS: usize = 2;

/// A track's aux buffer at its size for `frames` (zeroed when it grows).
fn aux_sized(aux: &mut Vec<f32>, frames: usize) {
    if aux.len() != AUX_PAIRS * frames * 2 {
        aux.clear();
        aux.resize(AUX_PAIRS * frames * 2, 0.0);
    }
}

/// `plugin`'s aux outputs this block, summed into `aux`
/// ([`AUX_PAIRS`] interleaved pairs, one after another). Whether it had
/// any.
pub(crate) fn add_aux(
    plugin: &dyn crate::plugin::PluginInstance,
    aux: &mut [f32],
    frames: usize,
) -> bool {
    let mut any = false;
    for pair in 0..AUX_PAIRS {
        let Some((l, r)) = plugin.aux_out(pair) else {
            continue;
        };
        let Some(out) = aux.get_mut(pair * frames * 2..(pair + 1) * frames * 2) else {
            continue;
        };
        for ((o, l), r) in out.chunks_exact_mut(2).zip(l).zip(r) {
            o[0] += *l;
            o[1] += *r;
        }
        any = true;
    }
    any
}

/// Render a stereo master block.
///
/// Keep one renderer alive across blocks: the track snapshot is cached
/// against the project's mutation revision, so steady-state playback
/// (no edits) re-walks nothing — REAPER's "graph is cheap, only
/// rebuild on change" model — and the per-block working buffers are
/// recycled. A throwaway renderer still works, it just re-snapshots
/// and re-allocates every call.
/// How many times the renderer retries the plugin-instance lock before
/// rendering a block without its FX stage — on the order of 10–50 µs.
const PLUGIN_LOCK_SPINS: usize = 2_000;

/// Blocks rendered WITHOUT their plugin stage because a control thread held
/// the plugin map (see the try-lock in `render_block_varispeed`). For an
/// instrument track that block is silent — a dropout no CPU measure shows.
static PLUGIN_STAGE_SKIPS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many blocks have rendered without their plugin stage (process-wide).
#[must_use]
pub fn plugin_stage_skips() -> u64 {
    PLUGIN_STAGE_SKIPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// A point in time for the block profile — a no-op on wasm, where
/// `std::time::Instant` panics (the browser rig renders here too).
#[derive(Clone, Copy)]
struct Stamp(#[cfg(not(target_arch = "wasm32"))] std::time::Instant);

impl Stamp {
    fn now() -> Self {
        Self(
            #[cfg(not(target_arch = "wasm32"))]
            std::time::Instant::now(),
        )
    }

    /// Microseconds since `now` (0 on wasm).
    fn us(self) -> u32 {
        #[cfg(not(target_arch = "wasm32"))]
        return self.0.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        #[cfg(target_arch = "wasm32")]
        0
    }
}

/// Where one rendered block spent its time (microseconds) — the worst
/// block of a window, for a live engine's health log: a block that runs
/// long because a lock was held or a snapshot was built reads very
/// differently from one that simply had too many voices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockProfile {
    /// The whole block.
    pub total_us: u32,
    /// Getting the project snapshot (and the scratch) — lock waits and any
    /// snapshot build land here.
    pub snapshot_us: u32,
    /// Waiting for the plugin map.
    pub plugin_lock_us: u32,
    /// Every track's FX chain (instruments included).
    pub fx_us: u32,
    /// The track whose FX chain took longest (project track index), and how
    /// long.
    pub slowest_track: u32,
    pub slowest_track_us: u32,
    /// Frames in the block.
    pub frames: u32,
}

/// The worst [`BlockProfile`] of the current window, written by one render
/// thread and taken by a reader — atomics, no lock.
#[derive(Debug, Default)]
pub struct BlockProfileCell {
    fields: [std::sync::atomic::AtomicU32; 7],
}

impl BlockProfileCell {
    fn record(&self, p: &BlockProfile) {
        use std::sync::atomic::Ordering::Relaxed;
        if p.total_us <= self.fields[0].load(Relaxed) {
            return;
        }
        let v = [
            p.total_us,
            p.snapshot_us,
            p.plugin_lock_us,
            p.fx_us,
            p.slowest_track,
            p.slowest_track_us,
            p.frames,
        ];
        for (f, x) in self.fields.iter().zip(v) {
            f.store(x, Relaxed);
        }
    }

    /// The worst block since the last take, and start a new window.
    pub fn take(&self) -> Option<BlockProfile> {
        use std::sync::atomic::Ordering::Relaxed;
        let v: Vec<u32> = self.fields.iter().map(|f| f.swap(0, Relaxed)).collect();
        (v[0] > 0).then(|| BlockProfile {
            total_us: v[0],
            snapshot_us: v[1],
            plugin_lock_us: v[2],
            fx_us: v[3],
            slowest_track: v[4],
            slowest_track_us: v[5],
            frames: v[6],
        })
    }
}

pub struct ProjectRenderer {
    daw: Standalone,
    project_guid: String,
    sample_rate: u32,
    /// `(project revision, snapshot)` — refreshed when the revision moves.
    cache: Arc<std::sync::Mutex<Option<(u64, Arc<RenderSnapshot>)>>>,
    /// Reusable block buffers. Mutex for `&self` access — the audio
    /// callback is the only steady-state caller, so it's uncontended.
    scratch: std::sync::Mutex<RenderScratch>,
    /// Live hardware input (project mode). Installed by `AudioEngine`
    /// when a track records from a hardware channel or `want_input` is
    /// set; `None` for pure-playback projects. Read once per block in
    /// stage 0, alongside the scratch lock.
    #[cfg(not(target_arch = "wasm32"))]
    live_input: std::sync::Mutex<Option<LiveInput>>,
    /// Live / programmatic MIDI queue (project mode). Installed by
    /// `AudioEngine`; events pushed by the UI thread through `Standalone`
    /// are drained once per block (before the FX stage) and merged into
    /// the target track's per-block MIDI events. `None` until installed.
    #[cfg(not(target_arch = "wasm32"))]
    live_midi: std::sync::Mutex<Option<LiveMidiQueue>>,
    /// Realtime mode (see [`set_realtime`](Self::set_realtime)): the
    /// snapshot builder the audio thread hands project changes to.
    #[cfg(not(target_arch = "wasm32"))]
    realtime: std::sync::OnceLock<SnapshotBuilder>,
    /// Realtime mode: the worker threads independent tracks render on.
    #[cfg(not(target_arch = "wasm32"))]
    lanes: std::sync::OnceLock<parallel::LanePool>,
}

/// Builds render snapshots off the audio thread, for a realtime renderer.
///
/// The audio thread must never wait: not on the project lock (every control
/// operation — a fader, a mute, a status query — holds it, and a holder
/// preempted mid-hold turns into a multi-millisecond block), and not on a
/// snapshot build (a walk of the whole project, allocating). So it only
/// ever *tries* the project lock to read the revision, asks this thread for
/// a snapshot when the revision has moved, and keeps playing the one it has
/// until the new one is handed over. A mixer change is heard a block or two
/// later; a late block is heard as a click.
#[cfg(not(target_arch = "wasm32"))]
struct SnapshotBuilder {
    /// The revision the audio thread wants built (`NO_REVISION`: none).
    wanted: Arc<std::sync::atomic::AtomicU64>,
    /// A built snapshot waiting for the audio thread.
    ready: Arc<std::sync::Mutex<Option<(u64, Arc<RenderSnapshot>)>>>,
    /// Snapshots the audio thread let go of, freed here rather than there.
    retired: Arc<std::sync::Mutex<Vec<Arc<RenderSnapshot>>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::Thread,
}

#[cfg(not(target_arch = "wasm32"))]
const NO_REVISION: u64 = u64::MAX;

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ProjectRenderer {
    fn drop(&mut self) {
        if let Some(b) = self.realtime.get() {
            b.stop.store(true, std::sync::atomic::Ordering::Release);
            b.thread.unpark();
        }
    }
}

impl ProjectRenderer {
    pub fn new(daw: &Standalone, project_guid: &str, sample_rate: u32) -> Self {
        Self {
            daw: daw.clone(),
            project_guid: project_guid.to_string(),
            sample_rate,
            cache: Arc::new(std::sync::Mutex::new(None)),
            scratch: std::sync::Mutex::new(RenderScratch::default()),
            #[cfg(not(target_arch = "wasm32"))]
            live_input: std::sync::Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            live_midi: std::sync::Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            realtime: std::sync::OnceLock::new(),
            #[cfg(not(target_arch = "wasm32"))]
            lanes: std::sync::OnceLock::new(),
        }
    }

    /// Render for a realtime callback: from here on the render thread never
    /// blocks on the project lock or the meter lock and never builds a
    /// snapshot — see [`SnapshotBuilder`]. Project edits reach the audio a
    /// block or two late. Offline renders (tests, bounces) leave this off
    /// and see every edit on the very next block. Idempotent.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_realtime(&self) {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let threads = parallel::default_threads();
        if threads > 0 {
            let pool = self.lanes.get_or_init(|| parallel::LanePool::new(threads));
            tracing::debug!(workers = pool.threads(), "render: lane pool");
        }
        self.realtime.get_or_init(|| {
            let wanted = Arc::new(AtomicU64::new(NO_REVISION));
            let ready = Arc::new(std::sync::Mutex::new(None));
            let retired = Arc::new(std::sync::Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let (daw, guid) = (self.daw.clone(), self.project_guid.clone());
            let (w, r, g, st) = (wanted.clone(), ready.clone(), retired.clone(), stop.clone());
            let handle = std::thread::Builder::new()
                .name("render-snapshot".into())
                .spawn(move || {
                    loop {
                        std::thread::park_timeout(std::time::Duration::from_millis(250));
                        if st.load(Ordering::Acquire) {
                            return;
                        }
                        // What the audio thread let go of is freed here.
                        if let Ok(mut old) = g.lock() {
                            old.clear();
                        }
                        if w.swap(NO_REVISION, Ordering::AcqRel) == NO_REVISION {
                            continue;
                        }
                        let built =
                            daw.read_project(&guid, |p| (p.revision, RenderSnapshot::build(p)));
                        if let Some(built) = built {
                            // A snapshot the audio thread never took is
                            // dropped here, on this thread.
                            let stale = r.lock().ok().and_then(|mut slot| slot.replace(built));
                            drop(stale);
                        }
                    }
                })
                .expect("spawn render-snapshot thread");
            SnapshotBuilder {
                wanted,
                ready,
                retired,
                stop,
                thread: handle.thread().clone(),
            }
        });
    }

    /// Realtime mode's snapshot: the one being played, swapped for the
    /// builder's newer one if it has handed one over, and a newer one asked
    /// for when the project has moved on. Never waits for a lock; builds
    /// only the very first snapshot here (before there is anything to play).
    #[cfg(not(target_arch = "wasm32"))]
    fn snapshot_rt(
        &self,
        b: &SnapshotBuilder,
        current: &mut Option<(u64, Arc<RenderSnapshot>)>,
    ) -> Option<Arc<RenderSnapshot>> {
        use std::sync::atomic::Ordering;
        // Take a handed-over snapshot.
        let handed = match b.ready.try_lock() {
            Ok(mut slot) => slot.take(),
            Err(_) => None,
        };
        if let Some(new) = handed {
            if let Some((_, old)) = current.replace(new) {
                match b.retired.try_lock() {
                    Ok(mut bin) => bin.push(old),
                    // The builder is emptying it this instant: the old one
                    // is freed here — rare, and still no wait.
                    Err(_) => drop(old),
                }
            }
        }
        // Has the project moved on? Read without waiting.
        let revision = match self.daw.state.try_lock() {
            Ok(s) => s.projects.get(&self.project_guid).map(|p| p.revision),
            Err(std::sync::TryLockError::Poisoned(p)) => p
                .into_inner()
                .projects
                .get(&self.project_guid)
                .map(|p| p.revision),
            Err(std::sync::TryLockError::WouldBlock) => None,
        };
        match (&*current, revision) {
            (Some((rev, _)), Some(now)) if *rev != now => {
                if b.wanted.swap(now, Ordering::AcqRel) != now {
                    b.thread.unpark();
                }
            }
            (None, _) => {
                // Nothing to play yet: the first snapshot is built here.
                let built = self.daw.read_project(&self.project_guid, |p| {
                    (p.revision, RenderSnapshot::build(p))
                })?;
                *current = Some(built);
            }
            _ => {}
        }
        current.as_ref().map(|(_, s)| s.clone())
    }

    /// The backend this renderer reads its project from. Lets the cpal
    /// callback reach the shared `Meters` bank without a second handle.
    pub(crate) fn daw(&self) -> &Standalone {
        &self.daw
    }

    /// Install the live hardware-input ring consumer (project mode).
    /// `AudioEngine` calls this after opening the cpal input stream so
    /// stage 0 can read mic/DI channels into the matching track buses.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_live_input(&self, cons: rtrb::Consumer<f32>, channels: usize) {
        if let Ok(mut slot) = self.live_input.lock() {
            *slot = Some(LiveInput {
                cons,
                channels,
                scratch: Vec::new(),
                underruns: 0,
                resyncs: 0,
            });
        }
    }

    /// Install the live / programmatic MIDI ring consumer (project mode).
    /// The producer end is fed by `Standalone::push_note_on`/`_off`/`_cc`.
    /// Mirrors `set_live_input`: stage 0.5 drains it once per block and
    /// merges events into the matching track's MIDI event list.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_live_midi(&self, cons: rtrb::Consumer<LiveMidiEvent>) {
        if let Ok(mut slot) = self.live_midi.lock() {
            *slot = Some(LiveMidiQueue { cons });
        }
    }

    /// Wire a fresh live-MIDI ring between this renderer and its backend —
    /// the headless-native path (no `AudioEngine`): after this,
    /// `Standalone::push_live_midi` feeds this renderer's per-block drain,
    /// exactly as under a running engine. Replaces any previous ring.
    #[cfg(all(not(target_arch = "wasm32"), feature = "audio"))]
    pub fn connect_live_midi(&self, capacity: usize) {
        let (prod, cons) = rtrb::RingBuffer::new(capacity.max(1));
        self.daw.set_live_midi_producer(prod);
        self.set_live_midi(cons);
    }

    /// Render `frames` stereo frames starting at `start_frame` (in
    /// output-rate samples). Returns a fresh `StereoBuffer`.
    pub fn render_block(&self, start_frame: u64, frames: usize) -> StereoBuffer {
        self.render_block_varispeed(start_frame as f64, frames, 1.0)
    }

    /// Render one audio buffer as the transport planned it
    /// ([`TransportShared::begin_block`](crate::transport_engine::TransportShared::begin_block)):
    /// each run at its own start and varispeed rate, a scheduled locate
    /// landing on its exact frame.
    ///
    /// `render_stopped`: a run the transport does not roll through is
    /// still rendered at its frozen position, 1x (the duplex path — live
    /// input and FX run while stopped); otherwise it is silence.
    ///
    /// A single run renders as one block, exactly as
    /// [`render_block_varispeed`](Self::render_block_varispeed). A buffer
    /// split by a locate, when only one side plays, renders the full
    /// buffer from that side's clock and silences the other side — so a
    /// plugin never sees its first block short (it sizes itself on its
    /// first block).
    pub fn render_plan(
        &self,
        plan: &crate::transport_engine::BlockPlan,
        render_stopped: bool,
    ) -> StereoBuffer {
        let segs = plan.segments();
        let total: usize = segs.iter().map(|s| s.frames).sum();
        let run = |seg: &crate::transport_engine::BlockSegment| -> Option<StereoBuffer> {
            if seg.playing {
                Some(self.render_block_varispeed(seg.start_samples, seg.frames, seg.rate))
            } else if render_stopped {
                Some(self.render_block_varispeed(
                    seg.start_samples.max(0.0).floor(),
                    seg.frames,
                    1.0,
                ))
            } else {
                None
            }
        };
        match segs {
            [one] => run(one).unwrap_or_else(|| StereoBuffer::zeroed(total, self.sample_rate)),
            [a, b] if !render_stopped && a.playing != b.playing => {
                // One side plays: render the whole buffer on its clock,
                // silence the side that does not.
                let (on, silent) = if a.playing {
                    (a, b.offset..total)
                } else {
                    (b, 0..b.offset)
                };
                let mut out = self.render_block_varispeed(on.start_at_frame_zero(), total, on.rate);
                if let Some(s) = out.samples.get_mut(silent.start * 2..silent.end * 2) {
                    s.fill(0.0);
                }
                out
            }
            _ => {
                let mut out = StereoBuffer::zeroed(total, self.sample_rate);
                for seg in segs {
                    let Some(block) = run(seg) else { continue };
                    let dst = seg.offset * 2..(seg.offset + seg.frames) * 2;
                    if let (Some(d), Some(src)) = (
                        out.samples.get_mut(dst),
                        block.samples.get(..seg.frames * 2),
                    ) {
                        d.copy_from_slice(src);
                    }
                }
                out
            }
        }
    }

    /// Render `frames` stereo frames of the timeline from `start_samples`
    /// (output-rate samples, fractional allowed) at transport `rate`:
    /// output frame `i` is the timeline at `start_samples + i * rate` —
    /// true varispeed (resampled, so pitch moves with the rate), not a
    /// playhead that skips. Everything timeline-driven follows: item
    /// sources, take / track / send envelopes, MIDI and the metronome.
    /// The next block continues from `start_samples + frames * rate`.
    ///
    /// At `rate == 1.0` with an integer start this is bit-identical to
    /// [`render_block`](Self::render_block).
    pub fn render_block_varispeed(
        &self,
        start_samples: f64,
        frames: usize,
        rate: f64,
    ) -> StereoBuffer {
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
        let _frame = crate::plugin::RenderFrameGuard::enter(start_samples.max(0.0).floor() as u64);
        let mut master = StereoBuffer::zeroed(frames, self.sample_rate);
        if frames == 0 {
            return master;
        }

        let t_start = Stamp::now();
        let mut scratch = self
            .scratch
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(not(target_arch = "wasm32"))]
        let snap = match self.realtime.get() {
            Some(b) => self.snapshot_rt(b, &mut scratch.current),
            None => self.snapshot(),
        };
        #[cfg(target_arch = "wasm32")]
        let snap = self.snapshot();
        let Some(snap) = snap else {
            return master;
        };
        let snapshot_us = t_start.us();
        let tracks = &snap.tracks;
        let n = tracks.len();
        let start_seconds = start_samples / self.sample_rate as f64;
        let end_seconds = start_seconds + (frames as f64 * rate / self.sample_rate as f64);
        // Output frames per timeline second (MIDI event offsets).
        let frames_per_second = self.sample_rate as f64 / rate;
        let any_soloed = snap.solo_pass.is_some();
        let pass = |i: usize| -> graph::Pass {
            match &snap.solo_pass {
                Some(p) => p[i],
                None => graph::Pass::On,
            }
        };
        let passes = |i: usize| -> bool { pass(i).passes() };

        // Poison-tolerant lock: a control-thread panic while holding one of
        // these mutexes must NOT take down the audio callback (a panic here
        // unwinds across the extern "C" PipeWire boundary → abort → process
        // death mid-song). The guarded state is plain buffers/maps — safe to
        // keep using after another thread's panic.
        scratch.reset(n, frames, self.sample_rate);
        // The meter bank. In realtime mode, re-read only when its lock is
        // free (its readers are UI polls) — the cached one otherwise.
        #[cfg(not(target_arch = "wasm32"))]
        let meters = if self.realtime.get().is_some() {
            if let Ok(m) = self.daw.meters.try_lock() {
                if !scratch.meters.as_ref().is_some_and(|c| Arc::ptr_eq(c, &m)) {
                    scratch.meters = Some(m.clone());
                }
            }
            scratch.meters.clone().unwrap_or_else(|| self.daw.meters())
        } else {
            self.daw.meters()
        };
        #[cfg(target_arch = "wasm32")]
        let meters = self.daw.meters();
        // Split-borrow every scratch field so the stages below can
        // hold disjoint &muts simultaneously.
        let RenderScratch {
            buses,
            dirty,
            in_l,
            in_r,
            out_l,
            out_r,
            src,
            pre_fx_tap,
            pre_fader_tap,
            sidechains,
            keyed,
            aux,
            aux_live,
            key_l,
            key_r,
            fx_param_events,
            panicked_fx,
            midi_buckets,
            midi_events,
            lane_jobs,
            fed,
            chain_done,
            lane_cost,
            ..
        } = &mut *scratch;

        // 0) Live hardware input → per-track buses. Tracks whose
        // `RecordInput::Audio { channel }` taps a valid input channel
        // get that channel's mono sample broadcast to L+R of their bus
        // (and marked dirty); the FX / gain / send / master stages then
        // process it like any other source. No-op when no input stream
        // is installed or no track taps a channel.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut live_guard = self
                .live_input
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(live) = live_guard.as_mut() {
                mix_live_input_into_buses(live, &snap.input_channels, passes, buses, dirty, frames);
            }
            // 0.1) Recording: the armed tracks' input this block, pushed to
            // the recording pass's rings (lock-free, never waits in
            // realtime) — see `crate::recording`.
            crate::recording::capture_block(
                &self.daw,
                &crate::recording::Block {
                    project_guid: &self.project_guid,
                    // Its snapshot cache's heap address: stable while it lives.
                    renderer: Arc::as_ptr(&self.cache) as usize,
                    sample_rate: self.sample_rate,
                    start_samples,
                    frames,
                    rate,
                    offline: self.realtime.get().is_none(),
                    live: live_guard
                        .as_ref()
                        .map(|l| (l.scratch.as_slice(), l.channels)),
                },
            );
        }

        // 0.5) Live / programmatic MIDI → per-track scratch buckets.
        // Drained once per block under the queue lock (like stage 0's
        // live input), resolving each event's track guid to a snapshot
        // index. The buckets merge into each track's `collect_midi_events`
        // list at the FX stage below. Empty / no-op when no queue is
        // installed and when nothing was pushed this block.
        // Native: the live-MIDI ring is `rtrb`-backed (native dep). On
        // wasm32 the queue is a `VecDeque` on `Standalone` (pushed by the
        // `WebRenderer` MIDI methods) — drained here with the same
        // guid-resolve + bucket merge.
        #[allow(unused_mut)]
        let live_midi_buckets = midi_buckets;
        for b in live_midi_buckets.iter_mut() {
            b.clear();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut midi_guard = self
                .live_midi
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(queue) = midi_guard.as_mut()
                && queue.cons.slots() > 0
            {
                live_midi_buckets.resize_with(n, Vec::new);
                // guid → index over the snapshot (built lazily on demand).
                let idx_of =
                    |guid: &str| -> Option<usize> { tracks.iter().position(|t| t.guid == guid) };
                drain_live_midi(queue, idx_of, live_midi_buckets);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let events = self.daw.take_live_midi_wasm();
            if !events.is_empty() {
                live_midi_buckets.resize_with(n, Vec::new);
                let idx_of =
                    |guid: &str| -> Option<usize> { tracks.iter().position(|t| t.guid == guid) };
                let mut queue = LiveMidiQueue { events };
                drain_live_midi(&mut queue, idx_of, live_midi_buckets);
            }
        }

        // 1) Item playback into per-track buses. Item-level envelopes
        // (take volume / pan / mute / pitch) are evaluated per-frame
        // inside mix_item_into_bus via cursors.
        for (ti, t) in tracks.iter().enumerate() {
            // Gate on the solo-routing MASK, not the raw per-track `soloed`
            // flag: soloing a folder must keep its children audible (they pass
            // as descendants) and soloing a leaf keeps its folder ancestors as
            // pass-through. `passes(ti)` is the same predicate stage 0 uses.
            if any_soloed && !passes(ti) {
                continue;
            }
            let bus = &mut buses[ti];
            for item in &t.items {
                if item.muted {
                    continue;
                }
                // Fixed lanes: only items on PLAYING lanes sound —
                // the rest are alternate takes (REAPER 7 comping).
                if t.lane_play_mask != 0
                    && let Some(lane) = item.fixed_lane
                    && (lane >= 64 || t.lane_play_mask & (1u64 << lane) == 0)
                {
                    continue;
                }
                let Some(audio) = &item.audio else { continue };
                dirty[ti] |= mix_item_into_bus(
                    bus,
                    audio,
                    item,
                    start_seconds,
                    end_seconds,
                    self.sample_rate,
                    rate,
                );
            }
        }

        // Debug tracing: FTS_RENDER_DEBUG=1 dumps per-track bus RMS at
        // each stage so silent-master bugs can be localized headlessly.
        // Cached — render_block runs on the audio thread.
        static DEBUG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let debug = *DEBUG.get_or_init(|| std::env::var("FTS_RENDER_DEBUG").is_ok());
        if debug {
            let rms = |s: &[f32]| {
                (s.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / s.len().max(1) as f64)
                    .sqrt()
            };
            for (ti, t) in tracks.iter().enumerate() {
                if t.items.is_empty() {
                    continue;
                }
                let v = rms(&buses[ti].samples);
                if v > 1e-9 {
                    eprintln!("[stage1] {:<24} items={} rms={v:.6}", t.name, t.items.len());
                }
            }
        }

        // Per-track meter bank (read above): post-fader block peaks,
        // written once per track per block (lock-free atomics — safe from
        // the audio callback). Cell index == project track index ==
        // snapshot index, matching `TrackRef::Index` on the Peaks service.

        // Plugin instances for the per-track FX stage inside the loop.
        // The map lives separately from ProjectState so the audio
        // thread doesn't block project mutations on the proto side.
        //
        // TRY, never block. A control thread holds this mutex while it
        // swaps an instrument in (`insert_plugin_instance`) — normally
        // microseconds, but the audio callback must not gamble on that: a
        // blocking acquire here turns any control-side hold into a dropout,
        // and in a wasm AudioWorklet a contended `Mutex` parks the audio
        // thread on `memory_atomic_wait32`, which is precisely the thing
        // the browser rig must never do (browser-keys-rig.md, W13).
        //
        // On contention this block renders WITHOUT its plugin stage: dry
        // audio for one 128-frame quantum (~2.7 ms), which is inaudible
        // next to the click of an underrun. The next block takes the lock
        // normally. `poisoned` still recovers, same as before — a
        // control-thread panic must not cascade into the callback.
        //
        // Before giving up, spin briefly: a control thread's hold is a batch
        // of map inserts (`with_plugin_instances`), a few microseconds, and
        // for an amp chain "dry for one block" is not inaudible — it is the
        // raw DI at full level. Bounded, never parked: a hold that outlasts
        // the spin still costs one dry block, not an underrun.
        let t_lock = Stamp::now();
        let mut plugins = None;
        for _ in 0..PLUGIN_LOCK_SPINS {
            match self.daw.plugin_instances.try_lock() {
                Ok(guard) => {
                    plugins = Some(guard);
                    break;
                }
                Err(std::sync::TryLockError::Poisoned(p)) => {
                    plugins = Some(p.into_inner());
                    break;
                }
                Err(std::sync::TryLockError::WouldBlock) => std::hint::spin_loop(),
            }
        }

        if plugins.is_none() {
            PLUGIN_STAGE_SKIPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let plugin_lock_us = t_lock.us();
        let (mut fx_us, mut slowest) = (0u32, (0u32, 0u32));

        // 1.5) Realtime with a lane pool: every track nothing feeds (no
        // children, no received sends) already holds its whole input, so its
        // FX chain — an instrument lane's synth — runs now, across cores. The
        // topo loop below then skips those chains. Only chains whose every
        // plugin is `parallel_safe` and prepared go; anything else waits for
        // the loop, exactly as before.
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(pool), Some(map)) = (self.lanes.get(), plugins.as_mut()) {
            fed.clear();
            fed.resize(n, false);
            for t in tracks.iter() {
                if t.parent_send
                    && let Some(pi) = t.parent_idx
                {
                    fed[pi] = true;
                }
                for snd in &t.sends {
                    if let Some(di) = snd.dest_idx {
                        fed[di] = true;
                    }
                }
            }
            let mut jobs = 0;
            // The tempo every plugin reads its note values against this
            // block — told here, on the render thread, before any runs.
            let tempo = snap.tempo_map.tempo_at(start_seconds);
            for &ti in &snap.order {
                let t = &tracks[ti];
                if fed[ti]
                    || !passes(ti)
                    || t.fx_chain.is_empty()
                    || t.sends
                        .iter()
                        .any(|s| s.mode == daw_proto::routing::SendMode::PreFx)
                {
                    continue;
                }
                if lane_jobs.len() <= jobs {
                    lane_jobs.push(LaneJob::default());
                }
                let job = &mut lane_jobs[jobs];
                job.chain.clear();
                let mut ok = true;
                for (i, fx_guid) in t.fx_chain.iter().enumerate() {
                    if !t.fx_enabled.get(i).copied().unwrap_or(true)
                        || panicked_fx.contains(fx_guid)
                    {
                        continue;
                    }
                    let Some(plugin) = map.get_mut(fx_guid) else {
                        continue;
                    };
                    if !plugin.is_prepared() || !plugin.parallel_safe() {
                        ok = false;
                        break;
                    }
                    plugin.set_tempo(tempo);
                    job.chain.push((i, PluginPtr::new(&mut **plugin)));
                }
                if !ok || job.chain.is_empty() {
                    job.chain.clear();
                    continue;
                }
                job.ti = ti;
                job.at = start_seconds;
                job.bus = buses[ti].samples.as_mut_ptr() as usize;
                job.aux = if t.aux_taps {
                    aux_sized(&mut aux[ti], frames);
                    aux[ti].as_mut_ptr() as usize
                } else {
                    0
                };
                job.aux_ran = false;
                for v in [&mut job.in_l, &mut job.in_r, &mut job.out_l, &mut job.out_r] {
                    if v.len() != frames {
                        v.resize(frames, 0.0);
                    }
                }
                collect_midi_events_into(
                    &mut job.midi,
                    t,
                    start_seconds,
                    end_seconds,
                    frames_per_second,
                    frames,
                );
                if let Some(bucket) = live_midi_buckets.get_mut(ti)
                    && !bucket.is_empty()
                {
                    job.midi.append(bucket);
                    job.midi.sort_by_key(|e| e.offset);
                }
                job.note_expr = collect_note_expressions(
                    t,
                    start_seconds,
                    end_seconds,
                    frames_per_second,
                    frames,
                );
                job.ran = false;
                job.panicked = None;
                jobs += 1;
            }
            if jobs >= 2 {
                // Longest lanes first, so the last job to finish is short.
                lane_jobs[..jobs].sort_unstable_by_key(|j| std::cmp::Reverse(lane_cost[j.ti]));
                pool.run_lanes(&mut lane_jobs[..jobs], tracks, frames);
                for job in &mut lane_jobs[..jobs] {
                    let ti = job.ti;
                    chain_done[ti] = true;
                    dirty[ti] |= job.ran;
                    aux_live[ti] |= job.aux_ran;
                    lane_cost[ti] = job.us;
                    fx_us += job.us;
                    if job.us > slowest.1 {
                        slowest = (ti as u32, job.us);
                    }
                    if let Some(i) = job.panicked
                        && let Some(fx_guid) = tracks[ti].fx_chain.get(i)
                    {
                        panicked_fx.insert(fx_guid.clone());
                        tracing::error!(
                            "plugin {fx_guid} PANICKED in process_block on track '{}' — \
                             bypassing it from now on",
                            tracks[ti].name,
                        );
                    }
                }
            }
            // No pointer into the map outlives this block.
            for job in &mut lane_jobs[..jobs] {
                job.chain.clear();
            }
        }

        // 2–4) Per-track processing in topo order over the routing
        // graph (children before folder parents, senders before their
        // destinations): each track's bus is gained/panned, its sends
        // dispatched, and the result summed into its folder parent's
        // bus *before* the parent's own gain applies — REAPER's folder
        // routing (the parent fader/FX scale the children's sum plus
        // the parent's own items). Top-level parent-send tracks sum to
        // master.
        let inv_rate = 1.0 / self.sample_rate as f64;
        for &ti in &snap.order {
            let t = &tracks[ti];
            if !passes(ti) {
                // Solo-skipped tracks meter as silence.
                if let Some(cell) = meters.cell(ti) {
                    cell.write(0.0, 0.0, crate::metering::HOLD_DECAY);
                }
                continue;
            }
            // Clean bus = all zeros: gain scales nothing, sends add
            // nothing, parent sums add nothing. Skip the per-frame
            // work entirely; only active tracks cost CPU. (A track
            // with FX must still run — synths generate from MIDI.)
            if !dirty[ti] && t.fx_chain.is_empty() {
                if let Some(cell) = meters.cell(ti) {
                    cell.write(0.0, 0.0, crate::metering::HOLD_DECAY);
                }
                continue;
            }

            // Pre-FX send tap: the bus BEFORE the FX chain runs. At
            // this point in topo order the bus already holds items +
            // children sums + received sends — the track's input.
            let has_pre_fx_tap = t
                .sends
                .iter()
                .any(|s| s.mode == daw_proto::routing::SendMode::PreFx);
            if has_pre_fx_tap {
                pre_fx_tap.clear();
                pre_fx_tap.extend_from_slice(&buses[ti].samples);
            }

            // FX chain: pipe the bus through every loaded plugin in
            // order. Running INSIDE the topo loop means a folder's FX
            // process the children's mix, and bus FX process their
            // received sends — REAPER's signal flow. Synthetic /
            // unloaded FX are skipped — no DSP, no work.
            let t_fx = Stamp::now();
            if !t.fx_chain.is_empty() && !chain_done[ti] {
                let bus = &mut buses[ti];
                // MIDI + note-expression events for this block. Both
                // lists go to every plugin in the chain (REAPER's
                // default) — non-MIDI plugins ignore them.
                collect_midi_events_into(
                    midi_events,
                    t,
                    start_seconds,
                    end_seconds,
                    frames_per_second,
                    frames,
                );
                // Merge programmatic / live MIDI pushed for this track
                // this block (drained in stage 0.5), then re-sort so the
                // combined list stays offset-ordered for the plugin.
                if let Some(bucket) = live_midi_buckets.get_mut(ti)
                    && !bucket.is_empty()
                {
                    midi_events.append(bucket);
                    midi_events.sort_by_key(|e| e.offset);
                }
                let note_expr_events = collect_note_expressions(
                    t,
                    start_seconds,
                    end_seconds,
                    frames_per_second,
                    frames,
                );
                // Mute is applied at the fader, after the chain, so a muted
                // track's plugins still run (tails, meters). Say so to the
                // plugins that need to know — see `plugin::track_muted`.
                let _muted = crate::plugin::TrackMutedGuard::enter(t.muted);
                // ...unless nothing would hear them: the fader zeroes a muted
                // track, and only a pre-fader (post-FX) send taps it before
                // that. Then its chain is not run at all — a mix's muted
                // alternatives (the drum Process's seven reverbs, 4% of a
                // core each) cost nothing while they are off.
                let heard_while_muted = t
                    .sends
                    .iter()
                    .any(|s| s.mode == daw_proto::routing::SendMode::PostFx);
                let chain = if t.muted && !heard_while_muted {
                    &[][..]
                } else {
                    t.fx_chain.as_slice()
                };
                if t.aux_taps {
                    aux_sized(&mut aux[ti], frames);
                }
                for (i, fx_guid) in chain.iter().enumerate() {
                    if !t.fx_enabled.get(i).copied().unwrap_or(true) {
                        continue;
                    }
                    // `None` = the map was busy this block (see the
                    // try_lock above): render dry rather than block.
                    let Some(plugin) = plugins.as_mut().and_then(|p| p.get_mut(fx_guid)) else {
                        continue; // synthetic / not loaded / map busy
                    };
                    if panicked_fx.contains(fx_guid) {
                        continue; // panicked earlier — permanently bypassed
                    }
                    if !plugin.is_prepared() {
                        // Lazy prepare on first use; bypass on failure. A
                        // panicking plugin must not unwind out of the audio
                        // callback (extern "C" boundary → abort), so it's
                        // caught, logged once, and bypassed from then on.
                        let prepared =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                plugin.prepare(self.sample_rate as f64, frames as u32)
                            }));
                        match prepared {
                            Ok(Ok(())) => {}
                            Ok(Err(_)) => continue,
                            Err(_) => {
                                panicked_fx.insert(fx_guid.clone());
                                tracing::error!(
                                    "plugin {fx_guid} PANICKED in prepare on track '{}' — \
                                     bypassing it from now on",
                                    t.name,
                                );
                                continue;
                            }
                        }
                    }
                    // De-interleave → process → re-interleave.
                    for f in 0..frames {
                        in_l[f] = bus.samples[f * 2];
                        in_r[f] = bus.samples[f * 2 + 1];
                    }
                    for v in out_l.iter_mut().chain(out_r.iter_mut()) {
                        *v = 0.0;
                    }
                    if keyed[ti] {
                        let sidechain = &sidechains[ti];
                        for f in 0..frames {
                            key_l[f] = sidechain[f * 2];
                            key_r[f] = sidechain[f * 2 + 1];
                        }
                        plugin.set_sidechain(key_l, key_r);
                    }
                    let fixed = t.fx_params.get(i).map(Vec::as_slice).unwrap_or(&[]);
                    let param_events = match t.fx_envs.get(i) {
                        Some(envs) if !envs.is_empty() => {
                            envelope::params_at(fixed, envs, start_seconds, fx_param_events);
                            fx_param_events.as_slice()
                        }
                        _ => fixed,
                    };
                    plugin.set_tempo(snap.tempo_map.tempo_at(start_seconds));
                    let events = crate::plugin::PluginEvents {
                        params: param_events,
                        midi: midi_events.as_slice(),
                        note_expressions: &note_expr_events,
                    };
                    // catch_unwind so a panicking third-party plugin bypasses
                    // (bus keeps its pre-plugin signal — pass-through) instead
                    // of unwinding across the extern "C" audio callback and
                    // aborting mid-song. No allocation on the happy path.
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        plugin.process_block(in_l, in_r, out_l, out_r, &events)
                    }));
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(_)) => continue,
                        Err(_) => {
                            panicked_fx.insert(fx_guid.clone());
                            tracing::error!(
                                "plugin {fx_guid} PANICKED in process_block on track '{}' — \
                                 bypassing it from now on",
                                t.name,
                            );
                            continue;
                        }
                    }
                    for f in 0..frames {
                        bus.samples[f * 2] = out_l[f];
                        bus.samples[f * 2 + 1] = out_r[f];
                    }
                    if t.aux_taps {
                        aux_live[ti] |= add_aux(&**plugin, &mut aux[ti], frames);
                    }
                    // A plugin ran — it may synthesize (MIDI → audio),
                    // so the bus can carry signal even with no items.
                    dirty[ti] = true;
                }
            }
            if !t.fx_chain.is_empty() && !chain_done[ti] {
                let us = t_fx.us();
                fx_us += us;
                if us > slowest.1 {
                    slowest = (ti as u32, us);
                }
            }
            if !dirty[ti] {
                if let Some(cell) = meters.cell(ti) {
                    cell.write(0.0, 0.0, crate::metering::HOLD_DECAY);
                }
                continue;
            }

            // Pre-fader send tap: post-FX, before the gain stage.
            let has_pre_fader_tap = t
                .sends
                .iter()
                .any(|s| s.mode == daw_proto::routing::SendMode::PostFx);
            if has_pre_fader_tap {
                pre_fader_tap.clear();
                pre_fader_tap.extend_from_slice(&buses[ti].samples);
            }

            // Gain + pan + polarity, per-frame envelopes.
            // VCA grouping (user guide §5.16): a follower's volume is
            // dB-added (= linear-multiplied) with every shared-group
            // lead's fader + volume envelope; the lead's pan (+ pan
            // envelope) offsets the follower's pan; the lead's mute —
            // button or MUTE ENVELOPE — silences followers. Follower
            // faders and mute buttons never move; this is playback-only.
            let vca_leads: Vec<&TrackSnapshot> = if t.vca_follow != 0 {
                tracks
                    .iter()
                    .filter(|l| l.vca_lead & t.vca_follow != 0)
                    .collect()
            } else {
                Vec::new()
            };
            let vca_lead_muted = vca_leads.iter().any(|l| l.muted);
            let mut vca_cursors: Vec<(
                Option<EnvelopeCursor>,
                Option<EnvelopeCursor>,
                Option<EnvelopeCursor>,
            )> = vca_leads
                .iter()
                .map(|l| {
                    (
                        l.volume_env.as_ref().map(|p| EnvelopeCursor::new(p)),
                        l.pan_env.as_ref().map(|p| EnvelopeCursor::new(p)),
                        l.mute_env.as_ref().map(|p| EnvelopeCursor::new(p)),
                    )
                })
                .collect();
            {
                let bus = &mut buses[ti];
                let mut c_vol = t.volume_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_pvol = t.volume_prefx_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_pan = t.pan_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_ppan = t.pan_prefx_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_mute = t.mute_env.as_ref().map(|p| EnvelopeCursor::new(p));
                // No envelope and no VCA lead: the gains are the same on every
                // frame — work them out once (the per-frame arithmetic below,
                // with its absent envelopes at their defaults) and just scale.
                let fixed = c_vol.is_none()
                    && c_pvol.is_none()
                    && c_pan.is_none()
                    && c_ppan.is_none()
                    && c_mute.is_none()
                    && vca_leads.is_empty();
                if fixed {
                    let vol = t.volume * 1.0 * 1.0;
                    let pan = (t.pan + 0.0 + 0.0).clamp(-1.0, 1.0);
                    if t.muted || vca_lead_muted {
                        bus.samples[..bus.frames * 2].fill(0.0);
                    } else {
                        let sign = if t.phase_inverted { -1.0 } else { 1.0 };
                        // The same balance law as the per-frame path below.
                        let (lg, rg) = balance(pan);
                        let lg = (lg * vol * sign) as f32;
                        let rg = (rg * vol * sign) as f32;
                        for fr in bus.samples[..bus.frames * 2].chunks_exact_mut(2) {
                            fr[0] *= lg;
                            fr[1] *= rg;
                        }
                    }
                }
                for frame in (0..bus.frames).filter(|_| !fixed) {
                    let time = start_seconds + frame as f64 * rate * inv_rate;
                    let v_main = c_vol.as_mut().and_then(|c| c.eval_at(time)).unwrap_or(1.0);
                    let v_prefx = c_pvol.as_mut().and_then(|c| c.eval_at(time)).unwrap_or(1.0);
                    // Read and the writing modes: the envelope is the
                    // volume; Trim/Read: the fader trims it.
                    let fader = if t.volume_replaces { 1.0 } else { t.volume };
                    let mut vol = fader * v_main * v_prefx;
                    let p_main = c_pan
                        .as_mut()
                        .and_then(|c| c.eval_at(time))
                        .map(|v| (v - 0.5) * 2.0)
                        .unwrap_or(0.0);
                    let p_prefx = c_ppan
                        .as_mut()
                        .and_then(|c| c.eval_at(time))
                        .map(|v| (v - 0.5) * 2.0)
                        .unwrap_or(0.0);
                    let knob = if t.pan_replaces { 0.0 } else { t.pan };
                    let mut pan = knob + p_main + p_prefx;
                    // VCA leads ride this follower: gain multiplies,
                    // pan offsets, lead mute envelopes gate.
                    let mut vca_env_muted = false;
                    for (li, lead) in vca_leads.iter().enumerate() {
                        let (c_lv, c_lp, c_lm) = &mut vca_cursors[li];
                        let lv = c_lv.as_mut().and_then(|c| c.eval_at(time)).unwrap_or(1.0);
                        vol *= lead.volume * lv;
                        let lp = c_lp
                            .as_mut()
                            .and_then(|c| c.eval_at(time))
                            .map(|v| (v - 0.5) * 2.0)
                            .unwrap_or(0.0);
                        pan += lead.pan + lp;
                        if c_lm
                            .as_mut()
                            .and_then(|c| c.eval_at(time))
                            .map(|v| v > 0.5)
                            .unwrap_or(false)
                        {
                            vca_env_muted = true;
                        }
                    }
                    let pan = pan.clamp(-1.0, 1.0);
                    let env_muted = c_mute
                        .as_mut()
                        .and_then(|c| c.eval_at(time))
                        .map(|v| v > 0.5)
                        .unwrap_or(false);
                    let muted = t.muted || env_muted || vca_lead_muted || vca_env_muted;
                    if muted {
                        bus.samples[frame * 2] = 0.0;
                        bus.samples[frame * 2 + 1] = 0.0;
                        continue;
                    }
                    // Polarity invert folds into the gain sign.
                    let sign = if t.phase_inverted { -1.0 } else { 1.0 };
                    let (lg, rg) = balance(pan);
                    let (lg, rg) = (lg * vol * sign, rg * vol * sign);
                    bus.samples[frame * 2] *= lg as f32;
                    bus.samples[frame * 2 + 1] *= rg as f32;
                }
            }

            // Sends — additive into destination buses, per-frame. (Copy the
            // post-fader source once so we can mutably borrow dest buses.)
            src.clear();
            src.extend_from_slice(&buses[ti].samples);

            // Meter the post-fader signal (folder children already
            // summed in — topo order guarantees the bus is complete).
            if let Some(cell) = meters.cell(ti) {
                let (mut pl, mut pr) = (0.0f32, 0.0f32);
                for fr in src.chunks_exact(2) {
                    pl = pl.max(fr[0].abs());
                    pr = pr.max(fr[1].abs());
                }
                cell.write(pl, pr, crate::metering::HOLD_DECAY);
            }

            if debug {
                let v = (src.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>()
                    / src.len().max(1) as f64)
                    .sqrt();
                if v > 1e-9 {
                    eprintln!(
                        "[stage2] {:<24} rms={v:.6} vol={:.3} ps={} pg={} hw={} sends={}",
                        t.name,
                        t.volume,
                        t.parent_send,
                        if t.parent_idx.is_some() {
                            "ok"
                        } else if t.has_parent_guid {
                            "MISS"
                        } else {
                            "none"
                        },
                        t.hw_out,
                        t.sends.len()
                    );
                }
            }
            for snd in &t.sends {
                if snd.muted {
                    continue;
                }
                let Some(di) = snd.dest_idx else { continue };
                // From a plugin's own outputs (3/4, 5/6): taken post-FX,
                // and through the fader when the send is post-fader — the
                // track's mute and level, not its envelopes.
                let mut from_aux = 1.0_f32;
                // Pick the tap matching the send's chain position.
                let send_src: &[f32] = if snd.from_pair > 0 {
                    if snd.mode == daw_proto::routing::SendMode::PostFader {
                        if t.muted {
                            continue;
                        }
                        from_aux = t.volume as f32;
                    }
                    let span = (snd.from_pair - 1) * frames * 2..snd.from_pair * frames * 2;
                    match aux
                        .get(ti)
                        .filter(|_| aux_live[ti])
                        .and_then(|a| a.get(span))
                    {
                        Some(pair) => pair,
                        None => continue,
                    }
                } else {
                    match snd.mode {
                        daw_proto::routing::SendMode::PreFx if has_pre_fx_tap => pre_fx_tap,
                        daw_proto::routing::SendMode::PostFx if has_pre_fader_tap => pre_fader_tap,
                        _ => src,
                    }
                };
                let dest_bus: &mut [f32] = if snd.to_sidechain {
                    let sidechain = &mut sidechains[di];
                    if sidechain.len() != frames * 2 {
                        sidechain.resize(frames * 2, 0.0);
                    }
                    keyed[di] = true;
                    sidechain
                } else {
                    dirty[di] = true;
                    &mut buses[di].samples
                };
                let mut c_vol = snd.volume_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_pan = snd.pan_env.as_ref().map(|p| EnvelopeCursor::new(p));
                let mut c_mute = snd.mute_env.as_ref().map(|p| EnvelopeCursor::new(p));
                for frame in 0..frames {
                    let time = start_seconds + frame as f64 * rate * inv_rate;
                    let env_muted = c_mute
                        .as_mut()
                        .and_then(|c| c.eval_at(time))
                        .map(|v| v > 0.5)
                        .unwrap_or(false);
                    if env_muted {
                        continue;
                    }
                    let v_env = c_vol.as_mut().and_then(|c| c.eval_at(time)).unwrap_or(1.0);
                    let vol = snd.volume * v_env;
                    let p_off = c_pan
                        .as_mut()
                        .and_then(|c| c.eval_at(time))
                        .map(|v| (v - 0.5) * 2.0)
                        .unwrap_or(0.0);
                    let pan = (snd.pan + p_off).clamp(-1.0, 1.0);
                    let (lg, rg) = balance(pan);
                    let (lg, rg) = ((lg * vol) as f32 * from_aux, (rg * vol) as f32 * from_aux);
                    dest_bus[frame * 2] += send_src[frame * 2] * lg;
                    dest_bus[frame * 2 + 1] += send_src[frame * 2 + 1] * rg;
                }
            }

            // Parent / master sum — not for a track that, under a solo,
            // only feeds what is soloed; and straight to master for one
            // soloed with no way there of its own (`graph::Pass`).
            let mode = pass(ti);
            if mode == graph::Pass::Monitor {
                for (m, smp) in master.samples.iter_mut().zip(src.iter()) {
                    *m += *smp;
                }
            } else if mode != graph::Pass::Feed && t.parent_send {
                match t.parent_idx {
                    Some(pi) => {
                        let parent_bus = &mut buses[pi];
                        for (m, smp) in parent_bus.samples.iter_mut().zip(src.iter()) {
                            *m += *smp;
                        }
                        dirty[pi] = true;
                    }
                    None => {
                        for (m, smp) in master.samples.iter_mut().zip(src.iter()) {
                            *m += *smp;
                        }
                        if debug {
                            let v = (src.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>()
                                / src.len().max(1) as f64)
                                .sqrt();
                            if v > 1e-9 {
                                eprintln!("[->master] {:<24} rms={v:.6}", t.name);
                            }
                        }
                    }
                }
            } else if mode != graph::Pass::Feed && t.hw_out {
                // No parent send but routed to hardware — v0 sums hw
                // outs straight to the master device buffer.
                for (m, smp) in master.samples.iter_mut().zip(src.iter()) {
                    *m += *smp;
                }
                if debug {
                    let v = (src.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>()
                        / src.len().max(1) as f64)
                        .sqrt();
                    if v > 1e-9 {
                        eprintln!("[hw->master] {:<24} rms={v:.6}", t.name);
                    }
                }
            }
        }

        // Metronome click: short sine bursts on the beat grid,
        // accented on measure starts. Overlaid pre-master-fader so
        // the click rides the master level like REAPER's default
        // routing. Gated by the transport engine's atomic (no
        // snapshot rebuild on toggle).
        // Peek the engine map WITHOUT creating an engine —
        // `transport_engine_for` lazily spawns pump tasks (needs a
        // runtime), and offline render paths run without one. No
        // engine yet ⇒ transport never started ⇒ no click.
        let metronome_on = self
            .daw
            .transport_engines
            .lock()
            .ok()
            .and_then(|m| m.get(&self.project_guid).map(|b| b.shared.metronome()))
            .unwrap_or(false);
        if metronome_on {
            let bpm_grid = &snap.tempo_map;
            let first_beat = bpm_grid.seconds_to_beat(start_seconds).ceil() as i64;
            let last_beat = bpm_grid.seconds_to_beat(end_seconds).floor() as i64;
            // Also catch a click whose tail started before this block.
            let click_len = 0.012f64; // 12ms burst
            for beat in (first_beat - 1)..=last_beat {
                if beat < 0 {
                    continue;
                }
                let t0 = bpm_grid.beat_to_seconds(beat as f64);
                if t0 + click_len <= start_seconds || t0 >= end_seconds {
                    continue;
                }
                let accent = (beat as u64).is_multiple_of(snap.beats_per_measure as u64);
                let freq = if accent { 1568.0 } else { 1046.5 }; // G6 / C6
                let gain = if accent { 0.5 } else { 0.35 };
                for frame in 0..frames {
                    let t = start_seconds + frame as f64 * rate * inv_rate - t0;
                    if t < 0.0 || t >= click_len {
                        continue;
                    }
                    // Exponential-ish decay envelope.
                    let env = (1.0 - t / click_len).powi(2);
                    let sample = ((t * freq * std::f64::consts::TAU).sin() * env * gain) as f32;
                    master.samples[frame * 2] += sample;
                    master.samples[frame * 2 + 1] += sample;
                }
            }
        }

        // Master fader (RPP `MASTER_VOLUME` / `MASTERMUTESOLO`).
        // Balance pan law: centre = unity (no constant-power dip on
        // an already-mixed stereo bus), panning attenuates the
        // opposite channel.
        if snap.master_muted {
            master.fill(0.0);
        } else if snap.master_volume != 1.0 || snap.master_pan != 0.0 {
            let vol = snap.master_volume as f32;
            let pan = (snap.master_pan as f32).clamp(-1.0, 1.0);
            let lg = vol * (1.0 - pan.max(0.0));
            let rg = vol * (1.0 + pan.min(0.0));
            for f in 0..master.frames {
                master.samples[f * 2] *= lg;
                master.samples[f * 2 + 1] *= rg;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        if self.realtime.get().is_some() {
            self.daw.block_profile.record(&BlockProfile {
                total_us: t_start.us(),
                snapshot_us,
                plugin_lock_us,
                fx_us,
                slowest_track: slowest.0,
                slowest_track_us: slowest.1,
                frames: frames as u32,
            });
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (snapshot_us, plugin_lock_us, fx_us, slowest);
        master
    }

    /// Transport-derived clock info at `seconds` for the aux post-render
    /// hook: `(pos_beats, tempo_bpm, time_sig_num, time_sig_den)` from
    /// the revision-cached snapshot (tempo map + project transport).
    /// Falls back to 120 bpm 4/4 when the project has no snapshot.
    ///
    /// Realtime mode reads the snapshot being played and never builds one:
    /// this runs in the audio callback after every block, and a build there
    /// (the whole project, every item cloned) is what froze playback once
    /// an edit had cut the kit into thousands of slices.
    pub(crate) fn clock_info(&self, seconds: f64) -> (f64, f64, u32, u32) {
        #[cfg(not(target_arch = "wasm32"))]
        let snap = if self.realtime.get().is_some() {
            self.scratch
                .try_lock()
                .ok()
                .and_then(|s| s.current.as_ref().map(|(_, snap)| snap.clone()))
        } else {
            self.snapshot()
        };
        #[cfg(target_arch = "wasm32")]
        let snap = self.snapshot();
        match snap {
            Some(snap) => (
                snap.tempo_map.seconds_to_beat(seconds),
                snap.tempo_map.tempo_at(seconds),
                snap.beats_per_measure,
                snap.time_sig_den,
            ),
            None => (seconds * 2.0, 120.0, 4, 4),
        }
    }

    /// Revision-cached [`RenderSnapshot`] — rebuilt only when the
    /// project's mutation revision moves. (A realtime renderer takes its
    /// snapshots from [`SnapshotBuilder`] instead.)
    fn snapshot(&self) -> Option<Arc<RenderSnapshot>> {
        let revision = self.daw.read_project(&self.project_guid, |p| p.revision)?;
        if let Ok(cache) = self.cache.lock()
            && let Some((cached_rev, snap)) = cache.as_ref()
            && *cached_rev == revision
        {
            return Some(snap.clone());
        }
        // Build + revision in ONE lock pass so the cached pair is
        // always internally consistent.
        let (revision, snap) = self.daw.read_project(&self.project_guid, |p| {
            (p.revision, RenderSnapshot::build(p))
        })?;
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some((revision, snap.clone()));
        }
        Some(snap)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod live_input_tests {
    use super::*;

    /// Build a `LiveInput` whose ring is pre-filled with `frames`
    /// interleaved frames of `channels` channels, where channel `c`
    /// carries the constant value `(c + 1) as f32 * 0.1`.
    fn filled_live(channels: usize, frames: usize) -> LiveInput {
        let want = frames * channels;
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(want.max(1));
        for f in 0..frames {
            for c in 0..channels {
                let _ = prod.push((c as f32 + 1.0) * 0.1);
                let _ = f; // silence unused in some channel counts
            }
        }
        LiveInput {
            cons,
            channels,
            scratch: Vec::new(),
            underruns: 0,
            resyncs: 0,
        }
    }

    #[test]
    fn live_input_routes_channel_into_track_bus() {
        let frames = 8;
        let channels = 4;
        let mut live = filled_live(channels, frames);

        // 3 tracks: track 0 taps ch 2, track 1 taps nothing, track 2 taps ch 0.
        let input_channels = vec![Some(2u32), None, Some(0u32)];
        let mut buses = vec![
            StereoBuffer::zeroed(frames, 48_000),
            StereoBuffer::zeroed(frames, 48_000),
            StereoBuffer::zeroed(frames, 48_000),
        ];
        let mut dirty = vec![false; 3];

        mix_live_input_into_buses(
            &mut live,
            &input_channels,
            |_| true,
            &mut buses,
            &mut dirty,
            frames,
        );

        // Track 0: channel 2 → value 0.3 on both L and R.
        for f in 0..frames {
            assert!((buses[0].samples[f * 2] - 0.3).abs() < 1e-6);
            assert!((buses[0].samples[f * 2 + 1] - 0.3).abs() < 1e-6);
        }
        assert!(dirty[0]);

        // Track 1: no input tap → still silent + clean.
        assert!(buses[1].samples.iter().all(|s| *s == 0.0));
        assert!(!dirty[1]);

        // Track 2: channel 0 → value 0.1.
        for f in 0..frames {
            assert!((buses[2].samples[f * 2] - 0.1).abs() < 1e-6);
            assert!((buses[2].samples[f * 2 + 1] - 0.1).abs() < 1e-6);
        }
        assert!(dirty[2]);
        // Whole block was available — no underrun.
        assert_eq!(live.underruns, 0);
    }

    #[test]
    fn solo_skipped_track_gets_no_live_input() {
        let frames = 4;
        let channels = 2;
        let mut live = filled_live(channels, frames);
        let input_channels = vec![Some(0u32)];
        let mut buses = vec![StereoBuffer::zeroed(frames, 48_000)];
        let mut dirty = vec![false; 1];

        // `passes` returns false → track contributes nothing.
        mix_live_input_into_buses(
            &mut live,
            &input_channels,
            |_| false,
            &mut buses,
            &mut dirty,
            frames,
        );
        assert!(buses[0].samples.iter().all(|s| *s == 0.0));
        assert!(!dirty[0]);
    }

    #[test]
    fn backlog_past_one_spare_block_is_dropped() {
        // Five blocks queued (a stall's worth): the render must play the
        // second-newest block and leave one spare, not the oldest — the
        // oldest would keep the input four blocks late forever.
        let frames = 4;
        let channels = 2;
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(5 * frames * channels);
        for f in 0..5 * frames {
            for _ in 0..channels {
                let _ = prod.push(f as f32);
            }
        }
        let mut live = LiveInput {
            cons,
            channels,
            scratch: Vec::new(),
            underruns: 0,
            resyncs: 0,
        };
        let mut buses = vec![StereoBuffer::zeroed(frames, 48_000)];
        let mut dirty = vec![false; 1];

        mix_live_input_into_buses(
            &mut live,
            &[Some(0)],
            |_| true,
            &mut buses,
            &mut dirty,
            frames,
        );

        for f in 0..frames {
            assert_eq!(buses[0].samples[f * 2], (3 * frames + f) as f32);
        }
        assert_eq!(live.cons.slots(), frames * channels);
        assert_eq!(live.resyncs, 1);
        assert_eq!(live.underruns, 0);
    }

    #[test]
    fn short_ring_zero_fills_and_counts_underrun() {
        let frames = 8;
        let channels = 2;
        // Only fill HALF the requested frames.
        let mut live = filled_live(channels, frames / 2);
        let input_channels = vec![Some(1u32)];
        let mut buses = vec![StereoBuffer::zeroed(frames, 48_000)];
        let mut dirty = vec![false; 1];

        mix_live_input_into_buses(
            &mut live,
            &input_channels,
            |_| true,
            &mut buses,
            &mut dirty,
            frames,
        );

        // First half carries channel-1 signal (0.2); second half zero-filled.
        for f in 0..frames / 2 {
            assert!((buses[0].samples[f * 2] - 0.2).abs() < 1e-6);
        }
        for f in frames / 2..frames {
            assert_eq!(buses[0].samples[f * 2], 0.0);
        }
        assert_eq!(live.underruns, 1);
        assert!(dirty[0]);
    }

    #[test]
    fn out_of_range_channel_is_ignored() {
        let frames = 4;
        let channels = 2;
        let mut live = filled_live(channels, frames);
        // Track taps channel 5 which doesn't exist on a 2-channel input.
        let input_channels = vec![Some(5u32)];
        let mut buses = vec![StereoBuffer::zeroed(frames, 48_000)];
        let mut dirty = vec![false; 1];

        mix_live_input_into_buses(
            &mut live,
            &input_channels,
            |_| true,
            &mut buses,
            &mut dirty,
            frames,
        );
        assert!(buses[0].samples.iter().all(|s| *s == 0.0));
        assert!(!dirty[0]);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod live_midi_tests {
    use super::*;
    use daw_proto::{Channel, ControllerNumber, ControllerValue, KeyNumber, MidiEvent, Velocity};

    // Terse constructors for the canonical `MidiEvent` (channel 0).
    fn note_on(key: u8, vel: u8) -> MidiEvent {
        MidiEvent::NoteOn {
            channel: Channel::new(0),
            key: KeyNumber::new(key),
            velocity: Velocity::new(vel),
        }
    }
    fn note_off(key: u8, vel: u8) -> MidiEvent {
        MidiEvent::NoteOff {
            channel: Channel::new(0),
            key: KeyNumber::new(key),
            velocity: Velocity::new(vel),
        }
    }
    fn control_change(controller: u8, value: u8) -> MidiEvent {
        MidiEvent::ControlChange {
            channel: Channel::new(0),
            controller: ControllerNumber::new(controller),
            value: ControllerValue::new(value),
        }
    }

    /// Build a `LiveMidiQueue` pre-filled with the given events.
    fn queue_with(events: Vec<LiveMidiEvent>) -> LiveMidiQueue {
        let (mut prod, cons) = rtrb::RingBuffer::<LiveMidiEvent>::new(events.len().max(1));
        for ev in events {
            let _ = prod.push(ev);
        }
        LiveMidiQueue { cons }
    }

    #[test]
    fn drain_routes_events_to_the_matching_track_bucket() {
        let guids = ["track-a", "track-b", "track-c"];
        let idx_of = |g: &str| guids.iter().position(|x| *x == g);

        let mut queue = queue_with(vec![
            LiveMidiEvent {
                track: "track-b".into(),
                offset: 0,
                message: note_on(60, 100),
            },
            LiveMidiEvent {
                track: "track-a".into(),
                offset: 0,
                message: control_change(7, 64),
            },
            // Unknown guid → dropped, not merged.
            LiveMidiEvent {
                track: "ghost".into(),
                offset: 0,
                message: note_off(60, 0),
            },
        ]);

        let mut buckets: Vec<Vec<crate::plugin::PluginMidiEvent>> = vec![Vec::new(); 3];
        drain_live_midi(&mut queue, idx_of, &mut buckets);

        // track-a (index 0) got the CC; track-b (index 1) got the NoteOn.
        assert_eq!(buckets[0].len(), 1);
        assert!(matches!(
            buckets[0][0].message,
            MidiEvent::ControlChange { controller, value, .. }
                if controller.get() == 7 && value.get() == 64
        ));
        assert_eq!(buckets[1].len(), 1);
        assert!(matches!(
            buckets[1][0].message,
            MidiEvent::NoteOn { key, velocity, .. }
                if key.get() == 60 && velocity.get() == 100
        ));
        // track-c untouched; the unknown-guid event was dropped.
        assert!(buckets[2].is_empty());
    }

    #[test]
    fn drained_events_merge_and_resort_with_collected_events() {
        // Mirror the render_block merge: an existing offset-ordered list
        // gets the live bucket appended, then re-sorted by offset.
        let mut midi_events: Vec<crate::plugin::PluginMidiEvent> = vec![
            crate::plugin::PluginMidiEvent {
                offset: 0,
                message: note_on(48, 90),
            },
            crate::plugin::PluginMidiEvent {
                offset: 256,
                message: note_off(48, 0),
            },
        ];

        let mut bucket = vec![
            crate::plugin::PluginMidiEvent {
                offset: 128,
                message: control_change(1, 100),
            },
            crate::plugin::PluginMidiEvent {
                offset: 0,
                message: note_on(60, 100),
            },
        ];

        midi_events.append(&mut bucket);
        midi_events.sort_by_key(|e| e.offset);

        let offsets: Vec<u32> = midi_events.iter().map(|e| e.offset).collect();
        assert_eq!(offsets, vec![0, 0, 128, 256]);
        assert!(bucket.is_empty());
    }
}

/// A stereo signal's gains at `pan` (−1 left … 1 right): REAPER's default,
/// stereo balance with a 0 dB pan law (`PANMODE 3`, `PANLAW 1`). Centred,
/// both sides pass at unity; panned, the far side is turned down and the
/// near side is left as it is.
///
/// It was a constant-power law, √0.5 (−3 dB) on each side at centre, on
/// every track, folder, bus and send: a centred kick mic summed through
/// its folders and buses lost 3 dB a stage — unbreakable's kit played
/// 24 dB under its gain-staged level.
#[must_use]
pub(crate) fn balance(pan: f64) -> (f64, f64) {
    let pan = pan.clamp(-1.0, 1.0);
    ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0))
}

#[cfg(test)]
mod poison_recovery_tests {
    use super::*;

    /// A control-thread panic while holding `plugin_instances` (or a
    /// renderer-internal mutex) must not cascade: the next audio block's
    /// lock recovers the poisoned mutex instead of panicking inside the
    /// extern "C" audio callback (which would abort the process).
    #[test]
    fn render_survives_poisoned_plugin_instances() {
        let daw = Standalone::new();
        let guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "p".into(),
            name: "p".into(),
            path: String::new(),
        });

        // Poison the shared map: panic on another thread while holding it.
        let map = daw.plugin_instances.clone();
        let _ = std::thread::spawn(move || {
            let _guard = map.lock().unwrap();
            panic!("poison plugin_instances");
        })
        .join();
        assert!(
            daw.plugin_instances.lock().is_err(),
            "mutex should be poisoned"
        );

        // Control seams recover in place of panicking…
        assert!(!daw.has_plugin_instance("nope"));
        assert!(daw.with_plugin_instance("nope", |_| ()).is_none());

        // …and the render path (which takes the same lock every block)
        // keeps producing blocks.
        let renderer = ProjectRenderer::new(&daw, &guid, 48_000);
        let out = renderer.render_block(0, 128);
        assert_eq!(out.frames, 128);
    }

    /// The renderer's own scratch mutex also recovers from poison.
    #[test]
    fn render_survives_poisoned_scratch() {
        let daw = Standalone::new();
        let guid = daw.seed_project(daw_proto::ProjectInfo {
            guid: "p".into(),
            name: "p".into(),
            path: String::new(),
        });
        let renderer = std::sync::Arc::new(ProjectRenderer::new(&daw, &guid, 48_000));

        let r = renderer.clone();
        let _ = std::thread::spawn(move || {
            let _guard = r.scratch.lock().unwrap();
            panic!("poison render scratch");
        })
        .join();
        assert!(renderer.scratch.lock().is_err(), "mutex should be poisoned");

        let out = renderer.render_block(0, 256);
        assert_eq!(out.frames, 256);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod folder_routing_tests {
    use super::*;
    use crate::audio_engine::decoder::DecodedAudio;
    use crate::audio_engine::materialize::attach_audio_source;
    use crate::media_seed::{StemSpec, seed_media_tracks};
    use daw_proto::{ProjectContext, ProjectInfo, TrackRef, Tracks};

    fn rms(s: &[f32]) -> f64 {
        if s.is_empty() {
            return 0.0;
        }
        (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt()
    }

    /// Seed the REAL media path — Vocals{Lead,BGV}, Drums{Kick,Snare}, ungrouped
    /// Loop — then inject synthetic constant audio into every take so the items
    /// sound headlessly (no real WAV files needed).
    fn seeded() -> (Standalone, String) {
        let daw = Standalone::new();
        let guid = daw.seed_project(ProjectInfo {
            guid: "route-test".into(),
            name: "RT".into(),
            path: String::new(),
        });
        let stems = vec![
            StemSpec::new("Lead", "/x/lead.wav", Some("Vocals")),
            StemSpec::new("BGV", "/x/bgv.wav", Some("Vocals")),
            StemSpec::new("Kick", "/x/kick.wav", Some("Drums")),
            StemSpec::new("Snare", "/x/snare.wav", Some("Drums")),
            StemSpec::new("Loop", "/x/loop.wav", None),
        ];
        seed_media_tracks(&daw, &guid, &stems, 0.0, 10.0);
        let takes: Vec<String> = daw
            .read_project(&guid, |p| {
                p.takes
                    .values()
                    .flat_map(|tl| tl.takes.iter().map(|t| t.guid.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for tg in takes {
            attach_audio_source(
                &daw,
                &guid,
                &tg,
                DecodedAudio::new(vec![0.5; 4096], 1, 48_000),
            );
        }
        (daw, guid)
    }

    fn guid_of(daw: &Standalone, guid: &str, name: &str) -> String {
        daw.read_project(guid, |p| {
            p.tracks
                .iter()
                .find(|t| t.name == name)
                .map(|t| t.guid.clone())
        })
        .flatten()
        .unwrap_or_else(|| panic!("no track named {name}"))
    }

    // (a) The seed populates folder parents.
    #[test]
    fn seed_populates_folder_parents() {
        let (daw, guid) = seeded();
        let tracks = daw.read_project(&guid, |p| p.tracks.clone()).unwrap();
        let by = |n: &str| tracks.iter().find(|t| t.name == n).expect(n);
        let vocals = by("Vocals");
        let drums = by("Drums");
        assert!(
            vocals.is_folder && vocals.parent_guid.is_none(),
            "Vocals folder"
        );
        assert!(
            drums.is_folder && drums.parent_guid.is_none(),
            "Drums folder"
        );
        assert!(!by("Lead").is_folder);
        assert_eq!(
            by("Lead").parent_guid.as_deref(),
            Some(vocals.guid.as_str())
        );
        assert_eq!(by("BGV").parent_guid.as_deref(), Some(vocals.guid.as_str()));
        assert_eq!(by("Kick").parent_guid.as_deref(), Some(drums.guid.as_str()));
        assert_eq!(
            by("Snare").parent_guid.as_deref(),
            Some(drums.guid.as_str())
        );
        assert_eq!(by("Loop").parent_guid, None);
    }

    // (b) The render snapshot resolves parent_idx for children.
    #[test]
    fn snapshot_resolves_parent_idx() {
        let (daw, guid) = seeded();
        let snap = daw.read_project(&guid, RenderSnapshot::build).unwrap();
        let sidx = |n: &str| snap.tracks.iter().position(|t| t.name == n).expect(n);
        let vi = sidx("Vocals");
        let di = sidx("Drums");
        assert_eq!(snap.tracks[sidx("Lead")].parent_idx, Some(vi));
        assert_eq!(snap.tracks[sidx("BGV")].parent_idx, Some(vi));
        assert_eq!(snap.tracks[sidx("Kick")].parent_idx, Some(di));
        assert_eq!(snap.tracks[sidx("Snare")].parent_idx, Some(di));
        assert_eq!(snap.tracks[sidx("Loop")].parent_idx, None);
    }

    // (c) Muting a folder must silence its children (folder-bus cascade).
    #[test]
    fn muting_folder_silences_children() {
        let (daw, guid) = seeded();
        let ctx = ProjectContext::Project(guid.clone());
        let vocals = guid_of(&daw, &guid, "Vocals");
        let lead = guid_of(&daw, &guid, "Lead");
        let bgv = guid_of(&daw, &guid, "BGV");
        let renderer = ProjectRenderer::new(&daw, &guid, 48_000);

        let base = renderer.render_block(0, 512);
        assert!(rms(&base.samples) > 0.01, "baseline should have audio");

        // Reference: mute the two vocal LEAVES individually.
        Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(lead.clone()), true).unwrap();
        Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(bgv.clone()), true).unwrap();
        let leaves = renderer.render_block(0, 512);
        Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(lead), false).unwrap();
        Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(bgv), false).unwrap();

        // Mute the FOLDER — must cascade to the same output.
        Tracks::set_muted(&daw, ctx.clone(), TrackRef::Guid(vocals), true).unwrap();
        let folder = renderer.render_block(0, 512);

        assert!(
            rms(&folder.samples) < rms(&base.samples) - 1e-4,
            "muting the Vocals folder must reduce master (base={:.4}, folder={:.4})",
            rms(&base.samples),
            rms(&folder.samples)
        );
        for (a, b) in folder.samples.iter().zip(leaves.samples.iter()) {
            assert!(
                (a - b).abs() < 1e-4,
                "folder mute must cascade exactly like muting its children"
            );
        }
    }

    // (d) Soloing a leaf must isolate it.
    #[test]
    fn soloing_leaf_isolates_it() {
        let (daw, guid) = seeded();
        let ctx = ProjectContext::Project(guid.clone());
        let kick = guid_of(&daw, &guid, "Kick");
        let others = ["Lead", "BGV", "Snare", "Loop"];
        let renderer = ProjectRenderer::new(&daw, &guid, 48_000);

        // Reference: mute everything except Kick.
        for n in others {
            Tracks::set_muted(
                &daw,
                ctx.clone(),
                TrackRef::Guid(guid_of(&daw, &guid, n)),
                true,
            )
            .unwrap();
        }
        let only_kick = renderer.render_block(0, 512);
        for n in others {
            Tracks::set_muted(
                &daw,
                ctx.clone(),
                TrackRef::Guid(guid_of(&daw, &guid, n)),
                false,
            )
            .unwrap();
        }

        // Solo Kick — must isolate to the same output.
        Tracks::set_soloed(&daw, ctx.clone(), TrackRef::Guid(kick), true).unwrap();
        let solo = renderer.render_block(0, 512);

        assert!(rms(&solo.samples) > 0.01, "soloed kick should be audible");
        for (a, b) in solo.samples.iter().zip(only_kick.samples.iter()) {
            assert!(
                (a - b).abs() < 1e-4,
                "solo must isolate exactly like muting all other tracks"
            );
        }
    }

    // (d′) Soloing a FOLDER must keep its children audible (REAPER: solo a folder
    // → its whole sub-mix plays, everything else muted). Stage 1 must gate item
    // playback on the solo-routing MASK (soloed track + ancestors + descendants),
    // NOT the raw per-track `soloed` flag — a folder's children aren't soloed
    // themselves, so raw-flag gating silences them. This reproduces the live
    // "solo doesn't behave" report for folder solo.
    #[test]
    fn soloing_folder_keeps_children_audible() {
        let (daw, guid) = seeded();
        let ctx = ProjectContext::Project(guid.clone());
        let vocals = guid_of(&daw, &guid, "Vocals");
        let others = ["Kick", "Snare", "Loop"];
        let renderer = ProjectRenderer::new(&daw, &guid, 48_000);

        // Reference: mute everything except the vocal children (what soloing the
        // Vocals folder should sound like).
        for n in others {
            Tracks::set_muted(
                &daw,
                ctx.clone(),
                TrackRef::Guid(guid_of(&daw, &guid, n)),
                true,
            )
            .unwrap();
        }
        let only_vocals = renderer.render_block(0, 512);
        for n in others {
            Tracks::set_muted(
                &daw,
                ctx.clone(),
                TrackRef::Guid(guid_of(&daw, &guid, n)),
                false,
            )
            .unwrap();
        }
        assert!(
            rms(&only_vocals.samples) > 0.01,
            "reference vocal sub-mix must be audible"
        );

        // Solo the Vocals FOLDER — its children must stay audible.
        Tracks::set_soloed(&daw, ctx.clone(), TrackRef::Guid(vocals), true).unwrap();
        let solo_folder = renderer.render_block(0, 512);

        assert!(
            rms(&solo_folder.samples) > 0.01,
            "soloing the Vocals folder silenced its children (rms={:.5}) — stage 1 gated on raw \
             `soloed` instead of the solo-routing mask",
            rms(&solo_folder.samples)
        );
        for (a, b) in solo_folder.samples.iter().zip(only_vocals.samples.iter()) {
            assert!(
                (a - b).abs() < 1e-4,
                "folder solo must isolate to exactly the folder's children"
            );
        }
    }
}
