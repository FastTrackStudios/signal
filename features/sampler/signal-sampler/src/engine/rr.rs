//! Round-robin counters — cycles through RR slots for each (section, articulation, dynamic).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Hash the (section, articulation, dynamic) triple into a single `u64` key.
///
/// Keyed by hash rather than owned `String`s so [`RrCounters::next`] allocates
/// nothing on the audio thread (it runs per note-on). A 64-bit collision would
/// only make two groups share a round-robin counter — harmless. The `0xff`
/// separators prevent `("ab","c",…)` and `("a","bc",…)` from colliding.
fn rr_key(section: &str, articulation: &str, dynamic: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    section.hash(&mut h);
    0xffu8.hash(&mut h);
    articulation.hash(&mut h);
    0xffu8.hash(&mut h);
    dynamic.hash(&mut h);
    h.finish()
}

/// Per-group RR state.
pub struct RrCounters {
    counters: HashMap<u64, usize>,
    /// Starting index applied to new groups (set by CC59 reset).
    rr_start: usize,
}

impl RrCounters {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            rr_start: 0,
        }
    }

    /// Advance and return the next RR index (0-based) for the given group.
    /// `max_rr` is the total number of RR slots (from the spec).
    pub fn next(
        &mut self,
        section: &str,
        articulation: &str,
        dynamic: &str,
        max_rr: usize,
    ) -> usize {
        if max_rr == 0 {
            return 0;
        }
        let key = rr_key(section, articulation, dynamic);
        let start = self.rr_start;
        let counter = self.counters.entry(key).or_insert(start);
        let idx = *counter % max_rr;
        *counter = (*counter + 1) % max_rr;
        idx
    }

    /// Reset all counters (e.g. on patch change).
    pub fn reset(&mut self) {
        self.counters.clear();
        self.rr_start = 0;
    }

    /// Set all existing counters to `start_index` and record it for new groups (CC59).
    ///
    /// After this call the next sample played for every group will use
    /// `start_index % max_rr`, giving deterministic playback of a passage.
    pub fn reset_to(&mut self, start_index: usize) {
        self.rr_start = start_index;
        for v in self.counters.values_mut() {
            *v = start_index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_correctly() {
        let mut rr = RrCounters::new();
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 0);
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 1);
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 2);
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 3);
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 0); // wraps
    }

    #[test]
    fn independent_groups() {
        let mut rr = RrCounters::new();
        rr.next("1v", "Staccato", "pp", 4);
        rr.next("1v", "Staccato", "pp", 4);
        // Different dynamic — separate counter
        assert_eq!(rr.next("1v", "Staccato", "mf", 4), 0);
    }

    #[test]
    fn reset_to_resets_existing_and_new_groups() {
        let mut rr = RrCounters::new();
        // Advance pp group to index 2.
        rr.next("1v", "Staccato", "pp", 4); // 0
        rr.next("1v", "Staccato", "pp", 4); // 1
        rr.next("1v", "Staccato", "pp", 4); // 2 — counter now at 3

        // CC59 reset to start index 1.
        rr.reset_to(1);

        // Existing group resets to 1.
        assert_eq!(rr.next("1v", "Staccato", "pp", 4), 1);
        // New group also starts at 1.
        assert_eq!(rr.next("1v", "Staccato", "mf", 4), 1);
    }
}
