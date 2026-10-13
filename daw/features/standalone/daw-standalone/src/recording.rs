//! Recording: every armed track's input captured to a WAV while the
//! transport records, and each pass placed on its track as a new take on a
//! fixed lane when it stops — REAPER 7's model.
//!
//! ## The pass
//!
//! [`begin`] runs when the transport goes into `Recording`. It looks at the
//! project once: every track that is armed and records a hardware channel
//! (`RecordInput::Audio { channel }`) gets a file in the project's media
//! folder and a lock-free ring. A writer thread drains the rings into the
//! files. Nothing about the pass is decided on the audio thread.
//!
//! [`capture_block`] runs on the audio thread, once per rendered block,
//! from the renderer's stage 0 (right after the live input is drained). It
//! never waits in realtime mode: it only *tries* the pass lock, and pushes
//! the block's input samples into the rings without allocating. A ring that
//! is full loses the samples that did not fit, and they are counted. In an
//! offline render (a test, a bounce — no realtime callback to miss) it
//! waits for the writer instead, so an offline pass is sample-exact.
//!
//! The pass takes its timeline position from the blocks it is given, not
//! from the transport's clock: the first captured block's start is where
//! the take goes, and the take is as long as the frames captured. A block
//! that does not follow on from the last one (a locate, a loop wrap) ends
//! the capture there; loop recording is not modelled.
//!
//! [`finish`] runs when the transport leaves `Recording`. It takes the
//! pass, lets the writer drain what is left, writes the WAV headers, and
//! puts every file on its track as a new item on a fixed lane, as one undo
//! step.
//!
//! ## Where a take goes
//!
//! [`TakeLanePolicy::LowestFree`] (the default, the spec's choice): the
//! lowest-numbered take lane that holds nothing over the recorded range,
//! else a new lane named with the next number. So the verse's takes 1–3
//! sit beside the intro's takes 1–3, on the same lanes.
//! [`TakeLanePolicy::AlwaysNew`] is REAPER's other setting: every pass on a
//! lane of its own. Switch with [`Standalone::set_take_lane_policy`].
//!
//! A *take lane* is any lane that is not a comp lane (the comping state's
//! lanes, or a lane named `C1`, `C2`, … or `COMP`) and not the drum
//! editor's `EDIT` lane. The first pass on a track without lanes turns them
//! on and names lane 0 `1`; anything already on the track goes on that
//! lane with it.
//!
//! What plays afterwards: the active comp lane, when there is one and it
//! holds something — it is left alone. Otherwise only the new take's lane.
//!
//! ## Testing without hardware
//!
//! [`Standalone::set_test_input`] makes an input channel play a buffer in
//! place of the device, from the first frame of every pass. A test arms a
//! track, sets the buffer, starts recording, renders the span through a
//! `ProjectRenderer`, and stops: the real capture, the real writer and the
//! real [`finish`].

use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use daw_proto::track::RecordInput;
use daw_proto::{
    DawError, DawResult, ItemRef, Items, ProjectContext, Projects, TakeRef, Takes, Track, TrackRef,
    Tracks,
    primitives::{Duration, PositionInSeconds},
};

use crate::sync::Standalone;
use crate::transport_engine::{PlayStateRepr, TransportShared};

/// Seconds of audio a track's ring holds before the writer must have
/// drained it.
const RING_SECONDS: usize = 4;

/// Bytes of the WAV header written in front of every take.
const HEADER_BYTES: u64 = 44;

/// Two spans closer than this are touching, not overlapping (seconds).
const EPSILON: f64 = 1e-6;

/// How long an offline capture waits for the writer before it gives up
/// on a block and counts it lost.
const OFFLINE_PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Which lane a new take goes on. See the module docs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TakeLanePolicy {
    /// The lowest-numbered take lane empty over the recorded range, else a
    /// new one.
    #[default]
    LowestFree,
    /// A new lane for every pass (the first pass on a track whose take
    /// lanes are all empty still uses the lowest of them).
    AlwaysNew,
}

impl TakeLanePolicy {
    fn to_u8(self) -> u8 {
        match self {
            Self::LowestFree => 0,
            Self::AlwaysNew => 1,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::AlwaysNew,
            _ => Self::LowestFree,
        }
    }
}

/// One take a recording pass placed.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordedTake {
    pub track_guid: String,
    pub item_guid: String,
    pub take_guid: String,
    /// The fixed lane it is on (0-based).
    pub lane: u32,
    /// The lane's name (`1`, `2`, …).
    pub lane_name: String,
    /// The WAV it plays.
    pub path: PathBuf,
    /// Where it starts, project seconds.
    pub position: f64,
    /// How long it is, seconds.
    pub length: f64,
    /// Samples the audio thread could not hand over in time (realtime
    /// only; an offline pass never loses any).
    pub dropped_frames: u64,
}

