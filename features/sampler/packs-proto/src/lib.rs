//! Pack-library wire contract — `.signalpack` distribution over vox.
//!
//! A pack host (the studio engine, a hosted library server, any peer)
//! serves its built `.signalpack` trees through [`packs::PackLibrary`]:
//! list what's available, stream any pack's bytes as ordered
//! [`PackChunk`]s. The transport is whatever the router is mounted on —
//! WebSocket to our servers, iroh QUIC peer-to-peer — the contract is
//! identical, so a phone pulls the LA Custom Rhodes proxy pack from the
//! studio machine the same way it would from a hosted mirror.
//!
//! All types are plain `facet::Facet` data, so this crate compiles for
//! wasm + embedded.

use facet::Facet;
use thiserror::Error;

/// One distributable `.signalpack` in the host's library.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PackInfo {
    /// Instrument name — the pack's file stem ("LA Custom C7 Grand").
    pub name: String,
    /// Library grouping — the pack's directory relative to the library
    /// root, variant tree stripped ("Keys/Keyscape/Packs").
    pub category: String,
    /// Body codec tree: "proxy" (Ogg Vorbis, streaming/mobile) or
    /// "full" (lossless FLAC).
    pub variant: String,
    pub size_bytes: u64,
    /// Hex SHA-256 of the pack file; empty while the host is still
    /// hashing (verify only when non-empty).
    pub sha256: String,
}

/// One streamed slice of a pack file. Chunks arrive in order with
/// contiguous, monotonically increasing `offset`s; the stream closing
/// without error marks the end of the file.
#[derive(Clone, PartialEq, Debug, Default, Facet)]
pub struct PackChunk {
    /// Absolute byte offset of this chunk within the pack file.
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// One byte range within a pack file. Crosses the wire as its
/// [`Display`](std::fmt::Display) form `"start+len"` (decimal), parsed
/// back with [`FromStr`](std::str::FromStr): the RPC surface caps at 4
/// params, and a struct-valued arg is not a wire shape the wasm reader's
/// phon compat path has proven — strings are (see `pack_plan`'s docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Facet)]
pub struct PackRange {
    /// Absolute byte offset of the range's first byte.
    pub start: u64,
    /// Range length in bytes.
    pub len: u64,
}

impl std::fmt::Display for PackRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}+{}", self.start, self.len)
    }
}

impl std::str::FromStr for PackRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (start, len) = s
            .split_once('+')
            .ok_or_else(|| format!("range {s:?} is not start+len"))?;
        Ok(PackRange {
            start: start.trim().parse().map_err(|e| format!("range start: {e}"))?,
            len: len.trim().parse().map_err(|e| format!("range len: {e}"))?,
        })
    }
}

/// One segment of a pack's download plan: a contiguous byte span plus its fetch
/// priority. Segments tile the pack file exactly once (no overlap, no gap);
/// fetching them in ascending `rank` order makes the pack *playable* long
/// before it is *complete*.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct PackSegment {
    /// Absolute byte offset of the segment.
    pub start: u64,
    /// Segment length in bytes.
    pub len: u64,
    /// Fetch priority — 0 first. Rank 0 covers everything the pack OPEN
    /// touches (the 64-byte header + the text index with its embedded
    /// spec); higher ranks are one audio entry each, ordered musically
    /// (middle velocity layer first, middle keys outward, round-robin 0
    /// before its repeats). u64 like every other numeric field on this
    /// wire (mixed-width structs hit a phon wasm-reader schema mismatch).
    pub rank: u64,
    /// Human-readable segment label ("header", "index", a sample path…) —
    /// diagnostics only, never parsed.
    pub label: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Facet, Error)]
#[repr(u8)]
pub enum PackError {
    #[error("pack not found")]
    NotFound,
    #[error("invalid range: {0}")]
    InvalidRange(String),
    #[error("io: {0}")]
    Io(String),
}

// Streamed payloads cross vox lanes by reference (`SelfRef<T>::get`),
// which needs the plain-data Reborrow witness — same as media-proto.
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for PackInfo {
    type Ref<'a> = PackInfo;
}
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for PackChunk {
    type Ref<'a> = PackChunk;
}
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for PackError {
    type Ref<'a> = PackError;
}

