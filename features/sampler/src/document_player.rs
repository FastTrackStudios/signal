//! Document playback in a host: the track's [`Schedule`] played ahead of
//! the transport, live input played beside it, seeks reconstructed off the
//! audio thread.
//!
//! A [`DocumentPlayer`] owns up to three banks of ONE patch, sharing one
//! sample cache (`PlayerPatch::shared_cache`), so the second and third
//! cost voices, not samples:
//!
//! - the **document** bank, walked by a [`RealtimeScheduler`]: every note
//!   of the document fired at its frame, legato transitions and shorts
//!   pre-fired by their leads, so each is heard on its grid tick;
//! - the **live** bank: what arrives as MIDI while the document plays. A
//!   live note has no future, so it can't be pre-fired: it plays the
//!   strict-live way (reactive legato, the low-latency curve), as the
//!   offline `render_live_replay` replays a part. Its output is summed
//!   with the document's, and it never touches the document bank, so
//!   playing over a track leaves the track's playback exact;
//! - the **shadow** bank, on a worker thread: where a seek's
//!   reconstruction runs ([`reconstruct_at`]) when replaying the activity
//!   span inline would stall the audio thread ([`SeekPolicy`]).
//!
//! ## Which input is live
//!
//! [`Input::Live`]: the host claimed the track's playback for the plugin
//! (`host-timeline/1`'s `set_playback_claimed`), so every event that
//! arrives is live — it plays on the live bank. [`Input::Track`]: the host
//! still sends the track's own notes, which the document already plays —
//! they are dropped while the document plays (phase 2's rule). With the
//! transport stopped or no document, events play on the document bank as
//! a plain live instrument.
//!
//! ## Seeks
//!
//! A discontinuity whose replay is short ([`SeekPolicy::inline_max_frames`])
//! reconstructs inline at the seek block, bit-exactly: the first frame out
//! is the full render's. A longer one is handed to the worker, which
//! reconstructs the shadow bank at `P + lead`; until the playhead gets
//! there the document is silent, then the shadow bank is swapped in (the
//! few frames between where it was built and the block are caught up on
//! the audio thread) behind a [`SeekPolicy::fade_frames`] fade-in. From
//! the end of the fade on, the output is the full render's again, bit for
//! bit. A host pre-roll at least `lead` long hides the gap entirely.
//! Offline renders use [`SeekPolicy::EXACT`] (always inline).
//!
//! ## Realtime
//!
//! [`process`](DocumentPlayer::process) allocates nothing, takes no lock
//! that can wait (`try_lock` only, on the worker's hand-over slots) and
//! does no I/O; banks move between threads as boxes, never dropped on the
//! audio thread. The worker is woken with `Thread::unpark`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::bank::SamplerBank;
use crate::document::Schedule;
use crate::document_rt::{
    BlockTransport, RealtimeScheduler, Reconstructed, WARM_CHUNK, reconstruct_at,
};

/// One live MIDI event, at a frame of the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveEvent {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    Cc { cc: u8, value: u8 },
}

/// What the events handed to [`DocumentPlayer::process`] are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The host claimed the track's playback: every event is live.
    Live,
    /// The host still plays the track's own notes as events.
    Track,
}

/// When a seek reconstructs inline, and how the deferred one comes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekPolicy {
    /// Replays up to this long (frames of audio replayed) run inline at
    /// the seek block, bit-exact from its first frame.
    pub inline_max_frames: u64,
    /// A deferred reconstruction is built for the playhead this far ahead.
    pub lead_frames: u64,
    /// At most this many frames are caught up on the audio thread when the
    /// shadow bank comes back late; beyond, it is built again further on.
    pub catch_up_max: u64,
    /// The fade-in after a deferred reconstruction comes in.
    pub fade_frames: u32,
}

impl SeekPolicy {
    /// Always inline: every start bit-exact (offline renders, tests).
    pub const EXACT: Self = Self {
        inline_max_frames: u64::MAX,
        lead_frames: 0,
        catch_up_max: 0,
        fade_frames: 0,
    };

    /// For a live host at `sample_rate`: replays of up to 50 ms of audio
    /// inline (a few ms of work, see `examples/seek_cost.rs`), otherwise
    /// built 150 ms ahead, a 5 ms fade-in.
    #[must_use]
    pub fn live(sample_rate: u32) -> Self {
        let ms = |n: u64| u64::from(sample_rate) * n / 1000;
        Self {
            inline_max_frames: ms(50),
            lead_frames: ms(150),
            catch_up_max: ms(40),
            fade_frames: u32::try_from(ms(5)).unwrap_or(240),
        }
    }
}

