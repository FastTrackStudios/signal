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
    /// Chunks the audio thread has asked for and the streamer hasn't filled.
    wanted: Mutex<Vec<u32>>,
    /// Set while this sample is queued with the streamer.
    queued: AtomicBool,
    /// Rolling counter used to evict the chunks nobody is reading.
    tick: AtomicU64,
    last_used: Mutex<HashMap<u32, u64>>,
    /// Sweep tick at which a voice last touched this sample. Chunks belonging
    /// to samples nobody is playing are dropped — otherwise "resident" would
    /// mean *ever played* instead of *playing*.
    last_touch: AtomicU64,
}

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
            wanted: Mutex::new(Vec::new()),
            queued: AtomicBool::new(false),
            tick: AtomicU64::new(0),
            last_used: Mutex::new(HashMap::new()),
            last_touch: AtomicU64::new(sweep_tick()),
        });
        streamer().register(Arc::downgrade(&sample));
        Some(sample)
    }

    /// Drop every decoded chunk, keeping the head. Called by the sweep when
    /// no voice has touched this sample recently.
    fn shed_chunks(&self) {
        if self.chunks.load().is_empty() {
            return;
        }
        self.chunks.store(Arc::new(HashMap::new()));
        if let Ok(mut used) = self.last_used.lock() {
            used.clear();
        }
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
    fn request(self: &Arc<Self>, chunk_no: u32) {
        if chunk_no as usize * CHUNK_FRAMES as usize >= self.num_frames {
            return;
        }
        self.last_touch.store(sweep_tick(), Ordering::Relaxed);
        if let Ok(mut wanted) = self.wanted.try_lock() {
            if !wanted.contains(&chunk_no) {
                wanted.push(chunk_no);
            }
        } else {
            // The streamer is holding it; it will pick this up next pass.
            return;
        }
        if !self.queued.swap(true, Ordering::AcqRel) {
            streamer().enqueue(Arc::downgrade(self));
        }
    }

    /// Decode everything the audio thread has asked for. Runs on the streamer
    /// thread.
    fn fill(&self) {
        let wanted: Vec<u32> = {
            let Ok(mut w) = self.wanted.lock() else { return };
            std::mem::take(&mut *w)
        };
        if wanted.is_empty() {
            return;
        }
        let Some(stream) = self.map.get(self.offset..self.offset + self.bytes) else { return };
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
        if next.len() > MAX_RESIDENT_CHUNKS {
            if let Ok(used) = self.last_used.lock() {
                let mut by_age: Vec<(u32, u64)> =
                    next.keys().map(|k| (*k, used.get(k).copied().unwrap_or(0))).collect();
                by_age.sort_by_key(|(_, t)| *t);
                for (chunk_no, _) in by_age.iter().take(next.len() - MAX_RESIDENT_CHUNKS) {
                    next.remove(chunk_no);
                }
            }
        }
        self.chunks.store(Arc::new(next));
        self.queued.store(false, Ordering::Release);
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
        let _ = std::thread::Builder::new()
            .name("signal-streamer".into())
            .spawn(|| streamer().run());
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
            if now.saturating_sub(sample.last_touch.load(Ordering::Relaxed)) >= 2 {
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
                self.sweep();
                continue;
            }
            for weak in batch {
                if let Some(sample) = weak.upgrade() {
                    sample.fill();
                }
            }
            self.sweep();
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

    /// A streamed sample must read back the same audio as decoding the whole
    /// file — head, chunk boundaries and all — while holding only its head.
    #[test]
    fn streams_a_flac_entry_chunk_by_chunk() {
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
            if s.chunks.load().len() > 0 {
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
                for _ in 0..50 {
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
}
