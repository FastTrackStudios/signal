//! The web demo's audio: the session mixed on the page and played through
//! an AudioWorklet.
//!
//! The engine is the one the panels already drive (daw-standalone,
//! in-process, see [`crate::web_engine`]), so a fader or a mute is heard
//! the moment the engine takes it. What a browser adds is three things:
//!
//! - **Stems** stream from their Ogg Vorbis proxies ([`add_stem`]): each
//!   take's source is a `Streamed` one, decoded a few seconds at a time
//!   around the playhead by a feeder this loop runs.
//! - **A reference** ([`reference`]) — the song's mix but the guide, one
//!   stereo stream — plays instead of the stems while the page is in
//!   reference mode: it is mixed in at the playhead over what the engine
//!   renders (the guide, live), and the stems' feeders are held, not run,
//!   so their takes are silent. [`multitracks`] trades it for the stems.
//!   A reference comes in two: its preview (mono, a quarter of the size)
//!   plays wherever the reference proper has not arrived yet.
//!   The stems' meters still move: each is read from its take's waveform
//!   at the playhead ([`PeakMeters`]), through its fader and up its
//!   folders, the way the engine would meter it.
//! - **Output** is an AudioWorklet that plays blocks the page posts it —
//!   a small queue, so the page renders ahead of the device by a quarter of
//!   a second and the playhead the panels see is held back by what is
//!   queued ([`latency_seconds`]).
//! - **Unlocking**: a browser starts audio only inside a gesture, so the
//!   context is made on the page's first press or key ([`unlock`]); until
//!   then the transport runs on the engine's own clock and plays silent.
//!
//! One loop does the work: feed the stems within a time budget, then
//! render until the queue is full. It renders only while the transport
//! advances, and a jump of the playhead (a seek, a stop, a loop) empties
//! the queue so the device follows at once.
//!
//! **In the background.** A hidden page's timers are throttled to about
//! one a second, which starves a quarter-second queue — the song breaks
//! up as soon as the window goes behind another. So the loop is driven by
//! the WORKLET's own reports (the audio thread's, every few milliseconds,
//! and not throttled) as well as by a timer, and the queue is deepened
//! while the page is hidden. The painting stops, as it should: only the
//! audio keeps its cadence.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use daw_standalone::audio_engine::render::ProjectRenderer;
use daw_standalone::audio_engine::streamed::{self, Decode, StreamFeeder};
use daw_standalone::sync::Standalone;
use daw_standalone::transport_engine::{TransportBundle, TransportShared};
use fts_sample::ogg_stream::OggStream;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

/// The output rate. The context is asked for it (the browser resamples to
/// the device), so it matches the engine's transport clock, which counts
/// at 48 kHz from before the context exists.
const RATE: u32 = 48_000;
/// Frames per rendered block.
const BLOCK: usize = 1024;
/// How far ahead of the device to render while the page is in front:
/// enough to ride out a busy frame of painting, little enough that a
/// fader answers at once.
const AHEAD: u64 = RATE as u64 / 4;
/// And while it is behind another window, where nothing is being watched
/// and a throttled timer may not come back for a second.
const AHEAD_HIDDEN: u64 = RATE as u64 * 2;
/// Chunks of each stem kept decoded ahead of the playhead (~3 s at
/// 44.1 kHz; ~1.1 MB of a stereo stem): this loop decodes every few
/// milliseconds, so it needs far less in hand than the native butler —
/// and a band's worth of the native ~9 s was most of the page's memory.
/// It covers [`AHEAD_HIDDEN`]'s two seconds rendered ahead.
const DECODED_AHEAD: usize = 8;
/// How long a tick may spend decoding stems before it renders.
const DECODE_BUDGET_MS: u128 = 6;
/// And while a stem is missing the audio at the playhead (after a jump):
/// long enough that the band sounds again within a beat or two.
const CATCH_UP_BUDGET_MS: u128 = 24;

