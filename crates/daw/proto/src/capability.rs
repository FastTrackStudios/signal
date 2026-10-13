//! Backend capability declarations
//!
//! DAW backends and file format implementations support different subsets of
//! the protocol. This module provides a way for backends to declare which
//! feature domains they can read and/or write, so consumers can query support
//! upfront rather than discovering it through `NotSupported` errors.
//!
//! # Feature Domains
//!
//! Each [`Capability`] corresponds to a coarse feature domain (e.g., tracks,
//! items, FX chains). A backend declares separate read and write support for
//! each domain via [`FeatureSupport`].
//!
//! # Compliance Profiles
//!
//! Named profiles group capabilities into meaningful levels:
//!
//! - [`TIMELINE_BASIC`]: Minimum viable timeline import (project + tracks + items)
//! - [`MIX_RECALL`]: Full mix state (routing, FX, automation)
//! - [`FULL`]: Everything the protocol defines
//!
//! # Example
//!
//! ```rust
//! use daw_proto::capability::{Capability, FeatureSupport};
//!
//! let support = FeatureSupport::new()
//!     .read_write(&[Capability::Project, Capability::Tracks, Capability::Items])
//!     .read_only(&[Capability::Automation, Capability::FxChain]);
//!
//! assert!(support.can_read(Capability::Automation));
//! assert!(!support.can_write(Capability::Automation));
//! assert!(support.can_read_write(Capability::Tracks));
//! ```

use facet::Facet;

/// A coarse feature domain in the DAW protocol.
///
/// Each variant maps to one or more service traits. Backends declare which
/// domains they support for reading and writing independently.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
pub enum Capability {
    /// Project lifecycle: open, save, close, metadata.
    /// Maps to: `ProjectService`
    Project,

    /// Transport controls: play, stop, record, position.
    /// Maps to: `TransportService`
    Transport,

    /// Track CRUD, volume, pan, mute, solo, arm, folder hierarchy.
    /// Maps to: `TrackService`
    Tracks,

    /// Send/receive routing, hardware outputs, channel mapping.
    /// Maps to: `RoutingService`
    TrackRouting,

    /// Media items on the timeline: position, length, fades, grouping.
    /// Maps to: `ItemService`
    Items,

    /// Multi-take support within items.
    /// Maps to: `TakeService`
    Takes,

    /// MIDI note/CC/sysex editing within takes.
    /// Maps to: `MidiService`
    Midi,

    /// FX chain structure: plugin list, add/remove/reorder.
    /// Maps to: `FxService` (chain queries, management)
    FxChain,

    /// FX parameter/preset state: read/write plugin parameters and presets.
    /// Maps to: `FxService` (parameters, presets, state chunks)
    FxState,

    /// Nested FX containers with parallel/serial routing.
    /// Maps to: `FxService` (containers, routing mode)
    FxContainers,

    /// Automation envelopes: points, shapes, modes.
    /// Maps to: `AutomationService`
    Automation,

    /// Timeline markers.
    /// Maps to: `MarkerService`
    Markers,

    /// Timeline regions.
    /// Maps to: `RegionService`
    Regions,

    /// Tempo and time signature map.
    /// Maps to: `TempoMapService`
    TempoMap,

    /// Undo/redo history.
    /// Maps to: `ProjectService` (undo methods)
    Undo,

    /// Live MIDI device I/O.
    /// Maps to: `LiveMidiService`
    LiveMidi,

    /// Audio engine state, latency, I/O config.
    /// Maps to: `AudioEngineService`
    AudioEngine,

    /// Peak metering data.
    /// Maps to: `PeakService`
    Peaks,

    /// Position/time format conversions.
    /// Maps to: `PositionConversionService`
    PositionConversion,

    /// Custom action registration and execution.
    /// Maps to: `ActionRegistryService`
    ActionRegistry,

    /// Persistent key-value storage (ext state).
    /// Maps to: `ExtStateService`
    ExtState,
}

impl Capability {
    /// All defined capabilities.
    pub const ALL: &[Capability] = &[
        Self::Project,
        Self::Transport,
        Self::Tracks,
        Self::TrackRouting,
        Self::Items,
        Self::Takes,
        Self::Midi,
        Self::FxChain,
        Self::FxState,
        Self::FxContainers,
        Self::Automation,
        Self::Markers,
        Self::Regions,
        Self::TempoMap,
        Self::Undo,
        Self::LiveMidi,
        Self::AudioEngine,
        Self::Peaks,
        Self::PositionConversion,
        Self::ActionRegistry,
        Self::ExtState,
    ];
}

impl core::fmt::Display for Capability {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::Project => "Project",
            Self::Transport => "Transport",
            Self::Tracks => "Tracks",
            Self::TrackRouting => "Track Routing",
            Self::Items => "Items",
            Self::Takes => "Takes",
            Self::Midi => "MIDI",
            Self::FxChain => "FX Chain",
            Self::FxState => "FX State",
            Self::FxContainers => "FX Containers",
            Self::Automation => "Automation",
            Self::Markers => "Markers",
            Self::Regions => "Regions",
            Self::TempoMap => "Tempo Map",
            Self::Undo => "Undo",
            Self::LiveMidi => "Live MIDI",
            Self::AudioEngine => "Audio Engine",
            Self::Peaks => "Peaks",
            Self::PositionConversion => "Position Conversion",
            Self::ActionRegistry => "Action Registry",
            Self::ExtState => "Ext State",
        };
        f.write_str(name)
    }
}