/// The recorder's state on a [`Standalone`]: the pass in progress, the test
/// inputs, where each project records to, and the lane policy.
#[derive(Default)]
pub(crate) struct Recorder {
    /// The pass being recorded. The audio thread only ever `try_lock`s it
    /// (realtime); the control thread holds it only to put a pass in or
    /// take it out.
    pass: Mutex<Option<ActivePass>>,
    /// Input channels playing a buffer instead of the device.
    test_inputs: Mutex<HashMap<u32, Arc<[f32]>>>,
    /// Record folders set by hand, by project guid.
    record_dirs: Mutex<HashMap<String, PathBuf>>,
    /// The last pass each project finished.
    finished: Mutex<HashMap<String, Vec<RecordedTake>>>,
    policy: AtomicU8,
}

/// A recording in progress.
struct ActivePass {
    project_guid: String,
    transport: Arc<TransportShared>,
    tracks: Vec<PassTrack>,
    /// The test inputs as they were when the pass began.
    test_inputs: HashMap<u32, Arc<[f32]>>,
    /// The renderer feeding the pass (the first to render a block while it
    /// records) — a bounce rendered meanwhile is not the input.
    renderer: Option<usize>,
    sample_rate: Option<u32>,
    /// Timeline sample of the first captured frame.
    start: Option<f64>,
    /// Where the next block must start to follow on.
    next: f64,
    /// Frames captured so far (also the test inputs' read position).
    frames: u64,
    dropped: u64,
    /// A block did not follow on: nothing more is captured.
    closed: bool,
    writer: std::thread::JoinHandle<Vec<TrackFile>>,
    writer_thread: std::thread::Thread,
}

/// One armed track in a pass: the audio thread's half.
struct PassTrack {
    guid: String,
    channel: u32,
    ring: rtrb::Producer<f32>,
    path: PathBuf,
}

/// One armed track in a pass: the writer's half.
struct TrackFile {
    ring: rtrb::Consumer<f32>,
    file: std::io::BufWriter<std::fs::File>,
    frames: u64,
    error: Option<std::io::Error>,
}

