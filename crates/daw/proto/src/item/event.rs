//! Item and Take events for subscriptions

use super::{Item, Take};
use facet::Facet;

/// Events related to item changes
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum ItemEvent {
    /// An item was created
    Created {
        project_guid: String,
        track_guid: String,
        item: Item,
    },
    /// An item was deleted
    Deleted {
        project_guid: String,
        track_guid: String,
        item_guid: String,
    },
    /// An item's position changed
    PositionChanged {
        project_guid: String,
        item_guid: String,
        old_position: f64,
        new_position: f64,
    },
    /// An item's length changed
    LengthChanged {
        project_guid: String,
        item_guid: String,
        old_length: f64,
        new_length: f64,
    },
    /// An item was moved to a different track
    MovedToTrack {
        project_guid: String,
        item_guid: String,
        old_track_guid: String,
        new_track_guid: String,
    },
    /// An item's mute state changed
    MuteChanged {
        project_guid: String,
        item_guid: String,
        muted: bool,
    },
    /// An item's selection state changed
    SelectionChanged {
        project_guid: String,
        item_guid: String,
        selected: bool,
    },
    /// An item's volume changed
    VolumeChanged {
        project_guid: String,
        item_guid: String,
        volume: f64,
    },
    /// An item's active take changed
    ActiveTakeChanged {
        project_guid: String,
        item_guid: String,
        old_take_index: u32,
        new_take_index: u32,
    },
    /// Some other property of the item changed — fades, snap offset,
    /// colour, lock, label, lane, group, loop/stretch/beat-attach mode,
    /// or several at once. Carries no values: re-read the item
    /// (`Items::get_item`) for its current state. The variants above
    /// still fire for the properties they name; this one covers the
    /// rest, so a subscriber that re-reads on any item event misses
    /// nothing.
    Changed {
        project_guid: String,
        item_guid: String,
    },
}

/// Events related to take changes
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum TakeEvent {
    /// A take was created
    Created {
        project_guid: String,
        item_guid: String,
        take: Take,
    },
    /// A take was deleted
    Deleted {
        project_guid: String,
        item_guid: String,
        take_guid: String,
    },
    /// A take's name changed
    NameChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        name: String,
    },
    /// A take's pitch changed
    PitchChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        pitch: f64,
    },
    /// A take's play rate changed
    PlayRateChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        play_rate: f64,
    },
    /// A take's volume changed
    VolumeChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        volume: f64,
    },
    /// A take's source changed
    SourceChanged {
        project_guid: String,
        item_guid: String,
        take_guid: String,
        source_path: Option<String>,
    },
    /// Some other property of the take changed — colour, start offset,
    /// preserve-pitch, or its take markers. Carries no values: re-read the
    /// take (`Takes::get_take`, and `Takes::get_take_markers` for the
    /// markers). The variants above still fire for the properties they
    /// name.
    Changed {
        project_guid: String,
        item_guid: String,
        take_guid: String,
    },
}

impl ItemEvent {
    /// The project the event is about.
    pub fn project_guid(&self) -> &str {
        match self {
            Self::Created { project_guid, .. }
            | Self::Deleted { project_guid, .. }
            | Self::PositionChanged { project_guid, .. }
            | Self::LengthChanged { project_guid, .. }
            | Self::MovedToTrack { project_guid, .. }
            | Self::MuteChanged { project_guid, .. }
            | Self::SelectionChanged { project_guid, .. }
            | Self::VolumeChanged { project_guid, .. }
            | Self::ActiveTakeChanged { project_guid, .. }
            | Self::Changed { project_guid, .. } => project_guid,
        }
    }

    /// The item the event is about.
    pub fn item_guid(&self) -> &str {
        match self {
            Self::Created { item, .. } => &item.guid,
            Self::Deleted { item_guid, .. }
            | Self::PositionChanged { item_guid, .. }
            | Self::LengthChanged { item_guid, .. }
            | Self::MovedToTrack { item_guid, .. }
            | Self::MuteChanged { item_guid, .. }
            | Self::SelectionChanged { item_guid, .. }
            | Self::VolumeChanged { item_guid, .. }
            | Self::ActiveTakeChanged { item_guid, .. }
            | Self::Changed { item_guid, .. } => item_guid,
        }
    }
}

impl TakeEvent {
    /// The project the event is about.
    pub fn project_guid(&self) -> &str {
        match self {
            Self::Created { project_guid, .. }
            | Self::Deleted { project_guid, .. }
            | Self::NameChanged { project_guid, .. }
            | Self::PitchChanged { project_guid, .. }
            | Self::PlayRateChanged { project_guid, .. }
            | Self::VolumeChanged { project_guid, .. }
            | Self::SourceChanged { project_guid, .. }
            | Self::Changed { project_guid, .. } => project_guid,
        }
    }

    /// The item whose take the event is about.
    pub fn item_guid(&self) -> &str {
        match self {
            Self::Created { item_guid, .. }
            | Self::Deleted { item_guid, .. }
            | Self::NameChanged { item_guid, .. }
            | Self::PitchChanged { item_guid, .. }
            | Self::PlayRateChanged { item_guid, .. }
            | Self::VolumeChanged { item_guid, .. }
            | Self::SourceChanged { item_guid, .. }
            | Self::Changed { item_guid, .. } => item_guid,
        }
    }
}
