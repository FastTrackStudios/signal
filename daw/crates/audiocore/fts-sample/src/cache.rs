//! Sample cache — loads sample files into RAM as f32 stereo interleaved buffers.
//!
//! All samples are normalised to f32 on load. Stereo files stay stereo;
//! mono files are stored as mono. The caller decides how to mix channels.
//!
//! Loaded buffers are reference-counted so multiple voices can share one
//! allocation without copying.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
#[cfg(feature = "engine-native")]
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
#[cfg(feature = "engine-native")]
use std::time::Instant;

use arc_swap::ArcSwap;
#[cfg(feature = "engine-native")]
use flacenc::component::BitRepr;
#[cfg(feature = "engine-native")]
use flacenc::error::Verify;
#[cfg(feature = "engine-native")]
use rayon::prelude::*;

use crate::SamplerError;

/// The bytes of an opened `.signalpack`: memory-mapped on native, an owned
/// buffer when the pack arrived as bytes (wasm, network fetches, tests).
/// Cheap to clone — both arms are `Arc`s over one backing allocation.
#[derive(Debug, Clone)]
pub enum PackBytes {
    /// The whole pack file, memory-mapped once at open. Sample reads slice
    /// straight out of this; residency is page cache the OS can evict.
    #[cfg(feature = "engine-native")]
    Mapped(Arc<memmap2::Mmap>),
    /// The whole pack resident in memory.
    Owned(Arc<[u8]>),
    /// The pack's bytes live OUTSIDE this process's address space, reachable
    /// only through the pluggable [`set_external_pack_reader`] hook — the
    /// browser seam, where multi-GB pack buffers stay on the JS heap instead
    /// of wasm linear memory. Every read goes through
    /// [`read_range`](Self::read_range) (which allocates a copy of just the
    /// requested span); this arm can NEVER be `Deref`'d.
    External {
        /// Reader-side handle for the buffer (assigned by whoever installed
        /// the reader, e.g. the worklet's JS pack map).
        id: u32,
        /// Total pack length in bytes.
        len: u64,
    },
}

impl std::ops::Deref for PackBytes {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        match self {
            #[cfg(feature = "engine-native")]
            Self::Mapped(map) => map,
            Self::Owned(bytes) => bytes,
            // Nothing may deref an External pack — its bytes are not in this
            // address space. Every read site goes through `read_range`;
            // reaching this is a bug, and a loud one beats silent silence.
            Self::External { id, .. } => {
                panic!("PackBytes::External({id}) cannot be deref'd — use read_range")
            }
        }
    }
}

#[cfg(feature = "engine-native")]
impl From<Arc<memmap2::Mmap>> for PackBytes {
    fn from(map: Arc<memmap2::Mmap>) -> Self {
        Self::Mapped(map)
    }
}

impl From<Vec<u8>> for PackBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes.into())
    }
}

impl PackBytes {
    /// Ask the OS to read `offset..offset + len` ahead, asynchronously.
    /// Meaningful only for the mapped arm; owned bytes are already resident,
    /// external bytes are another realm's problem.
    fn advise_will_need(&self, offset: usize, len: usize) {
        #[cfg(feature = "engine-native")]
        if let Self::Mapped(map) = self {
            let _ = map.advise_range(memmap2::Advice::WillNeed, offset, len);
        }
        let _ = (offset, len);
    }

    /// Total pack length in bytes. Inherent (shadowing the `Deref` slice's
    /// `len`) so it works for every arm, External included.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            #[cfg(feature = "engine-native")]
            Self::Mapped(map) => map.len(),
            Self::Owned(bytes) => bytes.len(),
            Self::External { len, .. } => *len as usize,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether this pack's bytes live outside the process (see
    /// [`PackBytes::External`]).
    #[inline]
    pub fn is_external(&self) -> bool {
        matches!(self, Self::External { .. })
    }

    /// Read `offset..offset + len`, borrowed straight out of the backing
    /// bytes for the in-process arms, copied through the external reader for
    /// [`External`](Self::External). THE read path for pack bytes — nothing
    /// may slice or deref an External pack any other way.
    pub fn read_range(&self, offset: u64, len: usize) -> Result<Cow<'_, [u8]>, SamplerError> {
        let start = usize::try_from(offset)
            .map_err(|_| invalid_data("pack read offset exceeds address space"))?;
        let end = start
            .checked_add(len)
            .filter(|&e| e <= self.len())
            .ok_or_else(|| invalid_data("pack read out of bounds"))?;
        match self {
            #[cfg(feature = "engine-native")]
            Self::Mapped(map) => Ok(Cow::Borrowed(&map[start..end])),
            Self::Owned(bytes) => Ok(Cow::Borrowed(&bytes[start..end])),
            Self::External { id, .. } => {
                let mut buf = vec![0u8; len];
                if !read_external(*id, offset, &mut buf) {
                    return Err(invalid_data(format!(
                        "external pack {id}: read of {len} bytes at {offset} failed \
                         (no reader installed, or the reader refused the range)"
                    )));
                }
                Ok(Cow::Owned(buf))
            }
        }
    }
}

// ── The external pack reader (process-global, pluggable) ─────────────────────

/// Reads `len = dst.len()` bytes at `offset` of external pack `id` into
/// `dst`, returning `false` on failure. Installed once per process by the
/// host that owns the out-of-process buffers (the browser worklet's JS glue);
/// fts-sample itself has no idea what an id points at.
pub type ExternalPackReader = Box<dyn Fn(u32, u64, &mut [u8]) -> bool + Send + Sync>;

static EXTERNAL_READER: std::sync::OnceLock<ExternalPackReader> = std::sync::OnceLock::new();

/// Install the process-wide reader behind [`PackBytes::External`]. First
/// caller wins (returns `false` when a reader was already installed —
/// harmless if it is the same one, which is the expected pattern).
pub fn set_external_pack_reader(reader: ExternalPackReader) -> bool {
    EXTERNAL_READER.set(reader).is_ok()
}

/// Read through the installed external reader. `false` when none is
/// installed or the reader failed.
fn read_external(id: u32, offset: u64, dst: &mut [u8]) -> bool {
    match EXTERNAL_READER.get() {
        Some(reader) => reader(id, offset, dst),
        None => false,
    }
}

/// A packed-sample lookup: the shared pack bytes plus the entry's offset/length.
type PackedSampleRef = (PackBytes, PackEntry);

/// Per-path preload plan: source path, optional prepared-cache entry, the
/// prepared-cache directory (if any), and the packed-sample lookup (if any).
type PreloadPlanEntry = (
    PathBuf,
    Option<PreparedEntry>,
    Option<PathBuf>,
    Option<PackedSampleRef>,
);

#[derive(Debug, Clone, Copy, Default)]
pub struct PreloadStats {
    pub loaded: usize,
    pub failed: usize,
    pub bytes: usize,
    /// Samples left on disk because the process-wide preload budget was
    /// used up — they stream on demand. See [`super::budget`].
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvictStats {
    pub evicted: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    pub bytes_freed: usize,
}

impl EvictStats {
    pub fn add(&mut self, other: EvictStats) {
        if self.bytes_before == 0 {
            self.bytes_before = other.bytes_before;
        }
        self.evicted += other.evicted;
        self.bytes_freed += other.bytes_freed;
        self.bytes_after = other.bytes_after;
    }
}

// ── Loaded sample data ────────────────────────────────────────────────────────

/// A run of interleaved samples (see [`SampleData::span_cursored`]).
#[derive(Clone, Copy, Debug)]
pub enum Span<'a> {
    F32(&'a [f32]),
    /// Resident 16-bit PCM (scaled like [`Pcm::sample`]).
    I16(&'a [i16]),
    /// A streamed chunk or head (scaled like the stream cursor's reads).
    Stream(&'a [i16]),
}

impl Span<'_> {
    /// Samples in the run.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::F32(v) => v.len(),
            Self::I16(v) | Self::Stream(v) => v.len(),
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Sample `i` of the run, as the per-frame reads convert it.
    #[inline]
    #[must_use]
    pub fn get(&self, i: usize) -> f32 {
        match self {
            Self::F32(v) => v[i],
            Self::I16(v) => v[i] as f32 * I16_SCALE,
            Self::Stream(v) => f32::from(v[i]) * (1.0 / 32768.0),
        }
    }
}

/// Where a sample's PCM actually lives.
///
/// Kontakt-style samplers keep libraries playable without keeping them
/// resident, and the trick is not a smarter cache — it is not decoding to
/// float in the first place. A 12-second stereo note is 4.6 MB as `f32` and
/// 1.2 MB as the 16-bit PCM it came from, or **zero** anonymous bytes when it
/// can be read straight out of a memory-mapped pack, where the OS pages it in
/// on demand and evicts it under pressure for free.
#[derive(Debug, Clone)]
pub enum Pcm {
    /// Decoded float PCM. Synthesis, offline tools, tests — anything that
    /// wants a plain slice.
    F32(Arc<Vec<f32>>),
    /// 16-bit PCM, converted per read. Half the memory of `F32` for the
    /// decoded-and-resident case.
    I16(Arc<Vec<i16>>),
    /// **Streamed**: the sample stays compressed in its mapped pack; a head
    /// is resident and the rest is decoded a chunk at a time as the voice
    /// plays it. See [`super::stream`].
    Streamed(Arc<super::stream::StreamedSample>),
    /// **Direct from disk**: a window into a memory-mapped file (or an
    /// in-memory pack buffer). Nothing is decoded and nothing is allocated;
    /// reads touch page-cache pages.
    Mapped {
        map: PackBytes,
        /// Byte offset of the first sample within the mapping.
        offset: usize,
        /// Number of PCM samples (not frames, not bytes).
        samples: usize,
        fmt: PcmFmt,
    },
}

/// Sample formats readable straight out of a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmFmt {
    /// 16-bit little-endian.
    I16,
    /// 24-bit little-endian, 3 bytes per sample, packed.
    I24,
    /// 32-bit float little-endian.
    F32,
}

impl PcmFmt {
    /// Bytes per sample on disk.
    pub const fn width(self) -> usize {
        match self {
            Self::I16 => 2,
            Self::I24 => 3,
            Self::F32 => 4,
        }
    }
}

/// Frames of a mapped sample the preloader faults in eagerly — half a second
/// at 48 kHz, comfortably more than any note's attack.
const WARM_HEAD_FRAMES: usize = 24_000;

const I16_SCALE: f32 = 1.0 / 32768.0;
const I24_SCALE: f32 = 1.0 / 8_388_608.0;

impl Pcm {
    /// Number of PCM samples held (frames × channels).
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::F32(v) => v.len(),
            Self::I16(v) => v.len(),
            Self::Streamed(s) => s.num_frames * s.channels.max(1) as usize,
            Self::Mapped { samples, .. } => *samples,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// One sample, converted to float. The audio thread's inner read.
    #[inline]
    pub fn sample(&self, index: usize) -> f32 {
        match self {
            Self::F32(v) => v.get(index).copied().unwrap_or(0.0),
            Self::I16(v) => v.get(index).map(|s| *s as f32 * I16_SCALE).unwrap_or(0.0),
            Self::Streamed(s) => s.sample(index),
            Self::Mapped {
                map,
                offset,
                samples,
                fmt,
            } => {
                // A Mapped window is never built over an External pack; if
                // one ever were, silence beats trapping the audio thread.
                if index >= *samples || map.is_external() {
                    return 0.0;
                }
                let at = offset + index * fmt.width();
                let bytes: &[u8] = map;
                match fmt {
                    PcmFmt::I16 => {
                        let Some(b) = bytes.get(at..at + 2) else {
                            return 0.0;
                        };
                        i16::from_le_bytes([b[0], b[1]]) as f32 * I16_SCALE
                    }
                    PcmFmt::I24 => {
                        let Some(b) = bytes.get(at..at + 3) else {
                            return 0.0;
                        };
                        // Sign-extend 24 → 32 by landing the bytes in the top
                        // three octets and shifting back down.
                        let v = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
                        v as f32 * I24_SCALE
                    }
                    PcmFmt::F32 => {
                        let Some(b) = bytes.get(at..at + 4) else {
                            return 0.0;
                        };
                        f32::from_le_bytes([b[0], b[1], b[2], b[3]])
                    }
                }
            }
        }
    }

    /// Anonymous (non-evictable) bytes this store costs the process. Mapped
    /// PCM costs nothing: it is file-backed page cache.
    pub fn resident_bytes(&self) -> usize {
        match self {
            Self::F32(v) => v.len() * std::mem::size_of::<f32>(),
            Self::I16(v) => v.len() * std::mem::size_of::<i16>(),
            // Head plus whatever chunks are under playheads right now.
            Self::Streamed(s) => s.resident_bytes(),
            Self::Mapped { .. } => 0,
        }
    }

    /// The samples as floats, copying only when they are not already floats.
    ///
    /// A BULK read: it allocates the whole sample, so it is never realtime-safe
    /// and has no business on the audio thread. Being off the audio thread is
    /// also why it must be COMPLETE rather than fast — a streamed sample is
    /// decoded in full here instead of being read through
    /// [`Pcm::sample`], which answers 0.0 for any chunk the streamer has not
    /// fetched yet and merely queues a request. Read that way, a streamed
    /// sample returns silence wherever streaming has not caught up, and
    /// different audio on every run; every offline consumer (loudness,
    /// extraction, analysis, reference matching) was quietly affected.
    pub fn to_f32(&self) -> std::borrow::Cow<'_, [f32]> {
        match self {
            Self::F32(v) => std::borrow::Cow::Borrowed(v.as_slice()),
            Self::Streamed(s) => std::borrow::Cow::Owned(s.decode_all()),
            _ => std::borrow::Cow::Owned((0..self.len()).map(|i| self.sample(i)).collect()),
        }
    }

    /// The float slice, when the store already is one.
    pub fn as_f32(&self) -> Option<&[f32]> {
        match self {
            Self::F32(v) => Some(v.as_slice()),
            _ => None,
        }
    }
}

/// A loaded sample: its PCM (wherever that lives) and its format.
#[derive(Debug, Clone)]
pub struct SampleData {
    /// PCM, normalised to [-1.0, 1.0] on read. Stereo is interleaved L/R.
    pub pcm: Pcm,
    pub channels: u16,
    pub sample_rate: u32,
    /// Total number of sample frames (frames = samples / channels).
    pub num_frames: usize,
}

impl SampleData {
    /// Decoded float PCM — the classic path.
    pub fn from_f32(frames: Vec<f32>, channels: u16, sample_rate: u32, num_frames: usize) -> Self {
        Self {
            pcm: Pcm::F32(Arc::new(frames)),
            channels,
            sample_rate,
            num_frames,
        }
    }

    /// A window into a memory-mapped pack — nothing decoded, nothing
    /// allocated.
    pub fn mapped(
        map: impl Into<PackBytes>,
        offset: usize,
        samples: usize,
        fmt: PcmFmt,
        channels: u16,
        sample_rate: u32,
        num_frames: usize,
    ) -> Self {
        Self {
            pcm: Pcm::Mapped {
                map: map.into(),
                offset,
                samples,
                fmt,
            },
            channels,
            sample_rate,
            num_frames,
        }
    }

    /// A sample that stays compressed in its pack and streams as it plays.
    ///
    /// `FTS_NO_STREAM=1` decodes the whole entry up front instead, giving a
    /// fully resident sample with no streamer involvement at all. It is a
    /// DIAGNOSTIC, not a mode to ship: it trades every byte of the library
    /// for certainty, and a real library will not fit. The point is to settle
    /// whether an artefact is the streamer's fault by removing the streamer —
    /// if the crackle survives this, it was never streaming.
    pub fn streamed(sample: Arc<super::stream::StreamedSample>) -> Self {
        let (channels, sample_rate, num_frames) =
            (sample.channels, sample.sample_rate, sample.num_frames);
        if no_stream() {
            let pcm = sample.decode_all();
            tracing::debug!(
                target: "fts_sample::stream",
                frames = num_frames,
                mib = (pcm.len() * 4) as f64 / (1024.0 * 1024.0),
                "FTS_NO_STREAM: decoded entry fully resident"
            );
            return Self::from_f32(pcm, channels, sample_rate, num_frames);
        }
        Self {
            pcm: Pcm::Streamed(sample),
            channels,
            sample_rate,
            num_frames,
        }
    }

    /// Anonymous bytes this sample costs; see [`Pcm::resident_bytes`].
    pub fn decoded_bytes(&self) -> usize {
        self.pcm.resident_bytes()
    }

    /// Number of PCM samples (frames × channels).
    #[inline]
    pub fn len(&self) -> usize {
        self.pcm.len()
    }

    /// Keep `from..to` resident while the returned pin lives. `None` unless
    /// the sample is streamed — a resident sample is already all there.
    pub fn pin_region(&self, from_frame: usize, to_frame: usize) -> Option<super::stream::LoopPin> {
        match &self.pcm {
            Pcm::Streamed(s) => Some(s.pin(from_frame, to_frame)),
            _ => None,
        }
    }