/// The worklet: a queue of interleaved stereo blocks, played in order,
/// silence when it runs dry. It reports how much it has played, tagged
/// with the epoch of the last flush, so the page knows what is queued.
const PROCESSOR: &str = r"
class FtsOut extends AudioWorkletProcessor {
  constructor() {
    super();
    this.q = []; this.off = 0; this.epoch = 0; this.played = 0; this.tick = 0;
    this.port.onmessage = (e) => {
      const m = e.data;
      if (typeof m === 'number') { this.q = []; this.off = 0; this.epoch = m; this.played = 0; return; }
      this.q.push(m);
    };
  }
  process(_inputs, outputs) {
    const out = outputs[0];
    const l = out[0], r = out[1] || out[0];
    const n = l.length;
    let i = 0;
    while (i < n && this.q.length) {
      const c = this.q[0];
      const frames = c.length >> 1;
      const take = Math.min(frames - this.off, n - i);
      for (let k = 0; k < take; k++) {
        const j = (this.off + k) << 1;
        l[i + k] = c[j];
        r[i + k] = c[j + 1];
      }
      i += take; this.off += take; this.played += take;
      if (this.off >= frames) { this.q.shift(); this.off = 0; }
    }
    for (; i < n; i++) { l[i] = 0; r[i] = 0; }
    if ((++this.tick & 3) === 0) this.port.postMessage([this.epoch, this.played]);
    return true;
  }
}
registerProcessor('fts-out', FtsOut);
";

/// The device side, once unlocked.
struct Out {
    context: web_sys::AudioContext,
    port: web_sys::MessagePort,
    /// Kept alive for as long as the port reports.
    _on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
}

/// What the player plays of the song on screen: its transport and its mix.
struct PlayerSong {
    project: String,
    bundle: Arc<TransportBundle>,
    shared: Arc<TransportShared>,
    renderer: ProjectRenderer,
}

impl PlayerSong {
    fn of(daw: &Standalone, project: &str) -> Self {
        let bundle = daw.transport_engine_for(project);
        let shared = Arc::clone(&bundle.shared);
        shared.set_sample_rate(RATE);
        Self {
            project: project.to_owned(),
            bundle,
            shared,
            renderer: ProjectRenderer::new(daw, project, RATE),
        }
    }
}

type Feeder = StreamFeeder<Box<dyn Decode + Send>>;

/// A song's reference, decoding: the reference proper, and its preview.
struct ReferenceFeeders {
    full: Feeder,
    preview: Option<Feeder>,
}

struct Player {
    daw: Standalone,
    /// The song on screen's transport and mix ([`switch`] replaces it).
    song: RefCell<PlayerSong>,
    /// Whether the device drives the transport — only while its context
    /// runs. A context that never starts (no output device, a browser
    /// holding it suspended) leaves the engine's own clock keeping time,
    /// so the song still moves, silently.
    clocked: Cell<bool>,
    /// Each song's streamed takes, by project: only the song on screen's
    /// are decoded.
    feeders: RefCell<std::collections::HashMap<String, Vec<Feeder>>>,
    /// Each song's reference, while it is heard by it: decoded and mixed in
    /// for the song on screen.
    references: RefCell<std::collections::HashMap<String, ReferenceFeeders>>,
    /// The stems of a song heard by its reference: attached (their
    /// waveforms draw) but not decoded, until [`multitracks`].
    held: RefCell<std::collections::HashMap<String, Vec<Feeder>>>,
    /// The meters of a song heard by its reference, from its waveforms.
    peak_meters: RefCell<std::collections::HashMap<String, PeakMeters>>,
    /// Which feeder a tick starts with — round robin, so one stem's
    /// catch-up does not starve the rest.
    first: Cell<usize>,
    out: RefCell<Option<Out>>,
    /// Set from the first unlock until the worklet is up, so a second
    /// press meanwhile makes no second context.
    starting: Cell<bool>,
    /// Frames posted, and played, since the last flush.
    sent: Cell<u64>,
    played: Rc<Cell<u64>>,
    epoch: Rc<Cell<u32>>,
    /// Where the next block renders from, while playing; `None` stopped.
    next: Cell<Option<u64>>,
    /// Whether the page is hidden — how deep the queue is kept.
    hidden: Cell<bool>,
}

thread_local! {
    static PLAYER: RefCell<Option<Rc<Player>>> = const { RefCell::new(None) };
}

fn player() -> Option<Rc<Player>> {
    PLAYER.with(|p| p.borrow().clone())
}