// =============================================================================
// FeatureSupport
// =============================================================================

/// Declares which capabilities a backend supports for reading and writing.
///
/// This is the primary type that backends expose to describe their compliance
/// level. Consumers can query it to decide whether an operation will succeed
/// before attempting it.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct FeatureSupport {
    /// Capabilities available for reading/querying.
    pub read: Vec<Capability>,
    /// Capabilities available for writing/mutating.
    pub write: Vec<Capability>,
}

impl FeatureSupport {
    /// Create an empty feature support declaration (supports nothing).
    pub fn new() -> Self {
        Self {
            read: Vec::new(),
            write: Vec::new(),
        }
    }

    /// Create a full-support declaration (reads and writes everything).
    pub fn full() -> Self {
        Self {
            read: Capability::ALL.to_vec(),
            write: Capability::ALL.to_vec(),
        }
    }

    /// Add capabilities as both readable and writable.
    pub fn read_write(mut self, caps: &[Capability]) -> Self {
        for &cap in caps {
            if !self.read.contains(&cap) {
                self.read.push(cap);
            }
            if !self.write.contains(&cap) {
                self.write.push(cap);
            }
        }
        self.sort();
        self
    }

    /// Add capabilities as read-only.
    pub fn read_only(mut self, caps: &[Capability]) -> Self {
        for &cap in caps {
            if !self.read.contains(&cap) {
                self.read.push(cap);
            }
        }
        self.sort();
        self
    }

    /// Add capabilities as write-only (rare, but possible for e.g. export-only formats).
    pub fn write_only(mut self, caps: &[Capability]) -> Self {
        for &cap in caps {
            if !self.write.contains(&cap) {
                self.write.push(cap);
            }
        }
        self.sort();
        self
    }

    /// Check if a capability is supported for reading.
    pub fn can_read(&self, cap: Capability) -> bool {
        self.read.contains(&cap)
    }

    /// Check if a capability is supported for writing.
    pub fn can_write(&self, cap: Capability) -> bool {
        self.write.contains(&cap)
    }

    /// Check if a capability is supported for both reading and writing.
    pub fn can_read_write(&self, cap: Capability) -> bool {
        self.can_read(cap) && self.can_write(cap)
    }

    /// Check if all given capabilities are supported for reading.
    pub fn can_read_all(&self, caps: &[Capability]) -> bool {
        caps.iter().all(|c| self.can_read(*c))
    }

    /// Check if all given capabilities are supported for writing.
    pub fn can_write_all(&self, caps: &[Capability]) -> bool {
        caps.iter().all(|c| self.can_write(*c))
    }

    /// Check if this backend satisfies a named profile for reading.
    pub fn satisfies_read(&self, profile: &[Capability]) -> bool {
        self.can_read_all(profile)
    }

    /// Check if this backend satisfies a named profile for writing.
    pub fn satisfies_write(&self, profile: &[Capability]) -> bool {
        self.can_write_all(profile)
    }

    /// List capabilities that are readable but not writable.
    pub fn read_only_caps(&self) -> Vec<Capability> {
        self.read
            .iter()
            .filter(|c| !self.write.contains(c))
            .copied()
            .collect()
    }

    /// List capabilities that are neither readable nor writable.
    pub fn unsupported(&self) -> Vec<Capability> {
        Capability::ALL
            .iter()
            .filter(|c| !self.read.contains(c) && !self.write.contains(c))
            .copied()
            .collect()
    }

    fn sort(&mut self) {
        self.read.sort();
        self.read.dedup();
        self.write.sort();
        self.write.dedup();
    }
}

impl Default for FeatureSupport {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for FeatureSupport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Feature Support:")?;
        for &cap in Capability::ALL {
            let r = if self.can_read(cap) { "R" } else { "-" };
            let w = if self.can_write(cap) { "W" } else { "-" };
            writeln!(f, "  [{r}{w}] {cap}")?;
        }
        Ok(())
    }
}

// =============================================================================
// Named compliance profiles
// =============================================================================

/// Minimum viable timeline import: project metadata + tracks + items.
pub const TIMELINE_BASIC: &[Capability] =
    &[Capability::Project, Capability::Tracks, Capability::Items];

/// Timeline with regions and markers (common interchange target).
pub const TIMELINE_FULL: &[Capability] = &[
    Capability::Project,
    Capability::Tracks,
    Capability::Items,
    Capability::Takes,
    Capability::Markers,
    Capability::Regions,
    Capability::TempoMap,
];

/// Full mix recall: routing, FX, and automation.
pub const MIX_RECALL: &[Capability] = &[
    Capability::Tracks,
    Capability::TrackRouting,
    Capability::FxChain,
    Capability::FxState,
    Capability::Automation,
];

/// Complete DAW control (live backends like REAPER).
pub const FULL: &[Capability] = Capability::ALL;

// =============================================================================
// Integration with DawError
// =============================================================================

impl crate::DawError {
    /// Create a `NotSupported` error for a specific capability.
    pub fn capability_not_supported(cap: Capability) -> Self {
        Self::NotSupported(format!("{cap} is not supported by this backend"))
    }

    /// Create a `NotSupported` error for a write attempt on a read-only capability.
    pub fn read_only(cap: Capability) -> Self {
        Self::NotSupported(format!("{cap} is read-only in this backend"))
    }
}
