//! The take going in, drawn on a folder that is still closed.
//!
//! REAPER draws nothing on a closed folder, which is why an overview
//! collapses the moment recording starts: the engineer folds the bass
//! away to see the session and then cannot see the take land. This
//! draws it.
//!
//! # Why the meter stream, and not the file being written
//!
//! Neither backend serves peaks for a take in progress — there is no
//! finished take to ask about, and reading a half-written file races
//! the writer. But both already serve a **30 Hz meter frame carrying
//! every track's peak**, for the mixer. Accumulating those into one
//! column each is enough: thirty columns a second is finer than any
//! zoom draws during a take, and it is live by construction rather than
//! by polling something.
//!
//! When the take lands, the real peaks replace all of this and the
//! preview is thrown away. Nothing here is project state.
//!
//! # Why only armed children
//!
//! **Armed is the only honest signal that a track is going in.** A
//! child that is not armed is playing back, not recording, and folding
//! its meter into the preview would draw the engineer a picture of the
//! take plus whatever else happened to be audible.

use daw_proto::peak::MeterFrame;

use expression_editor_core::kit::LaneRole;

use super::fold::{Child, Fold, FoldColumn, GroupBy, SLOTS, Side};

/// One recording child's accumulating column buffer.
struct Lane {
    guid: String,
    slot: usize,
    /// One `(min, max)` per meter frame since the record started.
    columns: Vec<(f32, f32)>,
}

/// A take in progress, folded the way a finished one would be.
pub struct Preview {
    group_by: GroupBy,
    lanes: Vec<Lane>,
    /// Frames taken, which is the column count every lane shares — a
    /// lane that missed a frame is padded, so the lanes never shear.
    frames: usize,
}

impl Preview {
    /// Start a preview over the children that are actually recording.
    ///
    /// `armed` is asked per child rather than read off the child,
    /// because arm is live state that the gangs move underneath us and
    /// the fold's `Child` is a snapshot of the project's shape.
    #[must_use]
    pub fn new(children: &[Child], group_by: GroupBy, armed: &dyn Fn(&str) -> bool) -> Self {
        let lanes = children
            .iter()
            .filter(|child| armed(&child.guid) && !child.muted)
            .filter_map(|child| {
                let slot = match group_by {
                    GroupBy::Role => LaneRole::ALL.iter().position(|r| *r == child.role),
                    GroupBy::Side => child.side.map(Side::slot).or(Some(0)),
                }?;
                Some(Lane {
                    guid: child.guid.clone(),
                    slot,
                    columns: Vec::new(),
                })
            })
            .collect();
        Self {
            group_by,
            lanes,
            frames: 0,
        }
    }

    /// Whether anything is being recorded into this folder.
    ///
    /// A folder with no armed child answers `true` here and draws
    /// nothing, which is the negative control the rule needs: a closed
    /// folder that is not recording must look exactly as it did.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }

    /// How many columns have accumulated.
    #[must_use]
    pub const fn columns(&self) -> usize {
        self.frames
    }

    /// Take one meter frame.
    ///
    /// A frame for another project is ignored; the stream carries every
    /// project with an attached engine and subscribers filter their own.
    /// A lane whose track is missing from the frame is padded with
    /// silence rather than skipped, so every lane stays the same length
    /// and the columns line up.
    pub fn push(
        &mut self,
        project_guid: &str,
        frame: &MeterFrame,
        level_of: &dyn Fn(&str, &MeterFrame) -> Option<f32>,
    ) {
        if frame.project_guid != project_guid {
            return;
        }
        for lane in &mut self.lanes {
            let peak = level_of(&lane.guid, frame).unwrap_or(0.0);
            // A meter is a magnitude; a waveform has two sides. Drawing
            // it symmetrically is honest about what is known: the level,
            // not the shape.
            lane.columns.push((-peak, peak));
        }
        self.frames += 1;
    }

    /// Fold what has accumulated, in the shape the finished picture
    /// uses — so the same drawing code renders both and the row does
    /// not change appearance when the take lands.
    #[must_use]
    pub fn fold(&self) -> Fold {
        let mut columns = Vec::with_capacity(self.frames);
        for i in 0..self.frames {
            let mut column = FoldColumn {
                slots: [None; SLOTS],
            };
            for lane in &self.lanes {
                let Some((lo, hi)) = lane.columns.get(i).copied() else {
                    continue;
                };
                let slot = &mut column.slots[lane.slot.min(SLOTS - 1)];
                *slot = Some(slot.map_or((lo, hi), |(a, b): (f32, f32)| (a.min(lo), b.max(hi))));
            }
            columns.push(column);
        }
        Fold {
            group_by: self.group_by,
            columns,
        }
    }
}

#[cfg(test)]
mod tests {
    use daw_proto::peak::MeterFrame;

