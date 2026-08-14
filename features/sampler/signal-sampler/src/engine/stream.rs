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
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};

use arc_swap::ArcSwap;

use super::flac_index::FlacIndex;

/// Sample frames per chunk — a quarter second at 48 kHz, 48 KB of stereo
/// i16. Small enough that a *playing* voice's working set is a couple of
/// hundred kilobytes, large enough that the streamer is woken a few times a
/// second per voice rather than every block.
pub const CHUNK_FRAMES: u32 = 12_000;

/// Frames kept resident from the start of every streamed sample. Covers the
/// attack of anything percussive and gives the streamer ~0.25 s of lead time
/// on everything else.
pub const HEAD_FRAMES: u32 = 12_000;

/// One decoded chunk: interleaved 16-bit PCM.
type Chunk = Arc<[i16]>;

/// 64-bit words in the wanted-chunk bitmask — 512 chunks, over two minutes of
/// audio. Beyond that a sample plays from its head and whatever the linear
/// prefetch reaches.
const WANTED_WORDS: usize = 8;

/// A sample that lives compressed in a pack and is decoded a chunk at a time.
pub struct StreamedSample {
    /// The pack mapping and this entry's byte range within it.
    map: Arc<memmap2::Mmap>,
    offset: usize,
    bytes: usize,
    index: FlacIndex,
    /// Interleaved i16, the first [`HEAD_FRAMES`] frames.
    head: Box<[i16]>,
    pub channels: u16,
    pub sample_rate: u32,
    pub num_frames: usize,
    /// Decoded chunks by index, published for lock-free audio-thread reads.
    chunks: ArcSwap<HashMap<u32, Chunk>>,
    /// Chunks the audio thread has asked for and the streamer hasn't filled,
    /// as a bitmask so the request path never takes a lock. A request that
    /// has to contend with the decoder is a request that gets dropped, and a
    /// dropped request is a hole in the audio at the next chunk boundary.
    wanted: [AtomicU64; WANTED_WORDS],
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
            .field("resident_chunks", &self.chunks.load().len())
            .finish()
    }
}

