//! Where a chosen chord goes.
//!
//! The panel knows what you picked; it deliberately does not know how to
//! reach a DAW. That's this seam. `chord-tool` depends only on keyflow
//! and dioxus, so the same component runs in a desktop window with
//! nothing behind it and inside REAPER with a real backend — swapping the
//! sink, not the UI.
//!
//! Provide one with `use_context_provider`; the panel falls back to
//! [`LogSink`] when nothing is provided, which is what makes the
//! standalone example work with no DAW at all.

use std::sync::Arc;

/// A destination for chords the panel fires.
pub trait ChordSink: Send + Sync + 'static {
    /// Audition without writing anything. The cheap, safe gesture.
    fn preview(&self, notes: &[u8]);

    /// Commit `notes` at the edit cursor, lasting `beats`, and advance
    /// the cursor past it so repeated inserts lay out a progression.
    ///
    /// Returns a message on failure rather than an error type: the only
    /// consumer is a status line, and a panel should say "no track
    /// selected" rather than swallow it.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message when there is nowhere to insert
    /// (e.g. no track/edit cursor selected).
    fn insert(&self, notes: &[u8], beats: u32) -> Result<(), String>;
}

/// Cloneable handle so the sink can live in Dioxus context.
#[derive(Clone)]
pub struct SinkHandle(pub Arc<dyn ChordSink>);

impl SinkHandle {
    pub fn new(sink: impl ChordSink) -> Self {
        Self(Arc::new(sink))
    }
}

impl PartialEq for SinkHandle {
    /// Identity, not contents — a sink is a service, and re-rendering
    /// shouldn't depend on comparing one.
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The no-DAW default: reports what *would* happen.
///
/// Used by the standalone example, and by the panel when no sink is
/// provided. Saying "would insert" is deliberate — a panel that silently
/// does nothing when its backend is missing is worse than one that says
/// so.
pub struct LogSink;

impl ChordSink for LogSink {
    fn preview(&self, notes: &[u8]) {
        tracing::debug!(?notes, "preview (no sink attached)");
    }

    fn insert(&self, notes: &[u8], beats: u32) -> Result<(), String> {
        Err(format!(
            "no DAW attached — would insert {notes:?} for {beats} beats"
        ))
    }
}
