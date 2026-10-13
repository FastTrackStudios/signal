//! Which REAPER group slot an FTS group lives in.
//!
//! REAPER has 128 slots and they are shared across every project of an
//! album, so the question is not "a free one" but "the same one, every
//! time, in every song". Two facts decided the shape, both measured
//! against a real REAPER rather than assumed (daw#12):
//!
//! * **Slot 128 names round-trip.** The SDK header still says a group
//!   name is `1..64`; it is out of date. So FTS takes slots from the
//!   **top downward** and leaves the front of the range for the session
//!   and for the catalog partition, which grows up from 1.
//! * **Slots 65–128 have no `.RPP` representation at all.** `GROUP_FLAGS`
//!   covers 1–32 and `GROUP_FLAGS_HIGH` 33–64, and there is no third
//!   key. A group up here **lives in the session and dies on save.**
//!
//! That second fact would sink a stored assignment. It does not sink
//! this one, because the assignment is **derived, not stored**: the
//! order below is a function of the template, so the watcher rebuilds
//! every FTS group when a project opens and lands on the same slots it
//! had. Persistence is not needed — which is just as well, since the
//! file format cannot offer it. The cost is honest and small: a project
//! opened without the watcher running has no FTS groups, which is the
//! truthful description of a group our software maintains.

use daw_proto::track::GROUP_SLOTS;

/// What an FTS-owned slot is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// One VCA per language, muting every source of the languages that
    /// are not active (`flow.vocals.language.active`).
    Language(String),
    /// A folder whose fader must lead its members' (`flow.guitars.mixing`).
    FolderVca(String),
}

impl Purpose {
    /// The slot's display name in REAPER's group matrix.
    ///
    /// The `FTS` prefix is not decoration: it is how the watcher
    /// recognises its own slots in a project it did not create, which
    /// is what makes re-deriving safe to run on someone else's session.
    #[must_use]
    pub fn slot_name(&self) -> String {
        match self {
            Self::Language(code) => format!("FTS LANG {code}"),
            Self::FolderVca(folder) => format!("FTS VCA {folder}"),
        }
    }
}

/// Assign slots from the top down, languages first.
///
/// Languages come first because they are the same three in every
/// project of an album while the folder set varies with the song, so
/// putting them first keeps *their* slots identical even when a song
/// has no guitars. Returns `(slot, purpose)` pairs, highest slot first.
///
/// A purpose past the range is dropped rather than wrapped into the
/// catalog's end of the range; the caller reports the overflow.
#[must_use]
pub fn assign(languages: &[String], folders: &[String]) -> Vec<(u32, Purpose)> {
    languages
        .iter()
        .map(|code| Purpose::Language(code.clone()))
        .chain(folders.iter().map(|f| Purpose::FolderVca(f.clone())))
        .enumerate()
        .filter_map(|(i, purpose)| {
            let offset = u32::try_from(i).ok()?;
            GROUP_SLOTS.checked_sub(offset).map(|slot| (slot, purpose))
        })
        .filter(|(slot, _)| *slot >= 1)
        .collect()
}

/// Whether a slot's contents survive a save.
///
/// `GROUP_FLAGS` covers 1–32 and `GROUP_FLAGS_HIGH` 33–64. Anything
/// above is session-only — see the module note. Exposed so a caller can
/// say so out loud rather than discovering it after a reopen.
#[must_use]
pub const fn survives_save(slot: u32) -> bool {
    slot <= 64
}