/// What a block of [`DocumentPlayer::process`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Played {
    /// No document playing: the events played on the document bank, live.
    Live,
    /// The document played (and live input beside it, or dropped).
    Document,
    /// The document is being reconstructed off-thread: silent until it
    /// comes in (live input still plays).
    Reconstructing,
}

type Done = (Box<SamplerBank>, Arc<Schedule>, Reconstructed);

/// The hand-over between the audio thread and the worker. The audio
/// thread only `try_lock`s.
struct Exchange {
    id: String,
    request: Mutex<Option<(Arc<Schedule>, i64)>>,
    idle: Mutex<Option<Box<SamplerBank>>>,
    done: Mutex<Option<Done>>,
    quit: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Exchange {
    /// The worker's loop: reconstruct each request on the idle bank and
    /// hand it back.
    fn run(&self) {
        let mut scratch = vec![0.0f32; WARM_CHUNK * 2];
        while !self.quit.load(Ordering::Acquire) {
            let Some((sched, at)) = lock(&self.request).take() else {
                std::thread::park();
                continue;
            };
            let Some(mut bank) = lock(&self.idle).take() else {
                // Its bank is still out: try again when woken.
                lock(&self.request).get_or_insert((sched, at));
                std::thread::park();
                continue;
            };
            let done = reconstruct_at(&mut bank, &self.id, &sched, at, &mut scratch);
            *lock(&self.done) = Some((bank, sched, done));
        }
    }
}

struct Worker {
    exchange: Arc<Exchange>,
    thread: std::thread::Thread,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.exchange.quit.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// What a deferred reconstruction is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// A seek: the document is silent until it comes in.
    Seek,
    /// A new schedule (an edit) while playing: the old one plays on until
    /// the new one comes in at the same frame.
    Edit,
}

/// A deferred reconstruction in flight.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// The frame it is built for.
    at: i64,
    /// Where the next block must start for the transport to be playing on.
    next: i64,
    why: Why,
}

/// Document playback, live input and seeks for one bank instrument. See
/// the module docs.
pub struct DocumentPlayer {
    id: String,
    doc: Box<SamplerBank>,
    live: Option<Box<SamplerBank>>,
    scheduler: RealtimeScheduler,
    adopted: Option<Arc<Schedule>>,
    worker: Option<Worker>,
    /// A bank back from the worker or swapped out, waiting to go back.
    spare: Option<Box<SamplerBank>>,
    pending: Option<Pending>,
    /// A schedule published while playing, built on the worker before it
    /// takes over ([`Why::Edit`]).
    incoming: Option<Arc<Schedule>>,
    policy: SeekPolicy,
    /// Frames of fade-in left, and its length.
    fade: (u32, u32),
    live_scratch: Vec<f32>,
}

