//! Echo suppression — prevents re-broadcasting changes that were applied from remote peers.
//!
//! When the sync engine applies a remote change (e.g., `track.set_volume()`), the next
//! poll cycle in daw-reaper will detect that change and try to broadcast it. Without
//! suppression, this creates an infinite echo loop between peers.
//!
//! # Strategy
//!
//! 1. **Record**: Before applying a remote change, insert a suppression key + timestamp.
//! 2. **Check**: Before broadcasting a local change, check if a matching suppression entry
//!    exists within the suppression window (~150ms, enough for 3-5 poll cycles at 30Hz).
//! 3. **Expire**: Entries auto-expire after the window, so genuine local changes are
//!    never permanently suppressed.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default suppression window — 500ms covers network round-trip + poll cycles.
/// Needs to be generous enough to catch echoes bouncing through TCP peers.
const DEFAULT_WINDOW: Duration = Duration::from_millis(500);

/// A key identifying a specific domain + entity for suppression matching.
///
/// The key must uniquely identify the "thing that changed" so we can match
/// the detected poll change against the recently-applied remote change.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct SuppressionKey {
    /// Domain discriminant (e.g., "track", "fx", "transport")
    pub domain: &'static str,
    /// Entity identifier (e.g., track GUID, "transport:{project_guid}")
    pub entity: String,
    /// Field that changed (e.g., "volume", "muted", "position")
    pub field: String,
}

impl SuppressionKey {
    pub fn new(domain: &'static str, entity: impl Into<String>, field: impl Into<String>) -> Self {
        Self {
            domain,
            entity: entity.into(),
            field: field.into(),
        }
    }

    /// Create a suppression key for a track field change.
    pub fn track(guid: &str, field: &str) -> Self {
        Self::new("track", guid, field)
    }

    /// Create a suppression key for transport state.
    ///
    /// Uses a fixed entity key because transport events always target the
    /// current project, and the remote/local project GUIDs differ. A per-GUID
    /// key would cause suppression misses and echo loops.
    pub fn transport(_project_guid: &str) -> Self {
        Self::new("transport", "current", "state")
    }

    /// Create a suppression key for an FX parameter change.
    pub fn fx_param(chain_key: &str, fx_guid: &str, param_index: u32) -> Self {
        Self::new(
            "fx",
            format!("{chain_key}:{fx_guid}"),
            param_index.to_string(),
        )
    }

    /// Create a suppression key for an item field change.
    pub fn item(guid: &str, field: &str) -> Self {
        Self::new("item", guid, field)
    }

    /// Create a suppression key for a routing change.
    pub fn routing(track_guid: &str, field: &str) -> Self {
        Self::new("routing", track_guid, field)
    }

    /// Create a suppression key for a tempo map change.
    pub fn tempo_map(project_guid: &str) -> Self {
        Self::new("tempo_map", project_guid, "map")
    }

    /// Create a suppression key for a marker change.
    pub fn marker(project_guid: &str, marker_id: u32) -> Self {
        Self::new("marker", project_guid, marker_id.to_string())
    }

    /// Create a suppression key for a region change.
    pub fn region(project_guid: &str, region_id: u32) -> Self {
        Self::new("region", project_guid, region_id.to_string())
    }
}

/// An entry in the suppression set. Carries a timestamp for window
/// expiry and an optional value fingerprint used by value-aware matching:
/// `is_suppressed_value` returns true only when the recorded fingerprint
/// matches the incoming one (so a genuine subsequent change with a
/// different value isn't dropped).
#[derive(Clone, Debug)]
struct Entry {
    inserted: Instant,
    /// Bit-pattern of the applied f64 value (or the encoded form for
    /// non-numeric fields). `None` means "any value matches" — falls
    /// back to key-only matching for callers that pre-date value-aware
    /// suppression.
    value_bits: Option<u64>,
}

/// Tracks recently-applied remote changes to suppress echoes.
pub struct SuppressionSet {
    entries: HashMap<SuppressionKey, Entry>,
    window: Duration,
}