    /// Warm the region a voice is about to read. No-op unless streamed.
    pub fn prefetch_at(&self, frame: usize) {
        if let Pcm::Streamed(s) = &self.pcm {
            s.prefetch_at(frame);
        }
    }

    /// Whether this sample is streamed from its pack rather than resident.
    pub fn is_streamed(&self) -> bool {
        matches!(self.pcm, Pcm::Streamed(_))
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pcm.is_empty()
    }

    /// The samples as floats — borrowed when they already are.
    pub fn to_f32(&self) -> std::borrow::Cow<'_, [f32]> {
        self.pcm.to_f32()
    }

    /// Fault in the sample's pages so the audio thread never takes a disk
    /// read inside the callback.
    ///
    /// Mapped PCM is free to *hold* but not free to *touch*: a cold page is a
    /// blocking read, and in a realtime callback that is a dropout. So the
    /// preloader does what Kontakt's does — it makes the head resident (the
    /// attack, which plays immediately) and asks the kernel to read the tail
    /// ahead in the background. The pages stay file-backed and evictable, so
    /// this costs the process nothing it can be blamed for.
    pub fn warm(&self, head_frames: usize) {
        let Pcm::Mapped {
            map,
            offset,
            samples,
            fmt,
        } = &self.pcm
        else {
            return;
        };
        // Belt-and-braces: `load_pack_sample` never builds a Mapped window
        // over an External pack (it materializes the entry first), so this
        // guard should be dead — but touching one would trap on the deref.
        if map.is_external() {
            return;
        }
        let len = samples * fmt.width();
        // Read-ahead for the whole sample, asynchronously (mapped arm only —
        // owned bytes are already resident).
        map.advise_will_need(*offset, len);
        // …and synchronously touch the head, one byte per page.
        let head = (head_frames * self.channels.max(1) as usize * fmt.width()).min(len);
        let page = 4096;
        let bytes: &[u8] = map;
        let mut acc = 0u8;
        let mut at = *offset;
        while at < offset + head {
            if let Some(b) = bytes.get(at) {
                acc = acc.wrapping_add(*b);
            }
            at += page;
        }
        // Keep the reads from being optimised away.
        std::hint::black_box(acc);
    }

    /// Read one stereo frame (or duplicate mono → stereo). Returns (L, R).
    #[inline]
    /// Read `out_l.len()` consecutive frames from `start` into separate L/R
    /// buffers, resolving a streamed chunk once per chunk crossed.
    ///
    /// This is what any BULK read of sample data should use. Reading frame by
    /// frame through [`SampleData::frame`] costs an arc-swap guard per
    /// channel per frame when the sample is streamed; priming a pitch
    /// shifter reads ~4-8k frames, so doing it the obvious way cost ~16,000
    /// guarded reads PER VOICE at note-on — measured at ~27% of the audio
    /// thread, and the reason a chord stalled the callback for hundreds of
    /// milliseconds.
    pub fn read_frames(
        &self,
        start: usize,
        out_l: &mut [f32],
        out_r: &mut [f32],
        cursor: &mut crate::stream::StreamCursor,
    ) {
        let n = out_l.len().min(out_r.len());
        let Pcm::Streamed(stream) = &self.pcm else {
            for i in 0..n {
                let (l, r) = self.frame(start + i);
                out_l[i] = l;
                out_r[i] = r;
            }
            return;
        };
        let ch = self.channels as usize;
        let mono = self.channels == 1;
        for i in 0..n {
            let base = (start + i) * ch;
            // The cursor holds the chunk; it only re-seeks when the read
            // leaves it, i.e. about once per 12,000 frames.
            let l = match cursor.get(base) {
                Some(v) => Some(v),
                None => cursor
                    .seek(stream, base)
                    .then(|| cursor.get(base))
                    .flatten(),
            };
            match l {
                Some(l) => {
                    let r = if mono {
                        l
                    } else {
                        cursor.get(base + 1).unwrap_or(l)
                    };
                    out_l[i] = l;
                    out_r[i] = r;
                }
                // Head, or a chunk that has not arrived: the ordinary path
                // answers both correctly (the head is always resident).
                None => {
                    let (l, r) = self.frame(start + i);
                    out_l[i] = l;
                    out_r[i] = r;
                }
            }
        }
    }

    /// The interpolator's two frames, read through a voice's own
    /// [`StreamCursor`].
    ///
    /// This is the hot read: one call per output frame per voice. When the
    /// cursor already holds the right chunk — which it does for thousands of
    /// consecutive samples — the whole thing is four range-checked slice
    /// indexes and no atomics at all. It re-seeks only when the playhead
    /// leaves the chunk, i.e. roughly once every 12,000 frames instead of
    /// four times per frame.
    pub fn frame_pair_cursored(
        &self,
        frame_idx: usize,
        last: usize,
        cursor: &mut crate::stream::StreamCursor,
    ) -> ((f32, f32), (f32, f32)) {
        let Pcm::Streamed(stream) = &self.pcm else {
            return self.frame_pair(frame_idx, last);
        };
        let ch = self.channels as usize;
        let next = (frame_idx + 1).min(last);
        let (a, b) = (frame_idx * ch, next * ch);
        // The common case: both frames inside what the cursor already holds
        // (the resident head, or the chunk in progress) — one range check.
        if let Some(pair) = cursor.frame_pair(a, b, ch) {
            return pair;
        }
        // Otherwise move to the chunk covering `a`. A pair straddling a chunk
        // boundary is rare (once per chunk) and handled by the per-sample
        // fallback rather than by holding two chunks.
        if cursor.seek(stream, a) {
            if let Some(pair) = cursor.frame_pair(a, b, ch) {
                return pair;
            }
        }
        self.frame_pair(frame_idx, last)
    }

    /// The contiguous run of interleaved samples holding frame `frame_idx`,
    /// for a reader that walks it directly instead of asking per frame:
    /// `(samples, the frame the run starts at)`. Streamed samples come from
    /// `cursor` (moved to the chunk covering the frame); `None` when there is
    /// no such run (a mapped window, a chunk not yet resident) — read per
    /// frame then. A streamed reader calls `cursor.touch` with the furthest
    /// sample it read, so the read-ahead fires.
    #[inline]
    pub fn span_cursored<'a>(
        &'a self,
        frame_idx: usize,
        cursor: &'a mut crate::stream::StreamCursor,
    ) -> Option<(Span<'a>, usize)> {
        let ch = self.channels.max(1) as usize;
        match &self.pcm {
            Pcm::F32(v) => Some((Span::F32(v.as_slice()), 0)),
            Pcm::I16(v) => Some((Span::I16(v.as_slice()), 0)),
            Pcm::Streamed(stream) => {
                let a = frame_idx * ch;
                let covered = cursor.held().is_some_and(|(d, lo)| a >= lo && a < lo + d.len());
                if !covered && !cursor.seek(stream, a) {
                    return None;
                }
                let (data, lo) = cursor.held()?;
                (lo % ch == 0).then_some((Span::Stream(data), lo / ch))
            }
            _ => None,
        }
    }

    /// The two consecutive stereo frames a linear interpolator needs, read
    /// with ONE chunk resolution when the samples are streamed.
    ///
    /// The obvious spelling — four `Pcm::sample` calls — costs four arc-swap
    /// guard loads per output frame per voice, which at a chord's worth of
    /// voices dominated the audio callback. `last` clamps the second frame
    /// the way the caller would.
    pub fn frame_pair(&self, frame_idx: usize, last: usize) -> ((f32, f32), (f32, f32)) {
        let ch = self.channels as usize;
        if let Pcm::Streamed(stream) = &self.pcm {
            let next = (frame_idx + 1).min(last);
            // Contiguous only when the second frame really does follow the
            // first; at the clamp they are the same frame.
            if next == frame_idx + 1 {
                let mut buf = [0.0f32; 4];
                let want = (ch * 2).min(4);
                if stream.run(frame_idx * ch, &mut buf[..want]) == want {
                    return match self.channels {
                        1 => ((buf[0], buf[0]), (buf[1], buf[1])),
                        _ => ((buf[0], buf[1]), (buf[2], buf[3])),
                    };
                }
            }
        }
        (self.frame(frame_idx), self.frame((frame_idx + 1).min(last)))
    }

    pub fn frame(&self, frame_idx: usize) -> (f32, f32) {
        let n = self.pcm.len();
        let base = frame_idx * self.channels as usize;
        if base >= n {
            return (0.0, 0.0);
        }
        match self.channels {
            1 => {
                let s = self.pcm.sample(base);
                (s, s)
            }
            _ => {
                let l = self.pcm.sample(base);
                let r = self.pcm.sample((base + 1).min(n - 1));
                (l, r)
            }
        }
    }
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Whether to bypass streaming entirely and decode every entry up front.
/// Read once — this is consulted per sample load, and `env::var` is not free.
fn no_stream() -> bool {
    static NO_STREAM: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *NO_STREAM.get_or_init(|| std::env::var_os("FTS_NO_STREAM").is_some())
}

/// Shared sample cache, safe to use across the audio thread and a
/// background preloader simultaneously.
///
/// The `loaded` map sits inside an `Arc<RwLock>`; every public method takes
/// `&self` and either grabs a read-lock (lock-free try_read on the audio
/// thread) or briefly write-locks while inserting a freshly decoded sample.
/// Cloning the cache via [`SampleCache::clone_handle`] yields an extra
/// reference to the same backing storage — used by the background
/// preloader thread.
#[derive(Clone)]
pub struct SampleCache {
    inner: Arc<CacheInner>,
}

struct CacheInner {
    /// Writer-owned path-keyed map of fully decoded samples.
    loaded: RwLock<HashMap<PathBuf, Arc<SampleData>>>,
    /// Lock-free read snapshot for the audio thread.
    loaded_snapshot: ArcSwap<HashMap<PathBuf, Arc<SampleData>>>,
    /// Read-only after construction.
    prepared: HashMap<PathBuf, PreparedEntry>,
    /// Read-only after construction.
    prepared_dir: Option<PathBuf>,
    /// Read-only after construction.
    pcm_pack: Option<SignalPcmPack>,
}

impl Drop for CacheInner {
    fn drop(&mut self) {
        let bytes: usize = self
            .loaded
            .read()
            .map(|l| l.values().map(|d| d.decoded_bytes()).sum())
            .unwrap_or(0);
        if bytes > 0 {
            super::budget::release(bytes);
        }
    }
}

#[derive(Debug, Clone)]
struct PreparedEntry {
    pcm_file: PathBuf,
    channels: u16,
    sample_rate: u32,
    num_frames: usize,
    samples: usize,
}

/// Parsed `.signalpack` reader. Cheap to clone.
///
/// Holds the absolute path to the pack file plus an in-memory map of
/// (relative source path → byte offset/length within the pack body).
/// Audio is **not** decoded — call [`SampleCache::get`] to decode on demand.
#[derive(Debug, Clone)]
pub struct SignalPcmPack {
    path: PathBuf,
    /// The whole pack, memory-mapped once at open (native) or handed over as
    /// an in-memory buffer ([`open_bytes`](Self::open_bytes)). Sample decode
    /// slices straight out of this — no per-sample `File::open`/seek/read
    /// syscalls (those were the preload bottleneck on multi-GB packs /
    /// external drives). Clones share one backing allocation.
    mmap: PackBytes,
    /// Header kind field (5 = FLAC i24 lossless, 6 = Ogg Vorbis proxy).
    kind: u32,
    /// The header's index span `(offset, len)` — what a range-streaming
    /// planner must deliver (with the 64-byte header) before the pack can
    /// open at all.
    index_span: (u64, u64),
    entries: HashMap<PathBuf, PackEntry>,
    /// Embedded styx/toml spec text recovered from the pack index.
    embedded_spec: Option<String>,
    /// Format hint declared in the index (`"styx"` or `"toml"`).
    embedded_spec_format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackEntry {
    /// The pack kind this entry came from (5 FLAC, 6 Ogg, 7 PCM i16, 8 PCM
    /// i24) — carried per entry so a reader can decide how to read it
    /// without the pack, and so mixed-codec packs stay possible.
    kind: u32,
    offset: u64,
    bytes: u64,
    channels: u16,
    sample_rate: u32,
    num_frames: usize,
    samples: usize,
}

impl PackEntry {
    pub fn kind(&self) -> u32 {
        self.kind
    }
    /// Raw PCM entries are read straight out of the mapping — no decode, no
    /// allocation. `None` for the compressed kinds.
    pub fn mapped_fmt(&self) -> Option<PcmFmt> {
        match self.kind {
            SIGNAL_PACK_KIND_PCM_I16 => Some(PcmFmt::I16),
            SIGNAL_PACK_KIND_PCM_I24 => Some(PcmFmt::I24),
            _ => None,
        }
    }
    pub fn offset(&self) -> u64 {
        self.offset
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn num_frames(&self) -> usize {
        self.num_frames
    }
    pub fn samples(&self) -> usize {
        self.samples
    }
}

const SIGNAL_PACK_MAGIC: &[u8; 8] = b"SIGPACK\0";
const SIGNAL_PACK_VERSION: u32 = 1;
const SIGNAL_PACK_HEADER_LEN: usize = 64;
const SIGNAL_PACK_KIND_FLAC_I24: u32 = 5;
/// Raw PCM packs: entries are interleaved little-endian samples with no
/// container and no codec, so a voice can read them directly out of the
/// mapping (see [`Pcm::Mapped`]).
const SIGNAL_PACK_KIND_PCM_I16: u32 = 7;
const SIGNAL_PACK_KIND_PCM_I24: u32 = 8;

/// Lossy proxy pack: entries are Ogg Vorbis streams instead of FLAC.
/// Same header/index/spec layout; index `num_frames`/`samples` still carry
/// the SOURCE PCM truth, so frame-indexed zone metadata (loop points,
/// sample_start/end) stays valid — decode trims/pads to the index length.
const SIGNAL_PACK_KIND_OGG_VORBIS: u32 = 6;

/// The fixed 64-byte `.signalpack` file header — THE one parse/encode
/// implementation. Every reader (pack open, extraction, in-place spec
/// rewrite) and the writer go through this type.
///
/// Layout (all integers little-endian):
///
/// | offset | bytes | field |
/// |--------|-------|-------|
/// | 0      | 8     | magic `"SIGPACK\0"` |
/// | 8      | 4     | version (currently 1) |
/// | 12     | 4     | kind (5 FLAC i24, 6 Ogg Vorbis, 7 PCM i16, 8 PCM i24) |
/// | 16     | 8     | body offset (always 64 — the header length) |
/// | 24     | 8     | index offset (byte position of the text index) |
/// | 32     | 8     | index length in bytes |
/// | 40     | 8     | entry count |
/// | 48     | 16    | reserved (zero) |
///
/// The raw 64 bytes are retained so a rewriter can patch the index span
/// while carrying every other byte (entry count, reserved area) forward
/// verbatim.
#[derive(Debug, Clone, Copy)]
pub struct PackFileHeader {
    raw: [u8; SIGNAL_PACK_HEADER_LEN],
    version: u32,
    kind: u32,
    body_offset: u64,
    index_offset: u64,
    index_len: u64,
    entry_count: u64,
}

impl PackFileHeader {
    /// Fixed on-disk header length in bytes.
    pub const LEN: usize = SIGNAL_PACK_HEADER_LEN;

    /// Read + parse the header from the start of an open pack file.
    /// Leaves the reader positioned at the body offset (byte 64).
    pub fn read(file: &mut File) -> Result<Self, SamplerError> {
        let mut raw = [0u8; Self::LEN];
        file.read_exact(&mut raw)?;
        Self::parse(raw)
    }

    /// Parse a raw 64-byte header. Validates the magic, the version, and
    /// offset plausibility (body at 64, index at or after the body).
    /// Kind is NOT validated here — spec rewriting copies unknown-kind
    /// bodies verbatim; readers that decode audio call
    /// [`require_known_kind`](Self::require_known_kind).
    pub fn parse(raw: [u8; Self::LEN]) -> Result<Self, SamplerError> {
        if &raw[0..8] != SIGNAL_PACK_MAGIC {
            return Err(invalid_data("invalid signal pack magic"));
        }
        let version = read_u32(&raw, 8)?;
        if version != SIGNAL_PACK_VERSION {
            return Err(invalid_data(format!(
                "unsupported signal pack version {version}"
            )));
        }
        let kind = read_u32(&raw, 12)?;
        let body_offset = read_u64(&raw, 16)?;
        let index_offset = read_u64(&raw, 24)?;
        let index_len = read_u64(&raw, 32)?;
        let entry_count = read_u64(&raw, 40)?;
        if body_offset != Self::LEN as u64 || index_offset < body_offset {
            return Err(invalid_data("implausible signal pack header offsets"));
        }
        Ok(Self {
            raw,
            version,
            kind,
            body_offset,
            index_offset,
            index_len,
            entry_count,
        })
    }