impl DocumentPlayer {
    /// A player for instrument `id` of `doc` (and `live`, the same patch
    /// on a bank of its own; `None`: live input is dropped while the
    /// document plays). `max_block` sizes the live scratch.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        doc: SamplerBank,
        live: Option<SamplerBank>,
        max_block: usize,
    ) -> Self {
        let id = id.into();
        Self {
            scheduler: RealtimeScheduler::new(id.clone()),
            id,
            doc: Box::new(doc),
            live: live.map(Box::new),
            adopted: None,
            worker: None,
            spare: None,
            pending: None,
            incoming: None,
            policy: SeekPolicy::EXACT,
            fade: (0, 0),
            live_scratch: vec![0.0; max_block.max(1) * 2],
        }
    }

    /// Reconstruct long seeks on `shadow` (the same patch, a bank of its
    /// own) on a worker thread, as `policy` says.
    ///
    /// # Errors
    ///
    /// When the worker thread can't be started.
    pub fn with_worker(mut self, shadow: SamplerBank, policy: SeekPolicy) -> std::io::Result<Self> {
        let exchange = Arc::new(Exchange {
            id: self.id.clone(),
            request: Mutex::new(None),
            idle: Mutex::new(Some(Box::new(shadow))),
            done: Mutex::new(None),
            quit: AtomicBool::new(false),
        });
        let run = Arc::clone(&exchange);
        let join = std::thread::Builder::new()
            .name("signal-sampler-reconstruct".into())
            .spawn(move || run.run())?;
        self.worker = Some(Worker {
            exchange,
            thread: join.thread().clone(),
        });
        self.policy = policy;
        Ok(self)
    }

    /// The document bank (offline tools, tests).
    #[must_use]
    pub fn document_bank(&self) -> &SamplerBank {
        &self.doc
    }

    /// The scheduler (its counters, for tests and reports).
    #[must_use]
    pub fn scheduler(&self) -> &RealtimeScheduler {
        &self.scheduler
    }

    /// One block. `published` is the schedule to play (adopted here, at
    /// the block boundary, when it changed); `events` are this block's
    /// MIDI at their frames, sorted; `out` is interleaved stereo, written.
    pub fn process(
        &mut self,
        published: Option<&Arc<Schedule>>,
        t: &BlockTransport,
        input: Input,
        events: &[(u32, LiveEvent)],
        out: &mut [f32],
    ) -> Played {
        let frames = out.len() / 2;
        self.return_spare();
        self.adopt_schedule(published, t);
        let want = t.playing && self.adopted.is_some();
        if !want {
            self.pending = None;
        }
        if want {
            self.take_edit(t);
        }
        let played = if want && self.deferring(t, frames) {
            out.fill(0.0);
            Played::Reconstructing
        } else if self.scheduler.process_block(&mut self.doc, t, out) {
            self.apply_fade(out);
            Played::Document
        } else {
            play_live(&mut self.doc, &self.id, events, out);
            Played::Live
        };
        if played != Played::Live && input == Input::Live {
            self.add_live(events, out);
        } else if let Some(live) = self.live.as_deref_mut() {
            // Its tails ring on.
            add_rendered(live, &[], &self.id, &mut self.live_scratch, out);
        }
        played
    }

    /// A newly published schedule: adopted at once, or — while the
    /// document plays on and its replay would be long — built on the
    /// worker for a frame ahead, the old schedule playing until then.
    fn adopt_schedule(&mut self, published: Option<&Arc<Schedule>>, t: &BlockTransport) {
        let is = |a: Option<&Arc<Schedule>>| match (a, published) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if is(self.adopted.as_ref()) {
            if self.incoming.take().is_some() {
                self.pending = None;
            }
            return;
        }
        if self.incoming.is_some() && is(self.incoming.as_ref()) {
            return; // being built
        }
        let playing_on = t.playing
            && self.pending.is_none()
            && self.scheduler.document_active()
            && self.scheduler.expected_frame() == Some(t.pos_frame);
        if let (true, Some(new), Some(_)) = (playing_on, published, &self.worker) {
            let pos = t.pos_frame.max(0) as u64;
            if pos - new.reconstruction_start(pos) > self.policy.inline_max_frames {
                self.incoming = Some(Arc::clone(new));
                self.request(Arc::clone(new), t.pos_frame, Why::Edit);
                return;
            }
        }
        self.incoming = None;
        self.adopted = published.cloned();
        self.scheduler.set_schedule(self.adopted.clone());
        self.pending = None;
    }

    /// The edit's bank, if it came in for this block: the new schedule
    /// takes over here. A jump of the transport meanwhile adopts it as a
    /// seek instead.
    fn take_edit(&mut self, t: &BlockTransport) {
        let Some(p) = self.pending.filter(|p| p.why == Why::Edit) else {
            return;
        };
        let Some(new) = self.incoming.clone() else {
            return;
        };
        if self.scheduler.expected_frame() != Some(t.pos_frame) {
            self.incoming = None;
            self.pending = None;
            self.adopted = Some(Arc::clone(&new));
            self.scheduler.set_schedule(Some(new));
            return;
        }
        if t.pos_frame < p.at {
            return;
        }
        if let Some((bank, sched, done)) = self.take_done(&new, p.at) {
            if t.pos_frame.saturating_sub(p.at) as u64 > self.policy.catch_up_max {
                self.spare = Some(bank);
                self.request(new, t.pos_frame, Why::Edit);
                return;
            }
            let mut bank = bank;
            std::mem::swap(&mut self.doc, &mut bank);
            self.scheduler.set_schedule(Some(Arc::clone(&sched)));
            if self
                .scheduler
                .adopt(&mut self.doc, &done, &sched, t.pos_frame)
            {
                self.adopted = Some(sched);
                self.incoming = None;
                self.pending = None;
            } else {
                // Not ours after all: back to the old schedule, seeking.
                std::mem::swap(&mut self.doc, &mut bank);
                self.scheduler.set_schedule(self.adopted.clone());
            }
            self.spare = Some(bank);
        }
    }

    /// The worker's bank if it was built for `sched` at `at` (any other
    /// goes back as a spare).
    fn take_done(&mut self, sched: &Arc<Schedule>, at: i64) -> Option<Done> {
        let worker = self.worker.as_ref()?;
        let done = worker.exchange.done.try_lock().ok()?.take()?;
        if done.2.at == at && Arc::ptr_eq(&done.1, sched) {
            Some(done)
        } else {
            self.spare = Some(done.0);
            None
        }
    }

    /// Whether the document is (still) waiting for a reconstruction: a new
    /// discontinuity is handed to the worker, a finished one swapped in.
    fn deferring(&mut self, t: &BlockTransport, frames: usize) -> bool {
        if self.worker.is_none() {
            return false;
        }
        if self.pending.is_some_and(|p| p.why == Why::Edit) {
            return false; // the old schedule plays on
        }
        let jumped = self.pending.is_some_and(|p| p.next != t.pos_frame);
        if self.pending.is_none() || jumped {
            if !self.scheduler.would_relocate(t)
                || self.scheduler.replay_frames(t.pos_frame) <= self.policy.inline_max_frames
            {
                self.pending = None;
                return false;
            }
            self.scheduler.hold(&mut self.doc);
            if let Some(sched) = self.adopted.clone() {
                self.request(sched, t.pos_frame, Why::Seek);
            }
        }
        if self.try_adopt(t.pos_frame) {
            return false;
        }
        if let Some(p) = self.pending.as_mut() {
            p.next = t.pos_frame + frames as i64;
        }
        true
    }

    /// Ask the worker for `sched`'s bank at `pos + lead`.
    fn request(&mut self, sched: Arc<Schedule>, pos: i64, why: Why) {
        let Some(worker) = &self.worker else {
            return;
        };
        let at = pos + i64::try_from(self.policy.lead_frames).unwrap_or(i64::MAX / 2);
        if let Ok(mut slot) = worker.exchange.request.try_lock() {
            *slot = Some((sched, at));
        }
        worker.thread.unpark();
        self.pending = Some(Pending { at, next: pos, why });
    }

    /// Swap in the worker's bank if it is the one this playhead needs.
    fn try_adopt(&mut self, now: i64) -> bool {
        let (Some(p), Some(sched)) = (self.pending, self.adopted.clone()) else {
            return false;
        };
        if now < p.at {
            return false; // built for a frame still ahead
        }
        let Some((bank, sched, done)) = self.take_done(&sched, p.at) else {
            return false;
        };
        if now.saturating_sub(p.at) as u64 > self.policy.catch_up_max {
            // Came back too late to catch up: build it further on.
            self.spare = Some(bank);
            self.request(sched, now, Why::Seek);
            return false;
        }
        let mut bank = bank;
        std::mem::swap(&mut self.doc, &mut bank);
        if !self.scheduler.adopt(&mut self.doc, &done, &sched, now) {
            std::mem::swap(&mut self.doc, &mut bank);
            self.spare = Some(bank);
            return false;
        }
        self.spare = Some(bank);
        self.pending = None;
        self.fade = (self.policy.fade_frames, self.policy.fade_frames);
        true
    }

    /// Hand a spare bank back to the worker.
    fn return_spare(&mut self) {
        let (Some(worker), Some(_)) = (&self.worker, &self.spare) else {
            return;
        };
        if let Ok(mut idle) = worker.exchange.idle.try_lock()
            && idle.is_none()
        {
            *idle = self.spare.take();
            worker.thread.unpark();
        }
    }

    fn apply_fade(&mut self, out: &mut [f32]) {
        let (left, len) = self.fade;
        if left == 0 || len == 0 {
            return;
        }
        let mut left = left;
        for frame in out.chunks_exact_mut(2) {
            if left == 0 {
                break;
            }
            let g = (len - left) as f32 / len as f32;
            frame[0] *= g;
            frame[1] *= g;
            left -= 1;
        }
        self.fade.0 = left;
    }

    /// Live input on the live bank, summed into `out`.
    fn add_live(&mut self, events: &[(u32, LiveEvent)], out: &mut [f32]) {
        if let Some(live) = self.live.as_deref_mut() {
            add_rendered(live, events, &self.id, &mut self.live_scratch, out);
        }
    }
}

