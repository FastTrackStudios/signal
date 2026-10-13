//! Cross-platform **duplex audio backend** — one realtime callback that gets
//! capture *and* playback buffers for the same cycle (no ring bridge between
//! separate input/output streams). This is what low-latency DAW engines use.
//!
//! The contract is platform-agnostic: a caller hands [`start`](DuplexBackend::start)
//! a [`DuplexConfig`] (how many capture/playback channels, desired latency) and a
//! `process` closure, and the backend invokes that closure from the realtime
//! thread every block. Each platform supplies its own implementation behind the
//! [`Backend`] alias:
//!
//! | OS | impl | mechanism |
//! |----|------|-----------|
//! | Linux | [`crate::duplex_pw::PipewireBackend`] | `pw_filter` (capture+playback ports, one process cb) |
//! | macOS | [`crate::duplex_coreaudio::CoreAudioBackend`] | AUHAL unit, input pulled in the output render callback |
//! | iOS | [`crate::duplex_ios::RemoteIoBackend`] | one RemoteIO unit, input pulled in the output render callback |
//! | Windows | _todo_ | WASAPI duplex / ASIO |
//!
//! The rig's signal processing (tap input channel → FX chain → stereo out) is the
//! `process` closure — identical on every platform; only the backend differs.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

/// What the backend opens: channel counts + the desired block size.
#[derive(Clone, Debug, Default)]
pub struct DuplexConfig {
    /// Graph node name (how it appears in patchbays).
    pub name: String,
    /// Number of capture (input) ports to create.
    pub inputs: usize,
    /// Number of playback (output) ports to create.
    pub outputs: usize,
    /// Desired block size + rate as `(frames, rate)` → a per-node latency request
    /// (e.g. `(64, 48000)`). `None` lets the graph decide.
    pub latency: Option<(u32, u32)>,
    /// With `latency` unset: a block size to ask for without touching the
    /// device's sample rate — for a client that shares a device another
    /// process owns the rate of (a headphone mixer beside the rig).
    pub buffer: Option<u32>,
    /// Only ever *lower* the device's block size, never raise it. CoreAudio's
    /// buffer size is one per process per device, so an output-only engine
    /// (the keys rig) sharing a process with a duplex one (the guitar rig at
    /// 64 frames) must not raise it to its own 256 — it takes the smaller.
    pub buffer_lower_only: bool,
    /// Capture / playback device by name substring (`None` = system
    /// default). Backends without a graph (CoreAudio) open these devices
    /// themselves; graph backends (PipeWire) ignore them — there the
    /// caller links the node to the hardware.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    /// Let a built-in microphone be the capture device — see
    /// [`input_guard`](crate::input_guard). Off by default.
    pub allow_builtin_mic: bool,
}

/// One realtime block handed to the `process` closure. `inputs[c]` and
/// `outputs[c]` are each `frames` long; input and output are the **same** cycle
/// (sample-synchronous — no ring, so no phase underruns).
pub struct ProcessBlock<'a> {
    pub inputs: &'a [&'a [f32]],
    pub outputs: &'a mut [&'a mut [f32]],
    pub frames: usize,
}

/// The `process` closure type: called from the realtime thread every block.
pub type ProcessFn = Box<dyn FnMut(&mut ProcessBlock) + Send>;

/// How many drop events [`EngineStats`] keeps for a reader to collect.
pub const DROP_RING: usize = 256;

/// A dropout the engine saw, for a drop log: when, what kind, and how long
/// the block took against its budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropEvent {
    /// Its sequence number (monotonic; a gap means the ring overflowed).
    pub seq: u64,
    /// [`clock_ns`] when it happened.
    pub at_ns: u64,
    /// What happened.
    pub kind: DropKind,
    /// The block's render time and its realtime budget, ns (0 for a device
    /// overload, which the HAL reports without a block).
    pub render_ns: u64,
    pub budget_ns: u64,
    /// The block's frames.
    pub frames: u32,
}

/// What kind of drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    /// Our render took longer than the block's duration.
    OverBudget,
    /// The device reported a processor overload (CoreAudio) / the graph an
    /// xrun (PipeWire).
    DeviceOverload,
}

/// Nanoseconds on one process-wide monotonic clock — the drop events' time
/// base, so a reader can line them up with its own events.
#[must_use]
pub fn clock_ns() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as u64
}

/// A fixed ring of drop events written from the realtime callback (and the
/// HAL's notification thread) without allocating or locking; a reader
/// collects what it has not seen by sequence number.
pub struct DropRing {
    next: AtomicU64,
    /// Per slot: seq + 1 (0 = empty), at, render, budget, kind | frames.
    slots: Box<[[AtomicU64; 5]]>,
}

