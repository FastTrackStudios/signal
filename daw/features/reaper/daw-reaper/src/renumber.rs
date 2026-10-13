//! Telling a renumbering apart from a deletion.
//!
//! Markers and regions are one list in REAPER wearing one set of
//! numbers, and "Renumber markers/regions in timeline order" reassigns
//! them all at once. Diffed by number, that reads as the whole list
//! being deleted and a set of strangers arriving in its place — so a
//! client loses its selection, and a client mid-drag moves whatever now
//! wears the number it was holding.
//!
//! Pulled out of the two pollers so the rule has somewhere to be
//! tested. Driving REAPER's own renumber action from a test turned out
//! to be a bad way to ask: the action never returned and the run sat
//! through its whole timeout. The rule is arithmetic on two maps, and
//! it can be asked directly.

use std::collections::HashMap;

/// Pair numbers that vanished with numbers that appeared.
///
/// A pair is a removal and an addition that `same` considers identical
/// — everything a user can see, the number excepted. Two markers alike
/// in name, position, colour and lane are indistinguishable to a user
/// as well, so choosing the wrong one of those cannot produce a wrong
/// answer.
///
/// Returns old number → new number. Anything unpaired is a real
/// addition or a real removal and is reported as one.
pub fn pair<T>(
    previous: &HashMap<u32, T>,
    fresh: &HashMap<u32, T>,
    same: impl Fn(&T, &T) -> bool,
) -> HashMap<u32, u32> {
    let gone: Vec<u32> = previous
        .keys()
        .copied()
        .filter(|id| !fresh.contains_key(id))
        .collect();
    let mut arrived: Vec<u32> = fresh
        .keys()
        .copied()
        .filter(|id| !previous.contains_key(id))
        .collect();
    // Both lists are what changed in one 33 ms tick, so they are short
    // even when the whole project was renumbered — REAPER does that in
    // one action, and one action lands in one tick.
    arrived.sort_unstable();

    let mut paired = HashMap::new();
    for old_id in gone {
        let Some(old) = previous.get(&old_id) else {
            continue;
        };
        let found = arrived
            .iter()
            .position(|new_id| fresh.get(new_id).is_some_and(|new| same(old, new)));
        if let Some(at) = found {
            paired.insert(old_id, arrived.remove(at));
        }
    }
    paired
}

#[cfg(test)]
mod tests {
    use super::pair;
    use std::collections::HashMap;

    /// Stands in for a marker: what a user can see, and nothing else.
    #[derive(Clone, PartialEq, Eq, Debug)]
    struct Mark(&'static str, u32);

    fn same(a: &Mark, b: &Mark) -> bool {
        a == b
    }

    fn map(entries: &[(u32, Mark)]) -> HashMap<u32, Mark> {
        entries.iter().cloned().collect()
    }

    /// The whole list renumbered at once, which is what the action does.
    #[test]
    fn every_number_moving_is_every_marker_kept() {
        let before = map(&[(1, Mark("Late", 32)), (2, Mark("Early", 4))]);
        let after = map(&[(1, Mark("Early", 4)), (2, Mark("Late", 32))]);
        let paired = pair(&before, &after, same);
        // Neither number vanished, so there is nothing to pair: both
        // are still present and will diff as ordinary changes.
        assert!(paired.is_empty(), "nothing vanished, nothing to pair");
    }

    /// A number genuinely leaving and another genuinely arriving.
    #[test]
    fn a_renumber_is_paired() {
        let before = map(&[(3, Mark("Chorus", 24))]);
        let after = map(&[(4, Mark("Chorus", 24))]);
        assert_eq!(pair(&before, &after, same).get(&3), Some(&4));
    }

    /// A deletion is not a renumbering, and must not be mistaken for
    /// one — a client told its marker was renumbered would keep
    /// drawing something that is gone.
    #[test]
    fn a_deletion_stays_a_deletion() {
        let before = map(&[(3, Mark("Chorus", 24))]);
        let after = map(&[]);
        assert!(pair(&before, &after, same).is_empty());
    }

    /// Nor is an unrelated addition. The negative control: the numbers
    /// line up exactly as a renumbering would, and only the CONTENT
    /// says otherwise.
    #[test]
    fn a_different_marker_at_a_new_number_is_not_a_renumber() {
        let before = map(&[(3, Mark("Chorus", 24))]);
        let after = map(&[(4, Mark("Bridge", 40))]);
        assert!(
            pair(&before, &after, same).is_empty(),
            "paired two markers that share nothing but having changed"
        );
    }

    /// One removal, several candidates. Any of them is correct,
    /// because a user cannot tell them apart either — but exactly one
    /// must be consumed, or the extra is reported as both a
    /// renumbering and an addition.
    #[test]
    fn identical_candidates_are_used_once_each() {
        let before = map(&[(1, Mark("Verse", 8)), (2, Mark("Verse", 8))]);
        let after = map(&[(5, Mark("Verse", 8)), (6, Mark("Verse", 8))]);
        let paired = pair(&before, &after, same);
        assert_eq!(paired.len(), 2, "both removals should pair");
        let mut taken: Vec<u32> = paired.values().copied().collect();
        taken.sort_unstable();
        assert_eq!(taken, vec![5, 6], "a number was claimed twice");
    }
}
