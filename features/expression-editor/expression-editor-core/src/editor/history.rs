//! Editor history behavior.
use super::*;

impl Editor {
    /// Apply an edit through the undo stack.
    pub fn apply(&mut self, edit: &Edit) -> bool {
        self.history.apply(&mut self.doc, edit)
    }

    /// Snapshot before a drag that will stream many edits, so the whole
    /// gesture collapses into one undo step.
    pub fn begin_gesture(&mut self) {
        self.history.begin_gesture(&self.doc);
    }

    /// Apply without recording — for the streaming edits inside a
    /// gesture already opened with [`Editor::begin_gesture`].
    pub fn apply_live(&mut self, edit: &Edit) -> bool {
        edit.apply(&mut self.doc)
    }

    /// Put the document back to how the open gesture found it.
    ///
    /// For destructive drags, which cannot be expressed as a delta.
    /// Moving a razor's contents *carves* — it splits notes at the area
    /// boundaries and clears the ground it lands on — so re-running it
    /// against a document it has already carved does not redo the move,
    /// it cuts a second time in a place the material has since left.
    /// Frame by frame that shreds the take: the notes that moved first
    /// stop moving, notes that were never in the area get dragged along,
    /// and everything ends up in pieces.
    ///
    /// A delta-based drag has no such problem, which is why nothing else
    /// here needs this. The answer for the ones that do is to recompute
    /// from the gesture's own starting point every frame — which costs
    /// nothing to remember, because [`Editor::begin_gesture`] already
    /// snapshotted it for undo.
    ///
    /// Undo is untouched: the snapshot is cloned, not consumed.
    pub fn revert_gesture(&mut self) -> bool {
        let Some(base) = self.history.gesture_base() else {
            return false;
        };
        self.doc = base.clone();
        true
    }

    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo(&mut self.doc);
        self.resync_row_space(ok);
        ok
    }

    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo(&mut self.doc);
        self.resync_row_space(ok);
        ok
    }

    /// Bring the editor's row-space view back in line with the
    /// document after history moves underneath it.
    ///
    /// Some edits change the row space itself — `SetBands` moves the
    /// band splits — and the document is the authority. Without this an
    /// undo restored the old splits in the document while the roll and
    /// the inspector kept drawing the new ones, and every later edit
    /// resorted against a row space nobody could see.
    pub(super) fn resync_row_space(&mut self, changed: bool) {
        if changed && self.row_space != self.doc.row_space {
            self.row_space = self.doc.row_space.clone();
            self.refresh_fold();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
}
