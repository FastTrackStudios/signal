//! FX data types
//!
//! Types for representing audio effects (plugins) and their parameters.

use facet::Facet;

/// Context specifying which FX chain to operate on
///
/// FX can exist in different locations within a DAW:
/// - Track output chain (normal FX processing)
/// - Track input chain (recording FX)
/// - Monitoring chain (global monitoring FX)
#[repr(u8)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Facet)]
pub enum FxChainContext {
    /// Normal track FX (output/playback chain)
    Track(String), // track GUID
    /// Input/recording FX chain
    Input(String), // track GUID
    /// Monitoring FX (global, not per-track)
    Monitoring,
}

impl FxChainContext {
    /// Create a track FX chain context
    pub fn track(guid: impl Into<String>) -> Self {
        Self::Track(guid.into())
    }

    /// Create an input FX chain context
    pub fn input(guid: impl Into<String>) -> Self {
        Self::Input(guid.into())
    }

    /// Create a monitoring FX chain context
    pub fn monitoring() -> Self {
        Self::Monitoring
    }
}

/// Reference to an FX - how to identify an FX for operations
///
/// FX can be identified by GUID (stable), index (position-based),
/// or name (first match).
#[repr(u8)]
#[derive(Clone, Debug, Facet)]
pub enum FxRef {
    /// FX GUID - stable across sessions
    Guid(String),
    /// FX index (0-based position in chain)
    Index(u32),
    /// FX name - matches first FX with this name
    Name(String),
}

impl FxRef {
    /// Create a reference by GUID
    pub fn guid(guid: impl Into<String>) -> Self {
        Self::Guid(guid.into())
    }

    /// Create a reference by index
    pub fn index(index: u32) -> Self {
        Self::Index(index)
    }

    /// Create a reference by name
    pub fn name(name: impl Into<String>) -> Self {
        Self::Name(name.into())
    }
}

impl From<u32> for FxRef {
    fn from(index: u32) -> Self {
        Self::Index(index)
    }
}

impl From<&str> for FxRef {
    fn from(name: &str) -> Self {
        Self::Name(name.to_string())
    }
}

impl From<String> for FxRef {
    fn from(name: String) -> Self {
        Self::Name(name)
    }
}

/// An installed FX plugin known to the DAW.
///
/// Returned by `FxService::list_installed_fx` — includes every plugin
/// REAPER has discovered (VST2, VST3, CLAP, AU, JS, etc.) with its
/// display name and the full identifier string needed for `add_fx`.
#[derive(Clone, Debug, Facet)]
pub struct InstalledFx {
    /// Human-readable display name (e.g. "Pro-Q 4")
    pub name: String,
    /// Full identifier string (e.g. "CLAP: Pro-Q 4 (FabFilter)")
    pub ident: String,
}

/// FX plugin type
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum FxType {
    /// VST2 plugin
    Vst2,
    /// VST3 plugin
    Vst3,
    /// Audio Unit (macOS)
    Au,
    /// REAPER JS effect
    Js,
    /// CLAP plugin
    Clap,
    /// Unknown or other type
    #[default]
    Unknown,
}

/// Complete FX state returned from queries
// PartialEq so a Vec<Fx> can be a Dioxus prop, like Track and Item.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Fx {
    /// Unique GUID for stable identification
    pub guid: String,
    /// Index in the FX chain (0-based)
    pub index: u32,
    /// Display name (may include preset name)
    pub name: String,
    /// Plugin name (e.g., "ReaComp", "Pro-C 2")
    pub plugin_name: String,
    /// Plugin type
    pub plugin_type: FxType,
    /// Whether the FX is enabled (not bypassed)
    pub enabled: bool,
    /// Whether the FX is offline (not processing)
    pub offline: bool,
    /// Whether the FX UI window is open
    pub window_open: bool,
    /// Number of parameters
    pub parameter_count: u32,
    /// Current preset name, if any
    pub preset_name: Option<String>,
}

impl Fx {
    /// Create a new FX with minimal info
    pub fn new(guid: String, index: u32, name: String) -> Self {
        Self {
            guid,
            index,
            name: name.clone(),
            plugin_name: name,
            plugin_type: FxType::Unknown,
            enabled: true,
            offline: false,
            window_open: false,
            parameter_count: 0,
            preset_name: None,
        }
    }

    /// Get an FxRef for this FX by GUID
    pub fn as_ref(&self) -> FxRef {
        FxRef::Guid(self.guid.clone())
    }

    /// Get an FxRef for this FX by index
    pub fn as_index_ref(&self) -> FxRef {
        FxRef::Index(self.index)
    }
}

impl Default for Fx {
    fn default() -> Self {
        Self::new(String::new(), 0, String::new())
    }
}

