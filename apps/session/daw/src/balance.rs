//! Volume balance groups: faders that move against each other.
//!
//! A drum bus with parallel compressors under it — Dry, Tight, Punch,
//! Smash, Crunch — is a blend, and a blend is a balance game: every
//! time one comes up the others should come down by the same amount
//! between them, or the loudest colour wins by being loudest rather
//! than by being right. So the members of a group are linked: moving
//! one fader by Δ dB moves each of the others by −Δ/(n − 1), where n
//! is how many are IN.
//!
//! A fader pulled all the way down is OUT — you have decided you do
//! not want that colour — so it stops taking its share of the
//! compensation, and the others split it between fewer of them. It
//! stays out until you bring it back up yourself; while it is out it
//! is not moved, and its first move back up does not move the others
//! (there is no Δ from silence).
//!
//! The rule is the old FTS Volume Balancer's, stated in decibels
//! rather than as a constant linear sum: a mixer moves faders in dB
//! and reads them in dB, and a quarter of a decibel is a quarter of a
//! decibel wherever the fader is.
//!
//! This is the rule, tested on its own. The retired winit window was
//! the one surface that applied it to a fader drag; the docked mixer
//! (`crate::mixer_panel`) does not yet.

use daw_proto::Track;

use crate::engine::{Edit, db_to_gain};

/// The fader's floor, in dB, for the purpose of a move to or from
/// silence: a fader dragged to the bottom has moved down to here, not
/// to minus infinity, and the others come up by that much between
/// them.
const FLOOR_DB: f64 = daw_theme_art::paint::tcp::FADER_BOTTOM_DB;

/// Below this a track is OUT of the balance.
const OUT: f64 = 1e-5;

/// How far above the floor compensation stops, so it never reaches
/// the silence that would take a member out.
const HAIR_DB: f64 = 0.5;

/// Every balance group in the session, as track GUIDs.
// r[impl flow.drums.mixing.balance]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    groups: Vec<Vec<String>>,
}

impl Groups {
    /// The groups a track list implies: the children of every folder
    /// named for parallel compression.
    ///
    /// The template puts the drum kit's parallel compressors — Dry,
    /// Tight, Punch, Smash, Crunch — in a folder called Compress, and
    /// a folder by that name is a group. A session that names its
    /// groups some other way will register them; this is the rule the
    /// template relies on.
    #[must_use]
    pub fn seed(tracks: &[Track]) -> Self {
        let mut groups = Vec::new();
        for folder in tracks
            .iter()
            .filter(|t| t.is_folder && is_group_folder(&t.name))
        {
            let members: Vec<String> = tracks
                .iter()
                .filter(|t| t.parent_guid.as_deref() == Some(folder.guid.as_str()))
                .map(|t| t.guid.clone())
                .collect();
            if members.len() >= 2 {
                groups.push(members);
            }
        }
        Self { groups }
    }

    /// How many groups there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The group a track is in, if any.
    #[must_use]
    pub fn group_of(&self, guid: &str) -> Option<&[String]> {
        self.groups
            .iter()
            .find(|g| g.iter().any(|m| m == guid))
            .map(Vec::as_slice)
    }

    /// The edits that keep a group balanced when one of its faders
    /// moves: one `SetVolume` per other member that is in.
    ///
    /// `to` is the moved track's new gain; its current gain is read
    /// from `tracks`. Empty when the track is in no group, when it was
    /// out (silent) before the move, or when nobody else is in.
    #[must_use]
    pub fn companions(&self, tracks: &[Track], guid: &str, to: f64) -> Vec<Edit> {
        let Some(group) = self.group_of(guid) else {
            return Vec::new();
        };
        let Some(moved) = tracks.iter().find(|t| t.guid == guid) else {
            return Vec::new();
        };
        // From silence there is no Δ: the fader rejoins, and its next
        // move is the one the others answer.
        if moved.volume <= OUT {
            return Vec::new();
        }
        let delta = db_of(to) - db_of(moved.volume);
        if delta.abs() < 1e-9 {
            return Vec::new();
        }
        let others: Vec<&Track> = group
            .iter()
            .filter(|m| m.as_str() != guid)
            .filter_map(|m| tracks.iter().find(|t| t.guid == *m))
            .filter(|t| t.volume > OUT)
            .collect();
        if others.is_empty() {
            return Vec::new();
        }
        let share = -delta / crate::num::coord(others.len());
        others
            .into_iter()
            .map(|t| {
                // Compensation never pushes a member OUT: the fader's
                // bottom is silence (see `db_to_gain`), so a member
                // that would reach it stops a hair above, still in, and
                // comes back up with the next move the other way. Only
                // the hand takes a track out.
                let db = (db_of(t.volume) + share)
                    .clamp(FLOOR_DB + HAIR_DB, daw_theme_art::paint::tcp::FADER_TOP_DB);
                Edit::SetVolume(t.guid.clone(), db_to_gain(db))
            })
            .collect()
    }
}