/// Start the audio loop for `project` on `daw`. Silent until [`unlock`].
/// Once it is going, another song is [`switch`]ed to instead.
pub fn install(daw: Standalone, project: &str) {
    if player().is_some() {
        switch(project);
        return;
    }
    set_meters(&daw, project);
    let player = Rc::new(Player {
        song: RefCell::new(PlayerSong::of(&daw, project)),
        daw,
        clocked: Cell::new(false),
        feeders: RefCell::default(),
        references: RefCell::default(),
        held: RefCell::default(),
        peak_meters: RefCell::default(),
        first: Cell::new(0),
        out: RefCell::new(None),
        starting: Cell::new(false),
        sent: Cell::new(0),
        played: Rc::new(Cell::new(0)),
        epoch: Rc::new(Cell::new(0)),
        next: Cell::new(None),
        hidden: Cell::new(false),
    });
    watch_visibility(&player);
    PLAYER.with(|p| *p.borrow_mut() = Some(Rc::clone(&player)));
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            player.tick();
            gloo_timers::future::TimeoutFuture::new(10).await;
        }
    });
}

/// The meters for `project`'s tracks.
fn set_meters(daw: &Standalone, project: &str) {
    let count =
        daw_proto::Tracks::count(daw, daw_proto::ProjectContext::Project(project.to_owned()))
            as usize;
    daw.set_meters(daw_standalone::metering::Meters::new(count));
}

/// Play `project` now — a song of the set picked. The one leaving stops
/// where it is (its own transport keeps its place), and the device's queue
/// empties so what plays next is the new song's.
pub fn switch(project: &str) {
    let Some(player) = player() else { return };
    if player.song.borrow().project == project {
        return;
    }
    set_meters(&player.daw, project);
    let next = PlayerSong::of(&player.daw, project);
    let leaving = player.song.replace(next);
    leaving
        .shared
        .set_play_state(daw_standalone::transport_engine::engine::PlayStateRepr::Stopped);
    if player.clocked.get() {
        leaving.bundle.enable_soft_clock();
        player.song.borrow().bundle.disable_soft_clock();
    }
    player.next.set(None);
    player.flush();
}

/// A stem streamed from its proxy's bytes, for the take `take` of
/// `project`.
///
/// # Errors
///
/// The bytes are not an Ogg Vorbis stream.
pub fn add_stem(project: &str, take: &str, bytes: Arc<[u8]>) -> Result<(), String> {
    use daw_standalone::audio_engine::source::AudioSource;
    use daw_standalone::audio_engine::streamed::Streamed;
    let Some(player) = player() else {
        return Err("no audio player installed".into());
    };
    let stream = OggStream::open(bytes).map_err(|e| e.to_string())?;
    let streamed = Streamed::new(stream.channels(), stream.sample_rate(), stream.frames());
    daw_standalone::audio_engine::materialize::attach_source(
        &player.daw,
        project,
        take,
        AudioSource::Streamed(streamed.clone()),
    );
    adopt(project, StreamFeeder::new(streamed, stream).boxed());
    Ok(())
}

/// Feed a streamed take from this page's loop — a whole proxy
/// ([`add_stem`]) or one arriving by range from a share link
/// (`song_stream::StreamedSong::attach`). A song heard by its
/// [`reference`] holds it instead, until [`multitracks`]. Before the
/// player is installed there is nothing to feed it from; it is dropped.
pub fn adopt(project: &str, feeder: Feeder) {
    if let Some(player) = player() {
        let feeder = feeder.with_ahead(DECODED_AHEAD);
        let into = if player.references.borrow().contains_key(project) {
            &player.held
        } else {
            &player.feeders
        };
        into.borrow_mut()
            .entry(project.to_owned())
            .or_default()
            .push(feeder);
    }
}

/// Hear `project` by its reference (`full`, and its `preview` where that
/// has not arrived) — before its takes are attached, so their feeders are
/// held ([`adopt`]).
pub fn reference(project: &str, full: Feeder, preview: Option<Feeder>) {
    if let Some(player) = player() {
        player.references.borrow_mut().insert(
            project.to_owned(),
            ReferenceFeeders {
                full: full.with_ahead(DECODED_AHEAD),
                preview: preview.map(|p| p.with_ahead(DECODED_AHEAD)),
            },
        );
    }
}

/// Meter `project`'s stems from their waveforms while it is heard by its
/// reference — `takes` are its attached stems.
pub fn peak_meters(project: &str, takes: Vec<MeterTake>) {
    if let Some(player) = player() {
        let meters = PeakMeters::new(&player.daw, project, takes);
        player
            .peak_meters
            .borrow_mut()
            .insert(project.to_owned(), meters);
    }
}

