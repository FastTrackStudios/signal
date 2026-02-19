//! Rack and Director types — higher-level groupings above Rig.
//!
//! ## Hierarchy
//!
//! ```text
//! Director → Rack → Rig → Engine → Layer → Module → Block
//! ```
//!
//! A **Director** manages a session's entire signal routing (multiple Racks).
//! A **Rack** groups related Rigs (e.g. "Guitar Rack" with clean/dirty rigs).

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::fx_send::FxSendBus;

crate::typed_uuid_id!(
    /// Unique identifier for a Rack.
    RackId
);

crate::typed_uuid_id!(
    /// Unique identifier for a Director.
    DirectorId
);

/// A slot in a Rack that references a Rig.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct RackSlot {
    /// Position index within the rack.
    pub position: u32,
    /// Reference to the rig loaded in this slot.
    pub rig_id: crate::rig::RigId,
    /// Whether this slot is active (unmuted).
    pub active: bool,
}

/// A Rack groups multiple Rigs for organized switching.
///
/// Example: "Guitar Rack" might contain a clean rig, crunch rig, and lead rig.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Rack {
    pub id: RackId,
    pub name: String,
    /// Ordered rig slots.
    pub slots: Vec<RackSlot>,
    /// Index of the currently selected slot.
    pub active_slot: Option<u32>,
    /// Rack-level FX send buses (e.g. "AUX", "TIME" sub-categories).
    #[serde(default)]
    pub fx_send_buses: Vec<FxSendBus>,
}

impl Rack {
    pub fn new(id: impl Into<RackId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            slots: Vec::new(),
            active_slot: None,
            fx_send_buses: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_fx_send_bus(mut self, bus: FxSendBus) -> Self {
        self.fx_send_buses.push(bus);
        self
    }

    pub fn active_rig_id(&self) -> Option<&crate::rig::RigId> {
        self.active_slot
            .and_then(|idx| self.slots.iter().find(|s| s.position == idx && s.active))
            .map(|s| &s.rig_id)
    }
}

/// A Director manages the entire signal routing session.
///
/// Contains multiple racks (e.g. Guitar Rack, Bass Rack, Keys Rack)
/// and controls global routing between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Director {
    pub id: DirectorId,
    pub name: String,
    /// Ordered rack references.
    pub rack_ids: Vec<RackId>,
}

impl Director {
    pub fn new(id: impl Into<DirectorId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            rack_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rack_new_is_empty() {
        let rack = Rack::new(RackId::new(), "Guitar Rack");
        assert!(rack.slots.is_empty());
        assert!(rack.active_rig_id().is_none());
    }

    #[test]
    fn rack_active_rig() {
        let rig_id = crate::rig::RigId::new();
        let mut rack = Rack::new(RackId::new(), "Guitar Rack");
        rack.slots.push(RackSlot {
            position: 0,
            rig_id: rig_id.clone(),
            active: true,
        });
        rack.active_slot = Some(0);
        assert_eq!(rack.active_rig_id(), Some(&rig_id));
    }

    #[test]
    fn director_new_is_empty() {
        let dir = Director::new(DirectorId::new(), "Main");
        assert!(dir.rack_ids.is_empty());
    }

    #[test]
    fn serde_round_trip() {
        let rack = Rack::new(RackId::new(), "Test Rack");
        let json = serde_json::to_string(&rack).unwrap();
        let parsed: Rack = serde_json::from_str(&json).unwrap();
        assert_eq!(rack, parsed);
    }
}
