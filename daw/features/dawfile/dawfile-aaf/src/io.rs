//! File I/O entry points for AAF session parsing.

use crate::error::AafResult;
use crate::parse;
use crate::types::AafSession;
use std::path::Path;

/// Read and parse an AAF session file.
///
/// Supports any standard AAF file (`.aaf`) produced by Pro Tools, Avid Media
/// Composer, Adobe Premiere, DaVinci Resolve, Logic Pro, or other AAF-capable
/// tools.
///
/// The entire compound file is read into memory; for very large files (>1 GB
/// of metadata — unusual) consider streaming access instead.
///
/// # Example
///
/// ```no_run
/// let session = dawfile_aaf::read_session("session.aaf")?;
///
/// println!("Sample rate: {} Hz", session.session_sample_rate);
/// for track in &session.tracks {
///     println!("  Track {:>3}: {} ({} clips)", track.slot_id, track.name, track.clips.len());
/// }
/// # Ok::<(), dawfile_aaf::AafError>(())
/// ```
pub fn read_session(path: impl AsRef<Path>) -> AafResult<AafSession> {
    parse::parse_session(path.as_ref())
}
