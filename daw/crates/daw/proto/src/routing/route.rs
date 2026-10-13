//! Route types for track sends, receives, and hardware outputs

use crate::primitives::AutomationMode;
use crate::track::TrackRef;
use facet::Facet;

/// Type of route
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum RouteType {
    /// Send to another track
    #[default]
    Send = 0,
    /// Receive from another track
    Receive = 1,
    /// Hardware output
    HardwareOutput = 2,
}

/// Reference to a route
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum RouteRef {
    /// Reference by index within the route type
    Index(u32),
    /// Find by destination track
    ByDestination(TrackRef),
}

/// Send mode - when in the signal chain the send is tapped
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum SendMode {
    /// After track fader (default)
    #[default]
    PostFader = 0,
    /// Before FX chain
    PreFx = 1,
    /// After FX chain, before fader
    PostFx = 3,
}

/// Channel mapping for route source/destination
#[derive(Clone, Debug, Default, Facet)]
pub struct ChannelMapping {
    /// Starting channel (0-indexed)
    pub start_channel: u32,
    /// Number of channels
    pub num_channels: u32,
}

/// MIDI source-channel selection for a route.
#[repr(C, u8)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum MidiSourceChannel {
    /// Forward all MIDI channels.
    #[default]
    All,
    /// Forward only a specific 1-based MIDI channel.
    Channel(u8),
}

/// MIDI destination-channel remapping for a route.
#[repr(C, u8)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub enum MidiDestinationChannel {
    /// Preserve the original MIDI channel.
    #[default]
    Original,
    /// Force output to a specific 1-based MIDI channel.
    Channel(u8),
}

/// Typed MIDI channel mapping for REAPER's MIDI send flags.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Facet)]
pub struct MidiChannelMapping {
    /// Source MIDI channel filter.
    pub source: MidiSourceChannel,
    /// Destination MIDI channel remapping.
    pub destination: MidiDestinationChannel,
}

/// Complete route state (send, receive, or hardware output)
#[derive(Clone, Debug, Facet)]
pub struct TrackRoute {
    /// Index of this route within its type
    pub index: u32,
    /// Type of route
    pub route_type: RouteType,
    /// Source track GUID
    pub source_track_guid: String,

    // Destination (None for hardware outputs)
    /// Destination track GUID (None for hardware outputs)
    pub dest_track_guid: Option<String>,
    /// Destination track name (None for hardware outputs)
    pub dest_track_name: Option<String>,

    // Hardware output info (if RouteType::HardwareOutput)
    /// Hardware output index
    pub hw_output_index: Option<u32>,
    /// Hardware output name
    pub hw_output_name: Option<String>,

    // Levels
    /// Route volume (1.0 = 0dB)
    pub volume: f64,
    /// Route pan (-1.0 = left, 0.0 = center, 1.0 = right)
    pub pan: f64,

    // State
    /// Whether the route is muted
    pub muted: bool,
    /// Whether the route is mono (summed)
    pub mono: bool,
    /// Whether phase is inverted
    pub phase_inverted: bool,

    // Mode
    /// Send mode (when tapped in signal chain)
    pub send_mode: SendMode,
    /// Automation mode for this route
    pub automation_mode: AutomationMode,

    // Channels
    /// Source channel mapping
    pub source_channels: ChannelMapping,
    /// Destination channel mapping
    pub dest_channels: ChannelMapping,
    /// MIDI channel mapping, when the route supports MIDI.
    pub midi_channel_mapping: Option<MidiChannelMapping>,
}

impl TrackRoute {
    /// Check if this is a send to another track
    pub fn is_send(&self) -> bool {
        matches!(self.route_type, RouteType::Send)
    }

    /// Check if this is a receive from another track
    pub fn is_receive(&self) -> bool {
        matches!(self.route_type, RouteType::Receive)
    }

    /// Check if this is a hardware output
    pub fn is_hardware_output(&self) -> bool {
        matches!(self.route_type, RouteType::HardwareOutput)
    }

    /// Get the destination name (track name or hardware output name)
    pub fn destination_name(&self) -> Option<&str> {
        self.dest_track_name
            .as_deref()
            .or(self.hw_output_name.as_deref())
    }
}

impl Default for TrackRoute {
    fn default() -> Self {
        Self {
            index: 0,
            route_type: RouteType::Send,
            source_track_guid: String::new(),
            dest_track_guid: None,
            dest_track_name: None,
            hw_output_index: None,
            hw_output_name: None,
            volume: 1.0,
            pan: 0.0,
            muted: false,
            mono: false,
            phase_inverted: false,
            send_mode: SendMode::PostFader,
            automation_mode: AutomationMode::TrimRead,
            source_channels: ChannelMapping::default(),
            dest_channels: ChannelMapping::default(),
            midi_channel_mapping: None,
        }
    }
}
