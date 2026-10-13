//! Which of a song's tracks a client loads and plays.
//!
//! A large session is more than every client needs: a player on stage
//! wants the click and guide, a keys player the keys and the guide, a
//! producer everything. A **selection** names the dynamic template's
//! groups to load — a track's group is the template folder it lives under
//! (`Drums`, `Bass`, `Keys`, `Guide`, …, the song's top-level folders once
//! it is organized) — and a client streams and plays only those. Cue is
//! the selection `Guide`; Engine is all of it.
//!
//! The Guide folder holds two kinds of track: the ones the guide generates
//! (click, count, spoken guide — [`crate::guide::is_cue_track`]) and the
//! multitrack's own click/guide stems filed beside them for reference.
//! The group `guide` is the generated ones; the stems are `guide audio`,
//! so a Cue client never plays both.

use std::collections::BTreeSet;

/// The group of the generated click, count and guide.
pub const GUIDE: &str = "guide";
/// The group of the multitrack's own click/guide stems in the Guide folder.
pub const GUIDE_AUDIO: &str = "guide audio";

/// The tracks a client loads and plays.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum LoadSelection {
    /// Every track.
    #[default]
    All,
    /// Only the tracks in these groups (lower-case template group names).
    Groups(BTreeSet<String>),
}

impl LoadSelection {
    /// What a Cue client loads: the generated click, count and guide.
    #[must_use]
    pub fn cue() -> Self {
        Self::Groups(BTreeSet::from([GUIDE.to_owned()]))
    }

    /// From what a person or a flag writes: `all`, or groups separated by
    /// commas (`guide, keys`). Empty is all.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let groups: BTreeSet<String> = text
            .split(',')
            .map(|g| g.trim().to_lowercase())
            .filter(|g| !g.is_empty())
            .collect();
        if groups.is_empty() || groups.contains("all") {
            Self::All
        } else {
            Self::Groups(groups)
        }
    }

    /// Whether a track in `group` is loaded.
    #[must_use]
    pub fn includes(&self, group: &str) -> bool {
        match self {
            Self::All => true,
            Self::Groups(groups) => groups.contains(&group.to_lowercase()),
        }
    }

    /// How it reads (`all`, `guide + keys`).
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::All => "all".to_owned(),
            Self::Groups(groups) => groups.iter().cloned().collect::<Vec<_>>().join(" + "),
        }
    }
}

/// A track's group: the template folder it lives under — its top-level
/// folder (or itself, when it is one), lower-case; a track under no folder
/// is its own group. `ancestors` are its folders, outermost first.
#[must_use]
pub fn group_of(ancestors: &[&str], name: &str, is_folder: bool) -> String {
    let top = ancestors.first().copied().unwrap_or(name).trim();
    let in_guide = top.eq_ignore_ascii_case(GUIDE);
    if in_guide {
        // The folder itself and what the guide generates are the guide; the
        // stems beside them are their own group.
        let generated = crate::guide::is_cue_track(name, is_folder && ancestors.is_empty());
        return if generated || (is_folder && ancestors.is_empty()) {
            GUIDE.to_owned()
        } else {
            GUIDE_AUDIO.to_owned()
        };
    }
    top.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_is_the_generated_guide_and_not_the_stems_beside_it() {
        let cue = LoadSelection::cue();
        assert!(cue.includes(&group_of(&[], "Guide", true)));
        for generated in ["Click", "Shaker", "Count", "Guide"] {
            assert!(
                cue.includes(&group_of(&["Guide"], generated, false)),
                "{generated}"
            );
        }
        for stem in ["Click Audio", "Guide Audio", "Count Audio"] {
            assert!(!cue.includes(&group_of(&["Guide"], stem, false)), "{stem}");
        }
        assert!(!cue.includes(&group_of(&["Drums"], "Kick", false)));
    }

    #[test]
    fn a_track_belongs_to_its_top_level_folder() {
        assert_eq!(group_of(&["Guitars", "Electric"], "EG 1", false), "guitars");
        assert_eq!(group_of(&[], "Keys", true), "keys");
        assert_eq!(group_of(&[], "Lyrics", false), "lyrics");
    }

    #[test]
    fn a_selection_reads_as_written() {
        assert_eq!(LoadSelection::parse(""), LoadSelection::All);
        assert_eq!(LoadSelection::parse("All"), LoadSelection::All);
        let keys = LoadSelection::parse("Guide, keys");
        assert!(keys.includes("keys") && keys.includes("Guide") && !keys.includes("drums"));
        assert_eq!(keys.label(), "guide + keys");
    }
}
