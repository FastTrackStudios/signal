//! Parse orchestration for `.logicx` bundles.
//!
//! Parsing proceeds in three stages:
//!
//! 1. **Bundle** — read the directory structure, parse the two metadata plists
//!    (`ProjectInformation.plist` and `MetaData.plist`) into
//!    [`bundle::BundleMeta`].
//! 2. **Chunks** — parse the binary `ProjectData` file into a flat list of
//!    [`crate::types::LogicChunk`] records.
//! 3. **Interpret** — walk the chunk list and extract tracks, markers, tempo
//!    events, and summing groups into a [`crate::types::LogicSession`].
//!
//! Stage 3 is intentionally minimal right now: the binary payload format
//! of individual chunk types is complex and partially reverse-engineered.
//! The raw chunks are always surfaced in [`crate::types::LogicSession::chunks`] so callers
//! can inspect them directly while the interpreter is being built out.

pub mod aufl;
pub mod aurg;
pub mod bundle;
pub mod chunk;
pub mod interpret;

use crate::error::LogicResult;
use crate::types::LogicSession;
use std::path::Path;

/// Parse a `.logicx` bundle at `path` into a [`LogicSession`].
pub fn parse_session(path: &Path) -> LogicResult<LogicSession> {
    // Stage 1: bundle metadata + raw ProjectData bytes
    let bundle = bundle::read_bundle(path)?;

    // Stage 2: chunk list
    let chunks = chunk::parse_chunks(&bundle.project_data)?;

    // Stage 3: interpret chunks into session data
    let session = interpret::build_session(bundle.meta, chunks);

    Ok(session)
}