    /// Build a fresh header for the writer (magic/version/body offset
    /// filled in; reserved bytes zero).
    pub fn new(kind: u32, index_offset: u64, index_len: u64, entry_count: u64) -> Self {
        let mut raw = [0u8; Self::LEN];
        raw[0..8].copy_from_slice(SIGNAL_PACK_MAGIC);
        write_u32(&mut raw, 8, SIGNAL_PACK_VERSION);
        write_u32(&mut raw, 12, kind);
        write_u64(&mut raw, 16, Self::LEN as u64);
        write_u64(&mut raw, 24, index_offset);
        write_u64(&mut raw, 32, index_len);
        write_u64(&mut raw, 40, entry_count);
        Self {
            raw,
            version: SIGNAL_PACK_VERSION,
            kind,
            body_offset: Self::LEN as u64,
            index_offset,
            index_len,
            entry_count,
        }
    }

    /// Err unless the kind is one this build knows how to decode.
    pub fn require_known_kind(&self) -> Result<u32, SamplerError> {
        if !matches!(
            self.kind,
            SIGNAL_PACK_KIND_FLAC_I24
                | SIGNAL_PACK_KIND_OGG_VORBIS
                | SIGNAL_PACK_KIND_PCM_I16
                | SIGNAL_PACK_KIND_PCM_I24
        ) {
            return Err(invalid_data(format!(
                "signal pack kind {} is not a known pack format",
                self.kind
            )));
        }
        Ok(self.kind)
    }

    /// Copy of this header with the index span replaced — the spec-rewrite
    /// path: every other byte (kind, entry count, reserved) rides along
    /// unchanged.
    pub fn with_index_span(&self, index_offset: u64, index_len: u64) -> Self {
        let mut out = *self;
        write_u64(&mut out.raw, 24, index_offset);
        write_u64(&mut out.raw, 32, index_len);
        out.index_offset = index_offset;
        out.index_len = index_len;
        out
    }

    /// The raw 64 bytes, ready to write to disk.
    pub fn as_bytes(&self) -> &[u8; Self::LEN] {
        &self.raw
    }

    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn kind(&self) -> u32 {
        self.kind
    }
    pub fn body_offset(&self) -> u64 {
        self.body_offset
    }
    pub fn index_offset(&self) -> u64 {
        self.index_offset
    }
    pub fn index_len(&self) -> u64 {
        self.index_len
    }
    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }
}

/// Codec used for the audio entries of a `.signalpack`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackCodec {
    /// Lossless FLAC (24-bit int, 16-bit when the source fits). The default.
    FlacI24,
    /// Lossy Ogg Vorbis proxy at the given quality (-0.2..=1.0 — libvorbis
    /// scale, i.e. oggenc `-q -2..10` divided by 10; 0.8 ≈ q8 ≈ ~256 kbps).
    OggVorbis { quality: f32 },
    /// **Raw 16-bit PCM**, interleaved little-endian. Bigger on disk than
    /// FLAC and *free* in memory: voices read it straight out of the
    /// memory-mapped pack, so residency is page cache the OS can evict —
    /// this is the format that makes a piano playable without a piano's
    /// worth of RAM.
    PcmI16,
    /// Raw 24-bit PCM, 3 bytes per sample. Same deal, lossless, 1.5× the
    /// size of `PcmI16`.
    PcmI24,
}

impl PackCodec {
    /// Vorbis proxy at q8 — transparent on orchestral content, ~7-8× smaller.
    pub const OGG_VORBIS_Q8: PackCodec = PackCodec::OggVorbis { quality: 0.8 };

    #[cfg(feature = "engine-native")]
    fn header_kind(self) -> u32 {
        match self {
            PackCodec::FlacI24 => SIGNAL_PACK_KIND_FLAC_I24,
            PackCodec::OggVorbis { .. } => SIGNAL_PACK_KIND_OGG_VORBIS,
            PackCodec::PcmI16 => SIGNAL_PACK_KIND_PCM_I16,
            PackCodec::PcmI24 => SIGNAL_PACK_KIND_PCM_I24,
        }
    }

    /// Parse a codec name: `flac`, `ogg[:quality]`, `pcm16`, `pcm24`.
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        match name.split_once(':') {
            Some(("ogg" | "vorbis", q)) => q
                .parse()
                .ok()
                .map(|quality| PackCodec::OggVorbis { quality }),
            _ => match name.as_str() {
                "flac" | "flac-i24" => Some(PackCodec::FlacI24),
                "ogg" | "vorbis" => Some(PackCodec::OGG_VORBIS_Q8),
                "pcm16" | "pcm-i16" | "i16" => Some(PackCodec::PcmI16),
                "pcm24" | "pcm-i24" | "i24" => Some(PackCodec::PcmI24),
                _ => None,
            },
        }
    }
}