/// `events` dispatched on `bank` at their frames, the gaps rendered into
/// `out` (cleared by the caller).
fn play_live(bank: &mut SamplerBank, id: &str, events: &[(u32, LiveEvent)], out: &mut [f32]) {
    let frames = out.len() / 2;
    let mut off = 0usize;
    for &(at, ev) in events {
        let at = (at as usize).min(frames);
        if at > off {
            bank.render(&mut out[off * 2..at * 2]);
            off = at;
        }
        match ev {
            LiveEvent::NoteOn { note, velocity } => bank.note_on(id, note, velocity.max(1)),
            LiveEvent::NoteOff { note } => bank.note_off(id, note),
            LiveEvent::Cc { cc, value } => bank.cc(id, cc, value),
        }
    }
    if frames > off {
        bank.render(&mut out[off * 2..frames * 2]);
    }
}

/// [`play_live`] into `scratch`, added to `out`.
fn add_rendered(
    bank: &mut SamplerBank,
    events: &[(u32, LiveEvent)],
    id: &str,
    scratch: &mut [f32],
    out: &mut [f32],
) {
    let Some(buf) = scratch.get_mut(..out.len()) else {
        return;
    };
    buf.fill(0.0);
    play_live(bank, id, events, buf);
    for (o, s) in out.iter_mut().zip(buf.iter()) {
        *o += *s;
    }
}
