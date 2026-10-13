//! Streamed sources: a take whose audio is decoded a few seconds at a time
//! around the playhead, from compressed bytes held in memory.
//!
//! The browser's answer to [`AudioSource::PcmFile`](super::source::AudioSource):
//! a page has no disk to map, and 21 seven-minute stems decoded whole are
//! 3 GB. So a take's source is a [`Streamed`] — fixed-size chunks of
//! decoded audio, resident only near the playhead — and a
//! [`StreamFeeder`] keeps the chunks ahead of it filled from the take's
//! proxy (an Ogg Vorbis file, see `fts_sample::ogg_stream`). The renderer
//! reads a chunk that is not there yet as silence and says where it is
//! reading ([`Streamed::want`]), which is where the feeder decodes next.
//!
//! Nothing here blocks or spawns: the feeder does a bounded amount of work
//! per [`StreamFeeder::pump`] and whoever drives playback calls it — a
//! browser's render loop, a test.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Frames per resident chunk (~0.37 s at 44.1 kHz).
pub const CHUNK: usize = 16_384;

/// Full scale of a stored sample.
const I16_SCALE: f32 = 32767.0;

/// A decoded sample, stored.
#[inline]
fn to_i16(x: f32) -> i16 {
    #[allow(clippy::cast_possible_truncation)]
    let s = (x.clamp(-1.0, 1.0) * I16_SCALE).round() as i16;
    s
}

/// A streamed take's audio: its shape, and the chunks resident now.
#[derive(Clone)]
pub struct Streamed {
    inner: Arc<Inner>,
}

struct Inner {
    channels: u16,
    sample_rate: u32,
    frames: u64,
    /// Chunk `i` holds frames `i * CHUNK ..`, interleaved, as 16-bit
    /// samples: half the memory of the decoder's floats, and far finer
    /// than the lossy proxy they were decoded from (its own noise sits
    /// well above 16-bit's). A band's worth of decode windows was ~70 MB
    /// as floats.
    chunks: RwLock<Vec<Option<Arc<[i16]>>>>,
    /// The frame playback last read from — where to decode next.
    wanted: AtomicU64,
    /// The waveform of the audio this stands in for, when the holder
    /// handed it in ([`Streamed::set_peaks`]) — what the arrangement draws
    /// before (and without) the audio arriving.
    #[cfg(feature = "reapeaks")]
    peaks: std::sync::OnceLock<Arc<dawfile_reaper::reapeaks::ReaPeaks>>,
}

impl Streamed {
    #[must_use]
    pub fn new(channels: u16, sample_rate: u32, frames: u64) -> Self {
        let count = usize::try_from(frames.div_ceil(CHUNK as u64)).unwrap_or(0);
        Self {
            inner: Arc::new(Inner {
                channels: channels.max(1),
                sample_rate,
                frames,
                chunks: RwLock::new(vec![None; count]),
                wanted: AtomicU64::new(0),
                #[cfg(feature = "reapeaks")]
                peaks: std::sync::OnceLock::new(),
            }),
        }
    }

    #[must_use]
    pub fn channels(&self) -> u16 {
        self.inner.channels
    }

    /// Hand in the waveform of the audio this streams — its original's
    /// peaks cache, fetched alongside (the first one handed in stays).
    #[cfg(feature = "reapeaks")]
    pub fn set_peaks(&self, peaks: Arc<dawfile_reaper::reapeaks::ReaPeaks>) {
        let _ = self.inner.peaks.set(peaks);
    }

    /// The waveform handed in, if any.
    #[cfg(feature = "reapeaks")]
    #[must_use]
    pub fn peaks(&self) -> Option<Arc<dawfile_reaper::reapeaks::ReaPeaks>> {
        self.inner.peaks.get().cloned()
    }

    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    #[must_use]
    pub fn frames(&self) -> u64 {
        self.inner.frames
    }

    /// Say where playback is reading, so the feeder decodes there.
    pub fn want(&self, frame: u64) {
        self.inner.wanted.store(frame, Ordering::Relaxed);
    }

    #[must_use]
    pub fn wanted(&self) -> u64 {
        self.inner.wanted.load(Ordering::Relaxed)
    }

