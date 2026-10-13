//! Diff result types for comparing two `ReaperProject` snapshots.
//!
//! All types are plain data — no logic. The diff engine populates these structs,
//! and consumers can inspect, serialize, or apply them.

use serde::{Deserialize, Serialize};

// ── Options ─────────────────────────────────────────────────────────────────

/// Options controlling how a [`ProjectDiff`] is applied to a target project.
///
/// The semantics assume the diff was computed as
/// `diff_projects_with_options(song, setlist_section, diff_options)`:
///
/// - `Removed` in the diff means the item exists in the song but NOT in the
///   setlist section → the applicator **inserts** it into the target (with
///   `position_offset` added to positions).
/// - `Added` means the item exists only in the setlist → the applicator
///   **keeps** it (no action needed since it is already in the target).
/// - `Modified` means the item exists in both but differs → the applicator
///   **updates** the target copy to match the song version.
#[derive(Debug, Clone)]
pub struct ApplyOptions {
    /// Offset to ADD to all positions when inserting new items from the song
    /// into the setlist. This is the song's start time within the setlist
    /// timeline.
    pub position_offset: f64,

    /// Match tracks by name instead of GUID (same semantics as
    /// [`DiffOptions::match_tracks_by_name`]).
    pub match_tracks_by_name: bool,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            position_offset: 0.0,
            match_tracks_by_name: false,
        }
    }
}

/// Options controlling how the diff is computed.
#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    /// Time offset (seconds) to subtract from all positions in the "new" project
    /// before comparing. Use this when diffing an individual song RPP against
    /// its section within a concatenated setlist — pass the song's global start
    /// time so that positions are compared relative to the song, not the setlist.
    pub position_offset: f64,

    /// If set, only diff items/points/events within this time window (in the
    /// "new" project's coordinate space, BEFORE offset subtraction). This lets
    /// you isolate a single song's region within a larger setlist project.
    pub time_window: Option<(f64, f64)>,

    /// Match tracks by name instead of GUID. Use this when diffing against
    /// a concatenated setlist where track GUIDs were cleared during generation.
    pub match_tracks_by_name: bool,

    /// Match items by name+position instead of GUID. Use when item GUIDs
    /// may differ between individual song and setlist representations.
    pub match_items_by_name: bool,
}

// ── Top-level ───────────────────────────────────────────────────────────────

/// Complete diff between two `ReaperProject` snapshots.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectDiff {
    /// Property-level changes on the project (tempo, sample rate, etc.)
    pub property_changes: Vec<PropertyChange>,
    /// Per-track diffs (matched by GUID)
    pub tracks: Vec<TrackDiff>,
    /// Project-level envelope diffs
    pub envelopes: Vec<EnvelopeDiff>,
    /// Marker and region diffs
    pub markers_regions: Vec<MarkerRegionDiff>,
    /// Tempo envelope diff
    pub tempo_envelope: Option<TempoEnvelopeDiff>,
    /// Whether ruler lanes changed
    pub ruler_lanes_changed: bool,
}

impl ProjectDiff {
    /// Returns true if nothing changed between the two projects.
    pub fn is_empty(&self) -> bool {
        self.property_changes.is_empty()
            && self.tracks.is_empty()
            && self.envelopes.is_empty()
            && self.markers_regions.is_empty()
            && self.tempo_envelope.is_none()
            && !self.ruler_lanes_changed
    }

    /// Returns true if the diff contains structural changes that require
    /// a full setlist regeneration rather than incremental patching.
    ///
    /// Structural changes include:
    /// - Tempo envelope changes (shifts all subsequent positions)
    /// - Tracks added or removed (setlist folder structure changes)
    /// - Significant item count changes on a track (measure added/removed)
    pub fn is_structural(&self) -> bool {
        // Tempo envelope changes affect all position calculations
        if self.tempo_envelope.is_some() {
            return true;
        }

        // Tracks added or removed
        let tracks_added_or_removed = self
            .tracks
            .iter()
            .any(|t| matches!(t.kind, ChangeKind::Added | ChangeKind::Removed));
        if tracks_added_or_removed {
            return true;
        }

        // Large number of item changes on any single track (likely measure insert/delete)
        let many_item_changes = self.tracks.iter().any(|t| {
            let item_add_remove = t
                .items
                .iter()
                .filter(|i| matches!(i.kind, ChangeKind::Added | ChangeKind::Removed))
                .count();
            item_add_remove >= 3
        });
        if many_item_changes {
            return true;
        }

        false
    }

