//! **Streaming samples** — Kontakt's DFD, on our FLAC packs.
//!
//! A streamed sample keeps a small **head** resident (the attack, as 16-bit
//! PCM) and leaves the rest compressed in the memory-mapped pack. As a voice
//! plays past the head it reads **chunks**, which a background thread decodes
//! out of the pack using [`super::flac_index`] to find the frames. Chunks are
//! dropped when the voice moves on.
//!
//! Resident memory is therefore `heads + chunks under playheads`, not
//! `library`. A 12-second stereo note costs ~48 KB instead of 4.6 MB, and the
//! pack stays FLAC on disk — no re-encoding, no second copy of the library.
//!
//! The audio thread never blocks and never decodes: it reads the head, or a
//! chunk that is already there, and otherwise asks for one and returns
//! silence for that block. Prefetch (the read of chunk *n* requests *n+1*)
//! means that only happens on a seek the streamer could not see coming.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(feature = "engine-native")]
use std::sync::{Condvar, OnceLock, Weak};

use arc_swap::ArcSwapOption;

use crate::cache::PackBytes;
use crate::flac_index::FlacIndex;

/// Sample frames per chunk — a quarter second at 48 kHz, 48 KB of stereo
/// i16. Small enough that a *playing* voice's working set is a couple of
/// hundred kilobytes, large enough that the streamer is woken a few times a
/// second per voice rather than every block.
pub const CHUNK_FRAMES: u32 = 12_000;

/// Frames kept resident from the start of every streamed sample. Covers the
/// attack of anything percussive and gives the streamer ~0.25 s of lead time
/// on everything else.
pub const HEAD_FRAMES: u32 = 12_000;

/// How far ahead of the end of a chunk to ask for the next one, in frames.
///
/// 4,800 frames is 100ms at 48kHz — two orders of magnitude more than a
/// chunk decode costs (~0.4ms measured), so the streamer has room even when
/// a chord asks for a hundred chunks at once. The same figure `sample()` and
/// `run()` use.
pub const READ_AHEAD_FRAMES: u32 = 4_800;

/// A single voice's private read cursor into a streamed sample.
///
/// Every production sampler streams this way: the audio thread reads from a
/// buffer the VOICE owns, never from a shared index. HISE keeps two streaming
/// buffers per voice and swaps them; JUCE's `streaming_sampler` preloads each
/// sample's head and fills per-voice buffers on a background thread. The
/// shared-index alternative — ask a table which chunk holds this sample, for
/// every sample — is what made a chord cost hundreds of milliseconds here:
/// an arc-swap guard and contended atomics per sample per voice, with every
/// voice hammering the same cache lines as the streamer thread.
///
/// The cursor holds the chunk it is reading, so a whole block of audio costs
/// ONE resolution. It is deliberately dumb: a range check and a slice index.
#[derive(Default, Clone)]
pub struct StreamCursor {
    chunk: Option<Chunk>,
    /// Held when the cursor is parked on the sample's resident head, which is
    /// where every note starts and therefore where most reads land. Without
    /// this the head fell through to the per-sample path and kept the slow
    /// read alive for exactly the part of the note the player hears first.
    head: Option<Arc<StreamedSample>>,
    /// Absolute sample-index range the cursor covers.
    lo: usize,
    hi: usize,
    /// Index range of a chunk we asked for and did NOT get, and how many more
    /// reads to answer from that knowledge before asking again.
    ///
    /// A failed `seek` used to leave the cursor holding nothing at all, so
    /// the very next sample re-entered `seek` -> `chunk_at` -> miss, for
    /// every sample until the chunk landed. One absent chunk therefore cost
    /// thousands of chunk-table lookups (measured: 1.1M lookups in 8s of
    /// playing, against 5.7k successful ones) — an atomic load and an
    /// `ArcSwap` guard each, on the audio thread, at exactly the moment the
    /// streamer is trying to publish into that table.
    ///
    /// Remembering the hole makes the wait nearly free, and re-asking every
    /// `MISS_RETRY` reads keeps the recovery inside one audio block.
    miss_lo: usize,
    miss_hi: usize,
    miss_skip: u32,
    /// The sample this cursor is parked on, so it can ask for the NEXT chunk
    /// as it advances. Without it the cursor could only read ahead at `seek`
    /// time — which is the moment it ENTERS a chunk, when the end is still a
    /// whole chunk away and the read-ahead condition is false. The result was
    /// that the cursor path never prefetched at all: every chunk boundary
    /// raced the streamer from a standing start, and about half of them lost.
    src: Option<Arc<StreamedSample>>,
    /// Whether the read-ahead for the chunk AFTER the held one has been
    /// issued. Once per chunk, not once per sample: `request` is cheap only
    /// while the chunk is already wanted AND queued, and the streamer clears
    /// `queued` as soon as it drains — after which every further call takes
    /// two read-modify-writes and the streamer's queue MUTEX, on the audio
    /// thread. `frame_pair_cursored` reads four samples per frame, so an
    /// unguarded prefetch did that four times a frame per voice and cost
    /// more render time than the gaps it prevented (measured: mean 1.17ms ->
    /// 1.93ms, and the deadline overruns came back).
    prefetched: bool,
}

/// Reads to answer from a known-absent chunk before consulting the chunk
/// table again. 256 is one audio block at the sizes this engine runs, so a
/// chunk that lands is picked up within a block — inaudible — while the
/// lookups drop by that factor.
const MISS_RETRY: u32 = 256;

impl StreamCursor {
    /// Read `index` from the held chunk, or `None` when it falls outside it.
    #[inline]
    pub fn get(&mut self, index: usize) -> Option<f32> {
        const SCALE: f32 = 1.0 / 32768.0;
        if index < self.lo || index >= self.hi {
            return None;
        }
        if let Some(stream) = self.head.as_ref() {
            return stream.head.get(index).map(|s| *s as f32 * SCALE);
        }
        let v = self
            .chunk
            .as_ref()?
            .get(index - self.lo)
            .map(|s| *s as f32 * SCALE);
        // Approaching the end of this chunk: ask for the next one now, so it
        // is resident before the read gets there. `request` is two atomic
        // loads once the chunk is already wanted, so this costs nothing on
        // the samples that follow the first.
        if !self.prefetched {
            if let Some(stream) = self.src.as_ref() {
                let ch = stream.channels.max(1) as usize;
                if index + ch * READ_AHEAD_FRAMES as usize >= self.hi {
                    let chunk_no = (self.lo / ch) as u32 / CHUNK_FRAMES;
                    stream.request(chunk_no + 1);
                    self.prefetched = true;
                }
            }
        }
        v
    }

