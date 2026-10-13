//! Track navigation: which track sits on which channel strip.
//!
//! CSI's model (TrackNavigationManager): the surface never binds
//! widgets to tracks — every refresh asks the navigator "what track
//! is strip N?" and the answer depends on the current mode + bank
//! offset + folder drill-down stack.
//!
//! - **Track mode** — the flat track list, windowed by `bank_offset`.
//! - **Folder mode** — at the root, only top-level tracks; selecting
//!   a folder *spills* it (CSI semantics): the strip list becomes
//!   `[folder, child0, child1, …]`, drillable recursively. Selecting
//!   the spilled folder again (strip 0) pops back up one level.

use daw_proto::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    Track,
    Folder,
    /// VCA mode: root shows VCA leads; spilling a lead shows
    /// `[lead, followers…]` (CSI's `TrackToggleVCASpill`).
    Vca,
}

#[derive(Debug)]
pub struct Navigator {
    pub mode: NavMode,
    /// Bank offset into the flat list (Track mode).
    bank_offset: usize,
    /// Bank offset within the current folder level (Folder mode).
    folder_offset: usize,
    /// Drill-down stack of folder guids; last = current parent.
    /// Empty = root level.
    folder_stack: Vec<String>,
}

impl Default for Navigator {
    fn default() -> Self {
        Self {
            mode: NavMode::Track,
            bank_offset: 0,
            folder_offset: 0,
            folder_stack: Vec::new(),
        }
    }
}