impl Default for DropRing {
    fn default() -> Self {
        Self {
            next: AtomicU64::new(0),
            slots: (0..DROP_RING)
                .map(|_| std::array::from_fn(|_| AtomicU64::new(0)))
                .collect(),
        }
    }
}

impl DropRing {
    /// Record one (realtime-safe).
    pub fn push(&self, kind: DropKind, render_ns: u64, budget_ns: u64, frames: u32) {
        let seq = self.next.fetch_add(1, Ordering::Relaxed);
        let slot = &self.slots[(seq % DROP_RING as u64) as usize];
        slot[0].store(0, Ordering::Release);
        slot[1].store(clock_ns(), Ordering::Relaxed);
        slot[2].store(render_ns, Ordering::Relaxed);
        slot[3].store(budget_ns, Ordering::Relaxed);
        let k = match kind {
            DropKind::OverBudget => 0u64,
            DropKind::DeviceOverload => 1,
        };
        slot[4].store(k << 32 | u64::from(frames), Ordering::Relaxed);
        slot[0].store(seq + 1, Ordering::Release);
    }

    /// Every event after `*seen` (a sequence number, 0 to start), oldest
    /// first; advances `*seen`. Events the ring overwrote before this call
    /// are skipped — the gap in `seq` says how many.
    pub fn collect(&self, seen: &mut u64) -> Vec<DropEvent> {
        let end = self.next.load(Ordering::Acquire);
        let start = (*seen).max(end.saturating_sub(DROP_RING as u64));
        let mut out = Vec::new();
        for seq in start..end {
            let slot = &self.slots[(seq % DROP_RING as u64) as usize];
            if slot[0].load(Ordering::Acquire) != seq + 1 {
                continue;
            }
            let kf = slot[4].load(Ordering::Relaxed);
            out.push(DropEvent {
                seq,
                at_ns: slot[1].load(Ordering::Relaxed),
                render_ns: slot[2].load(Ordering::Relaxed),
                budget_ns: slot[3].load(Ordering::Relaxed),
                kind: if kf >> 32 == 1 {
                    DropKind::DeviceOverload
                } else {
                    DropKind::OverBudget
                },
                frames: kf as u32,
            });
        }
        *seen = end;
        out
    }
}

/// Live engine metrics, written from the realtime callback, read by the UI.
/// These are the numbers the rig meters need — render time (DSP load) and
/// xruns — measured directly because the duplex callback is *our* code.
#[derive(Default)]
pub struct EngineStats {
    /// Total process callbacks fired.
    pub calls: AtomicU64,
    /// Frames in the last block (the running quantum).
    pub block_frames: AtomicU32,
    /// Render time of the last block, nanoseconds.
    pub render_ns: AtomicU64,
    /// Peak render time since the last [`reset_peak`](Self::reset_peak), ns.
    pub peak_render_ns: AtomicU64,
    /// Sum of every block's render time, ns — with `calls`, the MEAN.
    ///
    /// Peak is the right measure for "did we drop audio" and the wrong one
    /// for "is this faster than it was": one preempted block on a shared
    /// machine moves the peak by milliseconds and says nothing about the
    /// code. The mean is stable enough to optimise against.
    pub total_render_ns: AtomicU64,
    /// Blocks where the graph reported an xrun.
    ///
    /// NOTE: on a FOLLOWER node this reads the driver's clock and can stay at
    /// zero while the graph is dropping audio — measured against `pw-top`, it
    /// reported 0 while PipeWire counted 112 xruns on the same node in the
    /// same window. Trust [`over_budget`](Self::over_budget) for "did we
    /// cause a dropout"; this stays because a driver-side xrun is still worth
    /// seeing when it is reported.
    pub xruns: AtomicU64,
    /// Blocks whose render took longer than the block's own realtime budget.
    ///
    /// This is the one WE are responsible for: a callback that overruns its
    /// deadline is a dropout the player hears, whether or not the graph gets
    /// around to calling it an xrun. Counted from our own render timing, so
    /// it cannot be silent about our own overruns.
    pub over_budget: AtomicU64,
    /// Last backend stream/filter state code (backend-specific; on the
    /// PipeWire backend this is `pw_filter_state`, where `-1` = error,
    /// `3` = streaming). Written by the backend's state listener; owners
    /// can poll it to detect a dead/errored stream while `calls` stalls.
    pub stream_state: AtomicI32,
    /// Every drop, timestamped, for a drop log (see [`DropRing`]).
    pub drops: DropRing,
}

