//! The loro oplog, persisted (#173).
//!
//! `<name>.session` is the readable source of truth and the project is a
//! loro CRDT doc in memory. The hole that forces this module: if the
//! CRDT exists only in memory, every load builds a fresh doc from text
//! and throws away all causality — so two people who each edit offline
//! and save their own `.session` share no history, reconciling them
//! degrades to text merging, and the CRDT buys nothing the moment it
//! touches disk.
//!
//! So the oplog goes into `objects/`, where it fits exactly: large,
//! binary, append-only, immutable, content-addressed.
//!
//! ## Hand edits win
//!
//! The manifest is hand-editable, and that is a stated requirement. An
//! oplog built from text that has since been edited by hand is a *stale
//! log*: replaying it would resurrect the state the human just
//! overwrote. So the hash of the `.session` the oplog was built from is
//! stored beside it, and on load a mismatch **discards the oplog and
//! starts fresh history from the text**.
//!
//! The cost is that hand-editing forfeits offline merge for that
//! project. That is the acceptable side of the trade: losing merge
//! history is recoverable, silently losing an edit somebody typed is
//! not. The better long-term answer is a CLI that applies scripted
//! edits *through* the CRDT, which is why this is a trade rather than a
//! design.

use crate::error::DawResult;
use crate::id::ObjectId;
use facet::Facet;
use loro::LoroDoc;

/// Where a project's oplog lives, and what it was built from.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct OplogRef {
    /// The blob in `objects/` holding the exported oplog.
    pub object: ObjectId,
    /// Hash of the manifest text this oplog was built from.
    ///
    /// The staleness check, and the only thing standing between a hand
    /// edit and being silently replayed over.
    pub text_hash: String,
}

/// Digest of a manifest's text.
///
/// Content, not mtime: a project synced between machines arrives with
/// whatever timestamp the transport felt like, and a hash is the only
/// thing that answers "is this the text the log was built from".
pub fn hash_text(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

/// Export a doc's full history for storage.
pub fn export(doc: &LoroDoc) -> DawResult<Vec<u8>> {
    doc.export(loro::ExportMode::all_updates())
        .map_err(|e| crate::error::DawError::Oplog(format!("exporting oplog: {e}")))
}

/// Rebuild a doc from stored history.
pub fn import(bytes: &[u8]) -> DawResult<LoroDoc> {
    let doc = LoroDoc::new();
    doc.import(bytes)
        .map_err(|e| crate::error::DawError::Oplog(format!("importing oplog: {e}")))?;
    Ok(doc)
}

/// Load an oplog only if it still belongs to this text.
///
/// `None` means start fresh history — either nothing was stored, or the
/// manifest has been hand-edited since.
pub fn load_if_current(
    stored: Option<&OplogRef>,
    bytes: Option<&[u8]>,
    manifest_text: &str,
) -> Option<LoroDoc> {
    let stored = stored?;
    if stored.text_hash != hash_text(manifest_text) {
        // Deliberately silent about *discarding*: a hand edit is a
        // normal thing to do, not an error to report.
        return None;
    }
    import(bytes?).ok()
}

/// Merge another machine's history into this doc.
///
/// This is the whole point of persisting the log: two people who each
/// edited offline reconcile by *replaying causality*, not by diffing
/// text.
pub fn merge(into: &LoroDoc, other_oplog: &[u8]) -> DawResult<()> {
    into.import(other_oplog)
        .map_err(|e| crate::error::DawError::Oplog(format!("merging oplog: {e}")))?;
    Ok(())
}