#[cfg(feature = "vox")]
pub mod packs {
    //! Pack distribution. `PackLibrary` → `PackLibraryClient` /
    //! `PackLibraryService` / `pack_library_serve`.

    use super::{PackChunk, PackError, PackInfo};

    #[architect::rpc]
    pub trait PackLibrary {
        /// Every pack this host offers (proxy + full variants listed
        /// separately).
        fn packs(&self) -> Vec<PackInfo>;

        /// Stream `(name, variant)` from byte `start` to the end of the
        /// file as ordered [`PackChunk`]s on `tx` (start > 0 = resume).
        /// Returns after the last chunk is queued; the closed stream is
        /// the end-of-file signal.
        ///
        /// **Virtual names** (the browser's route to the W7 operations —
        /// this method is the one signature the wasm client provably
        /// drives; see [`pack_plan`](Self::pack_plan) for the findings):
        ///
        /// - `"plan:<pack name>"` — streams the pack's prioritized plan
        ///   (facet-json `Vec<PackSegment>` UTF-8 bytes) instead of pack
        ///   bytes; `start` = resume offset into the JSON.
        /// - `"range:<start>+<len>:<pack name>"` — streams exactly that
        ///   byte range of the pack (absolute chunk offsets); the
        ///   `start` argument is ignored.
        ///
        /// Real pack names never carry these prefixes (they are file
        /// stems).
        async fn read(
            &self,
            name: String,
            variant: String,
            start: u64,
            tx: vox::Tx<PackChunk>,
        ) -> Result<(), PackError>;

        /// Stream exactly `range` (a [`PackRange`](super::PackRange) in
        /// its `"start+len"` string form) of `(name, variant)` as ordered
        /// [`PackChunk`]s on `tx` — chunk `offset`s are absolute file
        /// offsets, contiguous within the range; the closed stream marks
        /// the end of the range. The progressive fetch path: ranges come
        /// from [`pack_plan`](Self::pack_plan) segments (and from
        /// play-driven misses jumping the queue).
        ///
        /// The wire shape deliberately mirrors [`read`](Self::read) —
        /// String/u64 args + `Tx<PackChunk>` + `Result<(), PackError)`
        /// are the kinds proven against the wasm reader; see
        /// [`pack_plan`](Self::pack_plan) for the story.
        async fn read_range(
            &self,
            name: String,
            variant: String,
            range: String,
            tx: vox::Tx<PackChunk>,
        ) -> Result<(), PackError>;

        /// Stream the prioritized download plan for `(name, variant)` on
        /// `tx`, as the UTF-8 bytes of **facet-json
        /// `Vec<PackSegment>`** chunked like a pack read (offsets from
        /// `start`, normally 0; stream-close = end of the JSON). Rank-0
        /// segments cover the pack open (header + index + embedded
        /// spec); higher ranks are one audio entry each, ranked
        /// musically. Segments tile the file exactly once, so their
        /// lengths sum to the listed `size_bytes` (total + sha256 come
        /// from [`packs`](Self::packs)). `NotFound`/`Io` errors mean
        /// unplannable — fall back to whole-file [`read`](Self::read).
        ///
        /// Why a byte stream and not a return value: this method's wire
        /// shape is a deliberate mirror of [`read`](Self::read), the one
        /// signature family proven against the wasm (browser) reader.
        /// Every composite reply tried here — `Result<PackPlan, _>`,
        /// `Vec<PackSegment>`, even a plain `String`, sync or async —
        /// decodes natively but fails in the browser client's phon
        /// schema-compat pass with "writer and reader schema kinds
        /// differ" (the complex-return family session-proto's
        /// `repro_inprocess.rs` chases). Chunk streaming sidesteps the
        /// whole question.
        async fn pack_plan(
            &self,
            name: String,
            variant: String,
            start: u64,
            tx: vox::Tx<PackChunk>,
        ) -> Result<(), PackError>;
    }
}

#[cfg(feature = "vox")]
pub use packs::PackLibrary;