/// What the renderer hands the pass for one block.
pub(crate) struct Block<'a> {
    pub project_guid: &'a str,
    /// Identifies the renderer: an address of its that does not move.
    pub renderer: usize,
    pub sample_rate: u32,
    /// The block's first frame on the timeline (output-rate samples).
    pub start_samples: f64,
    pub frames: usize,
    /// Timeline samples per output frame.
    pub rate: f64,
    /// No realtime callback behind this render: waiting is allowed.
    pub offline: bool,
    /// The device input drained this block: interleaved samples and the
    /// channel count. `None` when no input stream is open.
    pub live: Option<(&'a [f32], usize)>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ────────────────────────────────────────────────────────────────────
// The public surface on Standalone
// ────────────────────────────────────────────────────────────────────

impl Standalone {
    /// Make input `channel` play `samples` (mono, at the render's sample
    /// rate) in place of the device, from the first frame of every
    /// recording pass; silence after its end. Replaces any buffer the
    /// channel had. For tests: a pass then records exactly these samples.
    pub fn set_test_input(&self, channel: u32, samples: Vec<f32>) {
        lock(&self.recorder.test_inputs).insert(channel, samples.into());
    }

    /// [`set_test_input`](Self::set_test_input) from a file: its first
    /// channel, as decoded (not resampled — the file's rate should be the
    /// render's). Returns the file's sample rate.
    ///
    /// # Errors
    ///
    /// The file could not be read or decoded.
    pub fn set_test_input_wav(&self, channel: u32, path: &Path) -> Result<u32, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let decoded = crate::audio_engine::decoder::decode_audio_with_extension(&bytes, &ext)
            .ok_or_else(|| format!("{}: not audio this engine decodes", path.display()))?;
        let channels = usize::from(decoded.channels.max(1));
        let mono: Vec<f32> = decoded.samples.iter().step_by(channels).copied().collect();
        self.set_test_input(channel, mono);
        Ok(decoded.sample_rate)
    }

    /// Stop every input channel playing a test buffer.
    pub fn clear_test_inputs(&self) {
        lock(&self.recorder.test_inputs).clear();
    }

    /// Record `project_guid`'s takes into `dir` (`None`: back to the
    /// default, [`record_dir`](Self::record_dir)).
    pub fn set_record_dir(&self, project_guid: &str, dir: Option<PathBuf>) {
        let mut dirs = lock(&self.recorder.record_dirs);
        match dir {
            Some(d) => {
                dirs.insert(project_guid.to_owned(), d);
            }
            None => {
                dirs.remove(project_guid);
            }
        }
    }

    /// Where `project_guid` records to: the folder set with
    /// [`set_record_dir`](Self::set_record_dir), else `Media/` beside the
    /// project file (where a `.session` save resolves `Media/…` paths),
    /// else — a project never saved — a folder of its own under the
    /// system's temp directory.
    #[must_use]
    pub fn record_dir(&self, project_guid: &str) -> PathBuf {
        if let Some(dir) = lock(&self.recorder.record_dirs).get(project_guid) {
            return dir.clone();
        }
        let beside = self
            .read_project(project_guid, |p| {
                let path = Path::new(&p.info.path);
                (!p.info.path.is_empty())
                    .then(|| path.parent().map(|d| d.join("Media")))
                    .flatten()
            })
            .flatten();
        beside.unwrap_or_else(|| {
            std::env::temp_dir()
                .join("fts-recordings")
                .join(file_safe(project_guid))
        })
    }

    /// Which lane new takes go on (see [`TakeLanePolicy`]).
    pub fn set_take_lane_policy(&self, policy: TakeLanePolicy) {
        self.recorder
            .policy
            .store(policy.to_u8(), Ordering::Relaxed);
    }

    #[must_use]
    pub fn take_lane_policy(&self) -> TakeLanePolicy {
        TakeLanePolicy::from_u8(self.recorder.policy.load(Ordering::Relaxed))
    }

    /// Whether a pass is recording into `project_guid` right now.
    #[must_use]
    pub fn is_capturing(&self, project_guid: &str) -> bool {
        lock(&self.recorder.pass)
            .as_ref()
            .is_some_and(|p| p.project_guid == project_guid)
    }

    /// The takes the last finished pass on `project_guid` placed.
    #[must_use]
    pub fn last_recording(&self, project_guid: &str) -> Vec<RecordedTake> {
        lock(&self.recorder.finished)
            .get(project_guid)
            .cloned()
            .unwrap_or_default()
    }

    /// End the pass recording into `project_guid`: its files completed and
    /// placed on their tracks as new takes. The transport calls this when
    /// it leaves `Recording`; a caller that stops the transport some other
    /// way can call it itself. Nothing recording: nothing placed.
    ///
    /// # Errors
    ///
    /// A file could not be completed, or the engine refused a placement
    /// (what was placed before it stays, inside the one undo step).
    pub fn finish_recording(&self, project_guid: &str) -> DawResult<Vec<RecordedTake>> {
        finish(self, project_guid)
    }
}

// ────────────────────────────────────────────────────────────────────
// Begin
// ────────────────────────────────────────────────────────────────────

