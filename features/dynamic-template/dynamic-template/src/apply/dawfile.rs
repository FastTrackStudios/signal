//! [`TemplateTarget`] over an `.RPP` file — the offline backend.
//!
//! Wraps `dawfile_reaper::types::ReaperProject`, so a whole album's worth of
//! sessions can be organized in a batch without REAPER running. The live
//! counterpart is [`super::reaper`]; both are driven by
//! [`apply_buses`](super::apply_buses).
//!
//! # Track ids are indices
//!
//! [`RppTarget::TrackId`] is a position in `ReaperProject::tracks`. That is only
//! stable because this backend exclusively **appends** — inserting in the
//! middle would invalidate every id handed out so far, and REAPER's `AUXRECV`
//! records its source track by index too, so a mid-list insert would silently
//! repoint existing sends. If a reordering pass is ever added it has to fix up
//! both.

use color_palette::Color;
use daw_proto::FolderDepthChange;
use dawfile_reaper::types::track::{
    BusCompactSettings, FolderSettings, FolderState, MasterSendSettings, MuteSoloSettings,
    ReceiveSettings, Track, TrackSoloState,
};
use dawfile_reaper::types::ReaperProject;

use super::{find_group, TemplateTarget};

/// A parsed `.RPP` project the template can be applied to.
pub struct RppTarget<'a> {
    project: &'a mut ReaperProject,
}

impl<'a> RppTarget<'a> {
    /// Borrow `project` as a template target.
    pub const fn new(project: &'a mut ReaperProject) -> Self {
        Self { project }
    }

    /// The wrapped project.
    #[must_use]
    pub const fn project(&self) -> &ReaperProject {
        self.project
    }

    fn track(&mut self, id: usize) -> Option<&mut Track> {
        self.project.tracks.get_mut(id)
    }

    /// Tracks whose folder depth goes negative — each closes a folder that was
    /// never opened.
    ///
    /// REAPER tolerates this by clamping, so a project can carry the damage
    /// invisibly for years, but the nesting it describes is not a tree. Anything
    /// that reasons about folder membership (including
    /// [`gather_into_folder`](TemplateTarget::gather_into_folder), which will
    /// only move genuinely top-level tracks) is working from bad data until it
    /// is repaired.
    #[must_use]
    pub fn negative_depths(&self) -> Vec<(usize, String, i32)> {
        let mut depth: i32 = 0;
        let mut out = Vec::new();
        for (i, track) in self.project.tracks.iter().enumerate() {
            depth = depth.saturating_add(track.folder.as_ref().map_or(0, |f| f.indentation));
            if depth < 0 {
                out.push((i, track.name.clone(), depth));
            }
        }
        out
    }

    /// Folder nesting depth of each track, 0 at the top level.
    ///
    /// REAPER stores nesting as a per-track *change*, so a track's own depth is
    /// the running sum of every change before it — a track with no folder
    /// settings at all can still sit three folders deep.
    fn running_depths(&self) -> Vec<i32> {
        let mut depth: i32 = 0;
        self.project
            .tracks
            .iter()
            .map(|t| {
                let here = depth;
                depth = depth.saturating_add(t.folder.as_ref().map_or(0, |f| f.indentation));
                here
            })
            .collect()
    }
}

/// This backend edits an in-memory project; nothing here can fail, so the
/// error type is uninhabited and `?` on it is free.
#[derive(Debug)]
pub enum Never {}

impl std::fmt::Display for Never {
    // rustc requires the dereference for exhaustiveness — `&T` is always
    // considered inhabited regardless of `T` ("references are always
    // considered inhabited"), so `match self {}` doesn't type-check; only
    // `match *self {}` does, and that's exactly what this lint flags. No
    // rewrite avoids the deref here.
    #[allow(clippy::uninhabited_references)]
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}