    /// Two consecutive frames starting at sample index `a` (interleaved,
    /// `ch` channels; `b` the second frame's index) straight from the held
    /// head or chunk — ONE range check and plain slice reads, instead of a
    /// [`get`](Self::get) per sample (each re-checking the range, re-branching
    /// head vs chunk, re-testing the prefetch). `None` when the pair is not
    /// wholly inside what the cursor holds. Issues the read-ahead like `get`.
    #[inline]
    pub fn frame_pair(&mut self, a: usize, b: usize, ch: usize) -> Option<((f32, f32), (f32, f32))> {
        const SCALE: f32 = 1.0 / 32768.0;
        let hi_needed = b + ch.max(1);
        if a < self.lo || hi_needed > self.hi {
            return None;
        }
        let (data, base): (&[i16], usize) = if let Some(stream) = self.head.as_ref() {
            (&stream.head, 0)
        } else {
            (self.chunk.as_deref()?.as_slice(), self.lo)
        };
        let (ia, ib) = (a - base, b - base);
        if ib + ch.max(1) > data.len() {
            return None;
        }
        let pair = if ch >= 2 {
            (
                (f32::from(data[ia]) * SCALE, f32::from(data[ia + 1]) * SCALE),
                (f32::from(data[ib]) * SCALE, f32::from(data[ib + 1]) * SCALE),
            )
        } else {
            let (x, y) = (f32::from(data[ia]) * SCALE, f32::from(data[ib]) * SCALE);
            ((x, x), (y, y))
        };
        if !self.prefetched && self.head.is_none() {
            if let Some(stream) = self.src.as_ref() {
                let ch = stream.channels.max(1) as usize;
                if b + ch * READ_AHEAD_FRAMES as usize >= self.hi {
                    let chunk_no = (self.lo / ch) as u32 / CHUNK_FRAMES;
                    stream.request(chunk_no + 1);
                    self.prefetched = true;
                }
            }
        }
        Some(pair)
    }

    /// What the cursor holds — `(interleaved samples, the index of the
    /// first)` — for a reader that walks it directly (a voice's fast path).
    /// Call [`touch`](Self::touch) with the furthest index read, so the
    /// read-ahead still fires.
    #[inline]
    #[must_use]
    pub fn held(&self) -> Option<(&[i16], usize)> {
        if let Some(stream) = self.head.as_ref() {
            return Some((&stream.head[..self.hi.min(stream.head.len())], 0));
        }
        self.chunk.as_deref().map(|c| (c.as_slice(), self.lo))
    }

    /// A direct reader got as far as sample index `b`: issue the read-ahead
    /// for the next chunk if that is near the end of this one — what
    /// [`frame_pair`](Self::frame_pair) does on every read.
    #[inline]
    pub fn touch(&mut self, b: usize) {
        if !self.prefetched && self.head.is_none() {
            if let Some(stream) = self.src.as_ref() {
                let ch = stream.channels.max(1) as usize;
                if b + ch * READ_AHEAD_FRAMES as usize >= self.hi {
                    let chunk_no = (self.lo / ch) as u32 / CHUNK_FRAMES;
                    stream.request(chunk_no + 1);
                    self.prefetched = true;
                }
            }
        }
    }

    /// Point the cursor at whichever chunk covers `index`. Returns whether it
    /// now holds one.
    #[inline]
    pub fn seek(&mut self, stream: &Arc<StreamedSample>, index: usize) -> bool {
        // Still inside a hole we already know about: say so without touching
        // the chunk table.
        if index >= self.miss_lo && index < self.miss_hi && self.miss_skip > 0 {
            self.miss_skip -= 1;
            return false;
        }
        // The head is always resident: park on it rather than asking the
        // chunk table a question it will answer `None` to.
        if index < stream.head.len() {
            self.chunk = None;
            self.src = Some(Arc::clone(stream));
            self.head = Some(Arc::clone(stream));
            self.prefetched = false;
            self.lo = 0;
            self.hi = stream.head.len();
            // Ask for the chunk after the head the MOMENT a cursor parks on
            // the head, not three quarters of the way through it.
            //
            // The head is the one place a voice has no predecessor to have
            // prefetched for it: it starts here. Waiting until 3/4 left only
            // 62ms of lead, and that transition was the last one still
            // missing after the read-ahead fix — every later boundary gets a
            // full chunk of warning. Requesting on arrival costs nothing (the
            // request is idempotent and the streamer is idle) and buys the
            // whole head, 0.25s, as lead.
            let ch = stream.channels.max(1) as usize;
            stream.request((stream.head.len() / ch) as u32 / CHUNK_FRAMES);
            return true;
        }
        self.head = None;
        match stream.chunk_at(index) {
            Some((chunk, lo, hi)) => {
                self.chunk = Some(chunk);
                self.src = Some(Arc::clone(stream));
                self.lo = lo;
                self.hi = hi;
                self.miss_skip = 0;
                self.prefetched = false;
                true
            }
            None => {
                // Drop the old chunk: holding it would pin memory the sweep
                // wants back, and it cannot serve this read anyway.
                self.chunk = None;
                self.head = None;
                self.lo = 0;
                self.hi = 0;
                // Remember which chunk is missing, so the reads that follow
                // cost nothing until it is worth asking again.
                let ch = stream.channels.max(1) as usize;
                let chunk_no = (index / ch) as u32 / CHUNK_FRAMES;
                self.miss_lo = chunk_no as usize * CHUNK_FRAMES as usize * ch;
                self.miss_hi = self.miss_lo + CHUNK_FRAMES as usize * ch;
                self.miss_skip = MISS_RETRY;
                false
            }
        }
    }
}

/// Reads that found their chunk absent and returned silence, process wide.
///
/// The audio thread never blocks on the streamer: a read whose chunk has not
/// arrived asks for it and returns 0.0. That is the right trade, but it is
/// also silent damage — no xrun, no missed deadline, just a hole in the
/// output — so it has to be countable.
pub static STREAM_MISSES: AtomicU64 = AtomicU64::new(0);

/// Reads served from a resident chunk, process wide — the denominator.
pub static STREAM_HITS: AtomicU64 = AtomicU64::new(0);

/// Fill passes run by the streamer workers.
pub static FILLS: AtomicU64 = AtomicU64::new(0);
/// Chunks actually decoded.
pub static CHUNKS_DECODED: AtomicU64 = AtomicU64::new(0);
/// Total time inside `decode_chunk`, nanoseconds — the streamer's real cost.
pub static DECODE_NS: AtomicU64 = AtomicU64::new(0);
/// Chunks decoded that this sample had ALREADY decoded once before —
/// eviction thrash. A voice playing forward should never need the same
/// chunk twice; if it does, something threw the chunk away while it was
/// still being read, and the streamer is doing the same work repeatedly
/// while voices gap waiting for it.
pub static REDECODES: AtomicU64 = AtomicU64::new(0);