    use super::{GroupBy, LaneRole, Preview};
    use crate::folder_item::fold::Child;

    fn child(guid: &str, role: LaneRole) -> Child {
        Child {
            guid: guid.to_owned(),
            name: guid.to_owned(),
            role,
            side: None,
            muted: false,
            hidden: false,
            takes: Vec::new(),
        }
    }

    /// The slot a role folds into — the same rule `slot_of` uses, kept
    /// here so the test asserts against the mapping rather than against
    /// an index it invented.
    fn slot(role: LaneRole) -> usize {
        LaneRole::ALL
            .iter()
            .position(|r| *r == role)
            .expect("every role has a slot")
    }

    fn kit() -> Vec<Child> {
        vec![
            child("kick", LaneRole::Kick),
            child("snare", LaneRole::Snare),
            child("oh", LaneRole::Other),
        ]
    }

    fn frame(project: &str, levels: &[(&str, f32)]) -> (MeterFrame, Vec<(String, f32)>) {
        let owned: Vec<(String, f32)> = levels.iter().map(|(g, v)| ((*g).to_owned(), *v)).collect();
        (
            MeterFrame {
                project_guid: project.to_owned(),
                tracks: Vec::new(),
            },
            owned,
        )
    }

    /// Look a level up out of the side table the fixture carries, so
    /// the test does not depend on how a frame names its tracks.
    fn lookup<'a>(
        table: &'a [(String, f32)],
    ) -> impl Fn(&str, &MeterFrame) -> Option<f32> + use<'a> {
        move |guid, _| table.iter().find(|(g, _)| g == guid).map(|(_, v)| *v)
    }

    /// **The negative control.** A folder with nothing armed draws
    /// nothing — a closed folder that is not recording must look
    /// exactly as it did.
    ///
    /// r[verify flow.scenes.folder-record-preview]
    #[test]
    fn a_folder_with_nothing_armed_previews_nothing() {
        let preview = Preview::new(&kit(), GroupBy::Role, &|_| false);
        assert!(preview.is_empty(), "an unarmed folder drew a preview");
        assert!(preview.fold().columns.is_empty());
    }

    /// An unarmed child is playing back, not recording, so its meter
    /// must not reach the preview — otherwise the picture is the take
    /// plus whatever else was audible.
    ///
    /// r[verify flow.scenes.folder-record-preview]
    #[test]
    fn only_armed_children_contribute() {
        let mut preview = Preview::new(&kit(), GroupBy::Role, &|guid| guid == "kick");
        let (frame, table) = frame("p", &[("kick", 0.5), ("snare", 0.9)]);
        preview.push("p", &frame, &lookup(&table));

        let folded = preview.fold();
        let column = folded.columns.first().expect("a column");
        assert!(
            column.slots[slot(LaneRole::Kick)].is_some(),
            "the armed kick is missing"
        );
        assert!(
            column.slots[slot(LaneRole::Snare)].is_none(),
            "an unarmed snare reached the preview"
        );
    }

    /// The preview grows one column per frame, which is what makes it
    /// live rather than polled.
    ///
    /// r[verify flow.scenes.folder-record-preview]
    #[test]
    fn it_grows_one_column_per_meter_frame() {
        let mut preview = Preview::new(&kit(), GroupBy::Role, &|_| true);
        let (frame, table) = frame("p", &[("kick", 0.4), ("snare", 0.6), ("oh", 0.2)]);
        for _ in 0..60 {
            preview.push("p", &frame, &lookup(&table));
        }
        assert_eq!(preview.columns(), 60, "two seconds at 30 Hz");
        assert_eq!(preview.fold().columns.len(), 60);
    }

    /// A frame belonging to another project is ignored: the stream
    /// carries every project with an attached engine.
    #[test]
    fn another_projects_frame_is_ignored() {
        let mut preview = Preview::new(&kit(), GroupBy::Role, &|_| true);
        let (frame, table) = frame("elsewhere", &[("kick", 0.9)]);
        preview.push("p", &frame, &lookup(&table));
        assert_eq!(preview.columns(), 0, "another project's meter was taken");
    }

    /// A meter is a magnitude, so the preview draws it symmetrically —
    /// honest about knowing the level and not the shape.
    #[test]
    fn a_level_draws_symmetrically() {
        let mut preview = Preview::new(&kit(), GroupBy::Role, &|guid| guid == "kick");
        let (frame, table) = frame("p", &[("kick", 0.5)]);
        preview.push("p", &frame, &lookup(&table));
        let folded = preview.fold();
        let (lo, hi) = folded.columns[0].slots[slot(LaneRole::Kick)].expect("the kick");
        assert!((lo + 0.5).abs() < f32::EPSILON);
        assert!((hi - 0.5).abs() < f32::EPSILON);
    }
}
