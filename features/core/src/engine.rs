//! The audio side, and the handoff to it from the control side.
//!
//! A [`Processor`] runs a [`Graph`] on the audio thread. A [`Controller`]
//! (the control thread: a host's main thread, an action, the UI) sends it
//! commands through a lock-free ring: install a new graph, swap one block's
//! backend (the CLAP hot-reload override, or ending it), change a block's
//! parameter. Whatever the audio thread replaces goes back through a
//! second ring to be dropped by the controller, so the audio thread never
//! frees memory either.
//!
//! The processor splits each host block at every change point (a host
//! parameter or macro change, a MIDI event, a breakpoint of an offline
//! block's lane it plays itself) and at [`crate::MAX_BLOCK`], so
//! changes are sample-accurate and land at the same frame whichever
//! backend a block runs on.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::block::{Backend, ParamChange};
use crate::graph::Graph;
use crate::midi::MidiEvent;
use crate::param::{MACROS, host};

// ── A single-producer, single-consumer ring ─────────────────────────────

/// Fixed capacity, allocated once; push and pop never allocate.
struct Ring<T> {
    slots: Box<[UnsafeCell<Option<T>>]>,
    /// Next slot to read (consumer only writes it).
    head: AtomicUsize,
    /// Next slot to write (producer only writes it).
    tail: AtomicUsize,
}

// SAFETY: one producer and one consumer, which `Controller` and
// `Processor` enforce by owning one end each (`&mut self` on both). A slot
// is written only by the producer while it is outside `head..tail`, and
// read only by the consumer while it is inside; the Acquire/Release pair
// on `tail` and `head` orders the slot's contents with the index.
unsafe impl<T: Send> Sync for Ring<T> {}
// SAFETY: as above: the values move between threads, so `T: Send`.
unsafe impl<T: Send> Send for Ring<T> {}

impl<T> Ring<T> {
    fn new(capacity: usize) -> Self {
        Self {
            slots: (0..capacity.max(2))
                .map(|_| UnsafeCell::new(None))
                .collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn cap(&self) -> usize {
        self.slots.len()
    }

    fn free(&self) -> usize {
        let used = self
            .tail
            .load(Ordering::Acquire)
            .wrapping_sub(self.head.load(Ordering::Acquire));
        self.cap().saturating_sub(used)
    }

    /// Producer only. `Err(value)` when full.
    fn push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) >= self.cap() {
            return Err(value);
        }
        let Some(slot) = self.slots.get(tail % self.cap()) else {
            return Err(value);
        };
        // SAFETY: the slot is outside head..tail: only the producer touches it.
        unsafe { *slot.get() = Some(value) };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Consumer only.
    fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let slot = self.slots.get(head % self.cap())?;
        // SAFETY: the slot is inside head..tail: only the consumer touches it.
        let value = unsafe { (*slot.get()).take() };
        self.head.store(head.wrapping_add(1), Ordering::Release);
        value
    }

    /// Consumer only: what the next pop would return, by reference.
    fn peek_is(&self, f: impl FnOnce(&T) -> bool) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return false;
        }
        let Some(slot) = self.slots.get(head % self.cap()) else {
            return false;
        };
        // SAFETY: inside head..tail, consumer side.
        unsafe { (*slot.get()).as_ref().is_some_and(f) }
    }
}

/// What the controller asks of the processor.
pub enum Command {
    /// Replace the whole graph (a preset loaded, a chain edited).
    Install(Box<Graph>),
    /// Swap one block's backend at the next block boundary.
    SwapBackend {
        lane: usize,
        block: usize,
        backend: Box<Backend>,
    },
    /// Change one block's parameter.
    Param {
        lane: usize,
        block: usize,
        param: usize,
        change: ParamChange,
    },
    /// A piece's hits as the host has placed them (its hit markers, the
    /// whole song): `(song frame, velocity)`, sorted. They key the piece's
    /// blocks as notes do, on time, with nothing to detect.
    PlannedHits { piece: u16, hits: Box<[(u64, f32)]> },
}

impl Command {
    const fn retires(&self) -> bool {
        matches!(
            self,
            Self::Install(_) | Self::SwapBackend { .. } | Self::PlannedHits { .. }
        )
    }
}

/// What the processor hands back to be dropped off the audio thread.
pub enum Retired {
    Graph(Box<Graph>),
    Backend(Box<Backend>),
    Hits(Box<[(u64, f32)]>),
}

struct Link {
    commands: Ring<Command>,
    retired: Ring<Retired>,
}

/// Commands queued before the processor drops any.
pub const COMMAND_CAPACITY: usize = 1024;

/// A controller and the processor it drives.
#[must_use]
pub fn processor_pair() -> (Controller, Processor) {
    let link = Arc::new(Link {
        commands: Ring::new(COMMAND_CAPACITY),
        retired: Ring::new(64),
    });
    (
        Controller { link: link.clone() },
        Processor::with_link(Some(link)),
    )
}