/// Misses where NOBODY had asked for the chunk yet — the read is the first
/// to want it, so it had zero lead time by construction. This is a prefetch
/// failure: something is reading where no read-ahead predicted.
pub static MISS_UNREQUESTED: AtomicU64 = AtomicU64::new(0);

/// Misses where the chunk was already wanted and simply had not arrived —
/// the streamer being late rather than uninformed.
pub static MISS_PENDING: AtomicU64 = AtomicU64::new(0);

/// Chunks dropped by the idle SWEEP (a sample nobody has asked for lately).
pub static SHED_BY_SWEEP: AtomicU64 = AtomicU64::new(0);

/// Chunks dropped by the per-sample residency BUDGET during a fill.
pub static SHED_BY_BUDGET: AtomicU64 = AtomicU64::new(0);

/// Longest single fill pass, nanoseconds. A voice has ~62ms of lead time
/// (it asks for the next chunk 3/4 of the way through its head/chunk), so a
/// fill that takes longer than that is a gap somebody hears.
pub static FILL_PEAK_NS: AtomicU64 = AtomicU64::new(0);

/// One decoded chunk: interleaved 16-bit PCM.
///
/// `Vec` rather than a boxed slice because the chunk table stores these in
/// `ArcSwapOption`, and arc-swap only holds `Sized` payloads.
type Chunk = Arc<Vec<i16>>;

/// 64-bit words in the wanted-chunk bitmask — 512 chunks, over two minutes of
/// audio. Beyond that a sample plays from its head and whatever the linear
/// prefetch reaches.
const WANTED_WORDS: usize = 8;

/// Chunk slots, matching the reach of the `wanted` bitmask.
const MAX_CHUNKS: usize = WANTED_WORDS * 64;

/// A sample that lives compressed in a pack and is decoded a chunk at a time.
pub struct StreamedSample {
    /// The pack bytes and this entry's byte range within them.
    map: PackBytes,
    offset: usize,
    bytes: usize,
    index: FlacIndex,
    /// Interleaved i16, the first [`HEAD_FRAMES`] frames.
    head: Box<[i16]>,
    pub channels: u16,
    pub sample_rate: u32,
    pub num_frames: usize,
    /// Decoded chunks, one slot per chunk index, published for lock-free
    /// audio-thread reads.
    ///
    /// A slot table rather than a map, because `sample()` runs once per
    /// sample per voice: a `HashMap` made every one of those reads hash a
    /// key (16% of the audio thread's time, measured), and every chunk the
    /// streamer decoded cloned the WHOLE map to publish it — O(n) work that
    /// churned the readers' arc-swap debt exactly when a chord was starting
    /// voices. Indexing costs neither: a read is a bounds check, and a fill
    /// swaps one slot.
    chunks: Box<[ArcSwapOption<Vec<i16>>]>,
    /// Chunks the audio thread has asked for and the streamer hasn't filled,
    /// as a bitmask so the request path never takes a lock. A request that
    /// has to contend with the decoder is a request that gets dropped, and a
    /// dropped request is a hole in the audio at the next chunk boundary.
    wanted: [AtomicU64; WANTED_WORDS],
    /// Chunks this sample has decoded at least once, ever — for [`REDECODES`].
    ever: [AtomicU64; WANTED_WORDS],
    /// Set while this sample is queued with the streamer.
    queued: AtomicBool,
    /// Rolling counter used to evict the chunks nobody is reading.
    tick: AtomicU64,
    last_used: Mutex<HashMap<u32, u64>>,
    /// Sweep tick at which a voice last touched this sample. Chunks belonging
    /// to samples nobody is playing are dropped — otherwise "resident" would
    /// mean *ever played* instead of *playing*.
    last_touch: AtomicU64,
    /// Chunks a sounding voice depends on, by refcount. A looping voice reads
    /// the same region forever without ever *requesting* it, so without pins
    /// the eviction and the idle sweep both take that region away underneath
    /// it and the note goes silent. Pinned chunks are never evicted, never
    /// swept, and are fetched the moment the pin is taken.
    pins: Mutex<HashMap<u32, u32>>,
}

/// Keeps a region of a streamed sample resident for as long as it is held.
/// Dropped with the voice.
pub struct LoopPin {
    sample: Arc<StreamedSample>,
    chunks: Vec<u32>,
}

impl Drop for LoopPin {
    fn drop(&mut self) {
        if let Ok(mut pins) = self.sample.pins.lock() {
            for chunk in &self.chunks {
                if let Some(count) = pins.get_mut(chunk) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        pins.remove(chunk);
                    }
                }
            }
        }
    }
}

/// Most chunks one pin may hold — ~16 s of audio. A loop longer than that
/// falls back to ordinary prefetching for its tail.
const MAX_PINNED_CHUNKS: usize = 64;

impl std::fmt::Debug for StreamedSample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamedSample")
            .field("num_frames", &self.num_frames)
            .field("channels", &self.channels)
            .field("resident_chunks", &self.resident_chunk_count())
            .finish()
    }
}

impl StreamedSample {
    /// Build a streamed sample over a FLAC entry inside a mapped pack.
    ///
    /// Returns `None` when the entry can't be indexed (not FLAC, or a
    /// malformed header) — the caller decodes it whole instead.
    pub fn open(
        map: impl Into<PackBytes>,
        offset: usize,
        bytes: usize,
        channels: u16,
        sample_rate: u32,
        num_frames: usize,
    ) -> Option<Arc<Self>> {
        let map = map.into();
        // Streaming reads the compressed entry over and over; an External
        // pack (bytes outside this address space) cannot back that — its
        // entries are materialized as Owned bytes BEFORE streaming (see
        // `cache::load_pack_sample`). Refuse rather than trap on the deref.
        if map.is_external() {
            return None;
        }
        let stream = map.get(offset..offset + bytes)?;
        let index = FlacIndex::build(stream)?;
        let head = decode_chunk(stream, &index, 0, HEAD_FRAMES, channels)?;
        let sample = Arc::new(Self {
            map,
            offset,
            bytes,
            index,
            head: head.into_boxed_slice(),
            channels,
            sample_rate,
            num_frames,
            chunks: (0..MAX_CHUNKS)
                .map(|_| ArcSwapOption::empty())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            wanted: [const { AtomicU64::new(0) }; WANTED_WORDS],
            ever: [const { AtomicU64::new(0) }; WANTED_WORDS],
            queued: AtomicBool::new(false),
            tick: AtomicU64::new(0),
            last_used: Mutex::new(HashMap::new()),
            last_touch: AtomicU64::new(sweep_tick()),
            pins: Mutex::new(HashMap::new()),
        });
        #[cfg(feature = "engine-native")]
        streamer().register(Arc::downgrade(&sample));
        Some(sample)
    }