/// Start a pass on `project_guid`: a file and a ring for every armed track
/// recording a hardware channel, and the writer. Nothing armed: no pass
/// (the transport still records, as REAPER's does).
///
/// # Errors
///
/// Another project is recording, or a file could not be made.
pub(crate) fn begin(
    daw: &Standalone,
    project_guid: &str,
    transport: Arc<TransportShared>,
) -> DawResult<()> {
    let mut slot = lock(&daw.recorder.pass);
    if let Some(p) = slot.as_ref() {
        if p.project_guid == project_guid {
            return Ok(());
        }
        return Err(DawError::operation_failed(format!(
            "already recording into project {}",
            p.project_guid
        )));
    }
    let armed: Vec<(String, String, u32)> = daw
        .read_project(project_guid, |p| {
            p.tracks
                .iter()
                .filter(|t| t.armed)
                .filter_map(|t| {
                    let input = p
                        .track_ext
                        .get(&t.guid)
                        .map_or(t.record_input, |e| e.record_input);
                    match input {
                        RecordInput::Audio { channel } => {
                            Some((t.guid.clone(), t.name.clone(), channel))
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .ok_or_else(|| DawError::not_found("Project", project_guid))?;
    if armed.is_empty() {
        return Ok(());
    }
    let dir = daw.record_dir(project_guid);
    std::fs::create_dir_all(&dir)
        .map_err(|e| DawError::operation_failed(format!("{}: {e}", dir.display())))?;
    let capacity = (transport.sample_rate().max(48_000) as usize) * RING_SECONDS;
    let mut tracks = Vec::with_capacity(armed.len());
    let mut files = Vec::with_capacity(armed.len());
    let mut taken: Vec<PathBuf> = Vec::new();
    for (guid, name, channel) in armed {
        let path = take_path(&dir, &name, &taken);
        let mut file = std::fs::File::create(&path)
            .map_err(|e| DawError::operation_failed(format!("{}: {e}", path.display())))?;
        // Room for the header, written for real once the length is known.
        file.write_all(&wav_header(48_000, 0))
            .map_err(|e| DawError::operation_failed(format!("{}: {e}", path.display())))?;
        let (ring, drain) = rtrb::RingBuffer::<f32>::new(capacity);
        taken.push(path.clone());
        tracks.push(PassTrack {
            guid,
            channel,
            ring,
            path,
        });
        files.push(TrackFile {
            ring: drain,
            file: std::io::BufWriter::with_capacity(1 << 16, file),
            frames: 0,
            error: None,
        });
    }
    let writer = std::thread::Builder::new()
        .name("record-writer".into())
        .spawn(move || write_until_abandoned(files))
        .map_err(|e| DawError::operation_failed(format!("record writer: {e}")))?;
    let writer_thread = writer.thread().clone();
    let test_inputs = lock(&daw.recorder.test_inputs).clone();
    tracing::info!(
        record.project = project_guid,
        record.tracks = tracks.len(),
        record.dir = %dir.display(),
        "recording pass started"
    );
    *slot = Some(ActivePass {
        project_guid: project_guid.to_owned(),
        transport,
        tracks,
        test_inputs,
        renderer: None,
        sample_rate: None,
        start: None,
        next: 0.0,
        frames: 0,
        dropped: 0,
        closed: false,
        writer,
        writer_thread,
    });
    Ok(())
}

/// The writer: drain every ring into its file until every producer is gone
/// (the pass was taken) and every ring is empty.
fn write_until_abandoned(mut files: Vec<TrackFile>) -> Vec<TrackFile> {
    let mut bytes: Vec<u8> = Vec::with_capacity(1 << 16);
    loop {
        // Read before draining: a ring abandoned now has had its last
        // samples pushed, so once drained it stays empty.
        let abandoned = files.iter().all(|f| f.ring.is_abandoned());
        let mut moved = false;
        for f in &mut files {
            let n = f.ring.slots();
            if n == 0 {
                continue;
            }
            moved = true;
            let Ok(chunk) = f.ring.read_chunk(n) else {
                continue;
            };
            bytes.clear();
            let (a, b) = chunk.as_slices();
            for s in a.iter().chain(b) {
                bytes.extend_from_slice(&s.to_le_bytes());
            }
            chunk.commit_all();
            if f.error.is_none() {
                match f.file.write_all(&bytes) {
                    Ok(()) => f.frames += n as u64,
                    // Keep draining (and discarding): the audio thread
                    // must never back up behind a full disk.
                    Err(e) => f.error = Some(e),
                }
            }
        }
        if abandoned && files.iter().all(|f| f.ring.is_empty()) {
            return files;
        }
        if !moved {
            std::thread::park_timeout(std::time::Duration::from_millis(5));
        }
    }
}

// ────────────────────────────────────────────────────────────────────
// Capture (audio thread)
// ────────────────────────────────────────────────────────────────────

/// Hand one rendered block's input to the pass, if one is recording this
/// project. Realtime: never waits and never allocates. See the module docs.
pub(crate) fn capture_block(daw: &Standalone, block: &Block<'_>) {
    let mut slot = if block.offline {
        lock(&daw.recorder.pass)
    } else {
        match daw.recorder.pass.try_lock() {
            Ok(g) => g,
            Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        }
    };
    let Some(pass) = slot.as_mut() else {
        return;
    };
    if pass.closed
        || pass.project_guid != block.project_guid
        || block.frames == 0
        || pass.transport.play_state() != PlayStateRepr::Recording
    {
        return;
    }
    match pass.renderer {
        None => pass.renderer = Some(block.renderer),
        Some(r) if r != block.renderer => return,
        Some(_) => {}
    }
    match pass.start {
        None => {
            pass.start = Some(block.start_samples);
            pass.sample_rate = Some(block.sample_rate);
        }
        Some(_) if (block.start_samples - pass.next).abs() > 0.5 => {
            // A locate or a loop wrap: the take ends where the jump was.
            pass.closed = true;
            return;
        }
        Some(_) => {}
    }
    pass.next = block.start_samples + block.frames as f64 * block.rate;
    let read_from = pass.frames as usize;
    let mut lost = 0_u64;
    for track in &mut pass.tracks {
        let test = pass.test_inputs.get(&track.channel);
        let channel = track.channel as usize;
        let sample = |f: usize| -> f32 {
            if let Some(buf) = test {
                return buf.get(read_from + f).copied().unwrap_or(0.0);
            }
            match block.live {
                Some((interleaved, channels)) if channel < channels => interleaved
                    .get(f * channels + channel)
                    .copied()
                    .unwrap_or(0.0),
                _ => 0.0,
            }
        };
        let mut done = 0;
        let mut waited: Option<std::time::Instant> = None;
        while done < block.frames {
            let room = track.ring.slots().min(block.frames - done);
            if room > 0 {
                if let Ok(chunk) = track.ring.write_chunk_uninit(room) {
                    chunk.fill_from_iter((done..done + room).map(&sample));
                }
                done += room;
                continue;
            }
            if !block.offline {
                break;
            }
            // Offline: wait for the writer rather than lose samples.
            let since = *waited.get_or_insert_with(std::time::Instant::now);
            if since.elapsed() > OFFLINE_PATIENCE {
                break;
            }
            pass.writer_thread.unpark();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        lost = lost.max((block.frames - done) as u64);
    }
    pass.frames += block.frames as u64;
    pass.dropped += lost;
}

// ────────────────────────────────────────────────────────────────────
// Finish
// ────────────────────────────────────────────────────────────────────

/// End the pass on `project_guid` (see [`Standalone::finish_recording`]).
/// What it placed — nothing, when nothing was recording — becomes the
/// project's [`Standalone::last_recording`].
pub(crate) fn finish(daw: &Standalone, project_guid: &str) -> DawResult<Vec<RecordedTake>> {
    let pass = {
        let mut slot = lock(&daw.recorder.pass);
        match slot.take() {
            Some(p) if p.project_guid == project_guid => Some(p),
            other => {
                *slot = other;
                None
            }
        }
    };
    lock(&daw.recorder.finished).insert(project_guid.to_owned(), Vec::new());
    let Some(pass) = pass else {
        return Ok(Vec::new());
    };
    let ActivePass {
        tracks,
        sample_rate,
        start,
        dropped,
        writer,
        writer_thread,
        ..
    } = pass;
    // Dropping the producers is what tells the writer the pass is over.
    let placed: Vec<(String, PathBuf)> = tracks.into_iter().map(|t| (t.guid, t.path)).collect();
    writer_thread.unpark();
    let files = writer
        .join()
        .map_err(|_| DawError::operation_failed("the record writer panicked"))?;
    let sample_rate = sample_rate.unwrap_or(48_000);
    let mut written = Vec::with_capacity(files.len());
    let mut failure = None;
    for (f, (guid, path)) in files.into_iter().zip(placed) {
        match complete_wav(f, sample_rate) {
            Ok(frames) => written.push((guid, path, frames)),
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                failure.get_or_insert(format!("{}: {e}", path.display()));
            }
        }
    }
    let Some(start) = start else {
        // Nothing was rendered while it recorded: no takes, no files.
        for (_, path, _) in &written {
            let _ = std::fs::remove_file(path);
        }
        return match failure {
            Some(e) => Err(DawError::operation_failed(e)),
            None => Ok(Vec::new()),
        };
    };
    let position = start / f64::from(sample_rate);
    let policy = daw.take_lane_policy();
    let ctx = ProjectContext::Project(project_guid.to_owned());
    Projects::begin_undo_block(daw, ctx.clone(), "Record");
    let mut takes = Vec::with_capacity(written.len());
    let mut refused = None;
    for (guid, path, frames) in written {
        if frames == 0 {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let length = frames as f64 / f64::from(sample_rate);
        match place_take(daw, project_guid, &guid, &path, (position, length), policy) {
            Ok(mut take) => {
                take.dropped_frames = dropped;
                takes.push(take);
            }
            Err(e) => {
                refused.get_or_insert(e);
            }
        }
    }
    Projects::end_undo_block(daw, ctx, "Record", None);
    tracing::info!(
        record.project = project_guid,
        record.takes = takes.len(),
        record.position = position,
        record.dropped_frames = dropped,
        "recording pass finished"
    );
    lock(&daw.recorder.finished).insert(project_guid.to_owned(), takes.clone());
    if let Some(e) = refused {
        return Err(e);
    }
    if let Some(e) = failure {
        return Err(DawError::operation_failed(e));
    }
    Ok(takes)
}

/// Flush a take's file and write its real header. Returns its frames.
fn complete_wav(f: TrackFile, sample_rate: u32) -> std::io::Result<u64> {
    if let Some(e) = f.error {
        return Err(e);
    }
    let mut file = f.file.into_inner().map_err(|e| e.into_error())?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&wav_header(sample_rate, f.frames))?;
    file.flush()?;
    file.set_len(HEADER_BYTES + f.frames * 4)?;
    Ok(f.frames)
}

/// A 44-byte header for a mono 32-bit float WAV of `frames` frames. A
/// take past 4 GiB has its sizes capped (readers then go by the file).
fn wav_header(sample_rate: u32, frames: u64) -> [u8; 44] {
    let data = u32::try_from(frames.saturating_mul(4)).unwrap_or(u32::MAX - 36);
    let mut h = [0_u8; 44];
    let mut put = |at: usize, bytes: &[u8]| h[at..at + bytes.len()].copy_from_slice(bytes);
    put(0, b"RIFF");
    put(4, &data.saturating_add(36).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16_u32.to_le_bytes());
    put(20, &3_u16.to_le_bytes()); // IEEE float
    put(22, &1_u16.to_le_bytes()); // mono
    put(24, &sample_rate.to_le_bytes());
    put(28, &sample_rate.saturating_mul(4).to_le_bytes());
    put(32, &4_u16.to_le_bytes());
    put(34, &32_u16.to_le_bytes());
    put(36, b"data");
    put(40, &data.to_le_bytes());
    h
}

/// `<dir>/<track>-NN.wav`, the first number free on disk and in this pass.
fn take_path(dir: &Path, track_name: &str, taken: &[PathBuf]) -> PathBuf {
    let stem = match file_safe(track_name.trim()) {
        s if s.is_empty() => "Track".to_owned(),
        s => s,
    };
    (1_u32..)
        .map(|n| dir.join(format!("{stem}-{n:02}.wav")))
        .find(|p| !p.exists() && !taken.contains(p))
        .unwrap_or_else(|| dir.join(format!("{stem}.wav")))
}

/// `s` with what a file name cannot hold replaced.
fn file_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

// ────────────────────────────────────────────────────────────────────
// Placing a take
// ────────────────────────────────────────────────────────────────────

/// Which lane a take goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneChoice {
    /// This existing lane.
    Existing(u32),
    /// A new lane after the last.
    New,
}

/// Choose the lane for a take over `span` (seconds) on a track with
/// `lane_count` lanes, `take_lane(l)` saying which are take lanes and
/// `occupied` what is on them (`(lane, start, end)`). See the module docs.
#[must_use]
pub fn choose_take_lane(
    lane_count: u32,
    take_lane: impl Fn(u32) -> bool,
    occupied: &[(u32, f64, f64)],
    span: (f64, f64),
    policy: TakeLanePolicy,
) -> LaneChoice {
    let empty_over = |lane: u32, (from, to): (f64, f64)| {
        !occupied
            .iter()
            .any(|&(l, a, b)| l == lane && a < to - EPSILON && b > from + EPSILON)
    };
    let take_lanes = (0..lane_count).filter(|&l| take_lane(l));
    match policy {
        TakeLanePolicy::LowestFree => take_lanes
            .clone()
            .find(|&l| empty_over(l, span))
            .map_or(LaneChoice::New, LaneChoice::Existing),
        TakeLanePolicy::AlwaysNew => {
            let all = (f64::NEG_INFINITY, f64::INFINITY);
            let mut lanes = take_lanes.peekable();
            let first = lanes.peek().copied();
            if lanes.all(|l| empty_over(l, all)) {
                first.map_or(LaneChoice::New, LaneChoice::Existing)
            } else {
                LaneChoice::New
            }
        }
    }
}

/// Lane `lane`'s name — REAPER's default, its number from 1, when it has
/// none.
fn lane_name(track: &Track, lane: u32) -> String {
    track
        .lane_names
        .get(lane as usize)
        .filter(|n| !n.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| (lane + 1).to_string())
}

/// Whether a lane name is a comp's: `C1`, `C2`, …, or `COMP`.
fn names_a_comp(name: &str) -> bool {
    let n = name.trim();
    if n.eq_ignore_ascii_case("comp") {
        return true;
    }
    let mut chars = n.chars();
    matches!(chars.next(), Some('C' | 'c'))
        && !chars.as_str().is_empty()
        && chars.as_str().chars().all(|c| c.is_ascii_digit())
}

/// Whether a lane name is a comp's or the drum editor's (`EDIT`) — not a
/// take lane.
fn names_a_comp_or_edit(name: &str) -> bool {
    names_a_comp(name) || name.trim().eq_ignore_ascii_case("edit")
}

/// The lanes of `track` that are not take lanes, and its active comp lane.
fn non_take_lanes(
    track: &Track,
    comping: &daw_proto::track::LaneComping,
) -> (Vec<u32>, Option<u32>) {
    let mut lanes: Vec<u32> = comping.areas.iter().map(|a| a.comp_lane).collect();
    lanes.extend(comping.comp_lane);
    lanes.extend(comping.last_comp_lane);
    lanes.extend((0..track.lane_count).filter(|&l| names_a_comp_or_edit(&lane_name(track, l))));
    lanes.retain(|&l| l < track.lane_count);
    lanes.sort_unstable();
    lanes.dedup();
    let active = comping
        .comp_lane
        .filter(|&l| l < track.lane_count)
        .or_else(|| (0..track.lane_count).find(|&l| names_a_comp(&lane_name(track, l))));
    (lanes, active)
}

/// The name for a new take lane: one past the highest numbered take lane,
/// else one past the take-lane count.
fn next_take_lane_name(track: &Track, non_take: &[u32]) -> String {
    let take_lanes: Vec<u32> = (0..track.lane_count)
        .filter(|l| !non_take.contains(l))
        .collect();
    let highest = take_lanes
        .iter()
        .filter_map(|&l| lane_name(track, l).trim().parse::<u32>().ok())
        .max();
    let next = highest.unwrap_or(take_lanes.len() as u32).saturating_add(1);
    next.to_string()
}

/// Put the WAV at `path` on `track_guid` as a new take over `span`
/// (`(position, length)`, seconds), on the lane the policy picks.
fn place_take(
    daw: &Standalone,
    project_guid: &str,
    track_guid: &str,
    path: &Path,
    (position, length): (f64, f64),
    policy: TakeLanePolicy,
) -> DawResult<RecordedTake> {
    let ctx = || ProjectContext::Project(project_guid.to_owned());
    let at = || TrackRef::Guid(track_guid.to_owned());
    let track_now =
        || Tracks::get(daw, ctx(), at()).ok_or_else(|| DawError::not_found("Track", track_guid));
    let mut track = track_now()?;
    if track.lane_count == 0 {
        // The first pass on a track without lanes turns them on; what was
        // already on the track goes on lane 1 with it.
        Tracks::set_lane_count(daw, ctx(), at(), 1)?;
        Tracks::set_lane_name(daw, ctx(), at(), 0, "1")?;
        for item in Items::get_items(daw, ctx(), at()) {
            Items::set_fixed_lane(daw, ctx(), ItemRef::Guid(item.guid), 0)?;
        }
        track = track_now()?;
    }
    let comping = Tracks::comping(daw, ctx(), at()).unwrap_or_default();
    let (non_take, active_comp) = non_take_lanes(&track, &comping);
    let items = Items::get_items(daw, ctx(), at());
    let occupied: Vec<(u32, f64, f64)> = items
        .iter()
        .map(|i| {
            let start = i.position.as_seconds();
            (
                i.fixed_lane.unwrap_or(0),
                start,
                start + i.length.as_seconds(),
            )
        })
        .collect();
    let choice = choose_take_lane(
        track.lane_count,
        |l| !non_take.contains(&l),
        &occupied,
        (position, position + length),
        policy,
    );
    let lane = match choice {
        LaneChoice::Existing(l) => l,
        LaneChoice::New => {
            let lane = track.lane_count;
            let name = next_take_lane_name(&track, &non_take);
            Tracks::set_lane_count(daw, ctx(), at(), lane + 1)?;
            Tracks::set_lane_name(daw, ctx(), at(), lane, &name)?;
            lane
        }
    };
    let item_guid = Items::add_item(
        daw,
        ctx(),
        at(),
        PositionInSeconds::from_seconds(position),
        Duration::from_seconds(length),
    )
    .ok_or_else(|| DawError::operation_failed("the engine made no item for the take"))?;
    let item = || ItemRef::Guid(item_guid.clone());
    let take_guid = Takes::add_take(daw, ctx(), item())
        .ok_or_else(|| DawError::operation_failed("the engine made no take"))?;
    let path_text = path.to_string_lossy().into_owned();
    Takes::set_source_file(
        daw,
        ctx(),
        item(),
        TakeRef::Guid(take_guid.clone()),
        path_text.clone(),
    )?;
    if let Some(stem) = path.file_stem() {
        Takes::set_name(
            daw,
            ctx(),
            item(),
            TakeRef::Guid(take_guid.clone()),
            stem.to_string_lossy().into_owned(),
        )?;
    }
    Items::set_fixed_lane(daw, ctx(), item(), lane)?;
    // What plays: a comp that holds something stays; else the new take.
    let comp_holds_something = active_comp.is_some_and(|c| occupied.iter().any(|&(l, ..)| l == c));
    if !comp_holds_something && lane < 64 {
        Tracks::set_lane_play_mask(daw, ctx(), at(), 1_u64 << lane)?;
    }
    crate::audio_engine::materialize::materialize_take(
        daw,
        project_guid,
        &take_guid,
        &path_text,
        &mut |p: &str| std::fs::read(p).map_err(|e| e.to_string()),
        &mut |p: &str| Some(PathBuf::from(p)),
    )
    .map_err(DawError::operation_failed)?;
    let track = track_now()?;
    Ok(RecordedTake {
        track_guid: track_guid.to_owned(),
        item_guid,
        take_guid,
        lane,
        lane_name: lane_name(&track, lane),
        path: path.to_path_buf(),
        position,
        length,
        dropped_frames: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_lane(_: u32) -> bool {
        true
    }

    #[test]
    fn lowest_free_takes_the_first_lane_empty_over_the_range() {
        // Lanes 0 and 1 hold the intro (7–21 s); lane 2 is free.
        let occupied = [(0, 7.0, 21.0), (1, 7.0, 21.0)];
        let pick =
            |span| choose_take_lane(3, any_lane, &occupied, span, TakeLanePolicy::LowestFree);
        // Over the intro again: lane 2.
        assert_eq!(pick((7.0, 21.0)), LaneChoice::Existing(2));
        // The verse, after it: lane 0, beside the intro's take 1.
        assert_eq!(pick((21.0, 49.0)), LaneChoice::Existing(0));
    }

    #[test]
    fn lowest_free_makes_a_lane_when_every_take_lane_is_used() {
        let occupied = [(0, 0.0, 10.0), (1, 5.0, 6.0)];
        let pick = choose_take_lane(
            2,
            any_lane,
            &occupied,
            (4.0, 8.0),
            TakeLanePolicy::LowestFree,
        );
        assert_eq!(pick, LaneChoice::New);
    }

    #[test]
    fn a_comp_lane_is_never_a_take_lane() {
        // Lane 0 is C1, empty: the take still goes on lane 1.
        let pick = choose_take_lane(2, |l| l != 0, &[], (0.0, 1.0), TakeLanePolicy::LowestFree);
        assert_eq!(pick, LaneChoice::Existing(1));
    }

    #[test]
    fn always_new_uses_an_empty_track_s_first_lane_then_new_ones() {
        let fresh = choose_take_lane(1, any_lane, &[], (0.0, 1.0), TakeLanePolicy::AlwaysNew);
        assert_eq!(fresh, LaneChoice::Existing(0));
        let used = choose_take_lane(
            2,
            any_lane,
            &[(0, 0.0, 1.0)],
            (5.0, 6.0),
            TakeLanePolicy::AlwaysNew,
        );
        assert_eq!(used, LaneChoice::New);
    }

    #[test]
    fn touching_spans_do_not_overlap() {
        let pick = choose_take_lane(
            1,
            any_lane,
            &[(0, 0.0, 7.0)],
            (7.0, 9.0),
            TakeLanePolicy::LowestFree,
        );
        assert_eq!(pick, LaneChoice::Existing(0));
    }

    #[test]
    fn comp_and_edit_lanes_are_known_by_name() {
        assert!(names_a_comp_or_edit("C1"));
        assert!(names_a_comp_or_edit("c12"));
        assert!(names_a_comp_or_edit("COMP"));
        assert!(names_a_comp_or_edit("EDIT"));
        assert!(!names_a_comp_or_edit("1"));
        assert!(!names_a_comp_or_edit("C"));
        assert!(!names_a_comp_or_edit("Chorus"));
    }

    #[test]
    fn a_new_take_lane_is_named_one_past_the_highest_number() {
        let mut track = Track::default();
        track.lane_count = 3;
        track.lane_names = vec!["C1".into(), "1".into(), "2".into()];
        assert_eq!(next_take_lane_name(&track, &[0]), "3");
    }

    #[test]
    fn the_header_describes_a_mono_float_wav() {
        let h = wav_header(48_000, 10);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([h[4], h[5], h[6], h[7]]), 36 + 40);
        assert_eq!(u16::from_le_bytes([h[20], h[21]]), 3);
        assert_eq!(u32::from_le_bytes([h[24], h[25], h[26], h[27]]), 48_000);
        assert_eq!(u32::from_le_bytes([h[40], h[41], h[42], h[43]]), 40);
    }

    #[test]
    fn take_files_are_numbered_past_what_is_there() {
        let dir = std::env::temp_dir().join(format!("record-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let first = take_path(&dir, "Guitar/Clean", &[]);
        assert_eq!(
            first.file_name().and_then(|n| n.to_str()),
            Some("Guitar_Clean-01.wav")
        );
        let second = take_path(&dir, "Guitar/Clean", &[first]);
        assert_eq!(
            second.file_name().and_then(|n| n.to_str()),
            Some("Guitar_Clean-02.wav")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