/// Hear `project` by its stems from now on: the held feeders run, the
/// reference goes. What is queued already (a quarter of a second of the
/// reference) plays while the stems decode their first chunk.
pub fn multitracks(project: &str) {
    if let Some(player) = player() {
        player.references.borrow_mut().remove(project);
        player.peak_meters.borrow_mut().remove(project);
        let held = player.held.borrow_mut().remove(project).unwrap_or_default();
        player
            .feeders
            .borrow_mut()
            .entry(project.to_owned())
            .or_default()
            .extend(held);
    }
}

/// How far the device is behind the engine's playhead, in seconds — what
/// the transport reading subtracts, so the playhead drawn is the one heard.
#[must_use]
pub fn latency_seconds() -> f64 {
    player().map_or(0.0, |p| {
        if p.out.borrow().is_none() || p.next.get().is_none() {
            return 0.0;
        }
        p.queued() as f64 / f64::from(RATE)
    })
}

/// Make and start the audio context. Call from inside a user gesture (a
/// press, a key) — the only place a browser lets audio start. Idempotent.
pub fn unlock() {
    let Some(player) = player() else { return };
    if let Some(out) = player.out.borrow().as_ref() {
        // Made already; a context the browser suspended resumes here.
        let _ = out.context.resume();
        return;
    }
    if player.starting.replace(true) {
        return;
    }
    let options = web_sys::AudioContextOptions::new();
    options.set_sample_rate(RATE as f32);
    let context = match web_sys::AudioContext::new_with_context_options(&options) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = ?e, "audio: no AudioContext");
            return;
        }
    };
    let _ = context.resume();
    // The module: a blob URL over the processor's source, so the page
    // serves no second file.
    let parts = js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(PROCESSOR));
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type("text/javascript");
    let url = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag)
        .and_then(|blob| web_sys::Url::create_object_url_with_blob(&blob));
    let (Ok(worklet), Ok(url)) = (context.audio_worklet(), url) else {
        tracing::warn!("audio: no AudioWorklet");
        return;
    };
    let Ok(loading) = worklet.add_module(&url) else {
        tracing::warn!("audio: the worklet module did not load");
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = wasm_bindgen_futures::JsFuture::from(loading).await {
            tracing::warn!(error = ?e, "audio: the worklet module did not load");
            return;
        }
        let options = web_sys::AudioWorkletNodeOptions::new();
        options.set_number_of_outputs(1);
        options.set_output_channel_count(&js_sys::Array::of1(&2.into()));
        let node = match web_sys::AudioWorkletNode::new_with_options(&context, "fts-out", &options)
        {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = ?e, "audio: the worklet node did not start");
                return;
            }
        };
        let _ = node.connect_with_audio_node(&context.destination());
        let Ok(port) = node.port() else { return };
        let (played, epoch) = (Rc::clone(&player.played), Rc::clone(&player.epoch));
        let driven = Rc::clone(&player);
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
            move |event: web_sys::MessageEvent| {
                let report = js_sys::Array::from(&event.data());
                let (Some(tag), Some(count)) = (report.get(0).as_f64(), report.get(1).as_f64())
                else {
                    return;
                };
                if tag as u32 == epoch.get() {
                    played.set(count as u64);
                }
                // The audio thread's own cadence, which a hidden page's
                // throttled timers do not have: this is what keeps the
                // queue fed when the window is behind another.
                driven.tick();
            },
        );
        port.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        tracing::info!(rate = context.sample_rate(), "audio: started");
        *player.out.borrow_mut() = Some(Out {
            context,
            port,
            _on_message: on_message,
        });
        player.flush();
    });
}

impl Player {
    fn queued(&self) -> u64 {
        self.sent.get().saturating_sub(self.played.get())
    }

    /// How much audio to keep queued ahead of the device.
    fn ahead(&self) -> u64 {
        if self.hidden.get() {
            AHEAD_HIDDEN
        } else {
            AHEAD
        }
    }

    /// Empty the device's queue: a jump, a stop.
    fn flush(&self) {
        let epoch = self.epoch.get().wrapping_add(1);
        self.epoch.set(epoch);
        self.sent.set(0);
        self.played.set(0);
        if let Some(out) = self.out.borrow().as_ref() {
            let _ = out.port.post_message(&f64::from(epoch).into());
        }
    }

