//! Snapshot automation types for timeline-based parameter control.
//!
//! Automation events are placed on a timeline and trigger actions at specific
//! positions (measured in beats or seconds). The runtime processes them in
//! order during playback.

use crate::easing::EasingCurve;
use crate::rig::RigSceneId;
use serde::{Deserialize, Serialize};

/// An action triggered by an automation event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutomationAction {
    /// Instantly apply a snapshot/scene.
    ApplySnapshot { scene_id: RigSceneId },
    /// Start a gradual morph between current and target scene.
    StartMorph {
        target_scene_id: RigSceneId,
        /// Duration in beats.
        duration_beats: f64,
        /// Easing curve for the morph transition.
        easing: EasingCurve,
    },
    /// Set the morph slider to an absolute position.
    SetMorphPosition {
        /// Position (0.0 = scene A, 1.0 = scene B).
        position: f32,
    },
    /// Set a specific parameter to a value.
    SetParameter {
        block_id: String,
        parameter_id: String,
        value: f32,
    },
}

/// A single automation event on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationEvent {
    /// Unique ID for this event.
    pub id: String,
    /// Position on the timeline in beats (from song start).
    pub position_beats: f64,
    /// The action to perform.
    pub action: AutomationAction,
    /// Whether this event is active.
    pub enabled: bool,
}

impl AutomationEvent {
    pub fn new(
        id: impl Into<String>,
        position_beats: f64,
        action: AutomationAction,
    ) -> Self {
        Self {
            id: id.into(),
            position_beats,
            action,
            enabled: true,
        }
    }
}

/// A lane of automation events, typically associated with a song section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AutomationLane {
    /// Human-readable name (e.g. "Scene Changes", "Morph Automation").
    pub name: String,
    /// Events sorted by position.
    pub events: Vec<AutomationEvent>,
}

impl AutomationLane {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: Vec::new(),
        }
    }

    /// Add an event and maintain sorted order by position.
    pub fn add_event(&mut self, event: AutomationEvent) {
        let pos = event.position_beats;
        let insert_idx = self
            .events
            .iter()
            .position(|e| e.position_beats > pos)
            .unwrap_or(self.events.len());
        self.events.insert(insert_idx, event);
    }

    pub fn remove_event(&mut self, id: &str) {
        self.events.retain(|e| e.id != id);
    }

    /// Get all events in a time range (inclusive start, exclusive end).
    pub fn events_in_range(&self, start_beats: f64, end_beats: f64) -> Vec<&AutomationEvent> {
        self.events
            .iter()
            .filter(|e| e.enabled && e.position_beats >= start_beats && e.position_beats < end_beats)
            .collect()
    }
}

/// Collection of automation lanes for a song section.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotAutomation {
    pub lanes: Vec<AutomationLane>,
}

impl SnapshotAutomation {
    pub fn new() -> Self {
        Self { lanes: Vec::new() }
    }

    pub fn add_lane(&mut self, lane: AutomationLane) {
        self.lanes.push(lane);
    }

    /// All events across all lanes in a time range, sorted by position.
    pub fn all_events_in_range(
        &self,
        start_beats: f64,
        end_beats: f64,
    ) -> Vec<&AutomationEvent> {
        let mut events: Vec<&AutomationEvent> = self
            .lanes
            .iter()
            .flat_map(|lane| lane.events_in_range(start_beats, end_beats))
            .collect();
        events.sort_by(|a, b| a.position_beats.partial_cmp(&b.position_beats).unwrap());
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_maintain_sorted_order() {
        let mut lane = AutomationLane::new("Test");
        lane.add_event(AutomationEvent::new(
            "b",
            4.0,
            AutomationAction::SetMorphPosition { position: 0.5 },
        ));
        lane.add_event(AutomationEvent::new(
            "a",
            1.0,
            AutomationAction::SetMorphPosition { position: 0.0 },
        ));
        lane.add_event(AutomationEvent::new(
            "c",
            8.0,
            AutomationAction::SetMorphPosition { position: 1.0 },
        ));

        assert_eq!(lane.events[0].id, "a");
        assert_eq!(lane.events[1].id, "b");
        assert_eq!(lane.events[2].id, "c");
    }

    #[test]
    fn events_in_range_filters_correctly() {
        let mut lane = AutomationLane::new("Test");
        lane.add_event(AutomationEvent::new(
            "a",
            1.0,
            AutomationAction::SetMorphPosition { position: 0.0 },
        ));
        lane.add_event(AutomationEvent::new(
            "b",
            5.0,
            AutomationAction::SetMorphPosition { position: 0.5 },
        ));
        lane.add_event(AutomationEvent::new(
            "c",
            10.0,
            AutomationAction::SetMorphPosition { position: 1.0 },
        ));

        let range = lane.events_in_range(2.0, 8.0);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].id, "b");
    }

    #[test]
    fn serde_round_trip() {
        let event = AutomationEvent::new(
            "test",
            4.0,
            AutomationAction::StartMorph {
                target_scene_id: RigSceneId::new(),
                duration_beats: 2.0,
                easing: EasingCurve::EaseInOut,
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AutomationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}
