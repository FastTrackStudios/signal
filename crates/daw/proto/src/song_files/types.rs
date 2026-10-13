//! Wire types for [`SongFiles`](super::SongFiles).

use facet::Facet;

/// One file of a song folder.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct SongFile {
    /// Relative to the song folder, `/`-separated (`Media/Proxies/Bass.ogg`).
    pub path: String,
    /// Bytes.
    pub size: u64,
}

/// The most bytes one [`SongFiles::read`](super::SongFiles::read) returns.
pub const MAX_READ: u32 = 1 << 20;
