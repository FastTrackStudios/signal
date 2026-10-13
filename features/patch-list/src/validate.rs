//! The list against the room: one role, one entry; every role
//! resolved or marked unresolved.
//!
//! Validation is the one thing that can refuse a list, and it refuses
//! for exactly one reason: two entries name the same role and the
//! profile does not mark it shared — a double-booked channel, caught
//! before a take (spec #48, story 24). A role the room does not have
//! is not a refusal: the plan carries it as [`Resolved::Unresolved`]
//! so a list written elsewhere opens rather than fails (story 23).

use crate::list::{Bus, Lowered, PatchList};
use crate::profile::{Resolved, StudioProfile};

/// The list, lowered and resolved against one room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Every rig entry, in file order.
    pub entries: Vec<Planned>,
    /// Every headphone bus, in file order.
    pub buses: Vec<PlannedBus>,
}

/// One rig entry with what it comes to in the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub lowered: Lowered,
    pub resolved: Resolved,
}

/// One headphone bus with where it goes in the room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedBus {
    /// The bus name — the key under `headphones`.
    pub name: String,
    pub bus: Bus,
    pub resolved: Resolved,
}

/// Why a list is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Two or more entries name one role the profile does not share.
    #[error("role {role:?} is named by more than one entry ({}) and is not shared", entries.join(", "))]
    DuplicateRole {
        role: String,
        /// The entries' paths, `performer/kind/key` or `headphones/name`.
        entries: Vec<String>,
    },
    /// A MIDI entry names a channel MIDI does not have.
    ///
    /// The schema says `1..16` and the Rust type is a `u8`, which is
    /// wider, so a list built in code (or read past the schema) can
    /// carry a 200. Refused here so the two agree.
    #[error("{entry} names MIDI channel {channel}; MIDI has channels 1 to 16")]
    MidiChannel { entry: String, channel: u8 },
}

/// The channels MIDI has.
const MIDI_CHANNELS: std::ops::RangeInclusive<u8> = 1..=16;

impl Plan {
    /// How many entries and buses the room could not resolve.
    #[must_use]
    pub fn unresolved(&self) -> usize {
        let entries = self
            .entries
            .iter()
            .filter(|e| e.resolved == Resolved::Unresolved)
            .count();
        let buses = self
            .buses
            .iter()
            .filter(|b| b.resolved == Resolved::Unresolved)
            .count();
        entries.saturating_add(buses)
    }
}

/// Lower and resolve a list against a room, refusing a double-booked
/// role.
///
/// # Errors
///
/// [`Error::DuplicateRole`] for the first role (in file order) that
/// more than one entry names without the profile sharing it. Input
/// roles and bus roles are checked alike — an output pair is as
/// double-bookable as a preamp, and `shared` covers both tables — while
/// a MIDI entry names no role and cannot collide. [`Error::MidiChannel`]
/// for a MIDI entry on a channel MIDI does not have.
// r[impl flow.patch-list.plan]
// r[impl flow.patch-list.studio-profiles]
pub fn validate(list: &PatchList, profile: &StudioProfile) -> Result<Plan, Error> {
    for entry in list.entries() {
        if let crate::list::Entry::Midi {
            channel: Some(channel),
            ..
        } = entry.entry
            && !MIDI_CHANNELS.contains(&channel)
        {
            return Err(Error::MidiChannel {
                entry: format!("{}/{}/{}", entry.performer, entry.kind, entry.key),
                channel,
            });
        }
    }
    let entries: Vec<Planned> = list
        .entries()
        .into_iter()
        .map(|lowered| {
            let resolved = match &lowered.entry {
                crate::list::Entry::Role(role) => profile.resolve_input(role),
                crate::list::Entry::Midi { device, channel } => Resolved::Midi {
                    device: device.clone(),
                    channel: *channel,
                },
            };
            Planned { lowered, resolved }
        })
        .collect();
    let buses: Vec<PlannedBus> = list
        .headphones
        .iter()
        .map(|(name, bus)| PlannedBus {
            name: name.clone(),
            bus: bus.clone(),
            resolved: profile.resolve_output(&bus.output),
        })
        .collect();

    let inputs = entries.iter().filter_map(|e| {
        let role = e.lowered.entry.role()?;
        Some((
            role,
            format!(
                "{}/{}/{}",
                e.lowered.performer, e.lowered.kind, e.lowered.key
            ),
        ))
    });
    let outputs = buses
        .iter()
        .map(|b| (b.bus.output.as_str(), format!("headphones/{}", b.name)));
    one_entry_per_role(inputs, profile)?;
    one_entry_per_role(outputs, profile)?;

    Ok(Plan { entries, buses })
}

/// Refuse the first unshared role named more than once.
fn one_entry_per_role<'a>(
    named: impl Iterator<Item = (&'a str, String)>,
    profile: &StudioProfile,
) -> Result<(), Error> {
    let mut seen: Vec<(&str, Vec<String>)> = Vec::new();
    for (role, path) in named {
        match seen.iter_mut().find(|(r, _)| *r == role) {
            Some((_, paths)) => paths.push(path),
            None => seen.push((role, vec![path])),
        }
    }
    if let Some((role, paths)) = seen
        .into_iter()
        .find(|(role, paths)| paths.len() > 1 && !profile.is_shared(role))
    {
        return Err(Error::DuplicateRole {
            role: role.to_owned(),
            entries: paths,
        });
    }
    Ok(())
}