/// FX Parameter state
#[derive(Clone, Debug, Facet)]
pub struct FxParameter {
    /// Parameter index (0-based)
    pub index: u32,
    /// Parameter name
    pub name: String,
    /// Current value (normalized 0.0-1.0)
    pub value: f64,
    /// Formatted display value (e.g., "-12.5 dB", "50%")
    pub formatted: String,
    /// Whether this is a toggle/boolean parameter
    pub is_toggle: bool,
    /// Number of discrete steps (None = continuous, Some(n) = dropdown with n choices)
    pub step_count: Option<u32>,
    /// Labels for each discrete step (empty for continuous params).
    /// Each entry is (normalized_value, display_label).
    pub step_labels: Vec<(f64, String)>,
}

impl FxParameter {
    /// Create a new parameter
    pub fn new(index: u32, name: String, value: f64) -> Self {
        Self {
            index,
            name,
            value,
            formatted: format!("{:.2}", value),
            is_toggle: false,
            step_count: None,
            step_labels: Vec::new(),
        }
    }
}

impl Default for FxParameter {
    fn default() -> Self {
        Self::new(0, String::new(), 0.0)
    }
}

/// FX latency/PDC information
#[derive(Clone, Debug, Default, Facet)]
pub struct FxLatency {
    /// Plugin-reported PDC in samples
    pub pdc_samples: i32,
    /// Actual PDC being applied in the chain
    pub chain_pdc_actual: i32,
    /// PDC being reported to the host
    pub chain_pdc_reporting: i32,
}

/// Parameter modulation state (LFO, linking)
#[derive(Clone, Debug, Default, Facet)]
pub struct FxParamModulation {
    /// Whether LFO modulation is active
    pub lfo_active: bool,
    /// LFO speed
    pub lfo_speed: f64,
    /// LFO modulation strength/depth
    pub lfo_strength: f64,
    /// Whether parameter linking is active
    pub link_active: bool,
    /// Linked FX index (-1 if not linked)
    pub link_fx_index: i32,
    /// Linked parameter index (-1 if not linked)
    pub link_param_index: i32,
}

// =============================================================================
// Request Types (for service methods with many parameters)
// =============================================================================

/// Target for FX operations - combines chain context and FX reference
#[derive(Clone, Debug, Facet)]
pub struct FxTarget {
    /// Which FX chain to operate on
    pub context: FxChainContext,
    /// Which FX in the chain
    pub fx: FxRef,
}

impl FxTarget {
    /// Create a new FX target
    pub fn new(context: FxChainContext, fx: impl Into<FxRef>) -> Self {
        Self {
            context,
            fx: fx.into(),
        }
    }

    /// Create a target for track FX chain
    pub fn track(track_guid: impl Into<String>, fx: impl Into<FxRef>) -> Self {
        Self::new(FxChainContext::track(track_guid), fx)
    }

    /// Create a target for input FX chain
    pub fn input(track_guid: impl Into<String>, fx: impl Into<FxRef>) -> Self {
        Self::new(FxChainContext::input(track_guid), fx)
    }
}

/// Request to set a parameter by index
#[derive(Clone, Debug, Facet)]
pub struct SetParameterRequest {
    /// Target FX
    pub target: FxTarget,
    /// Parameter index
    pub index: u32,
    /// New value (normalized 0.0-1.0)
    pub value: f64,
}

/// Request to set a parameter by name
#[derive(Clone, Debug, Facet)]
pub struct SetParameterByNameRequest {
    /// Target FX
    pub target: FxTarget,
    /// Parameter name
    pub name: String,
    /// New value (normalized 0.0-1.0)
    pub value: f64,
}

/// Request to set a named config parameter
#[derive(Clone, Debug, Facet)]
pub struct SetNamedConfigRequest {
    /// Target FX
    pub target: FxTarget,
    /// Config key
    pub key: String,
    /// Config value
    pub value: String,
}

/// Request to add FX at a specific position
#[derive(Clone, Debug, Facet)]
pub struct AddFxAtRequest {
    /// Target chain context
    pub context: FxChainContext,
    /// Plugin name to add
    pub name: String,
    /// Position in chain
    pub index: u32,
}

// =============================================================================
// Container Request Types
// =============================================================================

/// Request to create a new container in an FX chain.
#[derive(Clone, Debug, Facet)]
pub struct CreateContainerRequest {
    /// Which FX chain to create the container in.
    pub context: FxChainContext,
    /// Display name for the container (e.g., "DRIVE", "PRE-FX").
    pub name: String,
    /// Position in the chain to insert the container (0-based).
    pub index: u32,
}

/// Request to move an FX node into a container.
#[derive(Clone, Debug, Facet)]
pub struct MoveToContainerRequest {
    /// Which FX chain the operation is in.
    pub context: FxChainContext,
    /// The node to move.
    pub node_id: super::FxNodeId,
    /// The target container to move the node into.
    pub container_id: super::FxNodeId,
    /// Position within the container's children (0-based).
    pub child_index: u32,
}

/// Request to move an FX node out of its container to a parent-level position.
#[derive(Clone, Debug, Facet)]
pub struct MoveFromContainerRequest {
    /// Which FX chain the operation is in.
    pub context: FxChainContext,
    /// The node to move out.
    pub node_id: super::FxNodeId,
    /// Position in the parent level to insert at (0-based).
    pub target_index: u32,
}