/// The control side. Not real-time: it allocates and drops.
pub struct Controller {
    link: Arc<Link>,
}

impl Controller {
    /// Queue `command`. `Err(command)` when the queue is full (the audio
    /// side hasn't run for a long time).
    ///
    /// # Errors
    ///
    /// The command back, when the queue is full.
    pub fn send(&mut self, command: Command) -> Result<(), Command> {
        self.collect();
        self.link.commands.push(command)
    }

    /// Install `graph` at the next block boundary.
    ///
    /// # Errors
    ///
    /// The graph back, when the queue is full.
    pub fn install(&mut self, graph: Graph) -> Result<(), Box<Graph>> {
        self.send(Command::Install(Box::new(graph)))
            .map_err(|c| match c {
                Command::Install(g) => g,
                _ => Box::new(Graph::empty()),
            })
    }

    /// Drop what the audio side has replaced.
    pub fn collect(&mut self) {
        while let Some(r) = self.link.retired.pop() {
            drop(r);
        }
    }
}

// ── The processor ───────────────────────────────────────────────────────

/// One of the instance's own parameters ([`crate::param::host`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostParam {
    InputGain,
    OutputGain,
    Bypass,
    /// 0-based.
    Macro(u8),
}

/// A change to one of the instance's parameters, at a frame of the block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostParamEvent {
    pub offset: u32,
    pub param: HostParam,
    /// Plain value (dB, 0/1, 0–1 for a macro).
    pub value: f32,
}

/// The audio side.
pub struct Processor {
    graph: Option<Box<Graph>>,
    link: Option<Arc<Link>>,
    in_gain: f32,
    out_gain: f32,
    bypass: bool,
    macros: [f32; MACROS],
}

impl Default for Processor {
    fn default() -> Self {
        Self::with_link(None)
    }
}