impl Default for SampleCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CacheInner {
                loaded: RwLock::new(HashMap::new()),
                loaded_snapshot: ArcSwap::from_pointee(HashMap::new()),
                prepared: HashMap::new(),
                prepared_dir: None,
                pcm_pack: None,
            }),
        }
    }

    /// Build a cache backed directly by an already-opened `.signalpack`.
    /// Audio decodes from the pack body on demand; no on-disk source files
    /// required.
    pub fn with_pack(pack: SignalPcmPack) -> Self {
        Self {
            inner: Arc::new(CacheInner {
                loaded: RwLock::new(HashMap::new()),
                loaded_snapshot: ArcSwap::from_pointee(HashMap::new()),
                prepared: HashMap::new(),
                prepared_dir: None,
                pcm_pack: Some(pack),
            }),
        }
    }

    #[cfg(feature = "engine-native")]
    pub fn with_prepared(cache_dir: Option<&Path>) -> Self {
        let mut prepared: HashMap<PathBuf, PreparedEntry> = HashMap::new();
        let mut prepared_dir: Option<PathBuf> = None;
        let mut pcm_pack: Option<SignalPcmPack> = None;
        if let Some(cache_dir) = cache_dir {
            let pack_path = default_signal_pack_path(cache_dir);
            if pack_path.exists() {
                match SignalPcmPack::open(&pack_path) {
                    Ok(pack) => {
                        tracing::info!(
                            "signal-sampler: loaded signal PCM pack {} ({} samples)",
                            pack_path.display(),
                            pack.entries.len()
                        );
                        pcm_pack = Some(pack);
                    }
                    Err(err) => {
                        tracing::warn!(
                            "signal-sampler: failed to load signal PCM pack {}: {err}",
                            pack_path.display()
                        );
                    }
                }
            }

            if pcm_pack.is_none() {
                if let Err(err) = load_prepared_index(cache_dir, &mut prepared, &mut prepared_dir) {
                    tracing::warn!(
                        "signal-sampler: failed to load prepared cache {}: {err}",
                        cache_dir.display()
                    );
                }
            }
        }
        Self {
            inner: Arc::new(CacheInner {
                loaded: RwLock::new(HashMap::new()),
                loaded_snapshot: ArcSwap::from_pointee(HashMap::new()),
                prepared,
                prepared_dir,
                pcm_pack,
            }),
        }
    }

    /// Load `path` in the background if it isn't cached yet.
    ///
    /// The audio thread drops a voice whose sample isn't resident — it can't
    /// decode, and it won't block. Without this that note stays silent
    /// forever; with it, the miss warms the sample and the next hit plays.
    /// Preloading is still what makes the *first* hit sound.
    pub fn warm_async(&self, path: &Path) {
        if self.inner.loaded_snapshot.load().contains_key(path) {
            return;
        }
        #[cfg(feature = "engine-native")]
        warm_queue().send(self.clone_handle(), path.to_owned());
        // No background threads without the native half: decode here and now.
        // Correctness over latency — a wasm host that cares warms up front.
        #[cfg(not(feature = "engine-native"))]
        let _ = self.get(path);
    }

    /// Whether this cache's samples stream (a FLAC pack) or must be decoded
    /// whole. Streaming changes what a preload *costs*: a head instead of the
    /// entire sample, so every zone can be warmed instead of a capped subset.
    pub fn is_streamable(&self) -> bool {
        self.inner
            .pcm_pack
            .as_ref()
            // ANY streamed entry makes it a streaming pack. Judging by the
            // first entry of a hash map was random: a pack mixing long notes
            // with short key-up samples came out "not streamable" whenever a
            // short one iterated first, its preload was then capped (to 64
            // samples by default) and the uncovered notes dropped — a
            // different set every run.
            .map(|pack| {
                pack.entries_iter()
                    .any(|(_, e)| e.mapped_fmt().is_none() && should_stream(e))
            })
            .unwrap_or(false)
    }

    /// Cheap clone of the cache handle — shares the underlying storage.
    /// Used to hand the cache to a background preloader thread.
    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Lock-free try-read for the audio thread. Returns `None` on cache
    /// miss; the audio thread should silently skip the voice. The background
    /// preloader publishes new snapshots as samples arrive.
    pub fn get_loaded(&self, path: &Path) -> Option<Arc<SampleData>> {
        self.inner.loaded_snapshot.load().get(path).map(Arc::clone)
    }

    /// Whether this cache can resolve `path` at all — a pack entry, a
    /// prepared-cache entry, or an already-loaded sample. The out-of-process
    /// decoder (browser decoder worker) uses this to route a decoded sample
    /// to the engine whose pack actually owns it.
    pub fn knows_path(&self, path: &Path) -> bool {
        if self.inner.loaded_snapshot.load().contains_key(path) {
            return true;
        }
        if let Some(pack) = self.inner.pcm_pack.as_ref() {
            if pack.entry_for_path(path).is_some() {
                return true;
            }
        }
        self.inner.prepared.contains_key(path)
    }

    /// Insert PCM that was decoded SOMEWHERE ELSE (another thread, another
    /// wasm instance) as if `get` had decoded it here: charges the
    /// process-wide budget via the one `insert_loaded` door and publishes a
    /// fresh audio-thread snapshot. Replacing an existing entry releases the
    /// old bytes first.
    pub fn insert_decoded(&self, path: PathBuf, data: Arc<SampleData>) {
        self.insert_loaded(path, data, true);
    }

    /// Remove one decoded sample, releasing its budget charge and
    /// publishing a fresh snapshot. Voices already holding the `Arc` keep
    /// playing; the bytes free when they finish. The decoder worker uses
    /// this to stay memory-flat: decode, hand the PCM out, take it back out
    /// of its own cache.
    pub fn take_loaded(&self, path: &Path) -> Option<Arc<SampleData>> {
        let removed = {
            let mut loaded = self.inner.loaded.write().unwrap_or_else(|e| e.into_inner());
            loaded.remove(path)
        };
        if let Some(data) = removed.as_ref() {
            super::budget::release(data.decoded_bytes());
            self.publish_loaded_snapshot();
        }
        removed
    }

    /// Decode one sample (if not already cached) and insert it into the
    /// shared map. Safe to call concurrently from any thread; the audio
    /// thread sees the new entry as soon as the write lock is released.
    pub fn preload_one(&self, path: &Path) -> Result<(), SamplerError> {
        if self
            .inner
            .loaded
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(path)
        {
            return Ok(());
        }
        let data = decode_path(&self.inner, path)?;
        self.insert_loaded(path.to_owned(), Arc::new(data), true);
        Ok(())
    }

    /// Get a sample, blocking-decoding on miss. Tests + the (rare) callers
    /// that genuinely need a synchronous load. **Never call this from the
    /// audio thread** — use [`get_loaded`](Self::get_loaded) there.
    pub fn get(&self, path: &Path) -> Result<Arc<SampleData>, SamplerError> {
        if let Some(entry) = self
            .inner
            .loaded
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
        {
            return Ok(Arc::clone(entry));
        }
        // `Instant::now` panics on wasm32-unknown-unknown; the timing is a
        // native diagnostic anyway.
        #[cfg(feature = "engine-native")]
        let start = Instant::now();
        let data = decode_path(&self.inner, path)?;
        #[cfg(feature = "engine-native")]
        {
            let elapsed = start.elapsed();
            if elapsed.as_millis() >= 5 {
                tracing::debug!(
                    "sample cache miss loaded {} in {:.2} ms",
                    path.display(),
                    elapsed.as_secs_f64() * 1000.0
                );
            }
        }
        let arc = Arc::new(data);
        self.insert_loaded(path.to_owned(), Arc::clone(&arc), true);
        Ok(arc)
    }

    /// Decode many paths in parallel via rayon, **inserting each sample
    /// into the cache as soon as it's decoded** so the audio thread sees
    /// voices come online incrementally (Kontakt-style streaming preload).
    pub fn preload<'a>(&self, paths: impl Iterator<Item = &'a Path>) -> PreloadStats {
        let paths: Vec<PreloadPlanEntry> = {
            let loaded = self.inner.loaded.read().unwrap_or_else(|e| e.into_inner());
            paths
                .filter(|p| !loaded.contains_key(*p))
                .map(|p| {
                    let prepared = self.inner.prepared.get(p).cloned();
                    let prepared_dir = self.inner.prepared_dir.clone();
                    let packed = self.inner.pcm_pack.as_ref().and_then(|pack| {
                        pack.entry_for_path(p)
                            .cloned()
                            .map(|entry| (pack.mmap.clone(), entry))
                    });
                    (p.to_owned(), prepared, prepared_dir, packed)
                })
                .collect()
        };

        let total = paths.len();
        let completed = AtomicUsize::new(0);
        let loaded_n = AtomicUsize::new(0);
        let failed_n = AtomicUsize::new(0);
        let bytes_n = AtomicUsize::new(0);
        let skipped_n = AtomicUsize::new(0);

        for_each_maybe_parallel(&paths, |(path, prepared, prepared_dir, packed)| {
            // The budget is the engine's hard RAM ceiling: past it, the
            // rest of this preload streams from disk instead. Reserve the
            // estimated size FIRST — decodes run in parallel, and charging
            // afterwards lets a poolful of them overshoot together.
            // Materialised by an earlier run? Map it and skip the decode.
            if let Some(mapped) = mapped_from_stream_cache(&self.inner, path, packed.as_ref()) {
                mapped.warm(WARM_HEAD_FRAMES);
                self.insert_loaded(path.clone(), Arc::new(mapped), false);
                loaded_n.fetch_add(1, Ordering::Relaxed);
                completed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let reserved = estimated_decoded_bytes(&self.inner, path, packed.as_ref());
            if !super::budget::try_reserve(reserved) {
                skipped_n.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let result = match packed {
                Some((pack_mmap, entry)) => load_pack_sample(pack_mmap, entry),
                None => match prepared {
                    Some(entry) => {
                        load_prepared_sample(&Some(prepared_dir.clone().unwrap()), entry)
                    }
                    None => load_sample(path),
                },
            };
            // The real size is charged by `insert_loaded`, so the
            // estimate goes back either way.
            super::budget::release(reserved);
            let result = result.map(|data| stream_cached(path, data));
            match result {
                Ok(data) => {
                    data.warm(WARM_HEAD_FRAMES);
                    let bytes = data.decoded_bytes();
                    // Brief write lock per sample — readers (audio thread)
                    // see the new entry as soon as the lock releases. Audio
                    // try_read calls that race with this just return None
                    // for one block, no big deal.
                    self.insert_loaded(path.clone(), Arc::new(data), false);
                    loaded_n.fetch_add(1, Ordering::Relaxed);
                    bytes_n.fetch_add(bytes, Ordering::Relaxed);
                }
                Err(e) => {
                    failed_n.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!("cache: failed to preload {}: {e}", path.display());
                }
            }
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done <= 8 || done == total || done.is_multiple_of(32) {
                self.publish_loaded_snapshot();
            }
            if total >= 100 && (done == total || done.is_multiple_of(250)) {
                tracing::info!("signal-sampler: preloaded {done}/{total} samples");
            }
        });

        // Always publish at the end: samples that took the stream-cache fast
        // path skip the in-loop cadence, and a snapshot nobody published is a
        // cache the audio thread cannot see.
        self.publish_loaded_snapshot();
        PreloadStats {
            loaded: loaded_n.load(Ordering::Relaxed),
            failed: failed_n.load(Ordering::Relaxed),
            bytes: bytes_n.load(Ordering::Relaxed),
            skipped: skipped_n.load(Ordering::Relaxed),
        }
    }

    pub fn preload_cancelable<'a>(
        &self,
        paths: impl Iterator<Item = &'a Path>,
        should_cancel: impl Fn() -> bool + Sync,
    ) -> PreloadStats {
        // Decode in PARALLEL across the rayon pool (FLAC decode is CPU-bound;
        // a sequential per-engine thread left most cores idle and the largest
        // engine — e.g. hats, ~2.8k samples — dominated wall time). The bank's
        // per-engine preload threads all feed the shared pool, so work from
        // every engine interleaves across all cores with no tail imbalance.
        // `should_cancel` (a new preset loaded) short-circuits remaining work.
        let work: Vec<(PathBuf, Option<PackedSampleRef>)> = {
            let loaded = self.inner.loaded.read().unwrap_or_else(|e| e.into_inner());
            paths
                .filter(|p| !loaded.contains_key(*p))
                .map(|p| {
                    let packed = self.inner.pcm_pack.as_ref().and_then(|pack| {
                        pack.entry_for_path(p)
                            .cloned()
                            .map(|entry| (pack.mmap.clone(), entry))
                    });
                    (p.to_owned(), packed)
                })
                .collect()
        };

        let total = work.len();
        let completed = AtomicUsize::new(0);
        let loaded_n = AtomicUsize::new(0);
        let failed_n = AtomicUsize::new(0);
        let bytes_n = AtomicUsize::new(0);
        let skipped_n = AtomicUsize::new(0);

        for_each_maybe_parallel(&work, |(path, packed)| {
            if should_cancel() {
                return;
            }
            if let Some(mapped) = mapped_from_stream_cache(&self.inner, path, packed.as_ref()) {
                mapped.warm(WARM_HEAD_FRAMES);
                self.insert_loaded(path.clone(), Arc::new(mapped), false);
                loaded_n.fetch_add(1, Ordering::Relaxed);
                completed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let reserved = estimated_decoded_bytes(&self.inner, path, packed.as_ref());
            if !super::budget::try_reserve(reserved) {
                skipped_n.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let result = match packed {
                Some((pack_mmap, entry)) => load_pack_sample(pack_mmap, entry),
                None => decode_path(&self.inner, path),
            };
            super::budget::release(reserved);
            let result = result.map(|data| stream_cached(path, data));
            match result {
                Ok(data) => {
                    data.warm(WARM_HEAD_FRAMES);
                    let bytes = data.decoded_bytes();
                    self.insert_loaded(path.clone(), Arc::new(data), false);
                    loaded_n.fetch_add(1, Ordering::Relaxed);
                    bytes_n.fetch_add(bytes, Ordering::Relaxed);
                }
                Err(e) => {
                    failed_n.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!("cache: failed to preload {}: {e}", path.display());
                }
            }
            // Publish the audio-thread snapshot incrementally so voices come
            // online during preload (publish clones the map — batch it).
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done <= 8 || done == total || done.is_multiple_of(64) {
                self.publish_loaded_snapshot();
            }
        });
        self.publish_loaded_snapshot();
        PreloadStats {
            loaded: loaded_n.load(Ordering::Relaxed),
            failed: failed_n.load(Ordering::Relaxed),
            bytes: bytes_n.load(Ordering::Relaxed),
            skipped: skipped_n.load(Ordering::Relaxed),
        }
    }

    /// Number of samples currently in cache.
    pub fn len(&self) -> usize {
        self.inner.loaded_snapshot.load().len()
    }

    pub fn bytes(&self) -> usize {
        self.inner
            .loaded_snapshot
            .load()
            .values()
            .map(|data| data.decoded_bytes())
            .sum()
    }

    /// Evict decoded samples until this cache is at or below `budget_bytes`.
    ///
    /// This removes entries from the cache map and publishes a new audio-thread
    /// snapshot. Existing voices may still hold `Arc<SampleData>` references,
    /// so their playback is not interrupted; the data is released once those
    /// voices finish.
    pub fn evict_until_under_budget(&self, budget_bytes: usize) -> EvictStats {
        let (stats, changed) = {
            let mut loaded = self.inner.loaded.write().unwrap_or_else(|e| e.into_inner());
            let bytes_before = loaded
                .values()
                .map(|data| data.decoded_bytes())
                .sum::<usize>();
            if bytes_before <= budget_bytes {
                return EvictStats {
                    bytes_before,
                    bytes_after: bytes_before,
                    ..EvictStats::default()
                };
            }

            let mut candidates = loaded
                .iter()
                .map(|(path, data)| (path.clone(), data.decoded_bytes()))
                .collect::<Vec<_>>();
            candidates.sort_by(|(left_path, left_bytes), (right_path, right_bytes)| {
                right_bytes
                    .cmp(left_bytes)
                    .then_with(|| left_path.cmp(right_path))
            });

            let mut bytes_after = bytes_before;
            let mut evicted = 0;
            for (path, bytes) in candidates {
                if bytes_after <= budget_bytes {
                    break;
                }
                if loaded.remove(&path).is_some() {
                    evicted += 1;
                    bytes_after = bytes_after.saturating_sub(bytes);
                    super::budget::release(bytes);
                }
            }

            (
                EvictStats {
                    evicted,
                    bytes_before,
                    bytes_after,
                    bytes_freed: bytes_before.saturating_sub(bytes_after),
                },
                evicted > 0,
            )
        };

        if changed {
            self.publish_loaded_snapshot();
        }
        stats
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn snapshot_len(&self) -> usize {
        self.inner.loaded_snapshot.load().len()
    }

    pub fn republish_snapshot(&self) {
        self.publish_loaded_snapshot();
    }

    fn insert_loaded(&self, path: PathBuf, data: Arc<SampleData>, publish: bool) {
        {
            let bytes = data.decoded_bytes();
            let mut loaded = self.inner.loaded.write().unwrap_or_else(|e| e.into_inner());
            let replaced = loaded.insert(path, data);
            // The process-wide preload budget is charged here because this is
            // the one door every decoded sample comes through.
            if let Some(old) = replaced {
                super::budget::release(old.decoded_bytes());
            }
            super::budget::charge(bytes);
        }
        if publish {
            self.publish_loaded_snapshot();
        }
    }

    fn publish_loaded_snapshot(&self) {
        // Clone and store under the WRITE lock: publishes serialize, so the
        // last one to store always carries the latest map. Under a read lock
        // two publishers could interleave — A clones, B inserts and stores,
        // A stores its older clone — and B's sample vanished from the
        // audio-thread view for good (a streaming pack's notes then dropped
        // at random, all or nothing per run).
        let loaded = self.inner.loaded.write().unwrap_or_else(|e| e.into_inner());
        self.inner.loaded_snapshot.store(Arc::new(loaded.clone()));
    }
}

/// Run `f` over every item — fanned out across the rayon pool on native,
/// plainly sequential under bare `engine-core` (wasm has no threads).
#[cfg(feature = "engine-native")]
fn for_each_maybe_parallel<T: Sync>(items: &[T], f: impl Fn(&T) + Sync + Send) {
    items.par_iter().for_each(f);
}

#[cfg(not(feature = "engine-native"))]
fn for_each_maybe_parallel<T>(items: &[T], f: impl Fn(&T)) {
    items.iter().for_each(f);
}

/// What a sample will cost once decoded, in bytes, without decoding it.
///
/// Packs and prepared caches declare their frame count, so the estimate is
/// exact (f32 per sample). A bare file on disk has no index — 2 MiB is the
/// house guess, which the real size replaces the moment it lands.
fn estimated_decoded_bytes(
    inner: &CacheInner,
    path: &Path,
    packed: Option<&PackedSampleRef>,
) -> usize {
    const F32: usize = std::mem::size_of::<f32>();
    const UNKNOWN: usize = 2 * 1024 * 1024;
    // Raw-PCM entries are mapped, not decoded: they cost no anonymous memory,
    // so they never spend budget and never stop a preload. A STREAMED FLAC
    // entry keeps only its head resident (i16 frames, plus the block index),
    // so that is what it costs — charging its whole decoded size made a big
    // streaming pack "exceed" the budget on paper, and the preload then
    // skipped entries at random (their notes dropped, all or nothing).
    let cost = |entry: &PackEntry| {
        if entry.mapped_fmt().is_some() {
            0
        } else if should_stream(entry) {
            super::stream::HEAD_FRAMES as usize * entry.channels as usize * 2 + 16 * 1024
        } else {
            entry.samples() * F32
        }
    };
    if let Some((_, entry)) = packed {
        return cost(entry);
    }
    if let Some(pack) = inner.pcm_pack.as_ref() {
        if let Some(entry) = pack.entry_for_path(path) {
            return cost(entry);
        }
    }
    if let Some(entry) = inner.prepared.get(path) {
        return entry.samples * F32;
    }
    UNKNOWN
}

/// A sample that has already been through the stream cache, mapped back in
/// without decoding anything. Needs the shape up front, which pack and
/// prepared entries carry and a bare file on disk does not.
#[cfg(feature = "engine-native")]
fn mapped_from_stream_cache(
    inner: &CacheInner,
    path: &Path,
    packed: Option<&PackedSampleRef>,
) -> Option<SampleData> {
    super::stream_cache::dir()?;
    let (channels, sample_rate, num_frames) = if let Some((_, entry)) = packed {
        if entry.mapped_fmt().is_some() {
            return None; // already mapped straight out of the pack
        }
        (entry.channels(), entry.sample_rate(), entry.num_frames())
    } else if let Some(entry) = inner.pcm_pack.as_ref().and_then(|p| p.entry_for_path(path)) {
        if entry.mapped_fmt().is_some() {
            return None;
        }
        (entry.channels(), entry.sample_rate(), entry.num_frames())
    } else if let Some(entry) = inner.prepared.get(path) {
        (entry.channels, entry.sample_rate, entry.num_frames)
    } else {
        return None;
    };
    super::stream_cache::lookup(path, channels, sample_rate, num_frames)
}

fn decode_path(inner: &CacheInner, path: &Path) -> Result<SampleData, SamplerError> {
    if let Some(pack) = inner.pcm_pack.as_ref() {
        if let Some(entry) = pack.entry_for_path(path) {
            // Raw PCM maps directly; compressed entries can still come back
            // mapped if this sample has been through the stream cache.
            if let Some(mapped) = mapped_from_stream_cache(inner, path, None) {
                return Ok(mapped);
            }
            let decoded = load_pack_sample(&pack.mmap, entry)?;
            return Ok(stream_cached(path, decoded));
        }
    }
    if let Some(entry) = inner.prepared.get(path) {
        return load_prepared_sample(&inner.prepared_dir, entry);
    }
    Ok(stream_cached(path, load_sample(path)?))
}

/// Hand a freshly decoded sample to the stream cache; play the mapping it
/// gives back, or the decoded audio when the cache is off.
#[cfg(feature = "engine-native")]
fn stream_cached(path: &Path, data: SampleData) -> SampleData {
    if matches!(data.pcm, Pcm::Mapped { .. }) || super::stream_cache::dir().is_none() {
        return data;
    }
    super::stream_cache::materialize(path, &data).unwrap_or(data)
}

/// No on-disk stream cache without the native half: nothing to look up, and
/// freshly decoded samples stay decoded.
#[cfg(not(feature = "engine-native"))]
fn mapped_from_stream_cache(
    _inner: &CacheInner,
    _path: &Path,
    _packed: Option<&PackedSampleRef>,
) -> Option<SampleData> {
    None
}

#[cfg(not(feature = "engine-native"))]
fn stream_cached(_path: &Path, data: SampleData) -> SampleData {
    data
}

#[cfg(feature = "engine-native")]
fn load_prepared_index(
    cache_dir: &Path,
    prepared_out: &mut HashMap<PathBuf, PreparedEntry>,
    prepared_dir_out: &mut Option<PathBuf>,
) -> Result<(), SamplerError> {
    let index = File::open(cache_dir.join("index.tsv"))?;
    let mut prepared = HashMap::new();

    for line in BufReader::new(index).lines() {
        let line = line?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let source = fields
            .next()
            .ok_or_else(|| invalid_data("prepared cache index missing source path"))?;
        let pcm_file = fields
            .next()
            .ok_or_else(|| invalid_data("prepared cache index missing pcm file"))?;
        let channels = parse_field(fields.next(), "channels")?;
        let sample_rate = parse_field(fields.next(), "sample_rate")?;
        let num_frames = parse_field(fields.next(), "num_frames")?;
        let samples = parse_field(fields.next(), "samples")?;

        prepared.insert(
            PathBuf::from(source),
            PreparedEntry {
                pcm_file: PathBuf::from(pcm_file),
                channels,
                sample_rate,
                num_frames,
                samples,
            },
        );
    }

    tracing::info!(
        "signal-sampler: loaded prepared cache index {} ({} samples)",
        cache_dir.display(),
        prepared.len()
    );
    *prepared_out = prepared;
    *prepared_dir_out = Some(cache_dir.to_owned());
    Ok(())
}

// ── Sample loaders ────────────────────────────────────────────────────────────

pub fn load_sample(path: &Path) -> Result<SampleData, SamplerError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("flac") => load_flac(path),
        Some(ext) if ext.eq_ignore_ascii_case("aif") || ext.eq_ignore_ascii_case("aiff") => {
            load_aiff(path)
        }
        _ => load_wav(path),
    }
}

/// Decode a standard AIFF (`FORM ... AIFF`) file.
///
/// Used by Stylus RMX groove loops. Handles 16-bit and 24-bit big-endian PCM.
/// Sample rate is parsed from the 80-bit IEEE-754 extended-precision field
/// in the `COMM` chunk and rounded to a `u32` (Stylus uses 44100).
fn load_aiff(path: &Path) -> Result<SampleData, SamplerError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"FORM" || &bytes[8..12] != b"AIFF" {
        return Err(invalid_data(format!(
            "{}: not a FORM/AIFF file",
            path.display()
        )));
    }
    let mut channels: u16 = 0;
    let mut bits: u16 = 0;
    let mut sample_rate: u32 = 0;
    let mut declared_frames: u32 = 0;
    let mut ssnd: Option<(usize, usize)> = None;

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_be_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let data = pos + 8;
        let end = data + size;
        if end > bytes.len() {
            break;
        }
        match id {
            b"COMM" if size >= 18 => {
                channels = u16::from_be_bytes([bytes[data], bytes[data + 1]]);
                declared_frames = u32::from_be_bytes([
                    bytes[data + 2],
                    bytes[data + 3],
                    bytes[data + 4],
                    bytes[data + 5],
                ]);
                bits = u16::from_be_bytes([bytes[data + 6], bytes[data + 7]]);
                sample_rate = parse_extended80(&bytes[data + 8..data + 18]) as u32;
            }
            b"SSND" if size >= 8 => {
                let offset = u32::from_be_bytes([
                    bytes[data],
                    bytes[data + 1],
                    bytes[data + 2],
                    bytes[data + 3],
                ]) as usize;
                let pcm_start = data + 8 + offset;
                let pcm_end = end;
                if pcm_end >= pcm_start {
                    ssnd = Some((pcm_start, pcm_end));
                }
            }
            _ => {}
        }
        pos = end + (size & 1);
    }

    let (pcm_start, pcm_end) =
        ssnd.ok_or_else(|| invalid_data(format!("{}: AIFF missing SSND chunk", path.display())))?;
    if channels == 0 || sample_rate == 0 || bits == 0 {
        return Err(invalid_data(format!(
            "{}: AIFF missing or invalid COMM",
            path.display()
        )));
    }
    let pcm = &bytes[pcm_start..pcm_end];
    let frames: Vec<f32> = match bits {
        16 => {
            let scale = 1.0_f32 / 32768.0;
            pcm.chunks_exact(2)
                .map(|c| (i16::from_be_bytes([c[0], c[1]]) as f32) * scale)
                .collect()
        }
        24 => {
            let scale = 1.0_f32 / 8_388_608.0;
            pcm.chunks_exact(3)
                .map(|c| {
                    let mut v =
                        ((c[0] as i32) << 24) | ((c[1] as i32) << 16) | ((c[2] as i32) << 8);
                    v >>= 8;
                    (v as f32) * scale
                })
                .collect()
        }
        32 => {
            let scale = 1.0_f32 / 2_147_483_648.0;
            pcm.chunks_exact(4)
                .map(|c| (i32::from_be_bytes([c[0], c[1], c[2], c[3]]) as f32) * scale)
                .collect()
        }
        other => {
            return Err(invalid_data(format!(
                "{}: unsupported AIFF bit depth {other} (declared {bits})",
                path.display()
            )));
        }
    };

    let num_frames = if declared_frames > 0 {
        declared_frames as usize
    } else {
        frames.len() / channels as usize
    };
    Ok(SampleData::from_f32(
        frames,
        channels,
        sample_rate,
        num_frames,
    ))
}

fn parse_extended80(b: &[u8]) -> f64 {
    let sign = (b[0] & 0x80) != 0;
    let exp = (((b[0] & 0x7F) as u16) << 8) | (b[1] as u16);
    let mant = u64::from_be_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
    if exp == 0 && mant == 0 {
        return 0.0;
    }
    let unbiased = exp as i32 - 16383;
    let f = (mant as f64) * 2f64.powi(unbiased - 63);
    if sign {
        -f
    } else {
        f
    }
}