/// Whether a folder's name says its children are a balance group.
fn is_group_folder(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("compress")
        || lower.starts_with("parallel comp")
        || lower.ends_with("(balance)")
}

/// A gain in dB, with silence at the fader's floor rather than at minus
/// infinity — so a move to or from the bottom has a size.
fn db_of(gain: f64) -> f64 {
    if gain <= OUT {
        FLOOR_DB
    } else {
        (20.0 * gain.log10()).max(FLOOR_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(guid: &str, name: &str, parent: Option<&str>, volume: f64) -> Track {
        Track {
            guid: guid.to_owned(),
            name: name.to_owned(),
            parent_guid: parent.map(str::to_owned),
            volume,
            ..Track::default()
        }
    }

    fn kit() -> Vec<Track> {
        let mut folder = track("comp", "Compress", None, 1.0);
        folder.is_folder = true;
        vec![
            folder,
            track("dry", "Dry", Some("comp"), 1.0),
            track("tight", "Tight", Some("comp"), 1.0),
            track("punch", "Punch", Some("comp"), 1.0),
            track("smash", "Smash", Some("comp"), 1.0),
            track("crunch", "Crunch", Some("comp"), 1.0),
            track("kick", "Kick", None, 1.0),
        ]
    }

    fn gain_of(edits: &[Edit], guid: &str) -> f64 {
        edits
            .iter()
            .find_map(|e| match e {
                Edit::SetVolume(g, v) if g == guid => Some(*v),
                _ => None,
            })
            .expect(guid)
    }

    fn db(gain: f64) -> f64 {
        20.0 * gain.log10()
    }

    /// The folder's children are the group, and a track outside it is
    /// in no group.
    #[test]
    fn the_compression_folder_is_a_group() {
        let groups = Groups::seed(&kit());
        assert_eq!(groups.len(), 1);
        assert!(groups.group_of("smash").is_some());
        assert!(groups.group_of("kick").is_none());
        assert!(groups.companions(&kit(), "kick", 0.5).is_empty());
    }

    /// One up a decibel, the other four down a quarter each.
    // r[verify flow.drums.mixing.balance]
    #[test]
    fn a_decibel_up_is_a_quarter_down_on_each_of_four() {
        let tracks = kit();
        let groups = Groups::seed(&tracks);
        let edits = groups.companions(&tracks, "tight", db_to_gain(1.0));
        assert_eq!(edits.len(), 4);
        for other in ["dry", "punch", "smash", "crunch"] {
            assert!((db(gain_of(&edits, other)) + 0.25).abs() < 1e-6, "{other}");
        }
        assert!(
            edits
                .iter()
                .all(|e| !matches!(e, Edit::SetVolume(g, _) if g == "tight"))
        );
    }

    /// The dry pulled to nothing is out: the next move spreads across
    /// the other three, and the dry stays where it was put.
    // r[verify flow.drums.mixing.balance]
    #[test]
    fn a_silent_member_is_out_of_the_split() {
        let mut tracks = kit();
        tracks
            .iter_mut()
            .find(|t| t.guid == "dry")
            .expect("dry")
            .volume = 0.0;
        let groups = Groups::seed(&tracks);
        let edits = groups.companions(&tracks, "tight", db_to_gain(1.5));
        assert_eq!(edits.len(), 3);
        for other in ["punch", "smash", "crunch"] {
            assert!((db(gain_of(&edits, other)) + 0.5).abs() < 1e-6, "{other}");
        }
        // And bringing the dry back up from nothing moves nobody: there
        // is no Δ from silence.
        assert!(groups.companions(&tracks, "dry", 0.5).is_empty());
    }

    /// Down is the mirror of up, and the size of the step is the size
    /// of the step wherever the fader is.
    #[test]
    fn down_is_the_mirror_of_up() {
        let mut tracks = kit();
        for t in &mut tracks {
            t.volume = db_to_gain(-12.0);
        }
        let groups = Groups::seed(&tracks);
        let edits = groups.companions(&tracks, "smash", db_to_gain(-16.0));
        assert_eq!(edits.len(), 4);
        assert!((db(gain_of(&edits, "dry")) - (-11.0)).abs() < 1e-6);
    }

    /// Compensation can put a member at the floor but never out — only
    /// the hand does that.
    #[test]
    fn compensation_stops_at_the_floor() {
        let mut tracks = kit();
        for t in &mut tracks {
            t.volume = db_to_gain(FLOOR_DB + 0.1);
        }
        tracks
            .iter_mut()
            .find(|t| t.guid == "tight")
            .expect("tight")
            .volume = 1.0;
        let groups = Groups::seed(&tracks);
        let edits = groups.companions(&tracks, "tight", db_to_gain(6.0));
        for e in &edits {
            let Edit::SetVolume(_, v) = e else {
                panic!("{e:?}")
            };
            assert!(*v > OUT);
        }
    }
}