    /// Human-readable summary of changes.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.property_changes.is_empty() {
            parts.push(format!("{} property changes", self.property_changes.len()));
        }
        let added = self
            .tracks
            .iter()
            .filter(|t| t.kind == ChangeKind::Added)
            .count();
        let removed = self
            .tracks
            .iter()
            .filter(|t| t.kind == ChangeKind::Removed)
            .count();
        let modified = self
            .tracks
            .iter()
            .filter(|t| t.kind == ChangeKind::Modified)
            .count();
        if added > 0 {
            parts.push(format!("{added} tracks added"));
        }
        if removed > 0 {
            parts.push(format!("{removed} tracks removed"));
        }
        if modified > 0 {
            parts.push(format!("{modified} tracks modified"));
        }
        if !self.markers_regions.is_empty() {
            parts.push(format!(
                "{} marker/region changes",
                self.markers_regions.len()
            ));
        }
        if self.tempo_envelope.is_some() {
            parts.push("tempo envelope changed".to_string());
        }
        if parts.is_empty() {
            "no changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ── Generic ─────────────────────────────────────────────────────────────────

/// What happened to an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

/// A scalar property change — field name + old/new as formatted strings.
///
/// We use strings rather than typed values because Track/Item have many
/// heterogeneous field types. Consumers who need typed access can re-parse.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

// ── Track ───────────────────────────────────────────────────────────────────

/// Diff for a single track, matched by GUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDiff {
    /// Track GUID (from track_id field). None if track has no GUID.
    pub guid: Option<String>,
    /// Track name (for display).
    pub name: String,
    pub kind: ChangeKind,
    /// Scalar property changes (name, volume, pan, mute, solo, etc.)
    pub property_changes: Vec<PropertyChange>,
    /// Item-level diffs within this track.
    pub items: Vec<ItemDiff>,
    /// Envelope diffs within this track.
    pub envelopes: Vec<EnvelopeDiff>,
    /// FX chain diff.
    pub fx_chain: Option<FxChainDiff>,
}

// ── Item ────────────────────────────────────────────────────────────────────

/// Diff for a single media item, matched by IGUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDiff {
    pub guid: Option<String>,
    pub name: String,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
    pub takes: Vec<TakeDiff>,
}

/// Diff for a single take within an item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TakeDiff {
    pub guid: Option<String>,
    pub name: String,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
    pub midi: Option<MidiDiff>,
}

// ── Envelope ────────────────────────────────────────────────────────────────

/// Diff for an envelope (track volume, pan, FX param, etc.), matched by GUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeDiff {
    pub guid: String,
    pub envelope_type: String,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
    pub point_changes: Vec<PointChange>,
    pub automation_item_changes: Vec<AutomationItemDiff>,
}

/// Change to a single envelope point (no GUID — matched by position).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PointChange {
    Added(PointSnapshot),
    Removed(PointSnapshot),
    Modified {
        old: PointSnapshot,
        new: PointSnapshot,
    },
}

/// Lightweight snapshot of an envelope point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointSnapshot {
    pub position: f64,
    pub value: f64,
    pub shape: i32,
}

/// Change to a pooled automation item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationItemDiff {
    pub pool_index: i32,
    pub instance_index: i32,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
}

// ── Tempo Envelope ──────────────────────────────────────────────────────────

/// Diff for the project's tempo/time-signature envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempoEnvelopeDiff {
    pub default_tempo_changed: Option<(f64, f64)>,
    pub default_time_sig_changed: Option<((i32, i32), (i32, i32))>,
    pub point_changes: Vec<TempoPointChange>,
}

/// Change to a single tempo point (matched by position).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TempoPointChange {
    Added {
        position: f64,
        tempo: f64,
        time_sig: Option<(i32, i32)>,
    },
    Removed {
        position: f64,
        tempo: f64,
        time_sig: Option<(i32, i32)>,
    },
    Modified {
        position: f64,
        old_tempo: f64,
        new_tempo: f64,
    },
}

// ── FX ──────────────────────────────────────────────────────────────────────

/// Diff for an FX chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxChainDiff {
    pub property_changes: Vec<PropertyChange>,
    pub nodes: Vec<FxNodeDiff>,
}

/// Diff for a single FX node (plugin or container), matched by fxid GUID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FxNodeDiff {
    pub fxid: Option<String>,
    pub name: String,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
    /// Whether the opaque plugin state blob changed.
    pub state_changed: bool,
    /// Parameter envelope changes.
    pub param_envelope_changes: Vec<EnvelopeDiff>,
    /// For containers: recursive child diff.
    pub children: Option<FxChainDiff>,
}

// ── Markers / Regions ───────────────────────────────────────────────────────

/// Diff for a marker or region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkerRegionDiff {
    pub id: i32,
    pub name: String,
    pub is_region: bool,
    pub kind: ChangeKind,
    pub property_changes: Vec<PropertyChange>,
}

// ── MIDI ────────────────────────────────────────────────────────────────────

/// Diff for a MIDI source within a take.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidiDiff {
    pub property_changes: Vec<PropertyChange>,
    pub event_changes: Vec<MidiEventChange>,
}

/// Change to a MIDI event (matched by absolute tick position + bytes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidiEventChange {
    Added { absolute_tick: u64, bytes: Vec<u8> },
    Removed { absolute_tick: u64, bytes: Vec<u8> },
}
