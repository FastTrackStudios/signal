//! Application operations shared by every host surface.
use crate::DrumHost;
use expression_editor_audio::{
    apply_quantize::GroupError, daw_bound::DrumDaw, quantize::SplitConfig,
};
use expression_editor_core::{Editor, drum::HitGesture};

impl<D: DrumDaw> DrumHost<D> {
    /// Reconcile every lane after a host edit. View state stays in the editor.
    pub fn refresh_editor(&self, editor: &mut Editor) {
        for (guid, doc) in self.refresh() {
            editor.reload_track_doc(&guid, doc);
        }
    }

    /// Dispatch one gesture, preserving kit grouping and the host undo domain.
    /// Hit-list changes do not write audio; committed audio edits reload all lanes.
    pub fn edit_hit(&self, editor: &mut Editor, gesture: HitGesture) -> Result<(), GroupError> {
        let cfg = SplitConfig {
            leading_pad_secs: 0.005,
            crossfade_secs: 0.005,
        };
        match gesture {
            HitGesture::Add { lane, at } => {
                if lane_track(editor, &lane).is_some() {
                    let landed = self.add_hit(at, 0.05);
                    add_hit_note(editor, &lane, landed);
                }
                return Ok(());
            }
            HitGesture::Remove { lane, hit } => {
                if lane_track(editor, &lane).is_some() {
                    self.remove_hit(hit);
                    remove_hit_note(editor, &lane, hit);
                }
                return Ok(());
            }
            HitGesture::Slip { hit, next, delta } => {
                let next = if next.is_finite() {
                    next
                } else {
                    self.take_secs
                };
                self.slip(hit, next, delta, cfg)?;
            }
            HitGesture::Stretch {
                hit,
                prev,
                next,
                delta,
                both,
            } => {
                let prev = if prev.is_finite() { prev } else { 0.0 };
                let next = if next.is_finite() {
                    next
                } else {
                    self.take_secs
                };
                self.stretch(hit, prev, next, delta, both)?;
            }
            HitGesture::Split { at } => {
                self.split(at, cfg)?;
            }
        }
        self.refresh_editor(editor);
        Ok(())
    }
}

/// Resolve a drawn role lane to its first visible member.
fn lane_track(ed: &Editor, lane: &str) -> Option<usize> {
    use expression_editor_core::kit::LaneRole;
    let role = LaneRole::ALL.into_iter().find(|r| r.label() == lane)?;
    ed.tracks
        .role_members(role)
        .iter()
        .filter_map(|g| ed.tracks.index_of_guid(g))
        .find(|&i| ed.tracks.track(i).is_some_and(|t| !t.hidden))
}

/// Draw a hand-added hit: a slice note in the lane's document, from the
/// refined onset to the next hit. The daw is untouched — this is the
/// hit *list* changing, which is all the spec allows before a drag or
/// Apply. r[impl drums.manual.add-remove]
fn add_hit_note(ed: &mut Editor, lane: &str, at: f64) {
    let Some(track) = lane_track(ed, lane) else {
        return;
    };
    // The active document is the only writable one; selecting the hit's
    // lane is what a click there does anyway.
    ed.switch_track(track);
    let ups = ed.doc.time_base.units_per_second(ed.bpm);
    let start = at * ups;
    let end = ed
        .doc
        .notes
        .iter()
        .map(|n| n.start)
        .filter(|&s| s > start + 1e-6)
        .fold(ed.doc.end, f64::min);
    let id = expression_editor_core::doc::NoteId(
        ed.doc.notes.iter().map(|n| n.id.0).max().unwrap_or(0) + 1,
    );
    let mut note = expression_editor_core::Note::new(id, start, end.max(start + 1.0), 1);
    // A hand-placed hit is intent — full weight, like the host's list.
    note.velocity = 1.0;
    note.weight = 1.0;
    ed.doc.push(note);
}

/// Erase a hit's slice note from the lane's document.
/// r[impl drums.manual.add-remove]
fn remove_hit_note(ed: &mut Editor, lane: &str, hit: f64) {
    let Some(track) = lane_track(ed, lane) else {
        return;
    };
    ed.switch_track(track);
    let ups = ed.doc.time_base.units_per_second(ed.bpm);
    let target = hit * ups;
    let tol = 0.015 * ups;
    ed.doc.notes.retain(|n| (n.start - target).abs() > tol);
}
