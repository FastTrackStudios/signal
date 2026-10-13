//! The studio profile: `studios/<name>.styx` in the app config
//! directory.
//!
//! A profile is the room's, not the album's (decision #29): the physical
//! inputs a location has and what is patched to them. It owns the role
//! vocabulary — an `inputs` table of role → device channel (or MIDI
//! device and channel), an `outputs` table of bus role → output pair —
//! and the patch list names roles from it, so the same list tracks the
//! same album in two rooms with two profiles and nothing in it changes.
//!
//! An optional patchbay endpoint on a role is what `crates/patchbay`
//! builds and verifies the profile against; resolving a role never
//! needs it.

use facet::Facet;
use indexmap::IndexMap;

/// One room.
// r[impl flow.patch-list.studio-profiles]
#[derive(Facet, Debug, Clone, Default, PartialEq, Eq)]
pub struct StudioProfile {
    /// Input role → what the device has there.
    #[facet(default)]
    pub inputs: IndexMap<String, Input>,
    /// Bus role → the output pair it goes to.
    #[facet(default)]
    pub outputs: IndexMap<String, Output>,
    /// Roles more than one entry may name — a talkback mic that is on
    /// two rigs. Any other role named twice is a validation error.
    #[facet(default)]
    pub shared: Vec<String>,
}

/// What an input role is on this room's device.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Input {
    /// A hardware input, by 0-based device channel — the daw's
    /// `RecordInput::Audio { channel }`.
    Audio {
        channel: u32,
        #[facet(default)]
        patchbay: Option<Endpoint>,
    },
    /// A MIDI device, on one channel or (absent) every channel.
    Midi {
        device: String,
        #[facet(default)]
        channel: Option<u8>,
        #[facet(default)]
        patchbay: Option<Endpoint>,
    },
}

/// Where a bus role goes: a 0-based output pair.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub left: u32,
    pub right: u32,
    #[facet(default)]
    pub patchbay: Option<Endpoint>,
}

/// A port on the live graph, as patchbay names it.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// The node (a device, an application).
    pub node: String,
    /// The port on it.
    pub port: String,
}

/// What a role comes to in this room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A hardware input channel.
    Audio { channel: u32 },
    /// A MIDI device and channel.
    Midi { device: String, channel: Option<u8> },
    /// A hardware output pair.
    Pair { left: u32, right: u32 },
    /// The profile has no such role. Shown, never failed on: a list
    /// written in another room opens here.
    Unresolved,
}

impl StudioProfile {
    /// Parse a profile.
    ///
    /// # Errors
    ///
    /// When the text is not a profile in this shape.
    pub fn from_styx(text: &str) -> Result<Self, facet_styx::DeserializeError> {
        crate::styx::read(text)
    }

    /// Write the profile back as styx, absent options omitted.
    ///
    /// # Errors
    ///
    /// When the value cannot be serialized — which a profile built from
    /// these types cannot fail at.
    pub fn to_styx(&self) -> Result<String, crate::styx::WriteError> {
        crate::styx::write(self)
    }

    /// An input role, resolved.
    // r[impl flow.patch-list.studio-profiles]
    #[must_use]
    pub fn resolve_input(&self, role: &str) -> Resolved {
        match self.inputs.get(role) {
            Some(Input::Audio { channel, .. }) => Resolved::Audio { channel: *channel },
            Some(Input::Midi {
                device, channel, ..
            }) => Resolved::Midi {
                device: device.clone(),
                channel: *channel,
            },
            None => Resolved::Unresolved,
        }
    }

    /// A bus role, resolved.
    #[must_use]
    pub fn resolve_output(&self, role: &str) -> Resolved {
        match self.outputs.get(role) {
            Some(Output { left, right, .. }) => Resolved::Pair {
                left: *left,
                right: *right,
            },
            None => Resolved::Unresolved,
        }
    }

    /// Whether more than one entry may name this role.
    #[must_use]
    pub fn is_shared(&self, role: &str) -> bool {
        self.shared.iter().any(|shared| shared == role)
    }
}
