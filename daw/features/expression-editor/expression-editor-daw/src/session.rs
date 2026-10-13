//! An editing session over a DAW take.
//!
//! Generic over the service traits rather than over a concrete backend,
//! so the same session drives REAPER and standalone — and so it is
//! testable without a DAW at all, which is what the tests here do.
//!
//! The session owns the round trip: it remembers where a document came
//! from, tracks whether it has been changed, and writes back to the
//! same place. Without that, "open the editor on the selected item" and
//! "save it" are two unrelated operations that can disagree about which
//! take they mean.

use daw::service::midi::{Midi, MidiTakeLocation, PpqRange};
use daw::service::{ItemRef, Items, ProjectContext, TakeRef};
use expression_editor_core::{Editor, ExpressionDoc, Viewport};

use crate::{to_content, to_doc, write_warnings};

/// A document, where it came from, and whether it has diverged.
pub struct Session {
    pub editor: Editor,
    /// The take this was loaded from, and where a write goes back to.
    pub location: MidiTakeLocation,
    /// Bend range the document was read with. Writing back with a
    /// different one would rescale every curve silently.
    pub bend_range: f64,
    /// Document as loaded, for change detection.
    baseline: ExpressionDoc,
}

impl Session {
    /// Load a specific take.
    pub fn load<D: Midi>(
        daw: &D,
        location: MidiTakeLocation,
        bend_range: f64,
        viewport: Viewport,
    ) -> Self {
        let snapshot = daw.read_take(location.clone());
        let doc = to_doc(&snapshot, bend_range);
        let baseline = doc.clone();
        let mut editor = Editor::new(doc, viewport);
        editor.reset_view();
        Self {
            editor,
            location,
            bend_range,
            baseline,
        }
    }

    /// Load the first selected MIDI item's active take.
    ///
    /// Returns `None` when nothing is selected, which the caller should
    /// report rather than silently opening an empty editor — an empty
    /// canvas looks like a broken load.
    pub fn load_selected<D: Midi + Items>(
        daw: &D,
        project: ProjectContext,
        bend_range: f64,
        viewport: Viewport,
    ) -> Option<Self> {
        let item = daw.get_selected_items(project.clone()).into_iter().next()?;
        let location =
            MidiTakeLocation::new(project, ItemRef::Guid(item.guid.clone()), TakeRef::Active);
        let mut session = Self::load(daw, location, bend_range, viewport);
        // The host owns track identity. Adopting it here is what lets
        // anything persisted against this track — a mode correction, a
        // lane layout — still resolve when the project is reopened.
        session
            .editor
            .adopt_track_identity(item.track_guid.clone(), None);
        Some(session)
    }

    /// Whether the document differs from what was loaded.
    pub fn is_dirty(&self) -> bool {
        self.editor.doc != self.baseline
    }

    /// Anything the caller should know before overwriting the take.
    pub fn warnings(&self) -> Vec<String> {
        write_warnings(&self.editor.doc)
    }

    /// Write the document back to its take, replacing it.
    ///
    /// Returns the note indices the backend assigned.
    pub fn write_back<D: Midi>(&mut self, daw: &D) -> Vec<u32> {
        let content = to_content(&self.editor.doc);
        let indices = daw.write_take(
            self.location.clone(),
            content,
            daw::service::midi::WriteMode::Replace,
        );
        // The document is now what is in the take, so a second write
        // without further edits is correctly a no-op.
        self.baseline = self.editor.doc.clone();
        indices
    }

    /// Write only a time span back, leaving the rest of the take.
    ///
    /// What an editor working on a selection wants: overwriting the
    /// whole take to change two bars would discard anything a user did
    /// elsewhere since loading.
    pub fn write_range<D: Midi>(&mut self, daw: &D, t0: f64, t1: f64) -> Vec<u32> {
        let mut doc = self.editor.doc.clone();
        doc.notes.retain(|n| n.end > t0 && n.start < t1);
        let content = to_content(&doc);
        let indices = daw.replace_range(self.location.clone(), PpqRange::new(t0, t1), content);
        self.baseline = self.editor.doc.clone();
        indices
    }

    /// Discard local edits and re-read the take.
    pub fn reload<D: Midi>(&mut self, daw: &D) {
        let snapshot = daw.read_take(self.location.clone());
        let doc = to_doc(&snapshot, self.bend_range);
        self.baseline = doc.clone();
        // The camera and tool survive a reload; only the material is
        // replaced, or every refresh would throw away the user's view.
        self.editor.doc = doc;
        self.editor.selection.clear();
    }

    /// Open a standard MIDI file as a session with no take behind it.
    ///
    /// The location is still required, because a file session has to
    /// know where a later "write back" would go — a file opened with
    /// nowhere to put it is a dead end.
    pub fn from_file<D: Midi>(
        daw: &D,
        path: &str,
        track: u32,
        location: MidiTakeLocation,
        bend_range: f64,
        viewport: Viewport,
    ) -> Option<Self> {
        let snapshot = daw.read_midi_file(path.to_string(), track)?;
        let doc = to_doc(&snapshot, bend_range);
        let baseline = doc.clone();
        let mut editor = Editor::new(doc, viewport);
        editor.reset_view();
        Some(Self {
            editor,
            location,
            bend_range,
            baseline,
        })
    }

    /// Export the document as a standard MIDI file.
    pub fn export_file<D: Midi>(&self, daw: &D, path: &str) -> bool {
        let content = to_content(&self.editor.doc);
        let ppq = match self.editor.doc.time_base {
            expression_editor_core::TimeBase::Ppq { ppq } => ppq,
            expression_editor_core::TimeBase::Frames { .. } => 960.0,
        };
        daw.write_midi_file(path.to_string(), content, ppq)
    }
}