    /// Keep `from_frame..to_frame` resident until the returned pin drops.
    ///
    /// Called when a voice starts a loop: the wrap reads backwards, which no
    /// forward prefetch predicts, and the region is read over and over
    /// without generating a single request.
    pub fn pin(self: &Arc<Self>, from_frame: usize, to_frame: usize) -> LoopPin {
        let first = (from_frame as u32) / CHUNK_FRAMES;
        let last = (to_frame.max(from_frame) as u32) / CHUNK_FRAMES;
        let chunks: Vec<u32> = (first..=last).take(MAX_PINNED_CHUNKS).collect();
        if let Ok(mut pins) = self.pins.lock() {
            for chunk in &chunks {
                *pins.entry(*chunk).or_insert(0) += 1;
            }
        }
        // Fetch them now rather than when the voice arrives at them.
        for chunk in &chunks {
            self.request(*chunk);
        }
        LoopPin {
            sample: Arc::clone(self),
            chunks,
        }
    }

    /// Ask for the chunk at `frame` and the one after it, without pinning.
    /// A voice that starts anywhere but the head — a sample-start offset, a
    /// round-robin window, a legato entry point — would otherwise begin on a
    /// miss.
    pub fn prefetch_at(self: &Arc<Self>, frame: usize) {
        let chunk = (frame as u32) / CHUNK_FRAMES;
        self.request(chunk);
        self.request(chunk + 1);
    }

    /// Whether any voice is holding a region of this sample.
    #[cfg(feature = "engine-native")]
    fn is_pinned(&self) -> bool {
        self.pins.lock().map(|p| !p.is_empty()).unwrap_or(true)
    }

    /// Drop every decoded chunk except the pinned ones, keeping the head.
    /// Called by the sweep when no voice has touched this sample recently.
    #[cfg(feature = "engine-native")]
    fn shed_chunks(&self) {
        if self.resident_chunk_count() == 0 {
            return;
        }
        let pinned = self.pins.lock().map(|p| p.clone()).unwrap_or_default();
        if pinned.is_empty() {
            SHED_BY_SWEEP.fetch_add(self.resident_chunk_count() as u64, Ordering::Relaxed);
            for slot in self.chunks.iter() {
                slot.store(None);
            }
            if let Ok(mut used) = self.last_used.lock() {
                used.clear();
            }
            return;
        }
        for (i, slot) in self.chunks.iter().enumerate() {
            if !pinned.contains_key(&(i as u32)) {
                if slot.load().is_some() {
                    SHED_BY_SWEEP.fetch_add(1, Ordering::Relaxed);
                }
                slot.store(None);
            }
        }
    }

    /// Decode the WHOLE sample, here and now, as interleaved floats.
    ///
    /// [`sample`](Self::sample) answers 0.0 for a chunk that has not been
    /// fetched yet and merely requests it — the only correct behaviour on the
    /// audio thread, where blocking is a dropout. But an offline consumer
    /// (analysis, loudness, extraction, reference matching) reading through
    /// that path gets SILENCE wherever the streamer has not caught up, and
    /// gets different audio every run depending on timing. This is the path
    /// for those callers: it decodes every chunk synchronously, touches
    /// neither the resident set nor the request queue, and always returns the
    /// same samples.
    ///
    /// Never call it from a realtime context — it decodes the entire sample.
    pub fn decode_all(&self) -> Vec<f32> {
        const SCALE: f32 = 1.0 / 32768.0;
        let ch = self.channels.max(1) as usize;
        let total = self.num_frames * ch;
        let mut out = Vec::with_capacity(total);
        // The head is already decoded and always resident.
        out.extend(self.head.iter().map(|s| *s as f32 * SCALE));
        let Some(stream) = self.map.get(self.offset..self.offset + self.bytes) else {
            out.resize(total, 0.0);
            return out;
        };
        let mut frame = (out.len() / ch) as u32;
        while (out.len()) < total {
            let want = CHUNK_FRAMES.min(self.num_frames as u32 - frame);
            match decode_chunk(stream, &self.index, frame, want, self.channels) {
                Some(chunk) if !chunk.is_empty() => {
                    out.extend(chunk.iter().map(|s| *s as f32 * SCALE));
                    frame += (chunk.len() / ch) as u32;
                }
                // A chunk that will not decode ends the decode; the length
                // contract is still honoured below.
                _ => break,
            }
        }
        // ALWAYS `num_frames * channels` long. Callers index this by
        // `num_frames` — a short buffer panics them, which is exactly what a
        // late chunk that would not decode did.
        out.resize(total, 0.0);
        out
    }

    /// How many chunk slots are filled.
    fn resident_chunk_count(&self) -> usize {
        self.chunks.iter().filter(|s| s.load().is_some()).count()
    }

    /// Anonymous bytes this sample holds right now: its head plus whatever
    /// chunks are resident.
    pub fn resident_bytes(&self) -> usize {
        let chunks: usize = self
            .chunks
            .iter()
            .filter_map(|slot| slot.load_full())
            .map(|c| c.len() * 2)
            .sum();
        self.head.len() * 2 + chunks
    }

    /// One sample, converted to float. **Audio-thread safe**: never blocks,
    /// never decodes, never allocates on the hit path.
    #[inline]
    pub fn sample(self: &Arc<Self>, index: usize) -> f32 {
        const SCALE: f32 = 1.0 / 32768.0;
        if index < self.head.len() {
            // Near the end of the head, ask for the chunk that follows it.
            // The head exists to buy exactly this lead time: a voice that
            // walks off it must never find nothing there.
            if index >= self.head.len() - self.head.len() / 4 {
                let ch = self.channels.max(1) as usize;
                self.request((self.head.len() / ch) as u32 / CHUNK_FRAMES);
            }
            return self.head[index] as f32 * SCALE;
        }
        let ch = self.channels.max(1) as usize;
        let frame = index / ch;
        let chunk_no = frame as u32 / CHUNK_FRAMES;
        let within = (frame as u32 % CHUNK_FRAMES) as usize * ch + index % ch;
        // `load()` (a guard), never `load_full()`: this runs once per sample
        // per voice — tens of thousands of times a block — and `load_full`
        // would clone an `Arc` every one of them, i.e. an atomic
        // increment/decrement pair per audio sample.
        let slot = self.chunks.get(chunk_no as usize).map(|s| s.load());
        match slot.as_ref().and_then(|g| g.as_deref()) {
            Some(chunk) => {
                // Ask for the next one while there is still a chunk of audio
                // left to play — that lead time is what keeps the read path
                // from ever missing.
                if within + ch * 4_800 >= chunk.len() {
                    self.request(chunk_no + 1);
                }
                chunk.get(within).map(|s| *s as f32 * SCALE).unwrap_or(0.0)
            }
            None => {
                STREAM_MISSES.fetch_add(1, Ordering::Relaxed);
                self.request(chunk_no);
                0.0
            }
        }
    }