fn load_wav(path: &Path) -> Result<SampleData, SamplerError> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;

    let spec = reader.spec();
    let channels = spec.channels;
    let sample_rate = spec.sample_rate;

    let frames: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string()))))
            .collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| {
                    s.map(|v| v as f32 / max)
                        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))
                })
                .collect::<Result<_, _>>()?
        }
    };

    let num_frames = frames.len() / channels as usize;
    Ok(SampleData::from_f32(
        frames,
        channels,
        sample_rate,
        num_frames,
    ))
}

fn load_prepared_sample(
    cache_dir: &Option<PathBuf>,
    entry: &PreparedEntry,
) -> Result<SampleData, SamplerError> {
    let cache_dir = cache_dir
        .as_ref()
        .ok_or_else(|| invalid_data("prepared cache entry without cache directory"))?;
    let mut file = File::open(cache_dir.join(&entry.pcm_file))?;
    let mut bytes = vec![0u8; entry.samples * std::mem::size_of::<f32>()];
    file.read_exact(&mut bytes)?;

    let mut frames = Vec::with_capacity(entry.samples);
    for chunk in bytes.chunks_exact(4) {
        frames.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    Ok(SampleData::from_f32(
        frames,
        entry.channels,
        entry.sample_rate,
        entry.num_frames,
    ))
}

impl SignalPcmPack {
    /// Open a `.signalpack` and parse its header + index.
    /// No audio is decoded.
    #[cfg(feature = "engine-native")]
    pub fn open(path: &Path) -> Result<Self, SamplerError> {
        let file = File::open(path)?;
        // Memory-map the whole pack once; header/index parse and sample
        // decode all slice from this.
        // Safety: the pack file is read-only for the cache's lifetime; we never
        // write it, and external truncation would be a deployment error.
        let mmap = Arc::new(unsafe { memmap2::Mmap::map(&file)? });
        Self::from_pack_bytes(path.to_owned(), PackBytes::Mapped(mmap))
    }

    /// Open a `.signalpack` handed over as one in-memory buffer — the wasm
    /// path (a fetched pack), and anything else with no file to map. Same
    /// parse as [`open`](Self::open); no audio is decoded.
    pub fn open_bytes(bytes: Vec<u8>) -> Result<Self, SamplerError> {
        Self::from_pack_bytes(PathBuf::from("<memory>"), PackBytes::from(bytes))
    }

    /// Open a pack whose bytes live OUTSIDE this process, reachable only
    /// through the installed [external reader](set_external_pack_reader) —
    /// the browser seam, where pack buffers stay on the JS heap. Same
    /// header + index parse as [`open`](Self::open), fetched through
    /// [`PackBytes::read_range`]; no audio is decoded, and audio entries
    /// materialize per entry at decode time.
    pub fn open_external(id: u32, len: u64) -> Result<Self, SamplerError> {
        Self::from_pack_bytes(
            PathBuf::from(format!("<external:{id}>")),
            PackBytes::External { id, len },
        )
    }

    /// Parse header + index out of the pack bytes, however they arrived.
    /// All reads go through [`PackBytes::read_range`] — this must work for
    /// the External arm too.
    fn from_pack_bytes(path: PathBuf, data: PackBytes) -> Result<Self, SamplerError> {
        if data.len() < PackFileHeader::LEN {
            return Err(invalid_data("signal pack is shorter than its header"));
        }
        let header_bytes = data.read_range(0, PackFileHeader::LEN)?;
        let header_raw: [u8; PackFileHeader::LEN] = header_bytes
            .as_ref()
            .try_into()
            .map_err(|_| invalid_data("signal pack is shorter than its header"))?;
        let header = PackFileHeader::parse(header_raw)?;
        let kind = header.require_known_kind()?;

        let index_len = usize::try_from(header.index_len())
            .map_err(|_| invalid_data("signal pack index out of bounds"))?;
        let index = data
            .read_range(header.index_offset(), index_len)
            .map_err(|_| invalid_data("signal pack index out of bounds"))?;
        let index: &[u8] = &index;

        let mut entries = HashMap::new();
        let mut in_embedded_spec = false;
        let mut spec_buf = String::new();
        let mut spec_format: Option<String> = None;
        for line in String::from_utf8_lossy(index).lines() {
            if line == "# spec_begin" {
                in_embedded_spec = true;
                continue;
            }
            if line == "# spec_end" {
                in_embedded_spec = false;
                continue;
            }
            if in_embedded_spec {
                spec_buf.push_str(line);
                spec_buf.push('\n');
                continue;
            }
            if let Some(rest) = line.strip_prefix("# spec_format\t") {
                spec_format = Some(rest.trim().to_string());
                continue;
            }
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let source = fields
                .next()
                .ok_or_else(|| invalid_data("signal pack index missing source path"))?;
            let offset = parse_field(fields.next(), "offset")?;
            let bytes = parse_field(fields.next(), "bytes")?;
            let channels = parse_field(fields.next(), "channels")?;
            let sample_rate = parse_field(fields.next(), "sample_rate")?;
            let num_frames = parse_field(fields.next(), "num_frames")?;
            let samples = parse_field(fields.next(), "samples")?;

            entries.insert(
                PathBuf::from(source),
                PackEntry {
                    kind,
                    offset,
                    bytes,
                    channels,
                    sample_rate,
                    num_frames,
                    samples,
                },
            );
        }

        let embedded_spec = if spec_buf.is_empty() {
            None
        } else {
            Some(spec_buf)
        };

        Ok(Self {
            path,
            mmap: data,
            kind,
            index_span: (header.index_offset(), header.index_len()),
            entries,
            embedded_spec,
            embedded_spec_format: spec_format,
        })
    }

    /// The header's index span `(offset, len)` — together with the fixed
    /// 64-byte header, the exact bytes [`open`](Self::open) reads before
    /// any audio: the rank-0 set for range streaming.
    pub fn index_span(&self) -> (u64, u64) {
        self.index_span
    }

    /// Embedded library spec text, if the pack carried one.
    pub fn embedded_spec(&self) -> Option<&str> {
        self.embedded_spec.as_deref()
    }

    /// Embedded spec format (`"styx"` or `"toml"`), if known.
    pub fn embedded_spec_format(&self) -> Option<&str> {
        self.embedded_spec_format.as_deref()
    }

    /// Number of audio entries indexed in the pack.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Header kind: 5 = FLAC i24 (lossless), 6 = Ogg Vorbis (proxy).
    pub fn kind(&self) -> u32 {
        self.kind
    }

    /// Human label for [`kind`](Self::kind).
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            SIGNAL_PACK_KIND_FLAC_I24 => "flac-i24",
            SIGNAL_PACK_KIND_OGG_VORBIS => "ogg-vorbis",
            SIGNAL_PACK_KIND_PCM_I16 => "pcm-i16",
            SIGNAL_PACK_KIND_PCM_I24 => "pcm-i24",
            _ => "unknown",
        }
    }

    /// Iterate (source-path, entry) pairs for debug inspection.
    pub fn entries_iter(&self) -> impl Iterator<Item = (&PathBuf, &PackEntry)> {
        self.entries.iter()
    }

    /// The pack's backing bytes, for readers that stream out of it.
    pub fn mmap_handle(&self) -> PackBytes {
        self.mmap.clone()
    }

    /// Pack file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Iterate over the relative paths of every entry stored in the pack.
    /// Useful when reconstructing a [`crate::SampleMap`] from a pack alone,
    /// without any on-disk source samples.
    pub fn entry_paths(&self) -> impl Iterator<Item = &Path> {
        self.entries.keys().map(|p| p.as_path())
    }

    pub(crate) fn entry_for_path(&self, path: &Path) -> Option<&PackEntry> {
        if let Some(entry) = self.entries.get(path) {
            return Some(entry);
        }

        for suffix in path_suffixes(path) {
            if let Some(entry) = self.entries.get(&suffix) {
                return Some(entry);
            }
        }

        None
    }
}

fn path_suffixes(path: &Path) -> Vec<PathBuf> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect::<Vec<_>>();
    let mut suffixes = Vec::new();
    for start in 0..components.len() {
        let mut suffix = PathBuf::new();
        for component in &components[start..] {
            suffix.push(component);
        }
        if !suffix.as_os_str().is_empty() {
            suffixes.push(suffix);
        }
    }
    suffixes
}

/// The queue's two halves: work still to do, and the set of paths
/// already queued so a held chord doesn't enqueue one sample twice.
#[cfg(feature = "engine-native")]
type WarmPending = (
    Vec<(SampleCache, PathBuf)>,
    std::collections::HashSet<PathBuf>,
);

/// Background loader for cache misses. One thread, deduplicated, so a held
/// chord of unloaded notes queues each sample once.
#[cfg(feature = "engine-native")]
struct WarmQueue {
    queue: std::sync::Mutex<WarmPending>,
    wake: std::sync::Condvar,
}

#[cfg(feature = "engine-native")]
impl WarmQueue {
    fn send(&self, cache: SampleCache, path: PathBuf) {
        if let Ok(mut q) = self.queue.lock() {
            if q.1.insert(path.clone()) {
                q.0.push((cache, path));
                self.wake.notify_one();
            }
        }
    }

    fn run(&self) {
        loop {
            let batch = {
                let Ok(mut q) = self.queue.lock() else { return };
                while q.0.is_empty() {
                    let Ok(next) = self.wake.wait(q) else { return };
                    q = next;
                }
                let taken = std::mem::take(&mut q.0);
                q.1.clear();
                taken
            };
            for (cache, path) in batch {
                let _ = cache.get(&path);
            }
        }
    }
}

#[cfg(feature = "engine-native")]
fn warm_queue() -> &'static WarmQueue {
    static QUEUE: std::sync::OnceLock<WarmQueue> = std::sync::OnceLock::new();
    QUEUE.get_or_init(|| {
        let q = WarmQueue {
            queue: std::sync::Mutex::new((Vec::new(), std::collections::HashSet::new())),
            wake: std::sync::Condvar::new(),
        };
        let _ = std::thread::Builder::new()
            .name("signal-warm".into())
            .spawn(|| warm_queue().run());
        q
    })
}

/// Whether a pack entry should stream rather than decode whole.
///
/// Streaming costs a little CPU per chunk and a background thread, which is
/// not worth it for a short one-shot: below the threshold a sample is smaller
/// decoded than the machinery around it. `FTS_STREAM=off` decodes everything
/// (offline renders, analysis tools); `FTS_STREAM=on` streams everything it
/// can.
fn should_stream(entry: &PackEntry) -> bool {
    if entry.mapped_fmt().is_some() {
        return false; // raw PCM already reads from the mapping
    }
    match std::env::var("FTS_STREAM").ok().as_deref() {
        Some("off" | "0" | "false") => return false,
        Some("on" | "1" | "true") => return true,
        _ => {}
    }
    // Only worth it when the sample outlives its own head: anything shorter
    // is already fully covered by the head, so streaming it would add a
    // decoder and save nothing.
    let head = super::stream::HEAD_FRAMES as usize * entry.channels.max(1) as usize;
    entry.samples >= head * 2
}

/// Read a pack entry.
///
/// Raw-PCM entries are **mapped, not decoded**: the returned `SampleData`
/// points into the pack's mapping, costs no anonymous memory, and pages in as
/// the voice plays it — the OS is the streaming engine. Compressed entries
/// decode to float as before.
fn load_pack_sample(map: &PackBytes, entry: &PackEntry) -> Result<SampleData, SamplerError> {
    // External packs: materialize THIS ENTRY's bytes once (a few MB — the
    // pack itself never enters this address space) and continue on an Owned
    // pack positioned at 0. Everything below — the mapped-PCM window, the
    // streaming open, the whole-entry decode — then reads in-process bytes,
    // and nothing ever derefs the External arm.
    if map.is_external() {
        let span = match entry.mapped_fmt() {
            Some(fmt) => entry
                .samples
                .checked_mul(fmt.width())
                .ok_or_else(|| invalid_data("signal pack PCM entry out of bounds"))?,
            None => usize::try_from(entry.bytes)
                .map_err(|_| invalid_data("signal pack entry out of bounds"))?,
        };
        let owned = map.read_range(entry.offset, span)?.into_owned();
        let mut local = entry.clone();
        local.offset = 0;
        return load_pack_sample(&PackBytes::from(owned), &local);
    }
    if let Some(fmt) = entry.mapped_fmt() {
        let start = entry.offset as usize;
        let end = start
            .checked_add(entry.samples * fmt.width())
            .filter(|&e| e <= map.len())
            .ok_or_else(|| invalid_data("signal pack PCM entry out of bounds"))?;
        let _ = end;
        return Ok(SampleData::mapped(
            map.clone(),
            start,
            entry.samples,
            fmt,
            entry.channels,
            entry.sample_rate,
            entry.num_frames,
        ));
    }
    // FLAC entries big enough to be worth it are STREAMED: head resident,
    // the rest decoded a chunk at a time straight out of the pack. This is
    // the Kontakt/Omnisphere model — the library stays compressed on disk and
    // never lands in the heap.
    if should_stream(entry) {
        if let Some(streamed) = super::stream::StreamedSample::open(
            map.clone(),
            entry.offset as usize,
            entry.bytes as usize,
            entry.channels,
            entry.sample_rate,
            entry.num_frames,
        ) {
            return Ok(SampleData::streamed(streamed));
        }
    }
    let bytes = map
        .read_range(entry.offset, entry.bytes as usize)
        .map_err(|_| invalid_data("signal pack entry out of bounds"))?;
    let bytes: &[u8] = &bytes;
    // Dispatch on the entry's own payload magic (not the pack-level kind) —
    // robust and leaves room for mixed-codec packs.
    let data = if bytes.starts_with(b"OggS") {
        load_ogg_vorbis_bytes(bytes)?
    } else {
        let data = load_flac_bytes(bytes)?;
        // flacenc pads the final block with silence (see `encode_flac_i24`),
        // so decode may run LONG; that trims below. Decoding SHORT of the
        // index is real corruption.
        if data.num_frames < entry.num_frames || data.len() < entry.samples {
            return Err(invalid_data(
                "signal pack FLAC decoded short of index metadata",
            ));
        }
        data
    };
    if data.channels != entry.channels || data.sample_rate != entry.sample_rate {
        return Err(invalid_data(
            "signal pack entry metadata does not match index",
        ));
    }
    // The index carries the SOURCE PCM length. Lossy codecs may decode a few
    // frames long/short at the tail; coerce to the authoritative length so
    // frame-indexed zone metadata (loop points, sample_start/end) stays exact.
    Ok(coerce_to_index_len(data, entry))
}

/// Trim or silence-pad decoded audio to the index's authoritative
/// `num_frames`/`samples`. No-op when they already match.
fn coerce_to_index_len(data: SampleData, entry: &PackEntry) -> SampleData {
    if data.num_frames == entry.num_frames && data.len() == entry.samples {
        return data;
    }
    let mut frames = data.to_f32().into_owned();
    frames.resize(entry.samples, 0.0);
    SampleData::from_f32(frames, data.channels, data.sample_rate, entry.num_frames)
}

#[cfg(feature = "engine-native")]
#[derive(Debug, Clone, Copy, Default)]
pub struct PrepareStats {
    pub prepared: usize,
    pub failed: usize,
    pub bytes: usize,
}

pub fn default_prepared_cache_dir(samples_root: &Path) -> PathBuf {
    samples_root.join(".signal-cache-v1")
}

pub fn default_signal_pack_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("library.signalpack")
}

#[cfg(feature = "engine-native")]
pub fn prepare_sample_cache<'a>(
    cache_dir: &Path,
    paths: impl Iterator<Item = &'a Path>,
) -> Result<PrepareStats, SamplerError> {
    let paths = paths.map(Path::to_owned).collect::<Vec<_>>();
    if cache_dir.exists() {
        std::fs::remove_dir_all(cache_dir)?;
    }
    std::fs::create_dir_all(cache_dir)?;

    let total = paths.len();
    let completed = AtomicUsize::new(0);
    let prepared = paths
        .par_iter()
        .enumerate()
        .map(|(i, path)| {
            let result = prepare_one_sample(cache_dir, i, path);
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if done == total || done.is_multiple_of(100) {
                tracing::info!("signal-sampler: prepared {done}/{total} samples");
                eprintln!("prepared {done}/{total} samples");
            }
            result
        })
        .collect::<Vec<_>>();

    let mut index = BufWriter::new(File::create(cache_dir.join("index.tsv"))?);
    writeln!(
        index,
        "# source\tpcm_file\tchannels\tsample_rate\tnum_frames\tsamples"
    )?;

    let mut stats = PrepareStats::default();
    for result in prepared {
        match result {
            Ok(entry) => {
                stats.bytes += entry.samples * std::mem::size_of::<f32>();
                stats.prepared += 1;
                writeln!(
                    index,
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    entry.source.display(),
                    entry.pcm_file.display(),
                    entry.channels,
                    entry.sample_rate,
                    entry.num_frames,
                    entry.samples
                )?;
            }
            Err((path, err)) => {
                stats.failed += 1;
                tracing::warn!(
                    "signal-sampler: failed to prepare {}: {err}",
                    path.display()
                );
            }
        }
    }
    index.flush()?;
    Ok(stats)
}