    /// One sample; silence where the chunk is not resident (or past the
    /// end).
    #[inline]
    #[must_use]
    pub fn sample(&self, frame: usize, channel: usize) -> f32 {
        let ch = usize::from(self.inner.channels);
        let chunks = self
            .inner
            .chunks
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        chunks
            .get(frame / CHUNK)
            .and_then(Option::as_ref)
            .and_then(|c| c.get((frame % CHUNK) * ch + channel.min(ch - 1)).copied())
            .map_or(0.0, |s| f32::from(s) / I16_SCALE)
    }

    /// Whether chunk `index` is resident.
    #[must_use]
    pub fn resident(&self, index: usize) -> bool {
        self.inner
            .chunks
            .read()
            .map(|c| c.get(index).is_some_and(Option::is_some))
            .unwrap_or(false)
    }

    fn put(&self, index: usize, pcm: Arc<[i16]>) {
        if let Ok(mut chunks) = self.inner.chunks.write()
            && let Some(slot) = chunks.get_mut(index)
        {
            *slot = Some(pcm);
        }
    }

    /// Drop every chunk outside `keep`.
    fn evict(&self, keep: std::ops::Range<usize>) {
        if let Ok(mut chunks) = self.inner.chunks.write() {
            for (i, slot) in chunks.iter_mut().enumerate() {
                if !keep.contains(&i) {
                    *slot = None;
                }
            }
        }
    }

    fn chunk_count(&self) -> usize {
        self.inner.chunks.read().map_or(0, |c| c.len())
    }
}

/// Why a decoder could not go on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes it needs have not arrived yet (a proxy streamed from
    /// elsewhere): the take is silent there for now, and tries again.
    NotYet,
    /// The stream is broken.
    Failed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotYet => f.write_str("not arrived yet"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

#[cfg(feature = "stream-ogg")]
impl From<fts_sample::SamplerError> for DecodeError {
    fn from(e: fts_sample::SamplerError) -> Self {
        if e.is_not_yet() {
            Self::NotYet
        } else {
            Self::Failed(e.to_string())
        }
    }
}

/// Where a feeder decodes from: anything that seeks to a frame and then
/// decodes forward. `fts_sample::ogg_stream::OggStream` is the one for a
/// proxy on disk; [`RemoteOgg`] for one streamed from elsewhere.
pub trait Decode {
    /// Position so the next [`Decode::decode`] starts at `frame`.
    ///
    /// # Errors
    ///
    /// The stream could not be read there (or not yet).
    fn seek(&mut self, frame: u64) -> Result<(), DecodeError>;
    /// Decode onward, appending interleaved audio to `out`; the frame it
    /// starts at and its length, or `None` at the end.
    ///
    /// # Errors
    ///
    /// The stream is corrupt, or its next bytes have not arrived.
    fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError>;
    /// A count that moves when more of the stream's bytes arrive — after a
    /// [`DecodeError::NotYet`], nothing new can decode until it does. A
    /// stream whose bytes are all here keeps it at 0.
    fn arrivals(&self) -> u64 {
        0
    }
}

impl<D: Decode + ?Sized> Decode for Box<D> {
    fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
        (**self).seek(frame)
    }

    fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
        (**self).decode(out)
    }

    fn arrivals(&self) -> u64 {
        (**self).arrivals()
    }
}

#[cfg(feature = "stream-ogg")]
impl Decode for fts_sample::ogg_stream::OggStream {
    fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
        Ok(fts_sample::ogg_stream::OggStream::seek(self, frame)?)
    }

    fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
        Ok(fts_sample::ogg_stream::OggStream::decode(self, out)?)
    }
}

/// A proxy streamed from elsewhere (Task, a peer): its bytes as they
/// arrive ([`fts_sample::sparse::SparseBytes`]) and its page index. Every
/// seek is a fresh open at the indexed page before the frame
/// ([`fts_sample::ogg_stream::OggStream::open_view`]), so a seek needs only
/// the bytes in front of it.
#[cfg(feature = "stream-ogg")]
pub struct RemoteOgg {
    bytes: Arc<fts_sample::sparse::SparseBytes>,
    index: fts_sample::ogg_index::OggIndex,
    stream: Option<fts_sample::ogg_stream::OggStream>,
}