    fn tick(&self) {
        self.feed();
        self.render();
    }

    /// Decode stems (or the reference) toward the playhead, for up to the
    /// budget.
    fn feed(&self) {
        let project = self.song.borrow().project.clone();
        let mut stems = self.feeders.borrow_mut();
        let mut references = self.references.borrow_mut();
        let reference = references.get_mut(&project);
        // With a preview, the reference proper is not what is heard until
        // it has arrived: it decodes, but is not waited on (last, below).
        let waiting_on_full = reference.as_ref().is_some_and(|r| r.preview.is_some());
        let mut feeders: Vec<&mut Feeder> = stems
            .get_mut(&project)
            .into_iter()
            .flatten()
            .chain(reference.into_iter().flat_map(|r| {
                r.preview
                    .as_mut()
                    .into_iter()
                    .chain(std::iter::once(&mut r.full))
            }))
            .collect();
        let count = feeders.len();
        if count == 0 {
            return;
        }
        let started = web_time::Instant::now();
        let first = self.first.get() % count;
        self.first.set(first + 1);
        // After a jump every stem is missing the audio at the playhead, and
        // until each has it the song is partly silent: decode harder until
        // they all do. The device is fed from the worklet's own queue, so a
        // longer tick here costs only the page a frame or two.
        let heard = count - usize::from(waiting_on_full);
        let behind = feeders[..heard].iter().any(|f| {
            let source = f.source();
            let wanted = source.wanted();
            let here = usize::try_from(wanted).unwrap_or(usize::MAX) / streamed::CHUNK;
            wanted < source.frames() && !source.resident(here)
        });
        let budget = if behind {
            CATCH_UP_BUDGET_MS
        } else {
            DECODE_BUDGET_MS
        };
        let mut busy = true;
        while busy && started.elapsed().as_millis() < budget {
            busy = false;
            for i in 0..count {
                busy |= feeders[(first + i) % count].pump(2048);
                if started.elapsed().as_millis() >= budget {
                    break;
                }
            }
        }
    }

    /// Render ahead of the device while the transport advances.
    fn render(&self) {
        let Some((out, running)) = self.out.borrow().as_ref().map(|o| {
            (
                o.port.clone(),
                o.context.state() == web_sys::AudioContextState::Running,
            )
        }) else {
            return;
        };
        // The device drives the transport only while it runs; otherwise
        // the engine's own clock does, and nothing is queued.
        if !running {
            if self.clocked.replace(false) {
                self.song.borrow().bundle.enable_soft_clock();
                self.next.set(None);
                self.flush();
            }
            return;
        }
        let song = self.song.borrow();
        if !self.clocked.replace(true) {
            song.bundle.disable_soft_clock();
            tracing::info!("audio: the device drives the transport");
        }
        if !song.shared.play_state().is_advancing() {
            if self.next.take().is_some() {
                self.flush();
                // Nothing renders while stopped, so nothing would write the
                // meters down: silence them, as a stopped engine's are.
                let meters = self.daw.meters();
                for i in 0..meters.len() {
                    if let Some(cell) = meters.cell(i) {
                        cell.write(0.0, 0.0, 0.0);
                    }
                }
            }
            return;
        }
        let playhead = song.shared.playhead_samples().0.max(0) as u64;
        // Anything but the block after the last is a jump: the queue holds
        // audio from somewhere else.
        if self.next.get() != Some(playhead) {
            self.flush();
        }
        let references = self.references.borrow();
        let reference = references.get(&song.project).map(|r| {
            (
                r.full.source(),
                r.preview.as_ref().map(StreamFeeder::source),
            )
        });
        let mut peak_meters = self.peak_meters.borrow_mut();
        let mut peak_meters = peak_meters
            .get_mut(&song.project)
            .filter(|_| reference.is_some());
        let mut at = playhead;
        while self.queued() < self.ahead() {
            let mut block = song.renderer.render_block(at, BLOCK);
            if let Some((full, preview)) = reference {
                mix_reference(full, preview, at, &mut block.samples);
            }
            if let Some(meters) = peak_meters.as_deref_mut() {
                meters.write(&self.daw, &song.project, at);
            }
            let samples = js_sys::Float32Array::from(&block.samples[..]);
            let transfer = js_sys::Array::of1(&samples.buffer());
            if out
                .post_message_with_transferable(&samples, &transfer)
                .is_err()
            {
                break;
            }
            self.sent.set(self.sent.get() + BLOCK as u64);
            // `advance` answers where the block started; the playhead is
            // where the next one does (a loop may have wrapped it).
            song.shared.advance(BLOCK as u32);
            at = song.shared.playhead_samples().0.max(0) as u64;
        }
        self.next.set(Some(at));
    }
}