    /// Read up to `out.len()` consecutive samples starting at `index`,
    /// resolving the chunk ONCE for the whole run.
    ///
    /// The per-sample [`sample`](Self::sample) path costs an arc-swap guard
    /// load every call, and the voice interpolator asks for four of them per
    /// output frame (two frames x two channels). At a chord's worth of voices
    /// that was ~200k guard loads per block, contending with the streamer
    /// publishing chunks. A run shares one lookup, so the same read costs one.
    ///
    /// Returns how many it filled; the caller falls back to `sample()` for a
    /// remainder that crosses a chunk boundary (rare, and never in the hot
    /// middle of a chunk).
    pub fn run(self: &Arc<Self>, index: usize, out: &mut [f32]) -> usize {
        const SCALE: f32 = 1.0 / 32768.0;
        if out.is_empty() {
            return 0;
        }
        if index < self.head.len() {
            let n = out.len().min(self.head.len() - index);
            for (o, s) in out[..n].iter_mut().zip(&self.head[index..index + n]) {
                *o = *s as f32 * SCALE;
            }
            if index + n >= self.head.len() - self.head.len() / 4 {
                let ch = self.channels.max(1) as usize;
                self.request((self.head.len() / ch) as u32 / CHUNK_FRAMES);
            }
            return n;
        }
        let ch = self.channels.max(1) as usize;
        let frame = index / ch;
        let chunk_no = frame as u32 / CHUNK_FRAMES;
        let within = (frame as u32 % CHUNK_FRAMES) as usize * ch + index % ch;
        let slot = self.chunks.get(chunk_no as usize).map(|s| s.load());
        let Some(chunk) = slot.as_ref().and_then(|g| g.as_deref()) else {
            self.request(chunk_no);
            return 0;
        };
        if within >= chunk.len() {
            return 0;
        }
        let n = out.len().min(chunk.len() - within);
        for (o, s) in out[..n].iter_mut().zip(&chunk[within..within + n]) {
            *o = *s as f32 * SCALE;
        }
        if within + n + ch * 4_800 >= chunk.len() {
            self.request(chunk_no + 1);
        }
        n
    }

    /// The decoded chunk covering `index`, with the absolute sample-index
    /// range it spans.
    ///
    /// This is the door a [`StreamCursor`] uses, and the reason it exists:
    /// resolving a chunk costs an arc-swap load and an `Arc` clone, which is
    /// fine ONCE per chunk and ruinous once per sample. Returns `None` for
    /// the head (read directly, it is always resident) and for a chunk that
    /// has not arrived, requesting it on the way out.
    ///
    /// Also issues the read-ahead for the following chunk here — once per
    /// resolution rather than once per sample.
    pub fn chunk_at(self: &Arc<Self>, index: usize) -> Option<(Chunk, usize, usize)> {
        if index < self.head.len() {
            return None;
        }
        let ch = self.channels.max(1) as usize;
        let frame = index / ch;
        let chunk_no = frame as u32 / CHUNK_FRAMES;
        let Some(chunk) = self
            .chunks
            .get(chunk_no as usize)
            .and_then(|slot| slot.load_full())
        else {
            STREAM_MISSES.fetch_add(1, Ordering::Relaxed);
            // Was this chunk already on order? If not, no read-ahead ever
            // predicted this read and the voice was always going to gap.
            let (w, b) = (chunk_no as usize / 64, chunk_no as usize % 64);
            let already = self
                .wanted
                .get(w)
                .is_some_and(|word| word.load(Ordering::Acquire) & (1u64 << b) != 0);
            if already {
                MISS_PENDING.fetch_add(1, Ordering::Relaxed);
            } else {
                MISS_UNREQUESTED.fetch_add(1, Ordering::Relaxed);
            }
            self.request(chunk_no);
            return None;
        };
        let lo = chunk_no as usize * CHUNK_FRAMES as usize * ch;
        let hi = lo + chunk.len();
        if index + ch * 4_800 >= hi {
            self.request(chunk_no + 1);
        }
        STREAM_HITS.fetch_add(1, Ordering::Relaxed);
        Some((chunk, lo, hi))
    }

