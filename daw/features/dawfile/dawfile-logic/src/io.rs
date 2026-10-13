//! File I/O entry point for Logic Pro session parsing.

use crate::error::LogicResult;
use crate::parse;
use crate::types::LogicSession;
use std::path::Path;

/// Read and parse a Logic Pro session bundle (`.logicx`).
///
/// The path must point to the `.logicx` directory bundle.  The bundle's
/// active alternative (`Alternatives/000/`) is parsed.
///
/// # Example
///
/// ```no_run
/// let session = dawfile_logic::read_session("MyProject.logicx")?;
///
/// println!("Tempo: {} BPM @ {}Hz", session.bpm, session.sample_rate);
/// for track in &session.tracks {
///     println!("  {}: {:?}", track.name, track.kind);
/// }
/// # Ok::<(), dawfile_logic::LogicError>(())
/// ```
pub fn read_session(path: impl AsRef<Path>) -> LogicResult<LogicSession> {
    parse::parse_session(path.as_ref())
}