impl Processor {
    /// A processor with no controller: a host that installs graphs itself
    /// while not processing ([`install`](Self::install)).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with_link(link: Option<Arc<Link>>) -> Self {
        Self {
            graph: None,
            link,
            in_gain: crate::util::db_to_gain(host::INPUT_GAIN.default),
            out_gain: crate::util::db_to_gain(host::OUTPUT_GAIN.default),
            bypass: false,
            macros: [0.0; MACROS],
        }
    }

    /// Install `graph` now (the caller isn't processing: `&mut self`).
    /// Returns the one it replaces.
    pub fn install(&mut self, graph: Graph) -> Option<Box<Graph>> {
        let mut graph = Box::new(graph);
        for (i, v) in self.macros.iter().enumerate() {
            graph.apply_macro(i, *v);
        }
        self.graph.replace(graph)
    }

    /// The running graph.
    #[must_use]
    pub fn graph(&self) -> Option<&Graph> {
        self.graph.as_deref()
    }

    /// Frames of latency.
    #[must_use]
    pub fn latency(&self) -> u32 {
        self.graph.as_ref().map_or(0, |g| g.latency())
    }

    /// The song position, in frames, of the block the next
    /// [`process`](Self::process) gets: the host's transport while it
    /// plays. Hits between a session's ports are stamped with it, so every
    /// instance must be told the same; when the host doesn't say, the
    /// graph counts on from the last.
    pub fn set_position(&mut self, frame: u64) {
        if let Some(g) = self.graph.as_mut() {
            g.set_clock(frame);
        }
    }

    /// Clear every block's state (reverb and delay tails, envelopes, the
    /// detectors) and the graph's own delays: what a host's reset asks.
    pub fn reset(&mut self) {
        if let Some(g) = self.graph.as_mut() {
            g.reset();
        }
    }

    /// The running graph, to change on the audio thread (calibrating its
    /// phases).
    pub fn graph_mut(&mut self) -> Option<&mut Graph> {
        self.graph.as_deref_mut()
    }

    /// Change one block's parameter now (the audio thread's own: a
    /// plugin wrapper applying its host's parameter values). From another
    /// thread, send [`Command::Param`].
    pub fn apply_param(&mut self, lane: usize, block: usize, param: usize, change: ParamChange) {
        if let Some(g) = self.graph.as_mut() {
            g.apply(lane, block, param, change);
        }
    }

    /// Set one of the instance's parameters now.
    pub fn set_host_param(&mut self, param: HostParam, value: f32) {
        match param {
            HostParam::InputGain => {
                self.in_gain = crate::util::db_to_gain(host::INPUT_GAIN.clamp(value))
            }
            HostParam::OutputGain => {
                self.out_gain = crate::util::db_to_gain(host::OUTPUT_GAIN.clamp(value))
            }
            HostParam::Bypass => self.bypass = value >= 0.5,
            HostParam::Macro(i) => {
                let i = usize::from(i);
                if let Some(m) = self.macros.get_mut(i) {
                    let v = value.clamp(0.0, 1.0);
                    *m = v;
                    if let Some(g) = self.graph.as_mut() {
                        g.apply_macro(i, v);
                    }
                }
            }
        }
    }

    fn run_commands(&mut self) {
        let Some(link) = self.link.clone() else {
            return;
        };
        loop {
            // A command that hands something back waits for room to.
            if link.commands.peek_is(Command::retires) && link.retired.free() == 0 {
                return;
            }
            let Some(cmd) = link.commands.pop() else {
                return;
            };
            match cmd {
                Command::Install(mut graph) => {
                    for (i, v) in self.macros.iter().enumerate() {
                        graph.apply_macro(i, *v);
                    }
                    if let Some(old) = self.graph.replace(graph) {
                        let _ = link.retired.push(Retired::Graph(old));
                    }
                }
                Command::SwapBackend {
                    lane,
                    block,
                    backend,
                } => {
                    if let Some(g) = self.graph.as_mut() {
                        // The box carries the new backend in and the old out.
                        let mut b = backend;
                        let new = core::mem::replace(
                            &mut *b,
                            Backend::Linked(Box::new(crate::block::Passthrough)),
                        );
                        *b = g.swap_backend(lane, block, new);
                        let _ = link.retired.push(Retired::Backend(b));
                    } else {
                        let _ = link.retired.push(Retired::Backend(backend));
                    }
                }
                Command::Param {
                    lane,
                    block,
                    param,
                    change,
                } => {
                    if let Some(g) = self.graph.as_mut() {
                        g.apply(lane, block, param, change);
                    }
                }
                Command::PlannedHits { piece, hits } => {
                    let old = match self.graph.as_mut() {
                        Some(g) => g.set_planned_hits(piece, hits),
                        None => hits,
                    };
                    let _ = link.retired.push(Retired::Hits(old));
                }
            }
        }
    }

    /// Process `frames` of `io` in place: on entry its first `in_channels`
    /// hold the input, on exit every channel holds the output. `midi` and
    /// `params` are sorted by offset.
    pub fn process(
        &mut self,
        io: &mut [&mut [f32]],
        in_channels: usize,
        frames: usize,
        midi: &[MidiEvent],
        params: &[HostParamEvent],
    ) {
        self.run_commands();
        let max = self
            .graph
            .as_ref()
            .map_or(crate::MAX_BLOCK, |g| g.max_block());
        let mut midi = midi.iter().peekable();
        let mut params = params.iter().peekable();
        let mut pos = 0_usize;
        while pos < frames {
            let at = u32::try_from(pos).unwrap_or(u32::MAX);
            while let Some(e) = params.next_if(|e| e.offset <= at) {
                self.set_host_param(e.param, e.value);
            }
            while let Some(e) = midi.next_if(|e| e.offset <= at) {
                if let Some(g) = self.graph.as_mut() {
                    g.midi(e.to_block());
                }
            }
            let next_event = [
                midi.peek().map(|e| e.offset),
                params.peek().map(|e| e.offset),
            ]
            .into_iter()
            .flatten()
            .min()
            .and_then(|o| usize::try_from(o).ok())
            .unwrap_or(frames);
            // And at the next breakpoint of a lane Signal plays itself.
            let next_break = self
                .graph
                .as_ref()
                .and_then(|g| g.frames_to_next_offline())
                .map_or(frames, |f| pos.saturating_add(f));
            let end = next_event
                .min(next_break)
                .min(frames)
                .min(pos.saturating_add(max))
                .max(pos.saturating_add(1));
            let len = end.saturating_sub(pos);
            self.process_piece(io, in_channels, pos, len);
            pos = end;
        }
    }

    fn process_piece(
        &mut self,
        io: &mut [&mut [f32]],
        in_channels: usize,
        start: usize,
        len: usize,
    ) {
        let end = start.saturating_add(len);
        match (self.bypass, self.graph.as_mut()) {
            (false, Some(g)) => g.process_piece(io, in_channels, start, len, self.in_gain),
            _ => {
                // Bypassed (or nothing installed): the input through, the
                // channels beyond it silent.
                for ch in io.iter_mut().skip(in_channels) {
                    if let Some(s) = ch.get_mut(start..end) {
                        s.fill(0.0);
                    }
                }
                return;
            }
        }
        let g = self.out_gain;
        if (g - 1.0).abs() > f32::EPSILON {
            for ch in io.iter_mut() {
                if let Some(s) = ch.get_mut(start..end) {
                    s.iter_mut().for_each(|x| *x *= g);
                }
            }
        }
    }
}

impl Graph {
    /// A graph with nothing in it: silence out.
    #[must_use]
    pub fn empty() -> Self {
        crate::graph::build(
            &crate::state::Preset {
                name: alloc::string::String::new(),
                inputs: 0,
                outputs: 0,
                lanes: Vec::new(),
                macros: Vec::new(),
            },
            &crate::graph::BuildOptions {
                sample_rate: 48_000.0,
                max_block: 1,
                port: None,
                factories: &[],
                host: &crate::NoHost,
            },
        )
        .0
    }
}