impl Navigator {
    /// The ordered list of candidate tracks for the current mode,
    /// before windowing. Indices into `tracks`.
    fn candidates(&self, tracks: &[Track]) -> Vec<usize> {
        match self.mode {
            NavMode::Track => (0..tracks.len()).collect(),
            NavMode::Folder => match self.folder_stack.last() {
                // Root: only TOP-LEVEL FOLDER tracks — CSI's
                // folderTopParentTracks_ (loose tracks don't appear
                // in folder mode; that's what Track mode is for).
                None => tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.parent_guid.is_none() && t.is_folder)
                    .map(|(i, _)| i)
                    .collect(),
                Some(parent) => {
                    // CSI spill: the folder itself leads its children.
                    let mut v: Vec<usize> = tracks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| &t.guid == parent)
                        .map(|(i, _)| i)
                        .collect();
                    v.extend(
                        tracks
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t.parent_guid.as_deref() == Some(parent.as_str()))
                            .map(|(i, _)| i),
                    );
                    v
                }
            },
            NavMode::Vca => match self.folder_stack.last() {
                // Root: every VCA lead.
                None => tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.grouping.vca_lead != 0)
                    .map(|(i, _)| i)
                    .collect(),
                // Spill: the lead, then every shared-group follower.
                Some(lead_guid) => {
                    let lead_mask = tracks
                        .iter()
                        .find(|t| &t.guid == lead_guid)
                        .map(|t| t.grouping.vca_lead)
                        .unwrap_or(0);
                    let mut v: Vec<usize> = tracks
                        .iter()
                        .enumerate()
                        .filter(|(_, t)| &t.guid == lead_guid)
                        .map(|(i, _)| i)
                        .collect();
                    v.extend(
                        tracks
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t.grouping.vca_follow & lead_mask != 0)
                            .map(|(i, _)| i),
                    );
                    v
                }
            },
        }
    }

    /// Resolve the `n` visible strips to track indices (`None` =
    /// empty strip past the end of the list).
    pub fn visible(&self, tracks: &[Track], n: usize) -> Vec<Option<usize>> {
        let cands = self.candidates(tracks);
        let offset = match self.mode {
            NavMode::Track => self.bank_offset,
            NavMode::Folder | NavMode::Vca => self.folder_offset,
        };
        (0..n).map(|i| cands.get(offset + i).copied()).collect()
    }

    /// Bank by `delta` strips, clamped so the window never scrolls
    /// past the end (CSI behavior: last page stays full).
    pub fn bank(&mut self, delta: isize, tracks: &[Track], strips: usize) {
        let len = self.candidates(tracks).len();
        let max = len.saturating_sub(strips);
        let off = match self.mode {
            NavMode::Track => &mut self.bank_offset,
            NavMode::Folder | NavMode::Vca => &mut self.folder_offset,
        };
        *off = (*off as isize + delta).clamp(0, max as isize) as usize;
    }

    /// Switch navigator mode (zones pin a mode on activation). A
    /// mode change resets the folder drill-down; re-setting the
    /// current mode is a no-op so zone hops within one mode keep
    /// their banking.
    pub fn set_mode(&mut self, mode: NavMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.folder_stack.clear();
        self.folder_offset = 0;
    }

    /// Handle a select gesture in folder mode. Returns `true` if it
    /// navigated (drilled in or popped out) — the caller then skips
    /// normal track selection. CSI's `ToggleFolderSpill`:
    /// - selecting a *folder* drills into it
    /// - selecting the *current spilled folder* (strip 0) pops up
    pub fn folder_select(&mut self, track: &Track) -> bool {
        if self.mode != NavMode::Folder {
            return false;
        }
        if self.folder_stack.last() == Some(&track.guid) {
            self.folder_stack.pop();
            self.folder_offset = 0;
            return true;
        }
        if track.is_folder {
            self.folder_stack.push(track.guid.clone());
            self.folder_offset = 0;
            return true;
        }
        false
    }

    /// VCA-mode spill toggle (CSI's `TrackToggleVCASpill`): spilling
    /// a lead shows `[lead, followers…]`; toggling the spilled lead
    /// pops back to the lead list. Returns `true` if it navigated.
    pub fn vca_select(&mut self, track: &Track) -> bool {
        if self.mode != NavMode::Vca {
            return false;
        }
        if self.folder_stack.last() == Some(&track.guid) {
            self.folder_stack.pop();
            self.folder_offset = 0;
            return true;
        }
        if track.grouping.vca_lead != 0 {
            // VCA spill is one level deep — replace, don't nest.
            self.folder_stack.clear();
            self.folder_stack.push(track.guid.clone());
            self.folder_offset = 0;
            return true;
        }
        false
    }

    /// Drop drill-down state that no longer resolves (tracks removed
    /// or re-parented). Call after the track list changes.
    pub fn revalidate(&mut self, tracks: &[Track]) {
        self.folder_stack
            .retain(|g| tracks.iter().any(|t| &t.guid == g));
        let len = self.candidates(tracks).len();
        self.bank_offset = self.bank_offset.min(len.saturating_sub(1));
        self.folder_offset = self.folder_offset.min(len.saturating_sub(1));
    }

    /// Depth of the folder drill-down (0 = root). For LCD breadcrumbs.
    pub fn depth(&self) -> usize {
        self.folder_stack.len()
    }

    /// Bank so the track at `track_idx` (index into `tracks`) lands
    /// on a strip — CSI's ScrollLink. No-op when already visible.
    pub fn scroll_to(&mut self, tracks: &[Track], track_idx: usize, strips: usize) {
        let cands = self.candidates(tracks);
        let Some(pos) = cands.iter().position(|&i| i == track_idx) else {
            return;
        };
        let off = match self.mode {
            NavMode::Track => &mut self.bank_offset,
            NavMode::Folder | NavMode::Vca => &mut self.folder_offset,
        };
        if pos < *off || pos >= *off + strips {
            *off = pos - (pos % strips);
        }
    }

    /// Guid of the folder currently spilled (drill-stack top).
    pub fn current_parent(&self) -> Option<&str> {
        self.folder_stack.last().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(guid: &str, parent: Option<&str>, is_folder: bool) -> Track {
        Track {
            guid: guid.into(),
            is_folder,
            parent_guid: parent.map(Into::into),
            ..Default::default()
        }
    }

    /// DRUMS(folder){Kick, Snare, OH(folder){OHL, OHR}}, BASS, VOX
    fn session() -> Vec<Track> {
        vec![
            track("drums", None, true),
            track("kick", Some("drums"), false),
            track("snare", Some("drums"), false),
            track("oh", Some("drums"), true),
            track("ohl", Some("oh"), false),
            track("ohr", Some("oh"), false),
            track("bass", None, false),
            track("vox", None, false),
        ]
    }

    #[test]
    fn track_mode_banks_flat() {
        let tracks = session();
        let mut nav = Navigator::default();
        let vis = nav.visible(&tracks, 4);
        assert_eq!(vis, vec![Some(0), Some(1), Some(2), Some(3)]);
        nav.bank(4, &tracks, 4);
        assert_eq!(
            nav.visible(&tracks, 4),
            vec![Some(4), Some(5), Some(6), Some(7)]
        );
        // Clamped at the end — last page stays full.
        nav.bank(4, &tracks, 4);
        assert_eq!(
            nav.visible(&tracks, 4),
            vec![Some(4), Some(5), Some(6), Some(7)]
        );
        nav.bank(-100, &tracks, 4);
        assert_eq!(nav.visible(&tracks, 4)[0], Some(0));
    }

    #[test]
    fn folder_root_shows_top_level_folders_only() {
        let tracks = session();
        let mut nav = Navigator::default();
        nav.set_mode(NavMode::Folder);
        // CSI semantics: the root shows TOP-LEVEL FOLDERS only —
        // drums. Loose tracks (bass, vox) live in Track mode.
        assert_eq!(nav.visible(&tracks, 4), vec![Some(0), None, None, None]);
    }

    #[test]
    fn folder_drill_and_pop() {
        let tracks = session();
        let mut nav = Navigator::default();
        nav.set_mode(NavMode::Folder);

        // Drill into DRUMS: spill = [drums, kick, snare, oh].
        assert!(nav.folder_select(&tracks[0]));
        assert_eq!(
            nav.visible(&tracks, 8)[..4],
            [Some(0), Some(1), Some(2), Some(3)]
        );

        // Drill into OH: spill = [oh, ohl, ohr].
        assert!(nav.folder_select(&tracks[3]));
        assert_eq!(
            nav.visible(&tracks, 4),
            vec![Some(3), Some(4), Some(5), None]
        );
        assert_eq!(nav.depth(), 2);

        // Selecting the spilled folder itself pops up one level.
        assert!(nav.folder_select(&tracks[3]));
        assert_eq!(nav.depth(), 1);
        assert_eq!(
            nav.visible(&tracks, 8)[..4],
            [Some(0), Some(1), Some(2), Some(3)]
        );

        // Selecting a plain track does NOT navigate.
        assert!(!nav.folder_select(&tracks[1]));
    }

    #[test]
    fn revalidate_drops_dead_folders() {
        let mut tracks = session();
        let mut nav = Navigator::default();
        nav.set_mode(NavMode::Folder);
        nav.folder_select(&tracks[0].clone());
        nav.folder_select(&tracks[3].clone());
        assert_eq!(nav.depth(), 2);
        // OH deleted → stack falls back to DRUMS.
        tracks.remove(3);
        nav.revalidate(&tracks);
        assert_eq!(nav.depth(), 1);
    }

    #[test]
    fn mode_toggle_resets_drill() {
        let tracks = session();
        let mut nav = Navigator::default();
        nav.set_mode(NavMode::Folder);
        nav.folder_select(&tracks[0].clone());
        nav.set_mode(NavMode::Track); // back to Track
        assert_eq!(nav.mode, NavMode::Track);
        nav.set_mode(NavMode::Folder); // Folder again — back at root
        assert_eq!(nav.depth(), 0);
    }
}