/// Request to enclose one or more FX nodes in a new container.
#[derive(Clone, Debug, Facet)]
pub struct EncloseInContainerRequest {
    /// Which FX chain the operation is in.
    pub context: FxChainContext,
    /// Node IDs to enclose (must be siblings at the same level).
    pub node_ids: Vec<super::FxNodeId>,
    /// Display name for the new container.
    pub name: String,
}

/// Request to set the channel configuration for a container.
#[derive(Clone, Debug, Facet)]
pub struct SetContainerChannelConfigRequest {
    /// Which FX chain the operation is in.
    pub context: FxChainContext,
    /// The container node.
    pub container_id: super::FxNodeId,
    /// New channel configuration.
    pub config: super::FxContainerChannelConfig,
}

// =============================================================================
// Preset Types
// =============================================================================

/// Current preset position and total count for an FX plugin.
#[derive(Clone, Debug, Default, Facet)]
pub struct FxPresetIndex {
    /// Current preset index (None if factory/no preset active).
    pub index: Option<u32>,
    /// Total number of presets available.
    pub count: u32,
    /// Current preset name (None if unnamed or factory default).
    pub name: Option<String>,
}

// =============================================================================
// FX Channel Config Types
// =============================================================================

/// Per-FX channel configuration (non-container).
///
/// REAPER's `channel_config` named config param returns 3 values:
/// - requested channel count (0 = VST3 auto)
/// - channel mode (0=multichannel, 1=multi-mono, 2=multi-stereo)
/// - supported flags (&1=multichannel, &2=auto, &4=multi-mono, &8=multi-stereo)
///
/// Writing accepts 1 or 2 values: channel count, and optionally channel mode.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct FxChannelConfig {
    /// Requested channel count (0 = VST3 auto, 2 = stereo, etc.)
    pub channel_count: u32,
    /// Channel mode: 0=multichannel, 1=multi-mono, 2=multi-stereo
    pub channel_mode: u32,
    /// Supported flags (read-only): &1=multichannel, &2=auto, &4=multi-mono, &8=multi-stereo
    pub supported_flags: u32,
}

impl FxChannelConfig {
    /// Standard stereo configuration.
    pub fn stereo() -> Self {
        Self {
            channel_count: 2,
            channel_mode: 0,
            supported_flags: 0,
        }
    }

    /// Silent configuration — zero output channels for gapless loading.
    pub fn silent() -> Self {
        Self {
            channel_count: 0,
            channel_mode: 0,
            supported_flags: 0,
        }
    }
}

// =============================================================================
// Pin Mapping Types
// =============================================================================

/// Saved output pin mappings for a single FX.
///
/// Captures the per-pin bitmasks so they can be restored after silencing.
/// Each entry is `(pin_index, low32, high32)` — the 64-bit channel bitmask
/// split into two 32-bit halves, matching REAPER's `TrackFX_GetPinMappings` format.
///
/// Pin mappings at the "second bank" level (pin + 0x1000000) are also captured
/// when non-zero, stored with the original pin index + 0x1000000 offset.
#[derive(Clone, Debug, Default, Facet)]
pub struct FxPinMappings {
    /// Output pin mappings: `(pin_index, low32_bits, high32_bits)`.
    /// Only non-zero mappings are stored.
    pub output_pins: Vec<(i32, i32, i32)>,
}

// =============================================================================
// State Chunk Types
// =============================================================================

/// Information about the last touched FX parameter.
///
/// Returned by `FxService::get_last_touched_fx` — identifies which FX parameter
/// was most recently adjusted by the user in the DAW UI.
#[derive(Clone, Debug, Facet)]
pub struct LastTouchedFx {
    /// GUID of the track containing the FX.
    pub track_guid: String,
    /// Index of the FX in the chain (0-based).
    pub fx_index: u32,
    /// Index of the parameter that was touched (0-based).
    pub param_index: u32,
    /// Whether the FX is in the input/recording FX chain (vs normal output chain).
    pub is_input_fx: bool,
}

/// Captured binary state of a single FX plugin.
///
/// Stores the FX GUID for matching on restore and the base64-encoded
/// binary chunk. This follows the Track Snapshot pattern of capturing
/// complete plugin state for full-fidelity recall.
#[derive(Clone, Debug, Facet)]
pub struct FxStateChunk {
    /// GUID of the FX instance this chunk belongs to.
    pub fx_guid: String,
    /// Index of the FX in the chain at capture time (for ordering).
    pub fx_index: u32,
    /// Plugin name at capture time (for display/diagnostics).
    pub plugin_name: String,
    /// Base64-encoded binary plugin state.
    ///
    /// This is the raw VST/CLAP state chunk that fully describes the
    /// plugin's internal state, including settings not exposed as
    /// automation parameters.
    pub encoded_chunk: String,
}