#[cfg(feature = "stream-ogg")]
impl RemoteOgg {
    #[must_use]
    pub const fn new(
        bytes: Arc<fts_sample::sparse::SparseBytes>,
        index: fts_sample::ogg_index::OggIndex,
    ) -> Self {
        Self {
            bytes,
            index,
            stream: None,
        }
    }
}

#[cfg(feature = "stream-ogg")]
impl Decode for RemoteOgg {
    fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
        self.stream = None;
        self.stream = Some(fts_sample::ogg_stream::OggStream::open_view(
            Arc::clone(&self.bytes),
            &self.index,
            frame,
        )?);
        Ok(())
    }

    fn arrivals(&self) -> u64 {
        self.bytes.arrivals()
    }

    fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
        let Some(stream) = self.stream.as_mut() else {
            return Err(DecodeError::NotYet);
        };
        match stream.decode(out) {
            Ok(done) => Ok(done),
            Err(e) => {
                // A read that stopped part way leaves the reader mid-page:
                // the next try opens afresh.
                self.stream = None;
                Err(e.into())
            }
        }
    }
}

/// Keeps a [`Streamed`] take's chunks filled around where playback reads.
pub struct StreamFeeder<D> {
    source: Streamed,
    decoder: D,
    /// The frame the decoder will produce next, when known.
    at: Option<u64>,
    /// Decoded audio not yet a whole chunk, starting at chunk `pending.0`.
    pending: (usize, Vec<f32>),
    failed: bool,
    /// Chunks kept decoded ahead of the playhead ([`AHEAD`] unless set).
    ahead: usize,
    /// Stopped short for bytes that had not arrived: the decoder's
    /// [`Decode::arrivals`] and the playhead's chunk then. Until one of
    /// them moves, a pump has nothing new to try.
    stalled: Option<(u64, usize)>,
}

/// How far ahead of the playhead to keep decoded, and how far behind.
/// Decoded stereo is ~128 KB a chunk, so a take holds ~3.4 MB — fine on a
/// desktop; a browser keeps less ([`StreamFeeder::with_ahead`]).
pub const AHEAD: usize = 24; // ~9 s at 44.1 kHz
const BEHIND: usize = 2;
/// Gaps tried in one pump when their bytes have not arrived.
const MAX_GAPS_TRIED: usize = 4;

/// What one fill did.
enum Filled {
    /// Decoded something.
    Some,
    /// The gap's bytes have not arrived.
    NotYet,
    /// Nothing to do, or the stream failed.
    Nothing,
}

impl<D: Decode> StreamFeeder<D> {
    #[must_use]
    pub fn new(source: Streamed, decoder: D) -> Self {
        Self {
            source,
            decoder,
            at: Some(0),
            pending: (0, Vec::new()),
            failed: false,
            ahead: AHEAD,
            stalled: None,
        }
    }

    /// Keep `chunks` decoded ahead of the playhead instead of [`AHEAD`] —
    /// less for a holder that must stay small and decodes often (a
    /// browser's page, every few milliseconds).
    #[must_use]
    pub fn with_ahead(mut self, chunks: usize) -> Self {
        self.ahead = chunks.max(2);
        self
    }

    #[must_use]
    pub fn source(&self) -> &Streamed {
        &self.source
    }

    /// The same feeder over a boxed decoder (what the butler holds).
    #[must_use]
    pub fn boxed(self) -> StreamFeeder<Box<dyn Decode + Send>>
    where
        D: Send + 'static,
    {
        StreamFeeder {
            source: self.source,
            decoder: Box::new(self.decoder),
            at: self.at,
            pending: self.pending,
            failed: self.failed,
            ahead: self.ahead,
            stalled: self.stalled,
        }
    }

