//! Port description + selection types, and the timestamped event that streams
//! across the I/O boundary. All derive [`facet::Facet`] so they ride the vox
//! wire that `#[architect::rpc]` generates.

use facet::Facet;

use crate::event::MidiEvent;

/// A stable-ish identifier for a physical or virtual port, as the backend
/// reports it. Opaque to `midicore`; adapters define its meaning.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Facet)]
pub struct PortId(pub String);

/// Whether a port carries data in or out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Direction {
    Input,
    Output,
}

/// What a backend knows about a port before you open it.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct PortInfo {
    pub id: PortId,
    pub name: String,
    pub direction: Direction,
    /// True if the backend created this port (a virtual endpoint), rather than
    /// it belonging to external hardware/software.
    pub virtual_port: bool,
}

/// How the caller chooses a port to open — the ergonomic front door. Mirrors
/// the device/all/virtual selection real rigs need.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PortSelector {
    /// The backend's default/first port for the direction.
    Default,
    /// Exact [`PortId`] match.
    Id(PortId),
    /// First port whose name contains this substring (case-insensitive).
    NameContains(String),
    /// Merge all ports of the direction into one stream (inputs) / fan out to
    /// all (outputs).
    All,
    /// Create a virtual port with this name (backends that support it).
    Virtual(String),
}

impl PortSelector {
    /// Does this selector want the source port called `port`?
    ///
    /// The one matching rule every input backend applies, so a port name
    /// stored in a rig preset selects the same device on each of them:
    /// `All`/`Default` want everything, `Id` is the whole name, `NameContains`
    /// is a case-insensitive substring (empty = everything). `Virtual` names a
    /// port the backend *creates* for others to connect to, so it never
    /// selects an existing source.
    #[must_use]
    pub fn matches(&self, port: &str) -> bool {
        match self {
            Self::All | Self::Default => true,
            Self::Id(PortId(id)) => port == id,
            Self::NameContains(needle) => {
                needle.is_empty() || port.to_lowercase().contains(&needle.to_lowercase())
            }
            Self::Virtual(_) => false,
        }
    }
}

/// A MIDI event tagged with the backend's capture timestamp (microseconds,
/// monotonic; epoch is backend-defined). This is the streamed unit.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct TimedEvent {
    pub timestamp_us: u64,
    pub event: MidiEvent,
}

/// Errors an I/O backend can surface. Adapters map their native errors onto
/// these; `Other` carries an adapter-specific message.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum MidiIoError {
    /// No port matched the [`PortSelector`].
    PortNotFound,
    /// The backend does not support the requested operation (e.g. virtual
    /// ports on a platform that lacks them).
    Unsupported,
    /// The connection was closed or the device disappeared.
    Disconnected,
    /// Adapter-specific failure.
    Other(String),
}

#[cfg(test)]
mod selector_tests {
    use super::{PortId, PortSelector};

    const S88: &str = "Midi-Bridge:KONTROL S88 MK3: Main (capture)";

    #[test]
    fn omni_wants_every_device() {
        assert!(PortSelector::All.matches(S88));
        assert!(PortSelector::NameContains(String::new()).matches(S88));
    }

    /// Case-insensitive substring — the rule stored rig presets were written
    /// against, so they select the same device on every backend.
    #[test]
    fn a_named_selector_matches_a_substring_case_insensitively() {
        assert!(PortSelector::NameContains("kontrol s88".into()).matches(S88));
        assert!(!PortSelector::NameContains("mioXM".into()).matches(S88));
    }

    #[test]
    fn an_id_selector_matches_the_whole_name_only() {
        assert!(PortSelector::Id(PortId(S88.into())).matches(S88));
        assert!(!PortSelector::Id(PortId("KONTROL".into())).matches(S88));
    }

    #[test]
    fn a_virtual_selector_selects_no_existing_source() {
        assert!(!PortSelector::Virtual("Signal".into()).matches(S88));
    }
}