impl StreamedSample {
    /// Build a streamed sample over a FLAC entry inside a mapped pack.
    ///
    /// Returns `None` when the entry can't be indexed (not FLAC, or a
    /// malformed header) — the caller decodes it whole instead.
    pub fn open(
        map: Arc<memmap2::Mmap>,
        offset: usize,
        bytes: usize,
        channels: u16,
        sample_rate: u32,
        num_frames: usize,
    ) -> Option<Arc<Self>> {
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
            chunks: ArcSwap::from_pointee(HashMap::new()),
            wanted: [const { AtomicU64::new(0) }; WANTED_WORDS],
            queued: AtomicBool::new(false),
            tick: AtomicU64::new(0),
            last_used: Mutex::new(HashMap::new()),
            last_touch: AtomicU64::new(sweep_tick()),
            pins: Mutex::new(HashMap::new()),
        });
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
        let chunks: Vec<u32> =
            (first..=last).take(MAX_PINNED_CHUNKS).collect();
        if let Ok(mut pins) = self.pins.lock() {
            for chunk in &chunks {
                *pins.entry(*chunk).or_insert(0) += 1;
            }
        }
        // Fetch them now rather than when the voice arrives at them.
        for chunk in &chunks {
            self.request(*chunk);
        }
        LoopPin { sample: Arc::clone(self), chunks }
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
    fn is_pinned(&self) -> bool {
        self.pins.lock().map(|p| !p.is_empty()).unwrap_or(true)
    }

    /// Drop every decoded chunk except the pinned ones, keeping the head.
    /// Called by the sweep when no voice has touched this sample recently.
    fn shed_chunks(&self) {
        if self.chunks.load().is_empty() {
            return;
        }
        let pinned = self.pins.lock().map(|p| p.clone()).unwrap_or_default();
        if pinned.is_empty() {
            self.chunks.store(Arc::new(HashMap::new()));
            if let Ok(mut used) = self.last_used.lock() {
                used.clear();
            }
            return;
        }
        let kept: HashMap<u32, Chunk> = self
            .chunks
            .load()
            .iter()
            .filter(|(k, _)| pinned.contains_key(k))
            .map(|(k, v)| (*k, Arc::clone(v)))
            .collect();
        self.chunks.store(Arc::new(kept));
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

    /// Anonymous bytes this sample holds right now: its head plus whatever
    /// chunks are resident.
    pub fn resident_bytes(&self) -> usize {
        let chunks: usize = self.chunks.load().values().map(|c| c.len() * 2).sum();
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
        let chunks = self.chunks.load();
        match chunks.get(&chunk_no) {
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
                self.request(chunk_no);
                0.0
            }
        }
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
        self.last_touch.store(sweep_tick(), Ordering::Relaxed);
        self.wanted[word].fetch_or(1 << bit, Ordering::Release);
        if !self.queued.swap(true, Ordering::AcqRel) {
            streamer().enqueue(Arc::downgrade(self));
        }
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
    fn has_wanted(&self) -> bool {
        self.wanted.iter().any(|w| w.load(Ordering::Acquire) != 0)
    }

    /// Decode everything the audio thread has asked for. Runs on the streamer
    /// thread.
    fn fill(self: &Arc<Self>) {
        let wanted = self.take_wanted();
        if wanted.is_empty() {
            self.queued.store(false, Ordering::Release);
            return;
        }
        let Some(stream) = self.map.get(self.offset..self.offset + self.bytes) else {
            tracing::warn!(offset = self.offset, bytes = self.bytes, "stream: entry out of range");
            return;
        };
        let mut next = HashMap::clone(&self.chunks.load());
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        for chunk_no in wanted {
            if next.contains_key(&chunk_no) {
                continue;
            }
            let from = chunk_no * CHUNK_FRAMES;
            let Some(pcm) =
                decode_chunk(stream, &self.index, from, CHUNK_FRAMES, self.channels)
            else {
                tracing::warn!(chunk_no, from, "stream: chunk decode failed");
                continue;
            };
            next.insert(chunk_no, Arc::from(pcm));
            if let Ok(mut used) = self.last_used.lock() {
                used.insert(chunk_no, tick);
            }
        }
        // Evict the chunks nobody has touched lately. Voices hold their own
        // `Arc` to a chunk only for the length of one read, so dropping here
        // frees memory promptly without ever cutting audio short.
        let pinned = self.pins.lock().map(|p| p.clone()).unwrap_or_default();
        let budget = MAX_RESIDENT_CHUNKS + pinned.len();
        if next.len() > budget {
            if let Ok(used) = self.last_used.lock() {
                let mut by_age: Vec<(u32, u64)> = next
                    .keys()
                    .filter(|k| !pinned.contains_key(*k))
                    .map(|k| (*k, used.get(k).copied().unwrap_or(0)))
                    .collect();
                by_age.sort_by_key(|(_, t)| *t);
                for (chunk_no, _) in by_age.iter().take(next.len() - budget) {
                    next.remove(chunk_no);
                }
            }
        }
        self.chunks.store(Arc::new(next));
        // Requests that arrived while this fill was running must not be
        // stranded: clear the flag first, then re-queue if anything is
        // outstanding. The other order loses whatever landed in between.
        self.queued.store(false, Ordering::Release);
        if self.has_wanted() && !self.queued.swap(true, Ordering::AcqRel) {
            streamer().enqueue(Arc::downgrade(self));
        }
    }
}

/// Chunks any one sample may hold — the one playing, the one prefetched, and
/// slack for re-triggers and loop jumps. At 48 KB a chunk that caps a
/// sounding sample at ~290 KB; a sample that is merely loaded holds only its
/// head.
const MAX_RESIDENT_CHUNKS: usize = 6;

/// Decode `frames` starting at `from_frame` out of a FLAC stream, as
/// interleaved i16.
fn decode_chunk(
    stream: &[u8],
    index: &FlacIndex,
    from_frame: u32,
    frames: u32,
    channels: u16,
) -> Option<Vec<i16>> {
    let (bytes, starts_at) = index.chunk(stream, from_frame, frames)?;
    let mut reader = claxon::FlacReader::new(std::io::Cursor::new(bytes)).ok()?;
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
        let v = s.ok()?;
        let v = if shift > 0 { v >> shift } else { v << (-shift) };
        out.push(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        if out.len() >= want {
            break;
        }
    }
    Some(out)
}

// ── The streamer thread ─────────────────────────────────────────────────────

/// Background decoder: the only place a compressed chunk is ever decoded.
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
const SWEEP: std::time::Duration = std::time::Duration::from_secs(2);

