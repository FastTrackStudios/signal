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
        async fn read(
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