    /// Decode toward the playhead: the first chunk it needs that is not
    /// resident, then onward. Stops after about `budget` frames of work.
    /// Returns whether anything was decoded — `false` means caught up.
    pub fn pump(&mut self, budget: usize) -> bool {
        if self.failed {
            return false;
        }
        let count = self.source.chunk_count();
        let here = usize::try_from(self.source.wanted()).unwrap_or(usize::MAX) / CHUNK;
        // Read before trying, so bytes arriving during the try are tried
        // on the next pump rather than taken as already seen.
        let arrivals = self.decoder.arrivals();
        if self.stalled == Some((arrivals, here)) {
            // Waiting on bytes, and none have come: trying again would
            // decode the same frames to stop at the same place. (It did,
            // every few milliseconds, for every take whose proxy was
            // only partly fetched — a core, for as long as the set was
            // open.)
            return false;
        }
        let window = here.saturating_sub(BEHIND)..(here + self.ahead).min(count);
        self.source.evict(window.start..window.end);
        // What will be heard first first: from the playhead on, then
        // behind — after a jump too. (Decoding from just behind it in one
        // pass saved a seek but held every stem silent for the chunks
        // behind; a band of stems after a seek heard nothing for seconds.)
        // A gap whose bytes have not arrived (a proxy streamed from
        // elsewhere) is passed over for the next, so what has arrived still
        // decodes.
        let order: Vec<usize> = (here.max(window.start)..window.end)
            .chain(window.start..here.min(window.end))
            .collect();
        let gaps: Vec<usize> = order
            .into_iter()
            .filter(|i| !self.source.resident(*i))
            .collect();
        let mut waiting = false;
        for next in gaps.into_iter().take(MAX_GAPS_TRIED) {
            match self.fill(next, &window, budget) {
                Filled::Some => {
                    self.stalled = None;
                    return true;
                }
                Filled::NotYet => waiting = true,
                Filled::Nothing => return false,
            }
        }
        if waiting {
            self.stalled = Some((arrivals, here));
        }
        false
    }

    /// Decode chunk `next` (and on while in the window), about `budget`
    /// frames of work.
    fn fill(&mut self, next: usize, window: &std::ops::Range<usize>, budget: usize) -> Filled {
        let window = window.clone();
        let ch = usize::from(self.source.channels());
        let want_frame = (next * CHUNK) as u64;
        // Carry on from where the decoder is if it is part way through the
        // chunk needed; otherwise seek there, dropping what was half built.
        let pending_end = (self.pending.0 * CHUNK + self.pending.1.len() / ch) as u64;
        if self.pending.0 != next || self.at != Some(pending_end) {
            match self.decoder.seek(want_frame) {
                Ok(()) => {}
                // Not arrived yet: silent here for now; the next pump tries
                // again (the fetcher has been told what is wanted).
                Err(DecodeError::NotYet) => {
                    self.at = None;
                    return Filled::NotYet;
                }
                Err(DecodeError::Failed(e)) => {
                    tracing::warn!(error = %e, frame = want_frame, "stream seek failed");
                    self.failed = true;
                    return Filled::Nothing;
                }
            }
            self.at = Some(want_frame);
            self.pending = (next, Vec::with_capacity(CHUNK * ch));
        }
        let mut done = 0usize;
        // Progress is a chunk kept: what a stop short drops does not count.
        let first = self.pending.0;
        while done < budget {
            let before = self.pending.1.len();
            match self.decoder.decode(&mut self.pending.1) {
                Ok(Some((_, frames))) => {
                    done += frames;
                    self.at = self.at.map(|a| a + frames as u64);
                }
                Ok(None) => {
                    // The end: whatever is pending is the last chunk.
                    self.flush_pending(ch, true);
                    self.at = None;
                    return if self.pending.1.len() != before || done > 0 {
                        Filled::Some
                    } else {
                        Filled::Nothing
                    };
                }
                Err(DecodeError::NotYet) => {
                    // What was half built is dropped; the next pump seeks to
                    // the chunk again once more has arrived. Only a chunk
                    // kept on the way is progress — counting what was just
                    // dropped kept the butler decoding it again forever.
                    self.at = None;
                    self.pending.1.clear();
                    return if self.pending.0 > first {
                        Filled::Some
                    } else {
                        Filled::NotYet
                    };
                }
                Err(DecodeError::Failed(e)) => {
                    tracing::warn!(error = %e, "stream decode failed");
                    self.failed = true;
                    return if done > 0 {
                        Filled::Some
                    } else {
                        Filled::Nothing
                    };
                }
            }
            self.flush_pending(ch, false);
            // Past the window, or onto a chunk already there: stop, and
            // let the next pump pick the next gap.
            let chunk = self.pending.0;
            if chunk >= window.end || self.source.resident(chunk) {
                break;
            }
        }
        Filled::Some
    }