impl TemplateTarget for RppTarget<'_> {
    /// Nest a "DI" capture under its sibling primary track, for any group
    /// that opts in via [`monarchy::Group::nest_secondary_mics`] — electric
    /// guitar turns this on for its DI feed.
    ///
    /// Only reshapes the exact adjacency the house convention already
    /// tracks in: a "DI" track immediately following, at the same folder
    /// depth, a non-DI sibling. No track changes position, so no `AUXRECV`
    /// index needs rewriting — only folder nesting, mute, and collapse
    /// flags change.
    ///
    /// Idempotent: once nested, "DI" sits inside its sibling rather than
    /// beside it, so a second pass no longer sees them as siblings.
    fn nest_secondary_mics(&mut self) {
        let config = crate::default_config();
        let entries = super::contextual_paths(self);

        for pair in entries.windows(2) {
            let [main, di] = pair else { continue };
            if main.context != di.context {
                continue; // not siblings
            }
            let is_di = |name: &str| name.trim().eq_ignore_ascii_case("di");
            if !is_di(&di.name) || is_di(&main.name) {
                continue;
            }
            let Some(leaf) = di.path.last() else {
                continue;
            };
            if !find_group(&config, leaf).is_some_and(|g| g.nest_secondary_mics) {
                continue;
            }

            let main_is_plain = self.project.tracks.get(main.track).is_some_and(|t| {
                t.folder
                    .as_ref()
                    .is_none_or(|f| f.folder_state == FolderState::Regular && f.indentation == 0)
            });
            let di_indentation = self
                .project
                .tracks
                .get(di.track)
                .and_then(|t| t.folder.as_ref())
                .map_or(0, |f| f.indentation);
            if !main_is_plain || di_indentation > 0 {
                // Not the plain "two siblings" shape this expects — leave
                // whatever unusual structure is already there alone.
                continue;
            }

            if let Some(t) = self.project.tracks.get_mut(main.track) {
                t.folder = Some(FolderSettings {
                    folder_state: FolderState::FolderParent,
                    indentation: 1,
                });
                t.bus_compact = Some(BusCompactSettings {
                    arrange_collapse: 2,
                    mixer_collapse: 2,
                    wiring_collapse: 0,
                    wiring_x: 0,
                    wiring_y: 0,
                });
            }
            if let Some(t) = self.project.tracks.get_mut(di.track) {
                t.folder = Some(FolderSettings {
                    folder_state: FolderState::LastInFolder,
                    // One more level to close than before: the DI folder we
                    // just opened on `main`, on top of whatever it already closed.
                    indentation: di_indentation.saturating_sub(1),
                });
                t.mutesolo = Some(MuteSoloSettings {
                    mute: true,
                    solo: TrackSoloState::NoSolo,
                    solo_defeat: false,
                });
            }
        }
    }

    type TrackId = usize;
    type Error = Never;

    fn find_track(&self, name: &str) -> Option<usize> {
        self.project
            .tracks
            .iter()
            .position(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    fn append_track(&mut self, name: &str) -> Result<usize, Never> {
        self.project.tracks.push(Track {
            name: name.to_string(),
            ..Track::default()
        });
        Ok(self.project.tracks.len().saturating_sub(1))
    }

    fn set_folder_depth(&mut self, id: &usize, depth: FolderDepthChange) -> Result<(), Never> {
        // REAPER's ISBUS is (folder_state, indentation): a folder parent is
        // (1, 1), the last track inside n folders is (2, -n), and an ordinary
        // track is (0, 0).
        let settings = match depth {
            FolderDepthChange::Normal => FolderSettings {
                folder_state: FolderState::Regular,
                indentation: 0,
            },
            FolderDepthChange::FolderStart => FolderSettings {
                folder_state: FolderState::FolderParent,
                indentation: 1,
            },
            FolderDepthChange::ClosesLevels(n) => FolderSettings {
                folder_state: FolderState::LastInFolder,
                indentation: i32::from(n),
            },
        };
        if let Some(track) = self.track(*id) {
            track.folder = Some(settings);
        }
        Ok(())
    }

    fn set_color(&mut self, id: &usize, hex: &str) -> Result<(), Never> {
        // An unparseable color leaves the track's own color alone rather than
        // failing the whole apply — the routing matters, the tint does not.
        let Ok(color) = Color::from_hex_str(hex) else {
            return Ok(());
        };
        let native = color.to_reaper_native();
        if let Some(track) = self.track(*id) {
            track.peak_color = Some(native);
        }
        Ok(())
    }

    fn set_channel_count(&mut self, id: &usize, channels: u32) -> Result<(), Never> {
        if let Some(track) = self.track(*id) {
            track.channel_count = channels;
        }
        Ok(())
    }

    fn has_send(&self, source: &usize, dest: &usize) -> bool {
        self.project.tracks.get(*dest).is_some_and(|t| {
            i32::try_from(*source).map_or(false, |src_i32| {
                t.receives.iter().any(|r| r.source_track_index == src_i32)
            })
        })
    }

    fn add_send(&mut self, source: &usize, dest: &usize) -> Result<(), Never> {
        // REAPER stores a send as an AUXRECV on the *destination* track, which
        // names its source by index — there is no send record on the source.
        let receive = ReceiveSettings {
            source_track_index: i32::try_from(*source).unwrap_or(-1),
            mode: 0, // post-fader
            volume: 1.0,
            pan: 0.0,
            mute: false,
            mono_sum: false,
            invert_polarity: false,
            source_audio_channels: 0,
            dest_audio_channels: 0,
            pan_law: -1.0,
            midi_channels: -1, // no MIDI
            automation_mode: -1,
        };
        if let Some(track) = self.track(*dest) {
            track.receives.push(receive);
        }
        Ok(())
    }

    fn set_parent_send(&mut self, id: &usize, enabled: bool) -> Result<(), Never> {
        if let Some(track) = self.track(*id) {
            track.master_send = Some(MasterSendSettings {
                enabled,
                unknown_field_2: 0,
            });
        }
        Ok(())
    }

    fn folder_depths(&self) -> Vec<(usize, String, i32)> {
        self.project
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    i,
                    t.name.clone(),
                    t.folder.as_ref().map_or(0, |f| f.indentation),
                )
            })
            .collect()
    }

    fn gather_into_folder(
        &mut self,
        folder: &str,
        tracks: &[usize],
    ) -> Result<Option<crate::apply::Gathered<usize>>, Never> {
        // Only top-level tracks that carry no folder structure can be pulled
        // out. Three ways a track fails that, each of which breaks the project
        // differently:
        //
        // - a folder parent — moving it orphans its children;
        // - the track that closes a folder — moving it leaves that folder open
        //   over everything after it;
        // - anything nested inside a folder — it belongs to that folder, and
        //   yanking it out silently changes what the folder sums.
        let depths = self.running_depths();
        let mut movable: Vec<usize> = tracks
            .iter()
            .copied()
            .filter(|i| {
                depths.get(*i) == Some(&0)
                    && self.project.tracks.get(*i).is_some_and(|t| {
                        t.folder.as_ref().is_none_or(|f| {
                            f.folder_state == FolderState::Regular && f.indentation == 0
                        })
                    })
            })
            .collect();
        movable.sort_unstable();
        movable.dedup();
        if movable.is_empty() {
            return Ok(None);
        }

        let moving: std::collections::HashSet<usize> = movable.iter().copied().collect();
        let total = self.project.tracks.len();

        // New order: everything staying, then the folder, then the moved
        // tracks in their original relative order.
        let staying: Vec<usize> = (0..total).filter(|i| !moving.contains(i)).collect();

        // old index → new index, for the AUXRECV rewrite below. The folder
        // track is inserted between the two runs, so the moved tracks shift by
        // one extra place.
        let mut remap = vec![usize::MAX; total];
        for (new, old) in staying.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = new;
            }
        }
        let folder_index = staying.len();
        for (offset, old) in movable.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = folder_index.saturating_add(1).saturating_add(offset);
            }
        }

        let mut reordered: Vec<Track> = Vec::with_capacity(total.saturating_add(1));
        let mut taken: Vec<Option<Track>> = std::mem::take(&mut self.project.tracks)
            .into_iter()
            .map(Some)
            .collect();
        for old in &staying {
            reordered.push(
                taken
                    .get_mut(*old)
                    .and_then(Option::take)
                    .unwrap_or_default(),
            );
        }
        reordered.push(Track {
            name: folder.to_string(),
            folder: Some(FolderSettings {
                folder_state: FolderState::FolderParent,
                indentation: 1,
            }),
            ..Track::default()
        });
        for old in &movable {
            let mut track = taken
                .get_mut(*old)
                .and_then(Option::take)
                .unwrap_or_default();
            // These carried no folder structure (checked above); inside the
            // folder they are plain members.
            track.folder = Some(FolderSettings {
                folder_state: FolderState::Regular,
                indentation: 0,
            });
            reordered.push(track);
        }
        // The last member closes the folder we just opened.
        if let Some(last) = reordered.last_mut() {
            last.folder = Some(FolderSettings {
                folder_state: FolderState::LastInFolder,
                indentation: -1,
            });
        }

        // A receive names its source by index, so every one of them has to
        // follow the tracks that moved.
        for track in &mut reordered {
            for receive in &mut track.receives {
                let old = receive.source_track_index;
                if old >= 0 {
                    if let Ok(old_idx) = usize::try_from(old) {
                        if let Some(&mapped) = remap.get(old_idx) {
                            if let Ok(new_idx) = i32::try_from(mapped) {
                                receive.source_track_index = new_idx;
                            }
                        }
                    }
                }
            }
        }

        self.project.tracks = reordered;
        Ok(Some(crate::apply::Gathered {
            folder: folder_index,
            moved: movable
                .iter()
                .map(|old| remap.get(*old).copied().unwrap_or(usize::MAX))
                .collect(),
            skipped: tracks
                .iter()
                .copied()
                .filter(|i| !movable.contains(i))
                .map(|old| remap.get(old).copied().unwrap_or(usize::MAX))
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{
        apply_buses, apply_colors, apply_routing, contextual_paths, gather_unsorted,
        normalize_folder_depths, reclassify_stem_splits, route_to_bus, UNSORTED_FOLDER,
    };
    use crate::buses::{all_buses, names};

    fn applied() -> ReaperProject {
        let mut project = ReaperProject::default();
        let mut target = RppTarget::new(&mut project);
        apply_buses(&mut target, &all_buses()).unwrap();
        project
    }

    #[test]
    fn buses_land_as_nested_folder_tracks() {
        let project = applied();
        let names_in_order: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names_in_order[0], names::MIX);
        assert_eq!(names_in_order[1], names::INST);
        assert_eq!(*names_in_order.last().unwrap(), names::UTILITY);

        let mix = &project.tracks[0];
        assert_eq!(
            mix.folder.as_ref().unwrap().folder_state,
            FolderState::FolderParent
        );

        // BGV BUS closes both VOX BUS and MIX BUS.
        let bgv = project
            .tracks
            .iter()
            .find(|t| t.name == names::BGV)
            .unwrap();
        let folder = bgv.folder.as_ref().unwrap();
        assert_eq!(folder.folder_state, FolderState::LastInFolder);
        assert_eq!(folder.indentation, -2);
    }

    #[test]
    fn folder_depths_balance() {
        let project = applied();
        let total: i32 = project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .sum();
        assert_eq!(
            total, 0,
            "unbalanced folders would swallow every later track"
        );
    }

    #[test]
    fn re_applying_creates_nothing_new() {
        let mut project = ReaperProject::default();
        let mut target = RppTarget::new(&mut project);
        let first = apply_buses(&mut target, &all_buses()).unwrap();
        let count = project.tracks.len();

        let mut target = RppTarget::new(&mut project);
        let second = apply_buses(&mut target, &all_buses()).unwrap();

        assert_eq!(project.tracks.len(), count);
        assert!(!first.created.is_empty());
        assert!(second.created.is_empty());
        assert_eq!(second.reused.len(), count);
    }

    #[test]
    fn routing_a_track_sends_it_and_drops_the_parent_send() {
        let mut project = ReaperProject::default();
        project.tracks.push(Track {
            name: "Drums".to_string(),
            ..Track::default()
        });
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();

        let drums = target.find_track("Drums").unwrap();
        let drum_bus = *buses.get(names::DRUM).unwrap();
        route_to_bus(&mut target, &drums, &drum_bus).unwrap();

        // The send lives on the destination as an AUXRECV naming the source.
        let bus_track = &project.tracks[drum_bus];
        assert_eq!(bus_track.receives.len(), 1);
        assert_eq!(
            bus_track.receives[0].source_track_index,
            i32::try_from(drums).unwrap()
        );
        assert_eq!(bus_track.receives[0].mode, 0, "post-fader");

        // ...and the source no longer feeds the master directly.
        assert!(!project.tracks[drums].master_send.as_ref().unwrap().enabled);
    }

    /// Applying to a project that already has some buses must leave the
    /// folder depths summing to zero. An unbalanced project makes REAPER
    /// swallow every track after the last bus into a folder that never closes.
    #[test]
    fn adopting_an_existing_project_leaves_folders_balanced() {
        let mut project = ReaperProject::default();
        for name in ["MIX BUS", "INST BUS", "DRUM BUS", "Some Song Track"] {
            project.tracks.push(Track {
                name: name.to_string(),
                ..Track::default()
            });
        }
        let mut target = RppTarget::new(&mut project);
        let applied = apply_buses(&mut target, &all_buses()).unwrap();
        assert!(!applied.nested, "existing buses force the flat placement");
        assert!(!applied.created.is_empty());

        let total: i32 = project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .sum();
        assert_eq!(total, 0);
    }

    /// The flat placement still routes: a bus that could not be nested reaches
    /// its parent by an explicit send instead.
    #[test]
    fn flat_placement_wires_sends_to_the_parent_bus() {
        let mut project = ReaperProject::default();
        project.tracks.push(Track {
            name: "MIX BUS".to_string(),
            ..Track::default()
        });
        let mut target = RppTarget::new(&mut project);
        let applied = apply_buses(&mut target, &all_buses()).unwrap();
        assert!(!applied.nested);

        let drum = *applied.get(names::DRUM).unwrap();
        let inst = *applied.get(names::INST).unwrap();
        // DRUM BUS feeds INST BUS by send, and no longer feeds the master.
        assert!(project.tracks[inst]
            .receives
            .iter()
            .any(|r| r.source_track_index == i32::try_from(drum).unwrap()));
        assert!(!project.tracks[drum].master_send.as_ref().unwrap().enabled);
    }

    #[test]
    fn a_fresh_project_gets_the_nested_tree() {
        let mut project = ReaperProject::default();
        let mut target = RppTarget::new(&mut project);
        let applied = apply_buses(&mut target, &all_buses()).unwrap();
        assert!(applied.nested);
        assert_eq!(
            project.tracks[0].folder.as_ref().unwrap().folder_state,
            FolderState::FolderParent
        );
    }

    #[test]
    fn an_existing_bus_under_an_old_name_is_reused() {
        // Real sessions call these GUITAR BUS / CLICK + GUIDE BUS / Keys Bus.
        // Applying the template must adopt them, not create rivals.
        let mut project = ReaperProject::default();
        for name in ["GTR BUS", "GUIDE BUS", "Keys Bus", "Guitar E BUS"] {
            project.tracks.push(Track {
                name: name.to_string(),
                ..Track::default()
            });
        }
        let mut target = RppTarget::new(&mut project);
        let applied = apply_buses(&mut target, &all_buses()).unwrap();

        for bus in [names::GUITAR, names::GUIDE, names::KEYS, names::ELECTRIC] {
            assert!(
                applied.reused.contains(&bus.to_string()),
                "{bus} not reused"
            );
            assert!(
                !applied.created.contains(&bus.to_string()),
                "{bus} duplicated"
            );
        }
        // The originals keep their names — adoption never renames a track.
        let track_names: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
        assert!(track_names.contains(&"GTR BUS"));
        assert!(track_names.contains(&"Guitar E BUS"));
        assert!(!track_names.contains(&"GUITAR BUS"));
        assert!(!track_names.contains(&"ELECTRIC BUS"));
    }

    fn named(names: &[&str]) -> ReaperProject {
        let mut project = ReaperProject::default();
        for n in names {
            project.tracks.push(Track {
                name: (*n).to_string(),
                ..Track::default()
            });
        }
        project
    }

    #[test]
    fn unsorted_tracks_are_gathered_into_a_closed_folder() {
        let mut project = named(&["Kick", "Mystery A", "Snare", "Mystery B"]);
        let mut target = RppTarget::new(&mut project);
        let g = gather_unsorted(&mut target, &[1, 3]).unwrap().unwrap();

        let order: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            order,
            vec!["Kick", "Snare", UNSORTED_FOLDER, "Mystery A", "Mystery B"]
        );
        assert_eq!(g.folder, 2);
        assert_eq!(g.moved, vec![3, 4]);
        assert!(g.skipped.is_empty());

        assert_eq!(
            project.tracks[2].folder.as_ref().unwrap().folder_state,
            FolderState::FolderParent
        );
        // The last member closes the folder, so the project stays balanced.
        assert_eq!(project.tracks[4].folder.as_ref().unwrap().indentation, -1);
        let total: i32 = project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .sum();
        assert_eq!(total, 0);
    }

    /// The move renumbers every track, and a receive names its source by
    /// index — so an un-remapped send silently repoints at a different track.
    #[test]
    fn gathering_repoints_existing_sends() {
        let mut project = named(&["Kick", "Mystery", "Snare", "DRUM BUS"]);
        // DRUM BUS (3) receives from Kick (0) and Snare (2).
        for source in [0, 2] {
            project.tracks[3].receives.push(ReceiveSettings {
                source_track_index: source,
                mode: 0,
                volume: 1.0,
                pan: 0.0,
                mute: false,
                mono_sum: false,
                invert_polarity: false,
                source_audio_channels: 0,
                dest_audio_channels: 0,
                pan_law: -1.0,
                midi_channels: -1,
                automation_mode: -1,
            });
        }

        let mut target = RppTarget::new(&mut project);
        gather_unsorted(&mut target, &[1]).unwrap().unwrap();

        // Kick 0→0, Snare 2→1, DRUM BUS 3→2, then UNSORTED, then Mystery.
        let order: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            order,
            vec!["Kick", "Snare", "DRUM BUS", UNSORTED_FOLDER, "Mystery"]
        );
        let sources: Vec<i32> = project.tracks[2]
            .receives
            .iter()
            .map(|r| r.source_track_index)
            .collect();
        assert_eq!(
            sources,
            vec![0, 1],
            "sends must follow the tracks they name"
        );
    }

    /// Pulling a folder parent out would orphan its children; pulling the
    /// closing track would leave its folder open over the rest of the project.
    #[test]
    fn structural_tracks_are_never_gathered() {
        let mut project = named(&["Guitars", "GTR 1", "GTR 2", "Mystery"]);
        project.tracks[0].folder = Some(FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 1,
        });
        project.tracks[2].folder = Some(FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: -1,
        });

        let mut target = RppTarget::new(&mut project);
        // Ask for all four; only the top-level, structureless "Mystery" moves.
        let g = gather_unsorted(&mut target, &[0, 1, 2, 3])
            .unwrap()
            .unwrap();
        assert_eq!(g.moved.len(), 1);
        assert_eq!(g.skipped.len(), 3, "the folder and its two children stay");

        let order: Vec<&str> = project.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(
            order,
            vec!["Guitars", "GTR 1", "GTR 2", UNSORTED_FOLDER, "Mystery"]
        );
        let total: i32 = project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn a_folder_closed_that_was_never_opened_is_reported() {
        let mut project = named(&["Kick", "Snare", "Tom"]);
        // "Snare" closes a folder nothing opened — the shape real sessions
        // arrive in, which REAPER clamps away rather than complaining about.
        project.tracks[1].folder = Some(FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: -1,
        });
        let target = RppTarget::new(&mut project);
        let bad = target.negative_depths();
        assert_eq!(bad.len(), 2, "Snare and everything after it");
        assert_eq!(bad[0].1, "Snare");
        assert_eq!(bad[0].2, -1);
    }

    #[test]
    fn a_well_formed_project_reports_nothing() {
        let mut project = named(&["Guitars", "GTR 1"]);
        project.tracks[0].folder = Some(FolderSettings {
            folder_state: FolderState::FolderParent,
            indentation: 1,
        });
        project.tracks[1].folder = Some(FolderSettings {
            folder_state: FolderState::LastInFolder,
            indentation: -1,
        });
        let target = RppTarget::new(&mut project);
        assert!(target.negative_depths().is_empty());
    }

    fn with_depths(rows: &[(&str, i32)]) -> ReaperProject {
        let mut project = ReaperProject::default();
        for (name, ind) in rows {
            project.tracks.push(Track {
                name: (*name).to_string(),
                folder: Some(FolderSettings {
                    folder_state: match ind {
                        i if *i > 0 => FolderState::FolderParent,
                        i if *i < 0 => FolderState::LastInFolder,
                        _ => FolderState::Regular,
                    },
                    indentation: *ind,
                }),
                ..Track::default()
            });
        }
        project
    }

    fn depths_of(project: &ReaperProject) -> Vec<i32> {
        project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .collect()
    }

    /// The shape the real album projects arrive in: a track closes a folder
    /// that was never opened, four tracks in, and the depth never recovers.
    #[test]
    fn a_close_with_nothing_open_becomes_an_ordinary_track() {
        let mut project = with_depths(&[("VCA", 0), ("Unused", 0), ("Bass DI", -1), ("Kick", 0)]);
        let mut target = RppTarget::new(&mut project);
        let fixes = normalize_folder_depths(&mut target).unwrap();

        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].name, "Bass DI");
        assert_eq!((fixes[0].from, fixes[0].to), (-1, 0));
        assert_eq!(depths_of(&project), vec![0, 0, 0, 0]);
        assert!(RppTarget::new(&mut project).negative_depths().is_empty());
    }

    /// `Amp 2  ISBUS 2 -3` with only two folders open — close two, not three.
    #[test]
    fn an_over_close_is_clamped_to_what_is_open() {
        let mut project = with_depths(&[("GTR E", 1), ("Chords", 1), ("Amp 1", 0), ("Amp 2", -3)]);
        let mut target = RppTarget::new(&mut project);
        let fixes = normalize_folder_depths(&mut target).unwrap();

        assert_eq!(fixes.len(), 1);
        assert_eq!((fixes[0].from, fixes[0].to), (-3, -2));
        assert_eq!(depths_of(&project), vec![1, 1, 0, -2]);
        assert_eq!(depths_of(&project).iter().sum::<i32>(), 0);
    }

    #[test]
    fn folders_left_open_are_closed_on_the_last_track() {
        let mut project = with_depths(&[("Drums", 1), ("Kick", 1), ("Kick In", 0)]);
        let mut target = RppTarget::new(&mut project);
        let fixes = normalize_folder_depths(&mut target).unwrap();

        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0].name, "Kick In");
        assert_eq!((fixes[0].from, fixes[0].to), (0, -2));
        assert_eq!(depths_of(&project).iter().sum::<i32>(), 0);
    }

    #[test]
    fn folder_state_is_rewritten_to_match_the_depth() {
        // "Bass DI" was LastInFolder; with nothing to close it is now Regular.
        let mut project = with_depths(&[("Kick", 0), ("Bass DI", -1)]);
        let mut target = RppTarget::new(&mut project);
        normalize_folder_depths(&mut target).unwrap();
        assert_eq!(
            project.tracks[1].folder.as_ref().unwrap().folder_state,
            FolderState::Regular
        );
    }

    #[test]
    fn a_well_formed_project_is_left_alone() {
        let mut project = with_depths(&[("Drums", 1), ("Kick", 0), ("Snare", -1), ("Bass", 0)]);
        let before = depths_of(&project);
        let mut target = RppTarget::new(&mut project);
        assert!(normalize_folder_depths(&mut target).unwrap().is_empty());
        assert_eq!(depths_of(&project), before);
    }

    /// Running it twice must change nothing the second time, or the album can
    /// never converge on a valid structure.
    #[test]
    fn normalizing_is_idempotent() {
        let mut project = with_depths(&[
            ("VCA", 0),
            ("Bass DI", -1),
            ("GTR E", 1),
            ("Amp 2", -3),
            ("Keys", 1),
        ]);
        let mut target = RppTarget::new(&mut project);
        assert!(!normalize_folder_depths(&mut target).unwrap().is_empty());
        let after_first = depths_of(&project);

        let mut target = RppTarget::new(&mut project);
        assert!(
            normalize_folder_depths(&mut target).unwrap().is_empty(),
            "second pass should find nothing"
        );
        assert_eq!(depths_of(&project), after_first);
        assert_eq!(after_first.iter().sum::<i32>(), 0);
        assert!(RppTarget::new(&mut project).negative_depths().is_empty());
    }

    #[test]
    fn colouring_paints_by_classification() {
        let mut project = named(&["Kick In", "Bass DI", "GTR E - Chords", "Mystery"]);
        let mut target = RppTarget::new(&mut project);
        let painted = apply_colors(&mut target).unwrap();

        assert_eq!(painted, 3, "the unclassifiable track is left alone");
        let kick = project.tracks[0].peak_color.unwrap();
        let bass = project.tracks[1].peak_color.unwrap();
        assert_ne!(kick, bass, "different groups get different colours");
        assert!(project.tracks[3].peak_color.is_none(), "Mystery untouched");

        // The colour is the one the taxonomy assigns, not an arbitrary pick:
        // a drum's is its shade of the drums' red (`drum_color`).
        let expected = super::super::drum_color(&["Drums", "Drum Kit", "Kick"])
            .unwrap()
            .to_reaper_native();
        assert_eq!(kick, expected);
    }

    /// The house style leaves a track bare when its parent already says what
    /// it is: a kick mic inside a `Kick` folder is just `In`. Alone that
    /// classifies to nothing; with its folder it is a kick.
    #[test]
    fn bare_names_classify_against_their_parent_folder() {
        let mut project = with_depths(&[("Drums", 1), ("Kick", 1), ("In", 0), ("Out", -2)]);
        let target = RppTarget::new(&mut project);
        let entries = contextual_paths(&target);

        let inside = entries.iter().find(|e| e.name == "In").unwrap();
        assert_eq!(inside.context, vec!["Drums", "Kick"]);
        assert!(
            inside.path.iter().any(|g| g == "Kick"),
            "\"In\" under Drums/Kick should classify as a kick, got {:?}",
            inside.path
        );

        // Alone it means nothing — which is why the context is needed.
        assert!(crate::track_schema::classify_track("In")
            .matched_groups
            .is_empty());
    }

    #[test]
    fn context_pops_when_a_track_closes_its_folders() {
        // "Out" closes both Kick and Drums, so "Bass DI" that follows is at the
        // top level with no inherited context.
        let mut project = with_depths(&[
            ("Drums", 1),
            ("Kick", 1),
            ("In", 0),
            ("Out", -2),
            ("Bass DI", 0),
        ]);
        let target = RppTarget::new(&mut project);
        let entries = contextual_paths(&target);
        let bass = entries.iter().find(|e| e.name == "Bass DI").unwrap();
        assert!(bass.context.is_empty(), "got {:?}", bass.context);
        assert!(bass.path.iter().any(|g| g == "Bass"));
    }

    #[test]
    fn a_folder_is_not_context_for_itself() {
        let mut project = with_depths(&[("Drums", 1), ("Kick", -1)]);
        let target = RppTarget::new(&mut project);
        let entries = contextual_paths(&target);
        assert!(entries[0].context.is_empty(), "Drums is not inside Drums");
        assert_eq!(entries[1].context, vec!["Drums"]);
    }

    #[test]
    fn colouring_uses_the_contextual_classification() {
        let mut project = with_depths(&[("Drums", 1), ("Kick", 1), ("In", -2)]);
        let mut target = RppTarget::new(&mut project);
        apply_colors(&mut target).unwrap();
        // "In" is painted the kick colour it inherits from its folder path,
        // not left grey for failing to classify alone.
        assert!(project.tracks[2].peak_color.is_some());
    }

    /// A demucs separation classifies as content one name at a time — the set
    /// is the only thing that gives it away. Left as content it sums into the
    /// mix beside the real tracks and doubles everything.
    #[test]
    fn a_cohesive_stem_set_is_reclassified_as_reference() {
        let mut project = named(&["Song_drums", "Song_bass", "Song_vocals", "Song_other"]);
        let target = RppTarget::new(&mut project);
        let entries = reclassify_stem_splits(contextual_paths(&target));

        for e in &entries {
            assert_eq!(
                e.path,
                vec!["Reference", "Stem Split"],
                "{} should be a stem split",
                e.name
            );
            assert_eq!(
                crate::bus_for_path(&e.path),
                Some(crate::buses::names::UTILITY)
            );
        }
    }

    /// Two stems is not a set — a live-tracked "Drums" and "Bass" must stay
    /// drums and bass.
    #[test]
    fn too_few_stems_stay_content() {
        let mut project = named(&["Drums", "Bass"]);
        let target = RppTarget::new(&mut project);
        let entries = reclassify_stem_splits(contextual_paths(&target));
        assert!(entries[0].path.iter().any(|g| g == "Drums"));
        assert!(entries[1].path.iter().any(|g| g == "Bass"));
    }

    /// A stem folder must not drag in a separately-tracked instrument that
    /// happens to sit elsewhere in the project.
    #[test]
    fn stem_detection_is_scoped_to_the_folder() {
        let mut project = with_depths(&[
            ("Stems", 1),
            ("Song_drums", 0),
            ("Song_bass", 0),
            ("Song_vocals", -1),
            ("Piano", 0),
        ]);
        let target = RppTarget::new(&mut project);
        let entries = reclassify_stem_splits(contextual_paths(&target));

        let piano = entries.iter().find(|e| e.name == "Piano").unwrap();
        assert!(
            piano.path.iter().any(|g| g == "Keys"),
            "the separately-tracked piano stays a piano, got {:?}",
            piano.path
        );
        let stem = entries.iter().find(|e| e.name == "Song_drums").unwrap();
        assert_eq!(stem.path, vec!["Reference", "Stem Split"]);
    }

    /// The doubling this guards against: if a folder routes to a bus and the
    /// tracks inside it route there too, everything arrives twice and the
    /// folder fader stops controlling anything.
    #[test]
    fn only_the_outermost_classified_track_carries_the_send() {
        let mut project = with_depths(&[("Guitars", 1), ("Electric", 1), ("Amp 1", -2)]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        let report = apply_routing(&mut target, &buses).unwrap();

        assert_eq!(report.routed, vec!["Guitars"]);
        assert_eq!(report.covered, vec!["Electric", "Amp 1"]);

        let gtr_bus = *buses.get(names::GUITAR).unwrap();
        let sources: Vec<i32> = project.tracks[gtr_bus]
            .receives
            .iter()
            .map(|r| r.source_track_index)
            .collect();
        assert_eq!(sources, vec![0], "only the Guitars folder feeds the bus");

        // The children keep feeding their folder, as they always did.
        for i in [1, 2] {
            assert!(project.tracks[i]
                .master_send
                .as_ref()
                .is_none_or(|m| m.enabled));
        }
    }

    #[test]
    fn a_top_level_track_routes_directly() {
        let mut project = named(&["Kick In", "Bass DI"]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        let report = apply_routing(&mut target, &buses).unwrap();

        assert_eq!(report.routed, vec!["Kick In", "Bass DI"]);
        let drum = *buses.get(names::DRUM).unwrap();
        let bass = *buses.get(names::BASS).unwrap();
        assert_eq!(project.tracks[drum].receives.len(), 1);
        assert_eq!(project.tracks[bass].receives.len(), 1);
        // Routed tracks feed their bus instead of the master.
        assert!(!project.tracks[0].master_send.as_ref().unwrap().enabled);
    }

    /// Sibling folders each carry their own send; closing one must not leave
    /// the next one looking "covered".
    #[test]
    fn coverage_is_released_when_a_folder_closes() {
        let mut project = with_depths(&[("Guitars", 1), ("Amp 1", -1), ("Drums", 1), ("Kick", -1)]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        let report = apply_routing(&mut target, &buses).unwrap();

        assert_eq!(report.routed, vec!["Guitars", "Drums"]);
        assert_eq!(report.covered, vec!["Amp 1", "Kick"]);
    }

    /// The doubling a real session produces: `GTR A` already feeds the
    /// acoustic bus, but classifies as electric. Routing it again puts it on
    /// both, 6 dB hot, with neither fader in charge.
    #[test]
    fn a_track_already_feeding_a_bus_is_left_alone() {
        let mut project = named(&["GTR A"]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();

        // The engineer routed it to the acoustic bus by hand.
        let gtr = target.find_track("GTR A").unwrap();
        let acoustic = *buses.get(names::ACOUSTIC).unwrap();
        route_to_bus(&mut target, &gtr, &acoustic).unwrap();

        let report = apply_routing(&mut target, &buses).unwrap();
        assert_eq!(report.already_routed, vec!["GTR A"]);
        assert!(report.routed.is_empty());

        // Exactly one destination, still the one the engineer chose.
        let fed: Vec<usize> = project
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.receives
                    .iter()
                    .any(|r| r.source_track_index == i32::try_from(gtr).unwrap())
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(fed, vec![acoustic]);
    }

    #[test]
    fn buses_are_never_routed_into_themselves() {
        let mut project = named(&["Kick In"]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        let report = apply_routing(&mut target, &buses).unwrap();

        assert!(!report.routed.iter().any(|n| n.ends_with("BUS")));
        let drum = *buses.get(names::DRUM).unwrap();
        assert!(project.tracks[drum]
            .receives
            .iter()
            .all(|r| r.source_track_index != i32::try_from(drum).unwrap()));
    }

    #[test]
    fn routing_twice_adds_nothing() {
        let mut project = named(&["Kick In", "Snare Top", "Bass DI"]);
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        apply_routing(&mut target, &buses).unwrap();
        let counts: Vec<usize> = project.tracks.iter().map(|t| t.receives.len()).collect();

        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        apply_routing(&mut target, &buses).unwrap();
        let again: Vec<usize> = project.tracks.iter().map(|t| t.receives.len()).collect();
        assert_eq!(counts, again);
    }

    #[test]
    fn colouring_leaves_bus_tracks_to_the_bus_spec() {
        let mut project = named(&["DRUM BUS", "VOX BUS", "Guitar E BUS"]);
        let mut target = RppTarget::new(&mut project);
        assert_eq!(apply_colors(&mut target).unwrap(), 0);
        assert!(project.tracks.iter().all(|t| t.peak_color.is_none()));
    }

    #[test]
    fn colouring_is_idempotent() {
        let mut project = named(&["Kick In", "Snare Top"]);
        let mut target = RppTarget::new(&mut project);
        apply_colors(&mut target).unwrap();
        let first: Vec<_> = project.tracks.iter().map(|t| t.peak_color).collect();

        let mut target = RppTarget::new(&mut project);
        apply_colors(&mut target).unwrap();
        let second: Vec<_> = project.tracks.iter().map(|t| t.peak_color).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn nothing_to_gather_creates_no_folder() {
        let mut project = named(&["Kick"]);
        let mut target = RppTarget::new(&mut project);
        assert!(gather_unsorted(&mut target, &[]).unwrap().is_none());
        assert_eq!(project.tracks.len(), 1, "no empty UNSORTED folder");
    }

    /// The exact real-world shape: a channel folder containing a plain
    /// "Main" capture immediately followed by "DI". Electric guitar opts
    /// into nesting, so DI ends up muted, folder-collapsed, inside Main.
    #[test]
    fn di_nests_under_its_sibling_main_for_an_opted_in_group() {
        let mut project = with_depths(&[
            ("Electric GTR", 1),
            ("Clean", 1),
            ("L", 1),
            ("Main", 0),
            ("DI", -3),
        ]);
        let mut target = RppTarget::new(&mut project);
        target.nest_secondary_mics();

        // Main is now the folder parent...
        let main = project.tracks.iter().find(|t| t.name == "Main").unwrap();
        assert_eq!(
            main.folder.as_ref().unwrap().folder_state,
            FolderState::FolderParent
        );
        assert_eq!(main.bus_compact.as_ref().unwrap().arrange_collapse, 2);

        // ...DI is its only child, muted, closing one more level than before.
        let di = project.tracks.iter().find(|t| t.name == "DI").unwrap();
        assert_eq!(
            di.folder.as_ref().unwrap().folder_state,
            FolderState::LastInFolder
        );
        assert_eq!(di.folder.as_ref().unwrap().indentation, -4);
        assert!(di.mutesolo.as_ref().unwrap().mute);

        // The project stays balanced.
        let total: i32 = project
            .tracks
            .iter()
            .map(|t| t.folder.as_ref().map_or(0, |f| f.indentation))
            .sum();
        assert_eq!(total, 0);
    }

    /// A second pass must not double-nest — DI is no longer Main's sibling
    /// once it is Main's child.
    #[test]
    fn nesting_di_is_idempotent() {
        let mut project = with_depths(&[
            ("Electric GTR", 1),
            ("Clean", 1),
            ("L", 1),
            ("Main", 0),
            ("DI", -3),
        ]);
        let mut target = RppTarget::new(&mut project);
        target.nest_secondary_mics();
        let after_first: Vec<_> = project
            .tracks
            .iter()
            .map(|t| (t.folder.clone(), t.mutesolo.clone()))
            .collect();

        let mut target = RppTarget::new(&mut project);
        target.nest_secondary_mics();
        let after_second: Vec<_> = project
            .tracks
            .iter()
            .map(|t| (t.folder.clone(), t.mutesolo.clone()))
            .collect();

        assert_eq!(after_first, after_second);
    }

    /// Bass DI is not opted in — the classic pair sits exactly as before.
    #[test]
    fn a_group_that_does_not_opt_in_is_left_alone() {
        let mut project = with_depths(&[("Bass", 1), ("Main", 0), ("DI", -1)]);
        let mut target = RppTarget::new(&mut project);
        target.nest_secondary_mics();

        let main = project.tracks.iter().find(|t| t.name == "Main").unwrap();
        assert!(main.folder.as_ref().is_none_or(|f| f.indentation == 0));
        let di = project.tracks.iter().find(|t| t.name == "DI").unwrap();
        assert!(di.mutesolo.is_none());
    }

    #[test]
    fn routing_twice_does_not_double_the_send() {
        let mut project = ReaperProject::default();
        project.tracks.push(Track {
            name: "Drums".to_string(),
            ..Track::default()
        });
        let mut target = RppTarget::new(&mut project);
        let buses = apply_buses(&mut target, &all_buses()).unwrap();
        let drums = target.find_track("Drums").unwrap();
        let drum_bus = *buses.get(names::DRUM).unwrap();

        route_to_bus(&mut target, &drums, &drum_bus).unwrap();
        route_to_bus(&mut target, &drums, &drum_bus).unwrap();

        assert_eq!(project.tracks[drum_bus].receives.len(), 1);
    }
}