impl SuppressionSet {
    /// Create a new suppression set with the default window.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            window: DEFAULT_WINDOW,
        }
    }

    /// Record that a remote change is about to be applied locally.
    ///
    /// Call this *before* applying the mutation so the next poll cycle
    /// (or callback-driven publish) can find the suppression entry.
    /// Key-only matching: any subsequent event with this key within the
    /// window is suppressed.
    pub fn suppress(&mut self, key: SuppressionKey) {
        self.entries.insert(
            key,
            Entry {
                inserted: Instant::now(),
                value_bits: None,
            },
        );
    }

    /// Record a remote change with the value being applied. Subsequent
    /// events with this key are only suppressed if they carry the same
    /// value — a genuine local change to a different value still
    /// broadcasts. Lets us run REAPER's IReaperControlSurface callbacks
    /// in `Full` mode without echo loops, since REAPER fires the
    /// callback with the applied value, which matches the recorded
    /// fingerprint exactly.
    pub fn suppress_value(&mut self, key: SuppressionKey, value: f64) {
        self.entries.insert(
            key,
            Entry {
                inserted: Instant::now(),
                value_bits: Some(value.to_bits()),
            },
        );
    }

    /// Check if a detected local change should be suppressed (was recently
    /// applied from a remote peer). Key-only — matches regardless of
    /// the value carried by the incoming event.
    pub fn is_suppressed(&self, key: &SuppressionKey) -> bool {
        if let Some(entry) = self.entries.get(key) {
            entry.inserted.elapsed() < self.window
        } else {
            false
        }
    }

    /// Value-aware variant of [`Self::is_suppressed`]. Returns true only
    /// if the recorded entry's value (set via [`Self::suppress_value`])
    /// matches `value`. If the entry was inserted without a value
    /// fingerprint (via plain [`Self::suppress`]), falls back to
    /// key-only matching. This lets the same suppression set serve both
    /// the value-strict callback path and the key-only poll path.
    pub fn is_suppressed_value(&self, key: &SuppressionKey, value: f64) -> bool {
        let Some(entry) = self.entries.get(key) else {
            return false;
        };
        if entry.inserted.elapsed() >= self.window {
            return false;
        }
        match entry.value_bits {
            None => true, // entry inserted without value fingerprint — match by key
            Some(bits) => bits == value.to_bits(),
        }
    }

    /// Remove expired entries to prevent unbounded growth.
    ///
    /// Call this periodically (e.g., once per second) from the engine loop.
    pub fn gc(&mut self) {
        self.entries
            .retain(|_, e| e.inserted.elapsed() < self.window);
    }
}

impl Default for SuppressionSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn suppress_and_check() {
        let mut set = SuppressionSet::new();
        let key = SuppressionKey::track("abc-123", "volume");

        assert!(!set.is_suppressed(&key));

        set.suppress(key.clone());
        assert!(set.is_suppressed(&key));

        // Different field is not suppressed
        let other = SuppressionKey::track("abc-123", "pan");
        assert!(!set.is_suppressed(&other));
    }

    #[test]
    fn entries_expire() {
        let mut set = SuppressionSet {
            entries: HashMap::new(),
            window: Duration::from_millis(10),
        };
        let key = SuppressionKey::track("abc-123", "volume");
        set.suppress(key.clone());
        assert!(set.is_suppressed(&key));

        thread::sleep(Duration::from_millis(15));
        assert!(!set.is_suppressed(&key));
    }

    #[test]
    fn gc_removes_expired() {
        let mut set = SuppressionSet {
            entries: HashMap::new(),
            window: Duration::from_millis(10),
        };
        set.suppress(SuppressionKey::track("a", "volume"));
        set.suppress(SuppressionKey::track("b", "pan"));

        thread::sleep(Duration::from_millis(15));
        set.gc();
        assert!(set.entries.is_empty());
    }
}
