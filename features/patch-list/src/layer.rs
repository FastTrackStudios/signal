//! The session override, layered on the album list (decision #29:
//! "the override is layered on top, marked as such in the Patch List,
//! and stays with that session alone").
//!
//! An override is a partial document in the album's own schema — same
//! `PatchList` type, usually with one performer's one rig holding one
//! entry. [`layer`] merges it onto the album **by key**: a performer /
//! kind / key already in the album is replaced, one not there is
//! added, and nothing in the album is ever removed — so an override
//! cannot delete an entry, only stand in front of one. Removing the
//! override key from ext-state (`apply::clear_override`) is what
//! "returns the session to the list": with no override to layer,
//! [`layer`] hands back the album unchanged.

use std::collections::HashSet;

use crate::list::PatchList;

/// The album, an override layered on top, and which keys the override
/// touched — the Patch List view marks exactly those rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layered {
    /// The merged list: what apply actually applies.
    pub list: PatchList,
    /// `(performer, kind, key)` triples the override replaced or added.
    pub overridden_entries: HashSet<(String, String, String)>,
    /// Headphone bus names the override replaced or added.
    pub overridden_buses: HashSet<String>,
}

/// Layer an optional override onto an album.
///
/// `over: None` is the common case — no override for this session —
/// and returns the album exactly, with nothing marked.
// r[impl flow.patch-list.session-override]
#[must_use]
pub fn layer(album: &PatchList, over: Option<&PatchList>) -> Layered {
    let mut list = album.clone();
    let mut overridden_entries = HashSet::new();
    let mut overridden_buses = HashSet::new();

    if let Some(over) = over {
        for (performer, rigs) in &over.performers {
            let dest_rigs = list.performers.entry(performer.clone()).or_default();
            for (kind, rig) in rigs {
                let dest_rig = dest_rigs.entry(kind.clone()).or_default();
                for (key, entry) in rig {
                    dest_rig.insert(key.clone(), entry.clone());
                    overridden_entries.insert((performer.clone(), kind.clone(), key.clone()));
                }
            }
        }
        for (name, bus) in &over.headphones {
            list.headphones.insert(name.clone(), bus.clone());
            overridden_buses.insert(name.clone());
        }
    }

    Layered {
        list,
        overridden_entries,
        overridden_buses,
    }
}
