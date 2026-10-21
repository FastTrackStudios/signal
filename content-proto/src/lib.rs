//! Content transfer wire contract — everything a rig plays from, moved
//! between devices.
//!
//! A host (the studio engine, a hosted library, any peer) serves its
//! libraries through [`content::ContentLibrary`]: the files it has
//! ([`ContentInfo`]), the bundles they make ([`BundleInfo`] — a drum kit,
//! a rig's library), and any file's bytes as ordered [`ContentChunk`]s.
//! The transport is whatever the router is mounted on — WebSocket to a
//! server, iroh QUIC peer to peer — the contract is the same.
//!
//! **Ids are paths.** A file's id is its path under the host's library
//! root (`Drum Kits/GGD Modern and Massive 2/Packs/Kick/….signalpack`),
//! the same for each variant (a `Proxy/` tree holds the `proxy` twin of
//! the `full` file). A device that saves each file at its id has the
//! studio's layout, so what it downloads plays where it lands.
//!
//! Plain `facet::Facet` data; compiles for wasm and embedded.

use facet::Facet;
use thiserror::Error;

/// One file a host offers.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ContentInfo {
    /// Its path under the library root, `/`-separated, variant tree
    /// stripped.
    pub id: String,
    /// What it is: `pack`, `engine`, `kit-preset`, `mix`, `module`,
    /// `space`, `nam`, `ir`, `config`, or `file`.
    pub kind: String,
    /// `proxy` (Ogg — streaming, phones) or `full` (lossless).
    pub variant: String,
    pub size_bytes: u64,
    /// Hex SHA-256; empty while the host is still hashing (verify only
    /// when there is one).
    pub sha256: String,
    /// Last modified, seconds since the Unix epoch — what sync compares.
    pub modified: u64,
}

/// A set of files that is one thing to have: a drum kit (its preset, its
/// engines, their packs, the library's mixes), a rig's library.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct BundleInfo {
    /// Stable id: `<kind>:<id of its root file>`.
    pub id: String,
    /// `drum-kit`, …
    pub kind: String,
    /// What a person calls it ("Organic Oomph").
    pub name: String,
    /// The library it belongs to ("Drum Kits/GGD Modern and Massive 2").
    pub library: String,
    pub variant: String,
    /// Every file's bytes, summed.
    pub size_bytes: u64,
    pub files: u64,
}

/// One streamed slice of a file. Chunks arrive in order, contiguous; the
/// stream closing without error is the end of the file.
#[derive(Clone, PartialEq, Eq, Debug, Default, Facet)]
pub struct ContentChunk {
    /// Absolute byte offset within the file.
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Debug, Facet, Error)]
#[repr(u8)]
pub enum ContentError {
    #[error("not found")]
    NotFound,
    #[error("invalid range: {0}")]
    InvalidRange(String),
    #[error("io: {0}")]
    Io(String),
}

// Streamed payloads cross vox lanes by reference (`SelfRef<T>::get`).
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for ContentInfo {
    type Ref<'a> = Self;
}
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for ContentChunk {
    type Ref<'a> = Self;
}
#[cfg(feature = "vox")]
unsafe impl vox_types::Reborrow for ContentError {
    type Ref<'a> = Self;
}

#[cfg(feature = "vox")]
pub mod content {
    //! `ContentLibrary` → `ContentLibraryClient` / `ContentLibraryService`.
    use super::{BundleInfo, ContentChunk, ContentError, ContentInfo};

    #[architect::rpc]
    pub trait ContentLibrary {
        /// Every file whose id starts with `prefix` ("" for all), each
        /// variant listed apart.
        fn catalog(&self, prefix: String) -> Vec<ContentInfo>;
        /// Every bundle of `kind` ("" for all).
        fn bundles(&self, kind: String) -> Vec<BundleInfo>;
        /// A bundle's files, in the variant it was listed in.
        fn bundle_files(&self, id: String, variant: String) -> Vec<ContentInfo>;
        /// Stream file `(id, variant)` from byte `start` to its end as
        /// ordered [`ContentChunk`]s on `tx` (start > 0 resumes). The
        /// closed stream is the end of the file.
        async fn read(
            &self,
            id: String,
            variant: String,
            start: u64,
            tx: vox::Tx<ContentChunk>,
        ) -> Result<(), ContentError>;
    }
}

#[cfg(feature = "vox")]
pub use content::ContentLibrary;

/// What a file is, by its name: the kinds a host serves. `None` for a file
/// no rig plays from (raw recordings, caches) — not offered.
#[must_use]
pub fn kind_of(id: &str) -> Option<&'static str> {
    let lower = id.to_ascii_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    let in_dir = |d: &str| lower.split('/').any(|seg| seg == d);
    Some(match ext {
        "signalpack" => "pack",
        "signalengine" => "engine",
        "signalpreset" => "kit-preset",
        "preset" if in_dir("mixes") => "mix",
        "nam" => "nam",
        "wav" if in_dir("irs") || in_dir("ir") => "ir",
        "styx" => "config",
        _ if lower.split('/').any(|seg| seg.ends_with(".space")) => "space",
        _ if in_dir("modules") => "module",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::kind_of;

    #[test]
    fn files_are_known_by_name() {
        assert_eq!(
            kind_of("Drum Kits/MM2/Packs/Kick/K.signalpack"),
            Some("pack")
        );
        assert_eq!(
            kind_of("Drum Kits/MM2/Engines/K.signalengine"),
            Some("engine")
        );
        assert_eq!(
            kind_of("Drum Kits/MM2/Presets/Organic Oomph.signalpreset"),
            Some("kit-preset")
        );
        assert_eq!(kind_of("Drum Kits/MM2/Mixes/Angr.preset"), Some("mix"));
        assert_eq!(
            kind_of("Drum Kits/MM2/Space/pieces.space/space.json"),
            Some("space")
        );
        assert_eq!(kind_of("rig/models/Deluxe.nam"), Some("nam"));
        assert_eq!(kind_of("rig/irs/Vox.wav"), Some("ir"));
        assert_eq!(kind_of("rig/presets.styx"), Some("config"));
        assert_eq!(kind_of("Sampled/Kick/Hit 1.wav"), None);
        assert_eq!(
            kind_of("Drum Kits/MM2/Packs/Kick/K.signalpack.sha256"),
            None
        );
    }
}