    /// Note the chunk as wanted and make sure the streamer knows about us.
    /// Lock-free: safe to call from the audio thread, and never dropped.
    fn request(self: &Arc<Self>, chunk_no: u32) {
        if chunk_no as usize * CHUNK_FRAMES as usize >= self.num_frames {
            return;
        }
        let (word, bit) = (chunk_no as usize / 64, chunk_no as usize % 64);
        if word >= WANTED_WORDS {
            return;
        }
        // FAST PATH — and it is the overwhelmingly common one. `sample()`
        // calls this once per audio sample for the last stretch of every
        // chunk, so with a chord's worth of voices it runs tens of thousands
        // of times a block. Doing the three read-modify-writes below every
        // time put three contended atomics per sample on lines the streamer
        // thread is also touching, and the resulting cache-line ping-pong
        // cost 200-600 ms of callback time per chord against a 5.33 ms
        // budget — the keys rig's glitch, measured.
        //
        // Once a chunk is wanted and the streamer knows about us, asking
        // again changes nothing, and that question can be answered with
        // plain loads: shared reads leave every core's copy valid.
        let mask = 1u64 << bit;
        let already_wanted = self.wanted[word].load(Ordering::Acquire) & mask != 0;
        #[cfg(feature = "engine-native")]
        let known = already_wanted && self.queued.load(Ordering::Acquire);
        #[cfg(not(feature = "engine-native"))]
        let known = already_wanted;
        if known {
            // Keep the sample off the eviction sweep, but only WRITE when the
            // tick actually moved — an unconditional store would invalidate
            // the line on every sample and undo the point of this path.
            let tick = sweep_tick();
            if self.last_touch.load(Ordering::Relaxed) != tick {
                self.last_touch.store(tick, Ordering::Relaxed);
            }
            return;
        }

        self.last_touch.store(sweep_tick(), Ordering::Relaxed);
        self.wanted[word].fetch_or(mask, Ordering::Release);
        #[cfg(feature = "engine-native")]
        if !self.queued.swap(true, Ordering::AcqRel) {
            streamer().enqueue(Arc::downgrade(self));
        }
        // Without the native half (wasm) there is no streamer THREAD — and
        // in an AudioWorklet the caller of this is the audio thread itself,
        // where a synchronous ogg decode is a dropout: it stalls `process()`
        // for tens of ms and silences every voice currently sounding. That
        // was the field bug ("goes silent every time I play a new note").
        //
        // With shared memory + atomics (W13) there ARE workers, sharing this
        // very heap, so the request behaves exactly like the native one:
        // hand the sample to the streamer queue and return. A CAS and a
        // notify — no allocation, no blocking, no decode.
        //
        // The `queued` guard is NOT optional here, for the same reason the
        // native arm has it: `request` is called from `sample()` on EVERY
        // read that misses a chunk — thousands of times per block per
        // voice. Enqueueing each one buried the workers under duplicate
        // jobs (and allocated an `Arc` clone per call), which wedged the
        // whole tab. One job per sample until a worker drains it; `fill`
        // clears the flag and re-arms if anything is still wanted.
        #[cfg(all(
            not(feature = "engine-native"),
            target_arch = "wasm32",
            target_feature = "atomics"
        ))]
        if !self.queued.swap(true, Ordering::AcqRel) {
            crate::stream_wasm::enqueue(self);
        }
        // No workers (single-threaded wasm build): the wanted set is
        // recorded and nothing decodes here. A voice that outruns its head
        // reads silence for those frames rather than taking the whole
        // render down with it; the host fills it off-thread by other means
        // (the W12 decoder-worker protocol).
    }

    /// Decode everything currently wanted ON THIS THREAD. The wasm hosts'
    /// off-thread seam: safe from a Worker (never from the audio thread).
    /// Native builds have the streamer pool and never need it.
    #[cfg(not(feature = "engine-native"))]
    pub fn fill_wanted_off_thread(self: &Arc<Self>) {
        self.fill();
    }

    /// Take the wanted set, clearing it.
    fn take_wanted(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for (w, word) in self.wanted.iter().enumerate() {
            let mut bits = word.swap(0, Ordering::Acquire);
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                out.push((w * 64 + bit) as u32);
            }
        }
        out
    }

    /// Anything still asked for after a fill?
    #[cfg(feature = "engine-native")]
    fn has_wanted(&self) -> bool {
        self.wanted.iter().any(|w| w.load(Ordering::Acquire) != 0)
    }

    /// Decode everything the audio thread has asked for. Runs on the streamer
    /// thread.
    fn fill(self: &Arc<Self>) {
        let fill_started = std::time::Instant::now();
        FILLS.fetch_add(1, Ordering::Relaxed);
        let wanted = self.take_wanted();
        if wanted.is_empty() {
            self.queued.store(false, Ordering::Release);
            return;
        }
        let Some(stream) = self.map.get(self.offset..self.offset + self.bytes) else {
            tracing::warn!(
                offset = self.offset,
                bytes = self.bytes,
                "stream: entry out of range"
            );
            return;
        };
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        let mut decoded_here = 0u64;
        for chunk_no in wanted {
            let Some(slot) = self.chunks.get(chunk_no as usize) else {
                continue;
            };
            if slot.load().is_some() {
                continue;
            }
            let from = chunk_no * CHUNK_FRAMES;
            let decode_started = std::time::Instant::now();
            let pcm = match decode_chunk_why(stream, &self.index, from, CHUNK_FRAMES, self.channels) {
                Ok(pcm) => pcm,
                Err(why) => {
                    tracing::warn!(
                        chunk_no,
                        from,
                        num_frames = self.num_frames,
                        offset = self.offset,
                        bytes = self.bytes,
                        "stream: chunk decode failed: {why}"
                    );
                    continue;
                }
            };
            DECODE_NS.fetch_add(
                decode_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
                Ordering::Relaxed,
            );
            CHUNKS_DECODED.fetch_add(1, Ordering::Relaxed);
            decoded_here += 1;
            {
                let (w, b) = (chunk_no as usize / 64, chunk_no as usize % 64);
                if let Some(word) = self.ever.get(w) {
                    let mask = 1u64 << b;
                    if word.fetch_or(mask, Ordering::Relaxed) & mask != 0 {
                        REDECODES.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            // Publishing is a single slot swap, so a voice reading a
            // DIFFERENT chunk of this sample is never disturbed by the fill.
            slot.store(Some(Arc::new(pcm)));
            if let Ok(mut used) = self.last_used.lock() {
                used.insert(chunk_no, tick);
            }
        }
        // Evict the chunks nobody has touched lately. Voices hold their own
        // `Arc` to a chunk only for the length of one read, so dropping here
        // frees memory promptly without ever cutting audio short.
        let pinned = self.pins.lock().map(|p| p.clone()).unwrap_or_default();
        let budget = MAX_RESIDENT_CHUNKS + pinned.len();
        let resident = self.resident_chunk_count();
        if resident > budget {
            if let Ok(used) = self.last_used.lock() {
                let mut by_age: Vec<(u32, u64)> = self
                    .chunks
                    .iter()
                    .enumerate()
                    .filter(|(i, slot)| slot.load().is_some() && !pinned.contains_key(&(*i as u32)))
                    .map(|(i, _)| (i as u32, used.get(&(i as u32)).copied().unwrap_or(0)))
                    .collect();
                by_age.sort_by_key(|(_, t)| *t);
                for (chunk_no, _) in by_age.iter().take(resident - budget) {
                    self.chunks[*chunk_no as usize].store(None);
                    SHED_BY_BUDGET.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        // Requests that arrived while this fill was running must not be
        // stranded: clear the flag first, then re-queue if anything is
        // outstanding. The other order loses whatever landed in between.
        let fill_ns = fill_started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        FILL_PEAK_NS.fetch_max(fill_ns, Ordering::Relaxed);
        // One event per fill, with the numbers that decide whether a voice
        // gaps: how many chunks this pass owed, and how long the caller had
        // to wait for them. A voice asks for its next chunk ~62ms before it
        // needs it, so `fill_ms` above that is a hole in somebody's output.
        tracing::debug!(
            target: "fts_sample::stream",
            chunks = decoded_here,
            fill_ms = fill_ns as f64 / 1.0e6,
            "stream fill"
        );
        self.queued.store(false, Ordering::Release);
        #[cfg(feature = "engine-native")]
        if self.has_wanted() && !self.queued.swap(true, Ordering::AcqRel) {
            streamer().enqueue(Arc::downgrade(self));
        }
    }
}

/// Chunks any one sample may hold — the one playing, the one prefetched, and
/// slack for re-triggers and loop jumps. At 48 KB a chunk that caps a
/// sounding sample at ~290 KB; a sample that is merely loaded holds only its
/// head.
const MAX_RESIDENT_CHUNKS: usize = 24;

/// Decode `frames` starting at `from_frame` out of a FLAC stream, as
/// interleaved i16.
fn decode_chunk(
    stream: &[u8],
    index: &FlacIndex,
    from_frame: u32,
    frames: u32,
    channels: u16,
) -> Option<Vec<i16>> {
    decode_chunk_why(stream, index, from_frame, frames, channels).ok()
}

/// [`decode_chunk`], saying which step failed — the streamer logs it.
fn decode_chunk_why(
    stream: &[u8],
    index: &FlacIndex,
    from_frame: u32,
    frames: u32,
    channels: u16,
) -> Result<Vec<i16>, String> {
    let (bytes, starts_at) = index
        .chunk(stream, from_frame, frames)
        .ok_or_else(|| format!("no frame covers it (stream {} bytes, {} total frames)", stream.len(), index.total_frames()))?;
    let n_bytes = bytes.len();
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("FLAC header: {e} ({n_bytes} bytes from frame {starts_at})"))?;
    let info = reader.streaminfo();
    let shift = info.bits_per_sample as i32 - 16;
    let ch = channels.max(1) as usize;
    // The chunk starts at a frame boundary at or before what was asked for.
    let skip = (from_frame.saturating_sub(starts_at) as usize) * ch;
    let want = frames as usize * ch;
    let mut out = Vec::with_capacity(want);
    for (i, s) in reader.samples().enumerate() {
        if i < skip {
            continue;
        }
        let v = s.map_err(|e| {
            format!("sample {i}: {e} ({n_bytes} bytes from frame {starts_at}, wanted {from_frame})")
        })?;
        let v = if shift > 0 { v >> shift } else { v << (-shift) };
        out.push(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        if out.len() >= want {
            break;
        }
    }
    Ok(out)
}

// ── The streamer thread ─────────────────────────────────────────────────────

/// Background decoder: the only place a compressed chunk is ever decoded.
/// Native only — under bare `engine-core` (wasm) chunks decode synchronously
/// in [`StreamedSample::request`].
#[cfg(feature = "engine-native")]
struct Streamer {
    queue: Mutex<Vec<Weak<StreamedSample>>>,
    wake: Condvar,
    /// Every live streamed sample, for the idle sweep.
    registry: Mutex<Vec<Weak<StreamedSample>>>,
}

/// Ticks the idle sweep counts in. One tick per [`SWEEP`].
static SWEEP_TICK: AtomicU64 = AtomicU64::new(0);
/// How often the sweep runs. Chunks survive at least one full sweep after
/// their last read, so a held note is never interrupted.
#[cfg(feature = "engine-native")]
const SWEEP: std::time::Duration = std::time::Duration::from_secs(2);

fn sweep_tick() -> u64 {
    SWEEP_TICK.load(Ordering::Relaxed)
}

#[cfg(feature = "engine-native")]
fn streamer() -> &'static Streamer {
    static STREAMER: OnceLock<Streamer> = OnceLock::new();
    STREAMER.get_or_init(|| {
        let s = Streamer {
            queue: Mutex::new(Vec::new()),
            wake: Condvar::new(),
            registry: Mutex::new(Vec::new()),
        };
        // A pool, not a thread: one decoder is fine until the machine is
        // busy, and then it is the difference between a held note and a
        // dropout. Two to four workers drain the same queue.
        let workers = std::thread::available_parallelism()
            .map(|n| (n.get() / 4).clamp(2, 4))
            .unwrap_or(2);
        for i in 0..workers {
            let _ = std::thread::Builder::new()
                .name(format!("signal-streamer-{i}"))
                .spawn(|| streamer().run());
        }
        s
    })
}

#[cfg(feature = "engine-native")]
impl Streamer {
    fn register(&self, sample: Weak<StreamedSample>) {
        if let Ok(mut r) = self.registry.lock() {
            r.retain(|w| w.strong_count() > 0);
            r.push(sample);
        }
    }

    /// Drop chunks from samples nobody has read since the last sweep, and
    /// forget samples that have been freed.
    fn sweep(&self) {
        let now = SWEEP_TICK.fetch_add(1, Ordering::Relaxed) + 1;
        let Ok(mut registry) = self.registry.lock() else {
            return;
        };
        registry.retain(|weak| {
            let Some(sample) = weak.upgrade() else {
                return false;
            };
            let idle = now.saturating_sub(sample.last_touch.load(Ordering::Relaxed)) >= 2;
            // A pinned sample is being played right now, however quiet its
            // request traffic is.
            if idle && !sample.is_pinned() {
                sample.shed_chunks();
            }
            true
        });
    }

    fn enqueue(&self, sample: Weak<StreamedSample>) {
        if let Ok(mut q) = self.queue.lock() {
            q.push(sample);
            self.wake.notify_one();
        }
    }

    fn run(&self) -> ! {
        loop {
            let batch = {
                let Ok(mut q) = self.queue.lock() else {
                    continue;
                };
                while q.is_empty() {
                    let Ok((next, timeout)) = self.wake.wait_timeout(q, SWEEP) else {
                        return_never()
                    };
                    q = next;
                    if timeout.timed_out() {
                        break;
                    }
                }
                std::mem::take(&mut *q)
            };
            if batch.is_empty() {
                // Whichever worker wakes on the timeout does the sweep.
                self.sweep();
                continue;
            }
            for weak in batch {
                if let Some(sample) = weak.upgrade() {
                    sample.fill();
                }
            }
            // NO sweep here. The sweep's clock is what decides "nobody has
            // read this lately", so it has to advance with TIME — ticking it
            // once per batch made it advance every few milliseconds, and a
            // chunk was shed almost as soon as it arrived. That is a note
            // that plays its head and then fizzles out.
            //
            // Only the idle timeout below ticks it.
        }
    }
}

/// A poisoned streamer queue means the process is already unwinding; parking
/// forever is better than spinning on a lock nobody will release.
#[cfg(feature = "engine-native")]
fn return_never() -> ! {
    loop {
        std::thread::park();
    }
}

#[cfg(all(test, feature = "engine-native"))]
mod tests {
    use super::*;

    /// Both streaming tests share one background streamer, so running them
    /// side by side makes fill latency (not correctness) the thing under
    /// test. Serialise them.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// A bulk read of a streamed sample must be COMPLETE and the same every
    /// time — the per-sample path answers 0.0 for a chunk that has not been
    /// fetched yet, which made `to_f32()` return silence wherever streaming
    /// had not caught up, and different audio on every run.
    #[test]
    fn decode_all_is_complete_and_deterministic() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let (sr, ch, secs) = (48_000u32, 2u16, 4.0f64);
        let n = (sr as f64 * secs) as usize;
        let pcm: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = (t * 220.0 * std::f64::consts::TAU).sin() as f32 * 0.8;
                [v, -v]
            })
            .collect();
        let ints: Vec<i32> = pcm
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
            .collect();
        let Ok(flac) = crate::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };
        let tmp = std::env::temp_dir().join(format!("fts-decode-all-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));
        let s =
            StreamedSample::open(Arc::clone(&map), 0, flac.len(), ch, sr, n).expect("index + head");

        // Nothing beyond the head has been fetched, which is exactly the
        // state that used to yield silence.
        assert_eq!(s.resident_chunk_count(), 0, "expected a cold sample");
        let all = s.decode_all();
        assert_eq!(
            all.len(),
            n * ch as usize,
            "the length contract is absolute: callers index this by num_frames"
        );

        // Correct well past the head, where the streamer had fetched nothing.
        let far = (HEAD_FRAMES as usize + CHUNK_FRAMES as usize + 500) * ch as usize;
        assert!(
            (all[far] - pcm[far]).abs() < 0.01,
            "far sample {far}: {} vs {}",
            all[far],
            pcm[far]
        );
        let tail = n * ch as usize - 4;
        assert!(
            (all[tail] - pcm[tail]).abs() < 0.01,
            "tail sample: {} vs {}",
            all[tail],
            pcm[tail]
        );
        // Same answer regardless of what streaming has done in between.
        assert_eq!(all, s.decode_all(), "bulk decode must be deterministic");
        let _ = std::fs::remove_file(&tmp);
    }

    /// A streamed sample must read back the same audio as decoding the whole
    /// file — head, chunk boundaries and all — while holding only its head.
    #[test]
    fn streams_a_flac_entry_chunk_by_chunk() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // A few seconds of tone, encoded the way the packer encodes.
        let (sr, ch, secs) = (48_000u32, 2u16, 4.0f64);
        let n = (sr as f64 * secs) as usize;
        let pcm: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = (t * 220.0 * std::f64::consts::TAU).sin() as f32 * 0.8;
                [v, -v]
            })
            .collect();
        let ints: Vec<i32> = pcm
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32)
            .collect();
        let Ok(flac) = crate::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };

        let tmp = std::env::temp_dir().join(format!("fts-stream-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));

        let s =
            StreamedSample::open(Arc::clone(&map), 0, flac.len(), ch, sr, n).expect("index + head");
        // Only the head is resident to begin with.
        assert!(s.resident_bytes() <= (HEAD_FRAMES as usize * ch as usize * 2) + 64);

        // Reading inside the head works immediately.
        for i in [0usize, 100, 1_000] {
            let got = s.sample(i);
            assert!(
                (got - pcm[i]).abs() < 0.01,
                "head sample {i}: {got} vs {}",
                pcm[i]
            );
        }

        // Past the head: the first read misses, the streamer fills, the
        // second read is correct. (A voice hears one silent block; that is
        // the same trade every streaming sampler makes on an unseen seek.)
        let far = (HEAD_FRAMES as usize + CHUNK_FRAMES as usize + 500) * ch as usize;
        let _ = s.sample(far);
        for _ in 0..200 {
            if s.resident_chunk_count() > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let got = s.sample(far);
        assert!(
            (got - pcm[far]).abs() < 0.01,
            "streamed sample {far}: {got} vs {}",
            pcm[far]
        );
        // Still nowhere near the whole file: head + a couple of chunks.
        let whole = n * ch as usize * 4;
        assert!(
            s.resident_bytes() < whole / 4,
            "resident {} of {whole} bytes",
            s.resident_bytes()
        );

        // Now walk the WHOLE sample the way a voice does, waiting for the
        // streamer the way an audio thread cannot. Every sample must match
        // the source: chunk boundaries, prefetch and eviction included.
        // In RUNS, the way a voice reads.
        //
        // This used to walk sample by sample and, on every miss, sleep
        // 2ms and re-read — up to 200 times, per sample. That is one
        // wait per SAMPLE where the streamer publishes one CHUNK at a
        // time, and it took 131 seconds: long enough that nextest's
        // slow timeout killed it, so the streaming path this covers was
        // never actually reported on. `run` resolves the chunk once for
        // the whole run, which is both the API a voice uses and one
        // wait per chunk instead of 24,000. See session#86.
        let total = n * ch as usize;
        let mut worst = 0.0f32;
        let mut i = 0usize;
        let mut out = vec![0.0f32; CHUNK_FRAMES as usize * ch as usize];
        while i < total {
            let want = out.len().min(total - i);
            let Some(buf) = out.get_mut(..want) else {
                break;
            };
            let mut filled = s.run(i, buf);
            if filled == 0 {
                // The chunk is not published yet. `run` has asked for
                // it; wait for it once.
                for _ in 0..500 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    let Some(buf) = out.get_mut(..want) else {
                        break;
                    };
                    filled = s.run(i, buf);
                    if filled > 0 {
                        break;
                    }
                }
            }
            assert!(filled > 0, "the streamer never filled the chunk at {i}");
            for (k, got) in out.iter().take(filled).enumerate() {
                let expected = pcm.get(i + k).copied().unwrap_or(0.0);
                worst = worst.max((got - expected).abs());
            }
            i += filled;
        }
        assert!(worst < 0.01, "worst streamed error {worst}");
        // And it never grew past its working set while doing it: the
        // residency budget (chunks), plus the head — a fixed cap however long
        // the sample, not a share of it (at a budget of 24 a 1.5 MB sample is
        // half resident, a 100 MB one about 1%).
        let cap = (MAX_RESIDENT_CHUNKS as usize * CHUNK_FRAMES as usize + HEAD_FRAMES as usize)
            * ch as usize
            * std::mem::size_of::<f32>();
        assert!(
            s.resident_bytes() <= cap,
            "resident {} bytes after a full pass, over the {cap}-byte working set",
            s.resident_bytes()
        );
        let _ = std::fs::remove_file(&tmp);
    }

    // NOTE: the "held looping voice over a streamed sample" test lives in
    // signal-sampler's `engine::voice` tests — it needs the `Voice` types,
    // which stayed behind when this module moved into fts-sample.
}