/// A stem of a song heard by its reference, for its meter: its track, its
/// span, and the stream its waveform is on.
pub struct MeterTake {
    pub track: String,
    pub start: f64,
    pub end: f64,
    pub source_offset: f64,
    pub playrate: f64,
    pub source: streamed::Streamed,
}

/// A track as its meter needs it.
struct MeterTrack {
    /// Its meter cell (the project's track index).
    cell: usize,
    parent: Option<usize>,
    gain: f32,
    /// Played live (the guide): the engine meters it.
    live: bool,
}

/// The meters of a song heard by its reference: each stem read from its
/// take's waveform where the playhead is, times its fader, the loudest
/// child lifting each folder — written over the engine's (silent) reading,
/// block by block. The tracks the reference leaves out play live and keep
/// the engine's own.
pub struct PeakMeters {
    takes: Vec<(usize, MeterTake)>,
    tracks: Vec<MeterTrack>,
    /// Blocks until the tracks are read again (a fader moved, by someone
    /// in the session).
    stale_in: u32,
    guids: Vec<String>,
}

/// Blocks between readings of the tracks' faders (~ a quarter second).
const METER_TRACKS_EVERY: u32 = 12;

impl PeakMeters {
    fn new(daw: &Standalone, project: &str, takes: Vec<MeterTake>) -> Self {
        let mut meters = Self {
            takes: Vec::new(),
            tracks: Vec::new(),
            stale_in: 0,
            guids: Vec::new(),
        };
        meters.read_tracks(daw, project);
        meters.takes = takes
            .into_iter()
            .filter_map(|t| Some((meters.guids.iter().position(|g| *g == t.track)?, t)))
            .collect();
        meters
    }

    /// The tracks' tree, faders and mutes, now.
    fn read_tracks(&mut self, daw: &Standalone, project: &str) {
        let all =
            daw_proto::Tracks::all(daw, daw_proto::ProjectContext::Project(project.to_owned()));
        let live = crate::reference::left_out(daw, project);
        let at = |guid: &str| all.iter().position(|t| t.guid == guid);
        self.tracks = all
            .iter()
            .map(|t| MeterTrack {
                cell: t.index as usize,
                parent: t.parent_guid.as_deref().and_then(at),
                gain: if t.muted { 0.0 } else { t.volume as f32 },
                live: live.contains(&t.guid),
            })
            .collect();
        // Takes keep pointing at the right track: the order is the same
        // unless tracks were added, and then the guids say.
        let guids: Vec<String> = all.into_iter().map(|t| t.guid).collect();
        if guids != self.guids {
            let old = std::mem::replace(&mut self.guids, guids);
            for (track, take) in &mut self.takes {
                *track = old
                    .get(*track)
                    .and_then(|g| self.guids.iter().position(|n| n == g))
                    .unwrap_or(usize::MAX);
                let _ = take;
            }
        }
    }

    /// Write the meters for the block at playhead frame `at`.
    fn write(&mut self, daw: &Standalone, project: &str, at: u64) {
        if self.stale_in == 0 {
            self.read_tracks(daw, project);
            self.stale_in = METER_TRACKS_EVERY;
        }
        self.stale_in -= 1;
        let t = at as f64 / f64::from(RATE);
        let span = BLOCK as f64 / f64::from(RATE);
        let mut level = vec![(0.0f32, 0.0f32); self.tracks.len()];
        for (track, take) in &self.takes {
            let Some(slot) = level.get_mut(*track) else {
                continue;
            };
            if t + span <= take.start || t >= take.end {
                continue;
            }
            let (l, r) = take.peak_at((t - take.start) * take.playrate + take.source_offset, span);
            slot.0 = slot.0.max(l);
            slot.1 = slot.1.max(r);
        }
        // Children come after their folders, so walking back each track is
        // whole (its own takes, its children's) before its folder takes it.
        for i in (0..self.tracks.len()).rev() {
            let track = &self.tracks[i];
            let (l, r) = (level[i].0 * track.gain, level[i].1 * track.gain);
            level[i] = (l, r);
            if let Some(parent) = track.parent {
                level[parent].0 = level[parent].0.max(l);
                level[parent].1 = level[parent].1.max(r);
            }
        }
        let cells = daw.meters();
        for (track, (l, r)) in self.tracks.iter().zip(level) {
            if track.live {
                continue;
            }
            if let Some(cell) = cells.cell(track.cell) {
                cell.write(l, r, daw_standalone::metering::HOLD_DECAY);
            }
        }
    }
}

