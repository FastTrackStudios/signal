//! How wide each track's strip is drawn.
//!
//! A mixer is read by scanning across it, and a session has tracks that
//! deserve very different amounts of that scan: a trigger or a reverb
//! return needs its name and its mute, while the track being worked on
//! wants room for an embedded FX display. One width for all of them
//! spends the same space on both.
//!
//! # Whose it is
//!
//! `Track::width` is in the proto and REAPER never fills it — there is
//! no REAPER setter, because REAPER's mixer has no per-track width to
//! set. So this is not "read the DAW's value": it is a value FTS owns,
//! and the only question was where to keep it.
//!
//! Project ext state, keyed by track guid. It travels with the project,
//! so a session opened on another machine is laid out the way it was
//! left; it works identically over a live REAPER and over
//! daw-standalone, because `ExtState` is the same trait on both; and it
//! does not pretend to be REAPER's, which a value written into the
//! track state chunk would.
//!
//! # One key, not one per track
//!
//! A mixer asks for every width at once and changes one at a time. One
//! key is one read on open rather than one per track — the difference
//! between a round trip and two hundred of them on a real session — and
//! the writer already holds the whole map, so writing it back whole
//! costs nothing it did not already have.

use std::collections::HashMap;

/// Where the widths live.
pub const SECTION: &str = "fasttrackstudio.mixer";

/// The one key, holding every track's.
pub const KEY: &str = "widths";

/// The narrowest a stored width may be.
///
/// Not a drawing limit — the mixer squeezes further than this when the
/// window is small. This is the floor on what can be SAVED, so a strip
/// cannot be stored at a width that would make it unfindable the next
/// time the session opens.
pub const NARROWEST: u32 = 40;

/// The widest.
///
/// A strip wider than this is a strip that has stopped being one. The
/// ceiling exists so a dragged edge cannot store a width that pushes
/// every other strip off the screen for good.
pub const WIDEST: u32 = 600;

/// Every track's strip width, by guid.
///
/// A track with no entry is drawn at the default, which is the common
/// case: a session stores the handful somebody chose to change.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Widths(HashMap<String, u32>);

impl Widths {
    /// One track's width, if it has one.
    #[must_use]
    pub fn get(&self, guid: &str) -> Option<u32> {
        self.0.get(guid).copied()
    }

    /// Set one, clamped to what may be stored.
    ///
    /// Clamped rather than refused: this arrives from a drag, and a
    /// drag that ran past the end should stop at the end rather than
    /// leaving the width at whatever it was before the pointer left.
    pub fn set(&mut self, guid: &str, width: u32) {
        self.0
            .insert(guid.to_owned(), width.clamp(NARROWEST, WIDEST));
    }

    /// Put a track back to the default.
    ///
    /// Removing rather than storing the default: a session where
    /// nobody chose a width should say nothing, so that changing the
    /// default later changes those strips.
    pub fn clear(&mut self, guid: &str) {
        self.0.remove(guid);
    }

    /// How many tracks have one.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How it is written into ext state: `guid=width`, one per line.
    ///
    /// Sorted, so a project file does not churn on save because a hash
    /// map iterated differently this time.
    #[must_use]
    pub fn stored(&self) -> String {
        let mut lines: Vec<String> = self
            .0
            .iter()
            .map(|(guid, width)| format!("{guid}={width}"))
            .collect();
        lines.sort();
        lines.join("\n")
    }

    /// Read one back.
    ///
    /// Unreadable lines are skipped rather than failing the lot: a
    /// mixer that lost every width because one line was corrupt would
    /// be a worse answer than a mixer that lost one.
    #[must_use]
    pub fn from_stored(text: &str) -> Self {
        let mut widths = Self::default();
        for line in text.lines() {
            let Some((guid, width)) = line.split_once('=') else {
                continue;
            };
            let Ok(width) = width.trim().parse::<u32>() else {
                continue;
            };
            if guid.is_empty() {
                continue;
            }
            widths.set(guid.trim(), width);
        }
        widths
    }
}

/// The project's stored widths.
#[must_use]
pub fn read<E: daw::service::ExtState>(ext: &E, project: daw::service::ProjectContext) -> Widths {
    ext.get_project(project, SECTION, KEY)
        .as_deref()
        .map(Widths::from_stored)
        .unwrap_or_default()
}

/// Keep them with the project: one write.
///
/// # Errors
///
/// Whatever the backend's `set_project` returns.
pub fn write<E: daw::service::ExtState>(
    ext: &E,
    project: daw::service::ProjectContext,
    widths: &Widths,
) -> daw_proto::DawResult<()> {
    ext.set_project(project, SECTION, KEY, &widths.stored())
}

#[cfg(test)]
mod tests {
    use super::{NARROWEST, WIDEST, Widths};

    #[test]
    fn a_track_with_no_width_has_no_width() {
        let widths = Widths::default();
        assert_eq!(widths.get("kick"), None);
        assert!(widths.is_empty());
    }

    /// What goes in comes back, and comes back the same whatever order
    /// the map felt like iterating in.
    #[test]
    fn the_widths_round_trip_and_do_not_churn() {
        let mut widths = Widths::default();
        widths.set("kick", 120);
        widths.set("snare", 90);
        widths.set("verb", 60);
        let stored = widths.stored();
        assert_eq!(stored, Widths::from_stored(&stored).stored());
        assert_eq!(Widths::from_stored(&stored), widths);
    }

    /// A drag that ran off the end stops at the end. Stored widths have
    /// a floor and a ceiling: a strip cannot be saved at a width that
    /// makes it unfindable, or one that pushes every other strip off
    /// the screen for good.
    #[test]
    fn a_stored_width_stays_between_its_ends() {
        let mut widths = Widths::default();
        widths.set("tiny", 1);
        widths.set("huge", 100_000);
        assert_eq!(widths.get("tiny"), Some(NARROWEST));
        assert_eq!(widths.get("huge"), Some(WIDEST));
    }

    /// Putting a track back to the default says nothing about it,
    /// rather than storing what the default happens to be today.
    #[test]
    fn clearing_a_width_forgets_it() {
        let mut widths = Widths::default();
        widths.set("kick", 120);
        widths.clear("kick");
        assert_eq!(widths.get("kick"), None);
        assert_eq!(widths.stored(), "");
    }

    /// One bad line costs one width, not all of them.
    #[test]
    fn a_corrupt_line_costs_one_width() {
        let widths = Widths::from_stored("kick=120\nnonsense\nsnare=not-a-number\nverb=60");
        assert_eq!(widths.get("kick"), Some(120));
        assert_eq!(widths.get("verb"), Some(60));
        assert_eq!(widths.len(), 2);
    }
}