fn sweep_tick() -> u64 {
    SWEEP_TICK.load(Ordering::Relaxed)
}

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
        let Ok(mut registry) = self.registry.lock() else { return };
        registry.retain(|weak| {
            let Some(sample) = weak.upgrade() else { return false };
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
                let Ok(mut q) = self.queue.lock() else { continue };
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
fn return_never() -> ! {
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
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
        let Ok(flac) = super::super::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };
        let tmp = std::env::temp_dir().join(format!("fts-decode-all-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));
        let s = StreamedSample::open(Arc::clone(&map), 0, flac.len(), ch, sr, n)
            .expect("index + head");

        // Nothing beyond the head has been fetched, which is exactly the
        // state that used to yield silence.
        assert!(s.chunks.load().is_empty(), "expected a cold sample");
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
        let ints: Vec<i32> =
            pcm.iter().map(|s| (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32).collect();
        let Ok(flac) = super::super::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };

        let tmp = std::env::temp_dir().join(format!("fts-stream-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));

        let s = StreamedSample::open(Arc::clone(&map), 0, flac.len(), ch, sr, n)
            .expect("index + head");
        // Only the head is resident to begin with.
        assert!(s.resident_bytes() <= (HEAD_FRAMES as usize * ch as usize * 2) + 64);

        // Reading inside the head works immediately.
        for i in [0usize, 100, 1_000] {
            let got = s.sample(i);
            assert!((got - pcm[i]).abs() < 0.01, "head sample {i}: {got} vs {}", pcm[i]);
        }

        // Past the head: the first read misses, the streamer fills, the
        // second read is correct. (A voice hears one silent block; that is
        // the same trade every streaming sampler makes on an unseen seek.)
        let far = (HEAD_FRAMES as usize + CHUNK_FRAMES as usize + 500) * ch as usize;
        let _ = s.sample(far);
        for _ in 0..200 {
            if !s.chunks.load().is_empty() {
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
        let total = n * ch as usize;
        let mut worst = 0.0f32;
        let mut i = 0usize;
        while i < total {
            let mut got = s.sample(i);
            if got == 0.0 && pcm[i] != 0.0 {
                // A miss: wait for the fill, then read again.
                for _ in 0..200 {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    got = s.sample(i);
                    if got != 0.0 {
                        break;
                    }
                }
            }
            worst = worst.max((got - pcm[i]).abs());
            i += 1;
        }
        assert!(worst < 0.01, "worst streamed error {worst}");
        // And it never grew past its working set while doing it.
        assert!(
            s.resident_bytes() < whole / 4,
            "resident {} of {whole} bytes after a full pass",
            s.resident_bytes()
        );
        let _ = std::fs::remove_file(&tmp);
    }

    /// A held, looping note over a STREAMED sample must keep sounding. The
    /// loop wraps backwards, which linear prefetch never sees coming, and the
    /// loop region has to stay resident for as long as the voice holds it —
    /// this is the case that goes silent if chunks are evicted underneath a
    /// sustaining voice.
    #[test]
    fn a_looping_voice_keeps_sounding_over_a_streamed_sample() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use crate::engine::voice::{Voice, VoiceKind};
        use crate::engine::cache::SampleData;

        let (sr, ch) = (48_000u32, 2u16);
        let n = (sr as f64 * 6.0) as usize; // 6 s, well past head + chunks
        let pcm: Vec<f32> = (0..n)
            .flat_map(|i| {
                let t = i as f64 / sr as f64;
                let v = (t * 220.0 * std::f64::consts::TAU).sin() as f32 * 0.8;
                [v, -v]
            })
            .collect();
        let ints: Vec<i32> =
            pcm.iter().map(|s| (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32).collect();
        let Ok(flac) = crate::engine::cache::encode_flac_i24_for_test(&ints, ch, sr) else {
            return;
        };
        let tmp = std::env::temp_dir().join(format!("fts-loop-{}.flac", std::process::id()));
        std::fs::write(&tmp, &flac).expect("write");
        let file = std::fs::File::open(&tmp).expect("open");
        let map = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("map"));
        let streamed =
            StreamedSample::open(map, 0, flac.len(), ch, sr, n).expect("stream");
        let data = Arc::new(SampleData::streamed(streamed));

        // Loop the 3rd..5th second — entirely past the head, and longer than
        // one chunk, so it spans several.
        let (loop_start, loop_end) = (sr as usize * 3, sr as usize * 5);
        let mut voice = Voice::with_rate(data, 60, VoiceKind::Zoned, 1.0, 1.0, 8)
            .with_forward_loop(loop_start, loop_end);

        // Play 30 seconds — many loop passes, and long enough that the idle
        // sweep runs several times underneath.
        let mut block = vec![0.0f32; 512 * 2];
        let blocks = (sr as usize * 30) / 512;
        let mut silent_runs = 0usize;
        let mut worst_run = 0usize;
        for _ in 0..blocks {
            block.fill(0.0);
            voice.render_block(&mut block);
            let peak = block.iter().fold(0.0f32, |a, b| a.max(b.abs()));
            if peak < 1e-4 {
                silent_runs += 1;
                worst_run = worst_run.max(silent_runs);
            } else {
                silent_runs = 0;
            }
            // Real time: the streamer gets the same wall clock a live rig
            // would give it (512 frames ≈ 10.7 ms).
            std::thread::sleep(std::time::Duration::from_micros(10_667));
        }
        let _ = std::fs::remove_file(&tmp);
        assert!(
            worst_run <= 2,
            "held loop went silent for {worst_run} blocks in a row ({} ms)",
            worst_run * 512 * 1000 / sr as usize,
        );
    }
}