impl MeterTake {
    /// The loudest sample of its waveform over `span` seconds from `seconds`
    /// into its file, per channel — silence before its waveform arrives.
    fn peak_at(&self, seconds: f64, span: f64) -> (f32, f32) {
        let Some(peaks) = self.source.peaks() else {
            return (0.0, 0.0);
        };
        let rate = f64::from(peaks.samplerate.max(1));
        let level = peaks.level_for(span * rate);
        let spp = f64::from(level.samples_per_peak.max(1));
        let from = (seconds.max(0.0) * rate / spp) as usize;
        let to = (((seconds + span) * rate / spp).ceil() as usize).max(from + 1);
        let nch = peaks.channels.max(1);
        let (mut l, mut r) = (0.0f32, 0.0f32);
        for peak in from..to.min(level.count) {
            let (max, min) = level.pair(nch, 0, peak);
            l = l.max(max.abs()).max(min.abs());
            let (max, min) = level.pair(nch, (nch > 1).into(), peak);
            r = r.max(max.abs()).max(min.abs());
        }
        (l, r)
    }
}

/// Mix a song's reference in from the playhead frame `at` (at [`RATE`]),
/// into interleaved stereo — [`crate::reference::mix`], the one mix the
/// app's device plays too.
fn mix_reference(
    full: &streamed::Streamed,
    preview: Option<&streamed::Streamed>,
    at: u64,
    samples: &mut [f32],
) {
    crate::reference::mix(
        full,
        preview,
        at as f64 / f64::from(RATE),
        f64::from(RATE),
        1.0,
        samples,
        2,
    );
}

/// Unlock audio on the page's first press or key — a gesture is the only
/// place a browser lets a context start.
pub fn unlock_on_first_gesture() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let handler = Closure::<dyn FnMut(web_sys::Event)>::new(|_event: web_sys::Event| unlock());
    for kind in ["pointerdown", "keydown"] {
        let _ = window.add_event_listener_with_callback_and_bool(
            kind,
            handler.as_ref().unchecked_ref(),
            true,
        );
    }
    // The page's lifetime: `unlock` is idempotent, so the listener stays.
    handler.forget();
}

/// Follow the page between foreground and background, so the queue is
/// deep enough for whichever it is in.
///
/// Two ways of being away, because they are not the same event: the page
/// HIDDEN (another tab, a minimised window), and the window merely behind
/// another — which on a Mac is how a browser usually sits while somebody
/// plays along to it, and which fires `blur`, not `visibilitychange`.
/// Either way nothing is being watched, so the queue goes deep.
fn watch_visibility(player: &Rc<Player>) {
    let (Some(window), Some(document)) = (
        web_sys::window(),
        web_sys::window().and_then(|w| w.document()),
    ) else {
        return;
    };
    player.hidden.set(document.hidden());
    let away = {
        let player = Rc::clone(player);
        move |away: bool| {
            player.hidden.set(away);
            // Coming back, the deep queue is left to drain: it is audio
            // already rendered from where the transport is, and throwing
            // it away would jump the song forward by everything queued.
            // Nothing renders again until it is down to the shallow
            // target, which takes a second or two, and edits are heard
            // from there.
            player.tick();
        }
    };
    let on_visibility = {
        let (away, document) = (away.clone(), document.clone());
        Closure::<dyn FnMut()>::new(move || away(document.hidden()))
    };
    let _ = document.add_event_listener_with_callback(
        "visibilitychange",
        on_visibility.as_ref().unchecked_ref(),
    );
    for (event, gone) in [("blur", true), ("focus", false)] {
        let away = away.clone();
        let handler = Closure::<dyn FnMut()>::new(move || away(gone));
        let _ = window.add_event_listener_with_callback(event, handler.as_ref().unchecked_ref());
        // The page's lifetime.
        handler.forget();
    }
    on_visibility.forget();
}
