//! FX events for reactive subscriptions

use super::tree::{FxNodeId, FxRoutingMode};
use super::{Fx, FxChainContext};
use facet::Facet;

/// Events emitted when FX state changes
#[repr(u8)]
#[derive(Debug, Clone, Facet)]
pub enum FxEvent {
    // =========================================================================
    // Plugin events (existing)
    // =========================================================================
    /// An FX was added to a chain
    Added { context: FxChainContext, fx: Fx },
    /// An FX was removed from a chain
    Removed {
        context: FxChainContext,
        fx_guid: String,
    },
    /// FX enabled/bypass state changed
    EnabledChanged {
        context: FxChainContext,
        fx_guid: String,
        enabled: bool,
    },
    /// FX was moved within the chain
    Moved {
        context: FxChainContext,
        fx_guid: String,
        old_index: u32,
        new_index: u32,
    },
    /// FX parameter value changed
    ParameterChanged {
        context: FxChainContext,
        fx_guid: String,
        param_index: u32,
        value: f64,
    },
    /// FX preset changed
    PresetChanged {
        context: FxChainContext,
        fx_guid: String,
        preset_name: Option<String>,
    },
    /// FX UI window opened/closed
    WindowChanged {
        context: FxChainContext,
        fx_guid: String,
        open: bool,
    },

    // =========================================================================
    // Container events (new)
    // =========================================================================
    /// A new container was created
    ContainerCreated {
        context: FxChainContext,
        container_id: FxNodeId,
        name: String,
    },
    /// A container was removed (children may have been moved or removed)
    ContainerRemoved {
        context: FxChainContext,
        container_id: FxNodeId,
    },
    /// Container routing mode changed (serial/parallel)
    RoutingModeChanged {
        context: FxChainContext,
        container_id: FxNodeId,
        mode: FxRoutingMode,
    },
    /// An FX was moved into a container
    MovedToContainer {
        context: FxChainContext,
        node_id: FxNodeId,
        source_container: Option<FxNodeId>,
        dest_container: FxNodeId,
    },
    /// A container was renamed
    ContainerRenamed {
        context: FxChainContext,
        container_id: FxNodeId,
        name: String,
    },
    /// Catch-all for complex tree structure mutations that affect container hierarchy.
    /// Emitted when the tree shape changes in ways that can't be described by
    /// individual add/remove/move events (e.g., bulk restructuring).
    TreeStructureChanged { context: FxChainContext },
}

/// An [`FxEvent`] stamped with the project it belongs to — the wire
/// shape for streaming subscriptions (mirrors `TrackStreamEvent`).
#[derive(Debug, Clone, Facet)]
pub struct FxStreamEvent {
    pub project_guid: String,
    pub event: FxEvent,
}

// FxEvent has no lifetime parameters, so Ref<'a> = Self is trivially sound.
#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for FxEvent {
    type Ref<'a> = FxEvent;
}

#[allow(unsafe_code)]
unsafe impl vox_types::Reborrow for FxStreamEvent {
    type Ref<'a> = FxStreamEvent;
}