impl EngineStats {
    /// Record a block's render time (updates last + peak).
    pub fn record_render(&self, ns: u64) {
        self.render_ns.store(ns, Ordering::Relaxed);
        self.peak_render_ns.fetch_max(ns, Ordering::Relaxed);
        self.total_render_ns.fetch_add(ns, Ordering::Relaxed);
    }

    /// Mean render time per block, milliseconds.
    pub fn mean_render_ms(&self) -> f64 {
        let calls = self.calls.load(Ordering::Relaxed);
        if calls == 0 {
            return 0.0;
        }
        self.total_render_ns.load(Ordering::Relaxed) as f64 / calls as f64 / 1e6
    }

    /// Record a block's render time and flag it if it overran the deadline.
    ///
    /// `rate` is needed because the budget is `block_frames / rate`; the
    /// caller knows both. A block that takes longer than its own duration has
    /// already made the next one late, which is what a listener hears as a
    /// click.
    pub fn record_render_at(&self, ns: u64, rate: u32) {
        self.record_render(ns);
        let frames = self.block_frames.load(Ordering::Relaxed) as u64;
        if frames > 0 && rate > 0 {
            let budget_ns = frames * 1_000_000_000 / rate as u64;
            if ns > budget_ns {
                self.over_budget.fetch_add(1, Ordering::Relaxed);
                self.drops
                    .push(DropKind::OverBudget, ns, budget_ns, frames as u32);
            }
        }
    }
    /// DSP load 0..1 = render time / block budget at `rate`.
    pub fn load(&self, rate: u32) -> f64 {
        let frames = self.block_frames.load(Ordering::Relaxed).max(1) as f64;
        let budget_ns = frames / rate.max(1) as f64 * 1e9;
        (self.render_ns.load(Ordering::Relaxed) as f64 / budget_ns).clamp(0.0, 1.0)
    }
    /// Last / peak render time in milliseconds.
    pub fn render_ms(&self) -> (f64, f64) {
        (
            self.render_ns.load(Ordering::Relaxed) as f64 / 1e6,
            self.peak_render_ns.load(Ordering::Relaxed) as f64 / 1e6,
        )
    }
    pub fn reset_peak(&self) {
        self.peak_render_ns.store(0, Ordering::Relaxed);
    }
}

/// A platform duplex audio backend. Dropping it stops audio.
pub trait DuplexBackend: Send + Sized {
    /// Open the device(s) and begin calling `process` from the realtime thread.
    fn start(cfg: DuplexConfig, process: ProcessFn) -> Result<Self, String>;
    /// The negotiated sample rate.
    fn sample_rate(&self) -> u32;
    /// Shared live metrics (render time, xruns, block size).
    fn stats(&self) -> Arc<EngineStats>;
    /// The graph node name, so the caller can wire device ports to it.
    fn node_name(&self) -> &str;
    /// Hardware latency in frames at [`sample_rate`](Self::sample_rate),
    /// as `(input, output)`: everything between the jack and the callback
    /// (converter, driver, safety offset, one buffer) and back out. Their
    /// sum is the round trip a player hears. `None` when the backend cannot
    /// tell.
    fn latency_frames(&self) -> Option<(u32, u32)> {
        None
    }
}

// The platform backend. Linux = native PipeWire `pw_filter`; other platforms
// land here as they're implemented.
#[cfg(target_os = "macos")]
pub use crate::duplex_coreaudio::CoreAudioBackend as Backend;
#[cfg(target_os = "linux")]
pub use crate::duplex_pw::PipewireBackend as Backend;
#[cfg(target_os = "ios")]
pub use crate::duplex_ios::RemoteIoBackend as Backend;

#[cfg(test)]
mod drop_ring_tests {
    use super::*;

    /// Events come back in order with their data, a reader sees each once,
    /// and an overflowed ring keeps the newest.
    #[test]
    fn drops_are_collected_once_in_order() {
        let r = DropRing::default();
        r.push(DropKind::OverBudget, 3_000_000, 2_666_666, 128);
        r.push(DropKind::DeviceOverload, 0, 0, 128);
        let mut seen = 0;
        let got = r.collect(&mut seen);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kind, DropKind::OverBudget);
        assert_eq!(got[0].render_ns, 3_000_000);
        assert_eq!(got[1].kind, DropKind::DeviceOverload);
        assert!(got[1].at_ns >= got[0].at_ns);
        assert!(r.collect(&mut seen).is_empty());
        for _ in 0..(DROP_RING + 10) {
            r.push(DropKind::OverBudget, 1, 1, 64);
        }
        let got = r.collect(&mut seen);
        assert_eq!(got.len(), DROP_RING);
        assert_eq!(got.last().unwrap().seq, (DROP_RING + 11) as u64);
    }
}