    /// Move every whole chunk out of `pending` into the source (and the
    /// last, partial one when `end`).
    fn flush_pending(&mut self, ch: usize, end: bool) {
        let whole = CHUNK * ch;
        while self.pending.1.len() >= whole || (end && !self.pending.1.is_empty()) {
            let take = self.pending.1.len().min(whole);
            let rest = self.pending.1.split_off(take);
            let floats = std::mem::replace(&mut self.pending.1, rest);
            let pcm: Arc<[i16]> = floats.iter().map(|&x| to_i16(x)).collect();
            self.source.put(self.pending.0, pcm);
            self.pending.0 += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A decoder over a known signal: frame `i`'s left sample is
    /// [`level`]`(i)` and its right is its negative, handed out in
    /// odd-sized packets.
    struct Ramp {
        frames: u64,
        at: u64,
        seeks: usize,
    }

    impl Decode for Ramp {
        fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
            self.at = frame;
            self.seeks += 1;
            Ok(())
        }

        fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
            if self.at >= self.frames {
                return Ok(None);
            }
            let start = self.at;
            let n = 1000.min(self.frames - start);
            for i in start..start + n {
                out.push(level(i));
                out.push(-level(i));
            }
            self.at += n;
            Ok(Some((start, n as usize)))
        }
    }

    /// Frame `i`'s level: its index (wrapping) in 16-bit steps, so it is
    /// told apart from its neighbours and survives being stored exactly.
    fn level(i: u64) -> f32 {
        (i % 30_000) as f32 / I16_SCALE
    }

    fn feeder(frames: u64) -> StreamFeeder<Ramp> {
        StreamFeeder::new(
            Streamed::new(2, 44_100, frames),
            Ramp {
                frames,
                at: 0,
                seeks: 0,
            },
        )
    }

    fn fill(f: &mut StreamFeeder<Ramp>) {
        while f.pump(50_000) {}
    }

    #[test]
    fn the_window_ahead_of_the_playhead_fills_and_reads_true() {
        let mut f = feeder(CHUNK as u64 * 100);
        assert_eq!(
            f.source().sample(10, 0),
            0.0,
            "nothing resident yet: silence"
        );
        fill(&mut f);
        let s = f.source();
        for frame in [0, 1, CHUNK - 1, CHUNK, CHUNK * 5 + 123, CHUNK * AHEAD - 1] {
            assert_eq!(s.sample(frame, 0), level(frame as u64), "left at {frame}");
            assert_eq!(s.sample(frame, 1), -level(frame as u64), "right at {frame}");
        }
        assert!(!s.resident(AHEAD), "nothing past the window");
    }

    #[test]
    fn a_jump_decodes_its_own_chunk_first_and_evicts_what_is_behind() {
        let mut f = feeder(CHUNK as u64 * 100);
        fill(&mut f);
        let far = CHUNK as u64 * 60 + 7;
        f.source().want(far);
        // The first work after the jump is the chunk being played — not
        // the chunks behind it, which would hold the stem silent.
        for _ in 0..64 {
            if f.source().resident(60) {
                break;
            }
            f.pump(1024);
        }
        assert!(f.source().resident(60), "the playhead's chunk comes first");
        assert!(!f.source().resident(59), "before what is behind it");
        fill(&mut f);
        let s = f.source();
        assert_eq!(s.sample(far as usize, 0), level(far));
        assert!(s.resident(59), "then what is behind");
        assert!(!s.resident(0), "the old window is let go");
    }

    #[test]
    fn the_last_chunk_is_short_and_the_end_reads_silence() {
        let frames = CHUNK as u64 * 2 + 500;
        let mut f = feeder(frames);
        fill(&mut f);
        let s = f.source();
        assert_eq!(s.sample(frames as usize - 1, 0), level(frames - 1));
        assert_eq!(s.sample(frames as usize, 0), 0.0, "past the end");
    }

