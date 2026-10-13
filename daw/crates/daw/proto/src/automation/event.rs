//! Automation events for subscriptions

use super::{EnvelopePoint, EnvelopeType};
use crate::primitives::AutomationMode;
use facet::Facet;

/// Events related to automation changes
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum AutomationEvent {
    /// An envelope's visibility changed
    VisibilityChanged {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        visible: bool,
    },
    /// An envelope's armed state changed
    ArmedChanged {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        armed: bool,
    },
    /// An envelope's automation mode changed
    ModeChanged {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        mode: AutomationMode,
    },
    /// A point was added to an envelope
    PointAdded {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        point: EnvelopePoint,
    },
    /// A point was deleted from an envelope
    PointDeleted {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        point_index: u32,
    },
    /// A point was modified
    PointChanged {
        project_guid: String,
        track_guid: String,
        envelope_type: EnvelopeType,
        point: EnvelopePoint,
    },
    /// Global automation override changed
    GlobalOverrideChanged {
        project_guid: String,
        mode: Option<AutomationMode>,
    },
}
