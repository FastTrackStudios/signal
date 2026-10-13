//! Which folders need a REAPER VCA, worked out from the routing.
//!
//! A REAPER folder already sums its children, so its fader scales them
//! for nothing. A VCA is only needed where that is not true: where the
//! children **route past the folder** to a bus, which is what the
//! guitar parts do to `GTR RHYTHM` and `GTR LEAD`. There the folder is
//! a view over tracks whose audio never passes through it, so its
//! fader would move nothing without a VCA lead.
//!
//! This is derived from the routing rather than kept as a list of
//! folder names. A hand-kept list is a second description of the
//! template that drifts from the first one the moment a shape is added.

use std::collections::HashMap;

use crate::scenes::Fact;

/// A folder that must lead its members, and who they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VcaLead {
    pub folder: String,
    pub folder_name: String,
    /// Every descendant whose audio bypasses the folder, in project
    /// order.
    pub members: Vec<String>,
}

/// The folders that need a VCA lead.
///
/// `parent_send` says whether a track's audio passes up to its parent.
/// A child with the parent send **off** is routed elsewhere, so a
/// folder with at least one such descendant gets a lead over exactly
/// those descendants — the ones it would otherwise not move.
#[must_use]
pub fn leads<S: std::hash::BuildHasher>(
    facts: &[Fact],
    parent_send: &HashMap<String, bool, S>,
) -> Vec<VcaLead> {
    let mut out: Vec<VcaLead> = Vec::new();
    let mut open: Vec<(u32, usize)> = Vec::new();

    for fact in facts {
        // Close every folder shallower than or level with this track.
        open.retain(|(depth, _)| *depth < fact.depth);

        if fact.is_folder {
            out.push(VcaLead {
                folder: fact.guid.clone(),
                folder_name: fact.name.clone(),
                members: Vec::new(),
            });
            // The index of the lead just pushed. `saturating_sub` only
            // to keep the arithmetic total: `out` is never empty here,
            // since the push above put something in it.
            open.push((fact.depth, out.len().saturating_sub(1)));
            continue;
        }

        // A track whose audio leaves the tree before its parent belongs
        // to every folder above it: each of them would fail to move it.
        let bypasses = !parent_send.get(&fact.guid).copied().unwrap_or(true);
        if bypasses {
            for (_, at) in &open {
                // `get_mut` rather than indexing: every open index came
                // from a push into this same vec, but saying so with a
                // lookup costs nothing and cannot panic if that ever
                // stops being true.
                if let Some(lead) = out.get_mut(*at) {
                    lead.members.push(fact.guid.clone());
                }
            }
        }
    }

    out.retain(|lead| !lead.members.is_empty());
    out
}