/// Where a pack's embedded spec comes from: an on-disk file or in-memory text
/// (builders that synthesize per-group specs — e.g. the Cinematic Studio
/// splitter — never touch disk).
#[cfg(feature = "engine-native")]
pub enum PackSpecSource<'s> {
    Path(&'s Path),
    Text { text: &'s str, format: &'s str },
}

#[cfg(feature = "engine-native")]
pub fn create_signal_pack<'a>(
    pack_path: &Path,
    spec_path: &Path,
    samples_root: &Path,
    paths: impl Iterator<Item = &'a Path>,
) -> Result<PrepareStats, SamplerError> {
    create_signal_pack_with(
        pack_path,
        PackSpecSource::Path(spec_path),
        samples_root,
        paths,
        PackCodec::FlacI24,
    )
}

#[cfg(feature = "engine-native")]
pub fn create_signal_pack_with<'a>(
    pack_path: &Path,
    spec: PackSpecSource<'_>,
    samples_root: &Path,
    paths: impl Iterator<Item = &'a Path>,
    codec: PackCodec,
) -> Result<PrepareStats, SamplerError> {
    let paths = paths.map(Path::to_owned).collect::<Vec<_>>();
    if let Some(parent) = pack_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let total = paths.len();
    let completed = AtomicUsize::new(0);
    let packed = paths
        .par_iter()
        .map(|path| {
            let result = pack_one_sample(path, codec);
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if total >= 100 && (done == total || done.is_multiple_of(100)) {
                tracing::info!("signal-sampler: packed {done}/{total} samples");
                eprintln!("packed {done}/{total} samples");
            }
            result
        })
        .collect::<Vec<_>>();

    let (spec_text, spec_format, spec_origin) = match spec {
        PackSpecSource::Path(spec_path) => (
            std::fs::read_to_string(spec_path)?,
            spec_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("styx")
                .to_string(),
            spec_path.display().to_string(),
        ),
        PackSpecSource::Text { text, format } => {
            (text.to_string(), format.to_string(), "<inline>".to_string())
        }
    };
    write_signal_pack_file(
        pack_path,
        &spec_text,
        &spec_format,
        &spec_origin,
        codec,
        packed,
        Some(samples_root),
    )
}

/// Re-encode an existing pack's audio entries with a different codec, copying
/// the embedded spec and per-entry index metadata (source frame counts —
/// loop points stay sample-exact) verbatim. No source samples needed: this is
/// how a lossless Full pack becomes an Ogg Vorbis Proxy pack (or back).
#[cfg(feature = "engine-native")]
pub fn transcode_signal_pack(
    in_path: &Path,
    out_path: &Path,
    codec: PackCodec,
) -> Result<PrepareStats, SamplerError> {
    let pack = SignalPcmPack::open(in_path)?;
    let spec_text = pack
        .embedded_spec()
        .ok_or_else(|| invalid_data("pack carries no embedded spec"))?
        .to_string();
    let spec_format = pack.embedded_spec_format().unwrap_or("styx").to_string();

    // Deterministic output ordering.
    let mut entries: Vec<(PathBuf, PackEntry)> = pack
        .entries
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let total = entries.len();
    let completed = AtomicUsize::new(0);
    let packed: Vec<Result<PackIndexRow, (PathBuf, SamplerError)>> = entries
        .par_iter()
        .map(|(source, entry)| {
            let row = (|| {
                let data = load_pack_sample(&pack.mmap, entry)?;
                let payload = match codec {
                    PackCodec::OggVorbis { quality } => {
                        encode_ogg_vorbis(&data.to_f32(), data.channels, data.sample_rate, quality)?
                    }
                    PackCodec::FlacI24 => {
                        let samples = data
                            .to_f32()
                            .iter()
                            .map(|s| f32_to_i24_i32(*s))
                            .collect::<Vec<_>>();
                        encode_flac_i24(&samples, data.channels, data.sample_rate)?
                    }
                    PackCodec::PcmI16 => encode_pcm_i16(&data.to_f32()),
                    PackCodec::PcmI24 => encode_pcm_i24(&data.to_f32()),
                };
                Ok(PackIndexRow {
                    source: source.clone(),
                    offset: 0,
                    bytes: payload.len() as u64,
                    uncompressed_bytes: (entry.samples * 3) as u64,
                    // Index metadata copied from the source pack — the
                    // authoritative PCM truth, NOT the re-decode.
                    channels: entry.channels,
                    sample_rate: entry.sample_rate,
                    num_frames: entry.num_frames,
                    samples: entry.samples,
                    payload,
                })
            })();
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if total >= 100 && (done == total || done.is_multiple_of(100)) {
                eprintln!("transcoded {done}/{total} samples");
            }
            row.map_err(|e: SamplerError| (source.clone(), e))
        })
        .collect();

    write_signal_pack_file(
        out_path,
        &spec_text,
        &spec_format,
        &format!("<transcode:{}>", in_path.display()),
        codec,
        packed,
        None,
    )
}

/// Shared pack-file writer: header + payload body + index (embedded spec +
/// per-entry rows). `samples_root` (when given) relativizes row source paths.
#[cfg(feature = "engine-native")]
fn write_signal_pack_file(
    pack_path: &Path,
    spec_text: &str,
    spec_format: &str,
    spec_origin: &str,
    codec: PackCodec,
    packed: Vec<Result<PackIndexRow, (PathBuf, SamplerError)>>,
    samples_root: Option<&Path>,
) -> Result<PrepareStats, SamplerError> {
    if let Some(parent) = pack_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut pack = BufWriter::new(File::create(pack_path)?);
    pack.write_all(&[0u8; SIGNAL_PACK_HEADER_LEN])?;

    let mut stats = PrepareStats::default();
    let mut rows = Vec::with_capacity(packed.len());
    let mut offset = SIGNAL_PACK_HEADER_LEN as u64;
    for result in packed {
        match result {
            Ok(mut row) => {
                row.offset = offset;
                pack.write_all(&row.payload)?;
                offset += row.bytes;
                stats.bytes += row.uncompressed_bytes as usize;
                stats.prepared += 1;
                if let Some(root) = samples_root {
                    if let Ok(relative) = row.source.strip_prefix(root) {
                        row.source = relative.to_owned();
                    }
                }
                rows.push(row);
            }
            Err((path, err)) => {
                stats.failed += 1;
                tracing::warn!("signal-sampler: failed to pack {}: {err}", path.display());
            }
        }
    }

    let index_offset = offset;
    let mut index = Vec::new();
    writeln!(index, "# signalpack-index-v1")?;
    writeln!(index, "# spec_path\t{spec_origin}")?;
    writeln!(index, "# spec_format\t{spec_format}")?;
    writeln!(index, "# spec_begin")?;
    index.extend_from_slice(spec_text.as_bytes());
    if !spec_text.ends_with('\n') {
        writeln!(index)?;
    }
    writeln!(index, "# spec_end")?;
    writeln!(
        index,
        "# source\toffset\tbytes\tchannels\tsample_rate\tnum_frames\tsamples"
    )?;
    for row in &rows {
        writeln!(
            index,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.source.display(),
            row.offset,
            row.bytes,
            row.channels,
            row.sample_rate,
            row.num_frames,
            row.samples
        )?;
    }
    pack.write_all(&index)?;
    pack.flush()?;
    drop(pack);

    let mut file = File::options().write(true).open(pack_path)?;
    let header = PackFileHeader::new(
        codec.header_kind(),
        index_offset,
        index.len() as u64,
        stats.prepared as u64,
    );
    file.seek(SeekFrom::Start(0))?;
    file.write_all(header.as_bytes())?;

    Ok(stats)
}

#[cfg(feature = "engine-native")]
pub fn extract_signal_pack(
    pack_path: &Path,
    output_dir: &Path,
) -> Result<PrepareStats, SamplerError> {
    let mut file = File::open(pack_path)?;
    let header = PackFileHeader::read(&mut file)?;
    header.require_known_kind()?;

    file.seek(SeekFrom::Start(header.index_offset()))?;
    let mut index = vec![0u8; header.index_len() as usize];
    file.read_exact(&mut index)?;

    let mut stats = PrepareStats::default();
    let mut in_embedded_spec = false;
    for line in String::from_utf8_lossy(&index).lines() {
        if line == "# spec_begin" {
            in_embedded_spec = true;
            continue;
        }
        if line == "# spec_end" {
            in_embedded_spec = false;
            continue;
        }
        if in_embedded_spec || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let source = fields
            .next()
            .ok_or_else(|| invalid_data("signal pack index missing source path"))?;
        let offset = parse_field(fields.next(), "offset")?;
        let bytes: u64 = parse_field(fields.next(), "bytes")?;
        let _channels: u16 = parse_field(fields.next(), "channels")?;
        let _sample_rate: u32 = parse_field(fields.next(), "sample_rate")?;
        let _num_frames: usize = parse_field(fields.next(), "num_frames")?;
        let samples: usize = parse_field(fields.next(), "samples")?;

        let source_path = Path::new(source);
        let relative = if source_path.is_absolute() {
            source_path.strip_prefix("/").unwrap_or(source_path)
        } else {
            source_path
        };
        let out_path = output_dir.join(relative).with_extension("wav");
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0u8; bytes as usize];
        file.read_exact(&mut data)?;
        let data = if data.starts_with(b"OggS") {
            load_ogg_vorbis_bytes(&data)
        } else {
            load_flac_bytes(&data)
        }
        .map_err(|err| {
            SamplerError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to decode packed sample {source}: {err}"),
            ))
        })?;
        // Encoder pads to a multiple of FLAC block size; trim trailing
        // silence using the index's authoritative sample count.
        let decoded = data.to_f32();
        let frames = decoded.as_ref();
        let frames = if frames.len() > samples {
            &frames[..samples]
        } else {
            frames
        };
        write_wav_f32(&out_path, data.channels, data.sample_rate, frames)?;
        stats.prepared += 1;
        stats.bytes += samples * 3;
    }

    Ok(stats)
}

#[cfg(feature = "engine-native")]
struct PreparedIndexRow {
    source: PathBuf,
    pcm_file: PathBuf,
    channels: u16,
    sample_rate: u32,
    num_frames: usize,
    samples: usize,
}

#[cfg(feature = "engine-native")]
struct PackIndexRow {
    source: PathBuf,
    offset: u64,
    bytes: u64,
    uncompressed_bytes: u64,
    channels: u16,
    sample_rate: u32,
    num_frames: usize,
    samples: usize,
    payload: Vec<u8>,
}

/// f32 → interleaved little-endian 16-bit PCM.
#[cfg(feature = "engine-native")]
fn encode_pcm_i16(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// f32 → interleaved little-endian 24-bit PCM (3 bytes per sample).
#[cfg(feature = "engine-native")]
fn encode_pcm_i24(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 3);
    for s in samples {
        let v = f32_to_i24_i32(*s);
        let b = v.to_le_bytes();
        out.extend_from_slice(&b[..3]);
    }
    out
}

#[cfg(feature = "engine-native")]
fn pack_one_sample(path: &Path, codec: PackCodec) -> Result<PackIndexRow, (PathBuf, SamplerError)> {
    let data = load_sample(path).map_err(|err| (path.to_owned(), err))?;
    let uncompressed_bytes = data.len() * 3;
    let payload = match codec {
        PackCodec::OggVorbis { quality } => {
            encode_ogg_vorbis(&data.to_f32(), data.channels, data.sample_rate, quality)
                .map_err(|err| (path.to_owned(), err))?
        }
        PackCodec::FlacI24 => {
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("flac"))
            {
                std::fs::read(path).map_err(|err| (path.to_owned(), SamplerError::Io(err)))?
            } else {
                let samples = data
                    .to_f32()
                    .iter()
                    .map(|sample| f32_to_i24_i32(*sample))
                    .collect::<Vec<_>>();
                encode_flac_i24(&samples, data.channels, data.sample_rate)
                    .map_err(|err| (path.to_owned(), err))?
            }
        }
        PackCodec::PcmI16 => encode_pcm_i16(&data.to_f32()),
        PackCodec::PcmI24 => encode_pcm_i24(&data.to_f32()),
    };

    Ok(PackIndexRow {
        source: path.to_owned(),
        offset: 0,
        bytes: payload.len() as u64,
        uncompressed_bytes: uncompressed_bytes as u64,
        channels: data.channels,
        sample_rate: data.sample_rate,
        num_frames: data.num_frames,
        samples: data.len(),
        payload,
    })
}

#[cfg(feature = "engine-native")]
fn prepare_one_sample(
    cache_dir: &Path,
    i: usize,
    path: &Path,
) -> Result<PreparedIndexRow, (PathBuf, SamplerError)> {
    let data = load_sample(path).map_err(|err| (path.to_owned(), err))?;
    let pcm_file = PathBuf::from(format!("{i:08}.pcm"));
    let mut pcm = BufWriter::new(
        File::create(cache_dir.join(&pcm_file)).map_err(|err| (path.to_owned(), err.into()))?,
    );
    for sample in data.to_f32().iter() {
        pcm.write_all(&sample.to_le_bytes())
            .map_err(|err| (path.to_owned(), err.into()))?;
    }
    pcm.flush().map_err(|err| (path.to_owned(), err.into()))?;

    Ok(PreparedIndexRow {
        source: path.to_owned(),
        pcm_file,
        channels: data.channels,
        sample_rate: data.sample_rate,
        num_frames: data.num_frames,
        samples: data.len(),
    })
}

fn parse_field<T: std::str::FromStr>(field: Option<&str>, name: &str) -> Result<T, SamplerError> {
    field
        .ok_or_else(|| invalid_data(format!("prepared cache index missing {name}")))?
        .parse()
        .map_err(|_| invalid_data(format!("prepared cache index invalid {name}")))
}

#[cfg(feature = "engine-native")]
fn write_wav_f32(
    path: &Path,
    channels: u16,
    sample_rate: u32,
    samples: &[f32],
) -> Result<(), SamplerError> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;

    for sample in samples {
        writer
            .write_sample(*sample)
            .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;
    }
    writer
        .finalize()
        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;
    Ok(())
}

#[cfg(feature = "engine-native")]
fn f32_to_i24_i32(sample: f32) -> i32 {
    let scaled = (sample.clamp(-1.0, 1.0) * 8_388_608.0).round();
    (scaled as i32).clamp(-8_388_608, 8_388_607)
}

/// Encode PCM through the system `flac` CLI when available — substantially
/// faster than the pure-Rust `flacenc` crate (5–10× on typical content).
/// Falls back silently if `flac` isn't on `$PATH` so dev environments
/// without it still function.
#[cfg(feature = "engine-native")]
fn encode_flac_via_cli(samples_i16: &[i16], channels: u16, sample_rate: u32) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    let total_samples = samples_i16.len();
    if total_samples == 0 {
        return None;
    }
    let frames = total_samples / channels as usize;
    if frames == 0 {
        return None;
    }

    let mut child = Command::new("flac")
        .args([
            "--silent",
            "--no-padding",
            "--no-seektable",
            "--force-raw-format",
            "--endian=little",
            "--sign=signed",
            "--bps=16",
        ])
        .arg(format!("--sample-rate={sample_rate}"))
        .arg(format!("--channels={channels}"))
        .arg("--stdout")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdin = child.stdin.take()?;
    let mut buf = Vec::with_capacity(samples_i16.len() * 2);
    for &s in samples_i16 {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        stdin.write_all(&buf)?;
        Ok(())
    });

    let mut out = Vec::new();
    let mut stdout = child.stdout.take()?;
    stdout.read_to_end(&mut out).ok()?;
    let _ = writer.join();
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    Some(out)
}

// Test-only in spirit, but `pub` so signal-sampler's relocated voice test
// (a held looping voice over a streamed sample) can still encode a fixture.
#[cfg(feature = "engine-native")]
#[doc(hidden)]
pub fn encode_flac_i24_for_test(
    samples: &[i32],
    channels: u16,
    sample_rate: u32,
) -> Result<Vec<u8>, SamplerError> {
    encode_flac_i24(samples, channels, sample_rate)
}