    /// A proxy whose bytes have arrived up to `have` frames: decoding past
    /// them is `NotYet`, and every frame decoded is counted.
    struct Partial {
        ramp: Ramp,
        have: u64,
        arrivals: u64,
        decoded: u64,
    }

    impl Decode for Partial {
        fn seek(&mut self, frame: u64) -> Result<(), DecodeError> {
            self.ramp.seek(frame)
        }

        fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, DecodeError> {
            if self.ramp.at >= self.have {
                return Err(DecodeError::NotYet);
            }
            let got = self.ramp.decode(out)?;
            if let Some((_, n)) = got {
                self.decoded += n as u64;
            }
            Ok(got)
        }

        fn arrivals(&self) -> u64 {
            self.arrivals
        }
    }

    #[test]
    fn a_take_short_of_its_bytes_waits_for_more_rather_than_decoding_again() {
        let frames = CHUNK as u64 * 100;
        // Two and a half chunks arrived: the third stops short.
        let have = CHUNK as u64 * 5 / 2;
        let mut f = StreamFeeder::new(
            Streamed::new(2, 44_100, frames),
            Partial {
                ramp: Ramp {
                    frames,
                    at: 0,
                    seeks: 0,
                },
                have,
                arrivals: 1,
                decoded: 0,
            },
        );
        let mut turns = 0;
        while f.pump(4_096) {
            turns += 1;
            assert!(turns < 1_000, "a take short of its bytes must stop pumping");
        }
        assert!(f.source().resident(1), "what arrived is decoded");
        assert!(
            !f.source().resident(2),
            "the chunk short of its bytes is not"
        );
        let decoded = f.decoder.decoded;
        for _ in 0..50 {
            assert!(!f.pump(4_096));
        }
        assert_eq!(
            f.decoder.decoded, decoded,
            "nothing arrived: nothing decoded again"
        );

        // The rest arrives: it decodes.
        f.decoder.have = frames;
        f.decoder.arrivals += 1;
        while f.pump(4_096) {}
        assert!(f.source().resident(2));
        assert_eq!(
            f.source().sample(CHUNK * 2 + 9, 0),
            level(CHUNK as u64 * 2 + 9)
        );
    }
}

/// The native driver for [`StreamFeeder`]s: one thread pumping every
/// streamed take a bounded amount at a time, round robin, resting when
/// all are caught up.
///
/// A browser pumps from its render loop; a desktop has none to borrow,
/// and a proxy decoded whole instead is ~115 MB of float per stem — a
/// setlist of them was 17 GB. Takes join with [`butler_adopt`] and stay
/// for the life of the process; their decoded window is evicted as the
/// playhead moves, so what a take holds is its compressed bytes plus
/// about [`AHEAD`] chunks.
#[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
pub mod butler {
    use super::{Decode, StreamFeeder};
    use std::sync::{Mutex, OnceLock};

    /// Any decoder: a proxy on disk, one streamed from elsewhere.
    type Feeders = Mutex<Vec<StreamFeeder<Box<dyn Decode + Send>>>>;

    fn feeders() -> &'static Feeders {
        static FEEDERS: OnceLock<Feeders> = OnceLock::new();
        FEEDERS.get_or_init(|| {
            std::thread::Builder::new()
                .name("stream-butler".into())
                .spawn(run)
                .ok();
            Mutex::new(Vec::new())
        })
    }

    /// Hand a streamed take to the butler.
    pub fn adopt<D: Decode + Send + 'static>(feeder: StreamFeeder<D>) {
        let feeder = feeder.boxed();
        if let Ok(mut all) = feeders().lock() {
            all.push(feeder);
        }
    }

    fn run() {
        loop {
            let busy = feeders()
                .lock()
                .map(|mut all| {
                    let mut busy = false;
                    for feeder in all.iter_mut() {
                        // ~0.1 s of audio per take per turn: enough to stay
                        // ahead of playback, small enough that one take's
                        // catch-up never starves the rest.
                        busy |= feeder.pump(4_096);
                    }
                    busy
                })
                .unwrap_or(false);
            if !busy {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[cfg(all(feature = "stream-ogg", not(target_arch = "wasm32")))]
pub use butler::adopt as butler_adopt;