#[cfg(feature = "engine-native")]
fn encode_flac_i24(
    samples: &[i32],
    channels: u16,
    sample_rate: u32,
) -> Result<Vec<u8>, SamplerError> {
    // flacenc 0.5.1 has pathological inflation on 24-bit input where the low
    // byte is always zero (i.e. 16-bit content padded to 24 bits) — easily
    // 100×–1000× larger than raw. Detect signals that fit in 16 bits and
    // encode them at the smaller bit depth instead. Lossless either way; the
    // FLAC stream itself carries `bits_per_sample` so the decoder restores
    // exact values regardless.
    let is_16bit_padded = samples.iter().all(|&s| s % 256 == 0);
    if is_16bit_padded {
        let s16: Vec<i16> = samples.iter().map(|&s| (s / 256) as i16).collect();
        if let Some(bytes) = encode_flac_via_cli(&s16, channels, sample_rate) {
            return Ok(bytes);
        }
    }
    let (bits, scaled): (usize, std::borrow::Cow<'_, [i32]>) = if is_16bit_padded {
        let mut v = Vec::with_capacity(samples.len());
        for &s in samples {
            v.push(s / 256);
        }
        (16, std::borrow::Cow::Owned(v))
    } else {
        (24, std::borrow::Cow::Borrowed(samples))
    };
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| invalid_data(format!("invalid FLAC encoder config: {e:?}")))?;
    // FLAC requires every block to be ≥16 frames; flacenc 0.5.1 does not
    // enforce this and writes a malformed STREAMINFO when the trailing
    // partial block is shorter, which then fails strict decoders. Pad with
    // silence to a multiple of `block_size` so no short tail block is
    // emitted. The pack index stores the original `num_frames`, so callers
    // that care about exact length truncate after decode.
    let block_samples = config.block_size * channels as usize;
    let padded: std::borrow::Cow<'_, [i32]> = if scaled.len() % block_samples == 0 {
        scaled
    } else {
        let target = scaled.len().next_multiple_of(block_samples);
        let mut v = Vec::with_capacity(target);
        v.extend_from_slice(scaled.as_ref());
        v.resize(target, 0);
        std::borrow::Cow::Owned(v)
    };
    let source = flacenc::source::MemSource::from_samples(
        padded.as_ref(),
        channels as usize,
        bits,
        sample_rate as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| invalid_data(format!("FLAC encode failed: {e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| invalid_data(format!("FLAC serialize failed: {e:?}")))?;
    Ok(sink.as_slice().to_vec())
}

/// Decode an in-pack FLAC sample. Tries **symphonia** first (faster on 24-bit
/// content) and falls back to **claxon** if symphonia errors, so we never
/// silently drop a sample on an unexpected stream.
fn load_flac_bytes(bytes: &[u8]) -> Result<SampleData, SamplerError> {
    match decode_flac_symphonia(bytes) {
        Ok(data) => Ok(data),
        Err(e) => {
            tracing::debug!("symphonia FLAC decode failed ({e}); falling back to claxon");
            decode_flac_claxon(bytes)
        }
    }
}

/// claxon FLAC decode (correctness fallback / loose-file path).
fn decode_flac_claxon(bytes: &[u8]) -> Result<SampleData, SamplerError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = claxon::FlacReader::new(cursor)
        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;
    load_flac_reader(&mut reader)
}

/// symphonia FLAC decode. Produces interleaved f32 in [-1, 1], matching the
/// claxon path.
///
/// Our packs are encoded with a FIXED blocking strategy, but when the final
/// block is short (the `flac` CLI path doesn't pad) libFLAC writes STREAMINFO
/// with `min_block_size < max_block_size`. symphonia reads that as a
/// VARIABLE-blocksize stream, so its `strict_frame_header_check` rejects every
/// fixed-strategy frame and `try_new` reads to EOF. We work around it by
/// normalizing `min_block_size := max_block_size` in the copy we hand
/// symphonia — frame data is untouched (the decoder uses each frame's own
/// block size), so the decoded audio is identical.
fn decode_flac_symphonia(bytes: &[u8]) -> Result<SampleData, SamplerError> {
    use symphonia_bundle_flac::{FlacDecoder, FlacReader};
    use symphonia_core::audio::SampleBuffer;
    use symphonia_core::codecs::{Decoder, DecoderOptions};
    use symphonia_core::errors::Error as SymErr;
    use symphonia_core::formats::{FormatOptions, FormatReader};
    use symphonia_core::io::MediaSourceStream;

    let sym_io = |e: SymErr| SamplerError::Io(std::io::Error::other(e.to_string()));

    // Own + (if needed) normalize the STREAMINFO block-size bounds. Layout:
    // bytes 0..4 = "fLaC", 4 = metadata block header (is_last|type), with the
    // first block being STREAMINFO (type 0). STREAMINFO body starts at byte 8:
    // [min_block u16 BE][max_block u16 BE]…
    let mut owned = bytes.to_vec();
    if owned.len() >= 12 && &owned[0..4] == b"fLaC" && (owned[4] & 0x7f) == 0 {
        let max_block = [owned[10], owned[11]];
        owned[8] = max_block[0];
        owned[9] = max_block[1];
    }

    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(owned)), Default::default());
    let mut format = FlacReader::try_new(mss, &FormatOptions::default()).map_err(sym_io)?;
    let track = format
        .default_track()
        .ok_or_else(|| invalid_data("flac: no default track"))?;
    let track_id = track.id;
    let mut decoder =
        FlacDecoder::try_new(&track.codec_params, &DecoderOptions::default()).map_err(sym_io)?;

    let mut frames: Vec<f32> = Vec::new();
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // symphonia signals normal end-of-stream as an IoError — either a
            // real UnexpectedEof or its own "end of stream" sentinel (Other).
            Err(SymErr::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.to_string() == "end of stream" =>
            {
                break;
            }
            Err(SymErr::ResetRequired) => break,
            Err(e) => return Err(sym_io(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                if sbuf.is_none() {
                    let spec = *decoded.spec();
                    channels = spec.channels.count() as u16;
                    sample_rate = spec.rate;
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sb = sbuf.as_mut().unwrap();
                sb.copy_interleaved_ref(decoded); // → interleaved f32, normalized
                frames.extend_from_slice(sb.samples());
            }
            // A corrupt frame is skippable; anything else is fatal.
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(sym_io(e)),
        }
    }

    if frames.is_empty() {
        return Err(invalid_data("flac: symphonia decoded no samples"));
    }
    let channels = channels.max(1);
    let num_frames = frames.len() / channels as usize;
    Ok(SampleData::from_f32(
        frames,
        channels,
        sample_rate,
        num_frames,
    ))
}

/// Decode an in-pack Ogg Vorbis sample (lossy proxy packs) to interleaved
/// f32. Pure-Rust symphonia path — wasm-clean, mirrors `decode_flac_symphonia`.
///
/// The decoded length may differ from the source PCM by a partial tail
/// block; callers coerce to the index's authoritative frame count
/// (`coerce_to_index_len`), keeping loop points sample-exact.
fn load_ogg_vorbis_bytes(bytes: &[u8]) -> Result<SampleData, SamplerError> {
    use symphonia_codec_vorbis::VorbisDecoder;
    use symphonia_core::audio::SampleBuffer;
    use symphonia_core::codecs::{Decoder, DecoderOptions};
    use symphonia_core::errors::Error as SymErr;
    use symphonia_core::formats::{FormatOptions, FormatReader};
    use symphonia_core::io::MediaSourceStream;
    use symphonia_format_ogg::OggReader;

    let sym_io = |e: SymErr| SamplerError::Io(std::io::Error::other(e.to_string()));

    let owned = bytes.to_vec();
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(owned)), Default::default());
    let mut format = OggReader::try_new(mss, &FormatOptions::default()).map_err(sym_io)?;
    let track = format
        .default_track()
        .ok_or_else(|| invalid_data("ogg: no default track"))?;
    let track_id = track.id;
    let mut decoder =
        VorbisDecoder::try_new(&track.codec_params, &DecoderOptions::default()).map_err(sym_io)?;

    let mut frames: Vec<f32> = Vec::new();
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut sbuf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymErr::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.to_string() == "end of stream" =>
            {
                break;
            }
            Err(SymErr::ResetRequired) => break,
            Err(e) => return Err(sym_io(e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                if decoded.frames() == 0 {
                    continue;
                }
                if sbuf.is_none() {
                    let spec = *decoded.spec();
                    channels = spec.channels.count() as u16;
                    sample_rate = spec.rate;
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sb = sbuf.as_mut().unwrap();
                sb.copy_interleaved_ref(decoded);
                frames.extend_from_slice(sb.samples());
            }
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(sym_io(e)),
        }
    }

    if frames.is_empty() {
        return Err(invalid_data("ogg: symphonia decoded no samples"));
    }
    let channels = channels.max(1);
    let num_frames = frames.len() / channels as usize;
    Ok(SampleData::from_f32(
        frames,
        channels,
        sample_rate,
        num_frames,
    ))
}

/// Encode interleaved f32 PCM to an Ogg Vorbis stream (builder-side only —
/// runtime never encodes). `quality` is the libvorbis base-quality scale
/// (-0.2..=1.0, oggenc's `-q` divided by 10).
#[cfg(feature = "engine-native")]
pub fn encode_ogg_vorbis(
    frames: &[f32],
    channels: u16,
    sample_rate: u32,
    quality: f32,
) -> Result<Vec<u8>, SamplerError> {
    use std::num::{NonZeroU32, NonZeroU8};
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

    let vorb = |e: vorbis_rs::VorbisError| invalid_data(format!("vorbis encode failed: {e}"));

    let channels_nz = NonZeroU8::new(
        channels
            .try_into()
            .map_err(|_| invalid_data(format!("vorbis: unsupported channel count {channels}")))?,
    )
    .ok_or_else(|| invalid_data("vorbis: zero channels"))?;
    let rate_nz =
        NonZeroU32::new(sample_rate).ok_or_else(|| invalid_data("vorbis: zero sample rate"))?;

    // De-interleave to planar, as libvorbis wants.
    let ch = channels as usize;
    let n_frames = frames.len() / ch;
    let mut planar: Vec<Vec<f32>> = vec![Vec::with_capacity(n_frames); ch];
    for frame in frames.chunks_exact(ch) {
        for (c, &s) in frame.iter().enumerate() {
            planar[c].push(s);
        }
    }

    let mut out = Vec::new();
    let mut encoder = VorbisEncoderBuilder::new(rate_nz, channels_nz, &mut out)
        .map_err(vorb)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality,
        })
        .build()
        .map_err(vorb)?;
    encoder.encode_audio_block(&planar).map_err(vorb)?;
    encoder.finish().map_err(vorb)?;
    Ok(out)
}

/// Write an Ogg Vorbis proxy of a WAV (16/24/32-bit integer or 32-bit
/// float), at its own rate and channel count, a block at a time — a
/// seven-minute stem never sits in memory as PCM. `quality` is libvorbis's
/// base quality (-0.2..=1.0; 0.4 ≈ 128 kbps stereo, oggenc `-q4`). Returns
/// the frames written.
///
/// The proxies a session plays from where the originals are too heavy to
/// move (a browser, a phone): `Media/Bass.wav` → `Media/Proxies/Bass.ogg`.
///
/// # Errors
///
/// The WAV does not open or read, or the encoder refuses its format.
#[cfg(feature = "engine-native")]
pub fn write_ogg_proxy(
    src: &std::path::Path,
    dst: &std::path::Path,
    quality: f32,
) -> Result<u64, SamplerError> {
    use std::num::{NonZeroU32, NonZeroU8};
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

    const BLOCK: usize = 16_384;
    let vorb = |e: vorbis_rs::VorbisError| invalid_data(format!("vorbis encode failed: {e}"));
    let wav = |e: hound::Error| invalid_data(format!("{}: {e}", src.display()));

    let mut reader = hound::WavReader::open(src).map_err(wav)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let channels_nz = NonZeroU8::new(u8::try_from(spec.channels).map_err(|_| {
        invalid_data(format!(
            "vorbis: unsupported channel count {}",
            spec.channels
        ))
    })?)
    .ok_or_else(|| invalid_data("vorbis: zero channels"))?;
    let rate_nz = NonZeroU32::new(spec.sample_rate)
        .ok_or_else(|| invalid_data("vorbis: zero sample rate"))?;

    let out = std::fs::File::create(dst)?;
    let mut encoder = VorbisEncoderBuilder::new(rate_nz, channels_nz, std::io::BufWriter::new(out))
        .map_err(vorb)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: quality,
        })
        .build()
        .map_err(vorb)?;

    let scale = match spec.sample_format {
        hound::SampleFormat::Float => 1.0,
        hound::SampleFormat::Int => 1.0 / (1u64 << (spec.bits_per_sample.max(1) - 1)) as f32,
    };
    let mut planar: Vec<Vec<f32>> = vec![Vec::with_capacity(BLOCK); channels];
    let mut frames = 0u64;
    let flush = |planar: &mut Vec<Vec<f32>>,
                 encoder: &mut vorbis_rs::VorbisEncoder<_>|
     -> Result<(), SamplerError> {
        if !planar[0].is_empty() {
            encoder.encode_audio_block(&*planar).map_err(vorb)?;
            for channel in planar.iter_mut() {
                channel.clear();
            }
        }
        Ok(())
    };
    let push = |i: usize, value: f32, planar: &mut Vec<Vec<f32>>| {
        planar[i % channels].push(value);
    };
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().enumerate() {
                push(i, sample.map_err(wav)?, &mut planar);
                if i % channels == channels - 1 {
                    frames += 1;
                    if planar[0].len() == BLOCK {
                        flush(&mut planar, &mut encoder)?;
                    }
                }
            }
        }
        hound::SampleFormat::Int => {
            for (i, sample) in reader.samples::<i32>().enumerate() {
                push(i, sample.map_err(wav)? as f32 * scale, &mut planar);
                if i % channels == channels - 1 {
                    frames += 1;
                    if planar[0].len() == BLOCK {
                        flush(&mut planar, &mut encoder)?;
                    }
                }
            }
        }
    }
    flush(&mut planar, &mut encoder)?;
    encoder.finish().map_err(vorb)?;
    write_ogg_index(dst, u64::from(spec.sample_rate))?;
    Ok(frames)
}

/// Write the page index beside an Ogg proxy (`Bass.ogg` → `Bass.ogg.idx`):
/// which bytes hold which second, so a client streaming it from elsewhere
/// fetches what is in front of the playhead first (see
/// [`crate::ogg_index`]). `step` is the frames between points — the sample
/// rate, a point a second.
///
/// # Errors
///
/// The proxy cannot be read, is not an Ogg stream, or the index cannot be
/// written.
#[cfg(feature = "engine-native")]
pub fn write_ogg_index(
    ogg: &std::path::Path,
    step: u64,
) -> Result<std::path::PathBuf, SamplerError> {
    let bytes = std::fs::read(ogg)?;
    let index = crate::ogg_index::OggIndex::build(&bytes, step)
        .ok_or_else(|| invalid_data(format!("{}: not an Ogg stream", ogg.display())))?;
    let path = crate::ogg_index::OggIndex::path_for(ogg);
    std::fs::write(&path, index.to_text())?;
    Ok(path)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, SamplerError> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_data("signal pack header is truncated"))?;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, SamplerError> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| invalid_data("signal pack header is truncated"))?;
    Ok(u64::from_le_bytes(slice.try_into().unwrap()))
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn invalid_data(message: impl Into<String>) -> SamplerError {
    SamplerError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
}

fn load_flac(path: &Path) -> Result<SampleData, SamplerError> {
    let mut reader = claxon::FlacReader::open(path)
        .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))?;
    load_flac_reader(&mut reader)
}

fn load_flac_reader<R: Read>(
    reader: &mut claxon::FlacReader<R>,
) -> Result<SampleData, SamplerError> {
    let info = reader.streaminfo();
    let channels = info.channels as u16;
    let sample_rate = info.sample_rate;
    let max = (1i64 << (info.bits_per_sample - 1)) as f32;

    let frames = reader
        .samples()
        .map(|s| {
            s.map(|v| v as f32 / max)
                .map_err(|e| SamplerError::Io(std::io::Error::other(e.to_string())))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let num_frames = frames.len() / channels as usize;
    Ok(SampleData::from_f32(
        frames,
        channels,
        sample_rate,
        num_frames,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(frames: usize) -> Arc<SampleData> {
        Arc::new(SampleData::from_f32(vec![0.0; frames], 1, 48_000, frames))
    }

    /// A raw-PCM pack is read straight out of its mapping: the samples come
    /// back correct and cost the process **nothing** in anonymous memory.
    /// This is the property the whole DFD path rests on.
    #[test]
    fn pcm_pack_reads_from_the_mapping_without_allocating() {
        let dir = std::env::temp_dir().join(format!("fts-pcm-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");

        // A short stereo ramp, written as a wav the packer can read.
        let (sr, n_frames) = (48_000u32, 1_024usize);
        let src: Vec<f32> = (0..n_frames)
            .flat_map(|i| {
                let v = (i as f32 / n_frames as f32) * 2.0 - 1.0;
                [v, -v]
            })
            .collect();
        let wav = dir.join("ramp.wav");
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: sr,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&wav, spec).expect("wav");
            for s in &src {
                w.write_sample(*s).expect("write");
            }
            w.finalize().expect("finalize");
        }
        std::fs::write(dir.join("library.styx"), "instrument \"test\"\n").expect("spec");

        for (codec, fmt, tol) in [
            (PackCodec::PcmI16, PcmFmt::I16, 1.0 / 32_000.0),
            (PackCodec::PcmI24, PcmFmt::I24, 1.0 / 8_000_000.0),
        ] {
            let pack_path = dir.join(format!("{}.signalpack", fmt.width()));
            create_signal_pack_with(
                &pack_path,
                PackSpecSource::Path(&dir.join("library.styx")),
                &dir,
                [wav.as_path()].into_iter(),
                codec,
            )
            .expect("build pack");

            let pack = SignalPcmPack::open(&pack_path).expect("open pack");
            let entry = pack.entry_for_path(&wav).expect("entry").clone();
            assert_eq!(entry.mapped_fmt(), Some(fmt));

            let data = load_pack_sample(&pack.mmap, &entry).expect("read");
            assert!(
                matches!(data.pcm, Pcm::Mapped { .. }),
                "must be mapped, not decoded"
            );
            assert_eq!(
                data.decoded_bytes(),
                0,
                "mapped PCM costs no anonymous memory"
            );
            assert_eq!(data.channels, 2);
            assert_eq!(data.sample_rate, sr);
            assert_eq!(data.num_frames, n_frames);

            // Values survive the round trip within the format's quantisation.
            for (i, want) in src.iter().enumerate() {
                let got = data.pcm.sample(i);
                assert!(
                    (got - want).abs() <= tol,
                    "sample {i}: got {got}, want {want} ({fmt:?})"
                );
            }
            // …and the stereo frame accessor still pairs them correctly.
            let (l, r) = data.frame(10);
            assert!((l - src[20]).abs() <= tol && (r - src[21]).abs() <= tol);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decodes_one_pack_sample() {
        let p = std::path::Path::new(
            "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD Modern and Massive 2/Packs/Kick/22x18'' Tama Starclassic Bubinga Kick.signalpack",
        );
        if !p.exists() {
            eprintln!("skip (pack missing)");
            return;
        }
        let pack = SignalPcmPack::open(p).expect("open pack");
        let (_path, entry) = pack.entries.iter().next().expect("at least one entry");
        let start = entry.offset as usize;
        let bytes = &pack.mmap[start..start + entry.bytes as usize];

        // symphonia (primary path) must succeed on its own — not silently fall
        // back to claxon — and agree with claxon bit-for-bit on the result.
        let sym = decode_flac_symphonia(bytes).expect("symphonia decode");
        let cla = decode_flac_claxon(bytes).expect("claxon decode");
        assert!(sym.num_frames > 0);
        assert_eq!(sym.channels, cla.channels, "channel mismatch");
        assert_eq!(sym.sample_rate, cla.sample_rate, "sample-rate mismatch");
        assert_eq!(sym.num_frames, cla.num_frames, "frame-count mismatch");
        assert_eq!(sym.len(), cla.len(), "sample-count mismatch");
        let (sym_pcm, cla_pcm) = (sym.to_f32(), cla.to_f32());
        let max_diff = sym_pcm
            .iter()
            .zip(cla_pcm.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_diff < 1e-6,
            "sample value mismatch: max_diff={max_diff}"
        );
    }

    #[test]
    fn ogg_vorbis_round_trip_is_sample_exact_in_length() {
        // Stereo sine, deliberately NOT a multiple of any codec block size.
        let sample_rate = 44_100u32;
        let channels = 2u16;
        let n_frames = 44_100 + 1234;
        let mut frames = Vec::with_capacity(n_frames * 2);
        for i in 0..n_frames {
            let t = i as f32 / sample_rate as f32;
            frames.push((t * 440.0 * std::f32::consts::TAU).sin() * 0.5);
            frames.push((t * 554.37 * std::f32::consts::TAU).sin() * 0.5);
        }

        let ogg = encode_ogg_vorbis(&frames, channels, sample_rate, 0.8).expect("encode");
        assert!(ogg.starts_with(b"OggS"), "payload must be an Ogg stream");
        assert!(
            ogg.len() < frames.len() * 2,
            "vorbis q8 should beat 16-bit PCM in size"
        );

        let decoded = load_ogg_vorbis_bytes(&ogg).expect("decode");
        assert_eq!(decoded.channels, channels);
        assert_eq!(decoded.sample_rate, sample_rate);
        // Raw decode may drift by a partial tail block; the pack-entry path
        // coerces to the index's authoritative length.
        let entry = PackEntry {
            kind: SIGNAL_PACK_KIND_OGG_VORBIS,
            offset: 0,
            bytes: ogg.len() as u64,
            channels,
            sample_rate,
            num_frames: n_frames,
            samples: n_frames * 2,
        };
        let coerced = coerce_to_index_len(decoded, &entry);
        assert_eq!(coerced.num_frames, n_frames);
        assert_eq!(coerced.len(), n_frames * 2);
        let coerced_pcm = coerced.to_f32();

        // Content sanity: same tone, so correlation with the source should be
        // high (lossy — not bit-exact).
        let dot: f64 = coerced_pcm
            .iter()
            .zip(frames.iter())
            .map(|(a, b)| (*a as f64) * (*b as f64))
            .sum();
        let norm_a: f64 = coerced_pcm.iter().map(|a| (*a as f64).powi(2)).sum();
        let norm_b: f64 = frames.iter().map(|b| (*b as f64).powi(2)).sum();
        let corr = dot / (norm_a.sqrt() * norm_b.sqrt()).max(f64::EPSILON);
        assert!(
            corr > 0.98,
            "decoded audio should correlate with source, got {corr}"
        );
    }

    /// A 24-bit stem, proxied a block at a time, decodes back to the same
    /// length and the same tone — the block boundaries leave no seams.
    #[test]
    fn a_wav_proxies_to_ogg_a_block_at_a_time() {
        let dir = std::env::temp_dir().join(format!("ogg-proxy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let (src, dst) = (dir.join("Bass.wav"), dir.join("Bass.ogg"));
        let (rate, frames) = (44_100u32, 44_100 * 3 + 777);
        let tone = |i: usize| (i as f32 / rate as f32 * 110.0 * std::f32::consts::TAU).sin() * 0.5;
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: rate,
                bits_per_sample: 24,
                sample_format: hound::SampleFormat::Int,
            };
            let mut writer = hound::WavWriter::create(&src, spec).expect("wav");
            for i in 0..frames {
                let v = (tone(i) * 8_388_607.0) as i32;
                writer.write_sample(v).expect("left");
                writer.write_sample(-v).expect("right");
            }
            writer.finalize().expect("finalize");
        }
        let written = write_ogg_proxy(&src, &dst, 0.4).expect("proxy");
        assert_eq!(written, frames as u64);

        let ogg = std::fs::read(&dst).expect("read proxy");
        assert!(
            ogg.len() < frames * 2 * 3 / 4,
            "a proxy is smaller than its stem"
        );
        let decoded = load_ogg_vorbis_bytes(&ogg).expect("decode");
        let decoded = decoded.to_f32();
        assert!(
            (decoded.len() / 2).abs_diff(frames) < 2048,
            "{} frames back, {frames} in",
            decoded.len() / 2
        );
        let worst = (4096..frames - 4096)
            .map(|i| {
                (decoded[i * 2] - tone(i))
                    .abs()
                    .max((decoded[i * 2 + 1] + tone(i)).abs())
            })
            .fold(0.0f32, f32::max);
        assert!(worst < 0.05, "the tone survives, both channels: {worst}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transcode_flac_pack_to_ogg_preserves_index_metadata() {
        let dir = std::env::temp_dir().join("signal-sampler-transcode-test");
        std::fs::create_dir_all(&dir).unwrap();
        // Tiny source: one stereo sine WAV + minimal spec.
        let wav = dir.join("tone.wav");
        let n_frames = 20_000usize;
        let mut w = hound::WavWriter::create(
            &wav,
            hound::WavSpec {
                channels: 2,
                sample_rate: 44_100,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for i in 0..n_frames {
            let s = ((i as f32) * 0.05).sin() * 0.4;
            w.write_sample(s).unwrap();
            w.write_sample(-s).unwrap();
        }
        w.finalize().unwrap();

        let flac_pack = dir.join("t.signalpack");
        let stats = create_signal_pack_with(
            &flac_pack,
            PackSpecSource::Text {
                text: "name \"t\"\n",
                format: "styx",
            },
            &dir,
            [wav.as_path()].into_iter(),
            PackCodec::FlacI24,
        )
        .unwrap();
        assert_eq!(stats.prepared, 1);

        let ogg_pack = dir.join("t-proxy.signalpack");
        let stats = transcode_signal_pack(&flac_pack, &ogg_pack, PackCodec::OGG_VORBIS_Q8).unwrap();
        assert_eq!((stats.prepared, stats.failed), (1, 0));

        let pack = SignalPcmPack::open(&ogg_pack).unwrap();
        assert_eq!(pack.kind(), SIGNAL_PACK_KIND_OGG_VORBIS);
        assert_eq!(pack.embedded_spec().map(str::trim), Some("name \"t\""));
        let (_, entry) = pack.entries.iter().next().unwrap();
        // Index carries the SOURCE frame counts, and decode coerces to them.
        assert_eq!(entry.num_frames, n_frames);
        let data = load_pack_sample(&pack.mmap, entry).unwrap();
        assert_eq!(data.num_frames, n_frames);
        assert_eq!(data.channels, 2);
        assert_eq!(data.sample_rate, 44_100);
    }

    /// A pack written by the native writer reopens through `open_bytes` (the
    /// wasm path: the whole pack as one in-memory buffer) and decodes
    /// bit-identically to the mmap-backed `open`.
    #[test]
    fn open_bytes_decodes_identically_to_mmap_open() {
        let dir = std::env::temp_dir().join(format!("fts-open-bytes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");

        // A stereo tone, written as a wav the packer can read.
        let (sr, n_frames) = (48_000u32, 4_096usize);
        let wav = dir.join("tone.wav");
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: sr,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&wav, spec).expect("wav");
            for i in 0..n_frames {
                let s = ((i as f32) * 0.03).sin() * 0.6;
                w.write_sample(s).expect("write");
                w.write_sample(-s).expect("write");
            }
            w.finalize().expect("finalize");
        }

        let pack_path = dir.join("tone.signalpack");
        create_signal_pack_with(
            &pack_path,
            PackSpecSource::Text {
                text: "name \"tone\"\n",
                format: "styx",
            },
            &dir,
            [wav.as_path()].into_iter(),
            PackCodec::FlacI24,
        )
        .expect("build pack");

        let mapped = SignalPcmPack::open(&pack_path).expect("mmap open");
        let bytes = std::fs::read(&pack_path).expect("read pack bytes");
        let owned = SignalPcmPack::open_bytes(bytes).expect("open_bytes");

        assert_eq!(owned.kind(), mapped.kind());
        assert_eq!(owned.entry_count(), mapped.entry_count());
        assert_eq!(owned.embedded_spec(), mapped.embedded_spec());
        assert!(matches!(owned.mmap, PackBytes::Owned(_)));

        let rel = Path::new("tone.wav");
        let m_entry = mapped.entry_for_path(rel).expect("mmap entry");
        let o_entry = owned.entry_for_path(rel).expect("owned entry");
        assert_eq!(m_entry.num_frames(), o_entry.num_frames());

        // Full decode is forced (FTS_STREAM aside, decode via the whole-
        // sample bulk path) so the comparison covers every sample.
        let m_data = load_pack_sample(&mapped.mmap, m_entry).expect("decode mmap");
        let o_data = load_pack_sample(&owned.mmap, o_entry).expect("decode owned");
        assert_eq!(o_data.channels, m_data.channels);
        assert_eq!(o_data.sample_rate, m_data.sample_rate);
        assert_eq!(o_data.num_frames, m_data.num_frames);
        assert_eq!(
            o_data.to_f32().as_ref(),
            m_data.to_f32().as_ref(),
            "open_bytes must decode the same PCM as the mmap open"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The External arm: a reader registered over an in-memory Vec must
    /// parse and decode identically to `open_bytes` over the same bytes —
    /// header, index, spec and PCM — with every read going through
    /// `read_range` (nothing may deref an External).
    #[test]
    fn open_external_decodes_identically_to_open_bytes() {
        static PACKS: std::sync::Mutex<Option<HashMap<u32, Vec<u8>>>> = std::sync::Mutex::new(None);

        let dir = std::env::temp_dir().join(format!("fts-open-external-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("tmp dir");

        let (sr, n_frames) = (48_000u32, 4_096usize);
        let wav = dir.join("tone.wav");
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: sr,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&wav, spec).expect("wav");
            for i in 0..n_frames {
                let s = ((i as f32) * 0.03).sin() * 0.6;
                w.write_sample(s).expect("write");
                w.write_sample(-s).expect("write");
            }
            w.finalize().expect("finalize");
        }
        let pack_path = dir.join("tone.signalpack");
        create_signal_pack_with(
            &pack_path,
            PackSpecSource::Text {
                text: "name \"tone\"\n",
                format: "styx",
            },
            &dir,
            [wav.as_path()].into_iter(),
            PackCodec::FlacI24,
        )
        .expect("build pack");
        let bytes = std::fs::read(&pack_path).expect("read pack bytes");
        let len = bytes.len() as u64;

        // Register the bytes with the process-global reader (first install
        // wins — a prior install by another test would use the same map).
        const ID: u32 = 7001;
        PACKS
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(ID, bytes.clone());
        set_external_pack_reader(Box::new(|id, offset, dst| {
            let packs = PACKS.lock().unwrap();
            let Some(bytes) = packs.as_ref().and_then(|m| m.get(&id)) else {
                return false;
            };
            let start = offset as usize;
            let Some(src) = bytes.get(start..start + dst.len()) else {
                return false;
            };
            dst.copy_from_slice(src);
            true
        }));

        let external = SignalPcmPack::open_external(ID, len).expect("open_external");
        let owned = SignalPcmPack::open_bytes(bytes).expect("open_bytes");

        assert!(external.mmap.is_external());
        assert_eq!(external.kind(), owned.kind());
        assert_eq!(external.entry_count(), owned.entry_count());
        assert_eq!(external.embedded_spec(), owned.embedded_spec());

        let rel = Path::new("tone.wav");
        let e_entry = external
            .entry_for_path(rel)
            .expect("external entry")
            .clone();
        let o_entry = owned.entry_for_path(rel).expect("owned entry");
        assert_eq!(e_entry.num_frames(), o_entry.num_frames());

        let e_data = load_pack_sample(&external.mmap, &e_entry).expect("decode external");
        let o_data = load_pack_sample(&owned.mmap, o_entry).expect("decode owned");
        assert_eq!(e_data.channels, o_data.channels);
        assert_eq!(e_data.sample_rate, o_data.sample_rate);
        assert_eq!(e_data.num_frames, o_data.num_frames);
        assert_eq!(
            e_data.to_f32().as_ref(),
            o_data.to_f32().as_ref(),
            "open_external must decode the same PCM as open_bytes"
        );

        // Out-of-bounds and truncated reads refuse instead of trapping.
        assert!(external.mmap.read_range(len, 1).is_err());
        assert!(external.mmap.read_range(0, len as usize + 1).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn eviction_removes_largest_samples_until_under_budget() {
        // Charges + releases the process-wide budget counter — serialise
        // against the budget tests' delta assertions.
        let _serial = crate::budget::TEST_COUNTER_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let cache = SampleCache::new();
        cache.insert_loaded(PathBuf::from("small.wav"), sample(4), false);
        cache.insert_loaded(PathBuf::from("large.wav"), sample(16), false);
        cache.insert_loaded(PathBuf::from("medium.wav"), sample(8), true);

        let stats = cache.evict_until_under_budget(48);

        assert_eq!(stats.bytes_before, 112);
        assert_eq!(stats.bytes_after, 48);
        assert_eq!(stats.bytes_freed, 64);
        assert_eq!(stats.evicted, 1);
        assert!(cache.get_loaded(Path::new("large.wav")).is_none());
        assert!(cache.get_loaded(Path::new("medium.wav")).is_some());
        assert!(cache.get_loaded(Path::new("small.wav")).is_some());
    }

    #[test]
    fn eviction_preserves_active_arc_handles() {
        let _serial = crate::budget::TEST_COUNTER_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let cache = SampleCache::new();
        let held = sample(16);
        cache.insert_loaded(PathBuf::from("held.wav"), Arc::clone(&held), true);

        let stats = cache.evict_until_under_budget(0);

        assert_eq!(stats.evicted, 1);
        assert!(cache.get_loaded(Path::new("held.wav")).is_none());
        assert_eq!(held.num_frames, 16);
        assert_eq!(Arc::strong_count(&held), 1);
    }
}
