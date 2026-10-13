//! Headphone buses: who hears what, planned before anything is written.
//!
//! Every performer gets a bus, and so do the engineer, the producer and
//! the broadcast feed. They are declared in the album's patch list, so
//! **apply creates them** — and never deletes one, because a bus
//! someone set up by hand is not ours to remove.
//!
//! # Why a send is the assignment
//!
//! A track routed to a performer's cue mix **is** that performer's.
//! That makes the send the record of who played what: the organizer
//! sorts by it, a track can be renamed into the Performer dimension
//! from it, and "can I have more of myself" works for everyone on every
//! instrument, because everyone's tracks are already on their own bus.
//! Where a track carries a Performer dimension too, the name wins and
//! the send is rewritten to match — so the two can never disagree.
//!
//! # Why "me" is at track level and "the band" at bus level
//!
//! A performer's own sources send into their bus **directly**, which is
//! what makes "more of me" one gesture over a known set. The rest of
//! the band arrives as the **instrument buses** at unity, because a cue
//! mix built from sixty individual sends is not a mix anyone can ride.
//! The master never sends anywhere: a cue fed from the master is a
//! feedback loop the moment the cue is in the master.

use std::collections::BTreeMap;

use crate::list::{Bus, PatchList};
use crate::profile::StudioProfile;

/// The folder every cue bus lives under, so a scene can fold them all
/// at once and the classifier reads them back as monitor paths.
pub const MONITOR_FOLDER: &str = "HEADPHONE MIXES";

/// What a bus is for, which decides what it receives.
///
/// This is not decoration. **Broadcast is the one feed that must never
/// carry the click or the control room's chatter** — it goes out of the
/// building — so it is a kind rather than a name to special-case at
/// each send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Someone playing: their own tracks, the band, the guide.
    Performer,
    /// The control room: the mix and the talkbacks.
    Engineer,
    /// The mix, and nothing that would embarrass anyone.
    Broadcast,
}

/// A cue bus the project should have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CueBus {
    /// The track name, `HP` prefixed so the existing classifier reads
    /// it as a monitor path and keeps it out of the mix.
    pub track: String,
    /// The profile's output role — unresolved here on purpose, so a
    /// plan can be made in a room whose profile is missing.
    pub output: String,
    /// Who listens. More than one is a shared bus (a choir), and their
    /// identities stay their own.
    pub listeners: Vec<String>,
    pub audience: Audience,
}

impl CueBus {
    /// Whether this bus takes the guide (click, cues, shaker).
    ///
    /// Everyone playing needs the count. The engineer does not need it
    /// in their cue, and broadcast must not have it.
    #[must_use]
    pub const fn takes_guide(&self) -> bool {
        matches!(self.audience, Audience::Performer)
    }

    /// Whether this bus takes the talkback mics.
    ///
    /// The control room hears the room. Broadcast never does.
    #[must_use]
    pub const fn takes_talkback(&self) -> bool {
        matches!(self.audience, Audience::Engineer)
    }
}

/// Read a bus's audience from who it is for.
fn audience_of(name: &str, bus: &Bus) -> Audience {
    let says = |needle: &str| {
        name.eq_ignore_ascii_case(needle)
            || bus.audience.iter().any(|a| a.eq_ignore_ascii_case(needle))
    };
    if says("broadcast") {
        Audience::Broadcast
    } else if says("engineer") || says("producer") {
        Audience::Engineer
    } else {
        Audience::Performer
    }
}

/// The track name for a bus.
///
/// `HP` is not decoration either: `dynamic_template::groups::headphones`
/// already classifies that prefix as a monitor path, which is what keeps
/// a cue bus out of `MIX BUS` — the mistake that printed somebody's
/// foldback into ten of eleven album projects before the group existed.
fn track_name(key: &str) -> String {
    let mut chars = key.chars();
    let titled = chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    });
    format!("HP {titled}")
}

/// Every cue bus the list declares, in the list's own order.
///
/// The profile is taken but not required: an output role it does not
/// know stays as written, so a plan made for one room opens in another
/// with its unresolved roles visible rather than failing.
#[must_use]
pub fn plan(list: &PatchList, _profile: Option<&StudioProfile>) -> Vec<CueBus> {
    list.headphones
        .iter()
        .map(|(key, bus)| CueBus {
            track: track_name(key),
            output: bus.output.clone(),
            listeners: if bus.audience.is_empty() {
                vec![key.clone()]
            } else {
                bus.audience.clone()
            },
            audience: audience_of(key, bus),
        })
        .collect()
}

/// Which bus a performer listens to.
///
/// A shared bus (a choir section) answers for each of its listeners, so
/// twenty singers need one bus and keep twenty identities.
#[must_use]
pub fn bus_for<'a>(buses: &'a [CueBus], performer: &str) -> Option<&'a CueBus> {
    buses.iter().find(|bus| {
        bus.listeners
            .iter()
            .any(|listener| listener.eq_ignore_ascii_case(performer))
    })
}

/// The cue sends a set of performer-owned tracks needs: every track to
/// its own performer's bus, once.
///
/// Returns `(track, bus track name)` pairs. A performer with no bus is
/// skipped and reported by the caller rather than silently routed
/// somewhere plausible.
#[must_use]
pub fn sends<'a>(
    buses: &'a [CueBus],
    owned: &'a BTreeMap<String, String>,
) -> Vec<(&'a str, &'a str)> {
    owned
        .iter()
        .filter_map(|(track, performer)| {
            bus_for(buses, performer).map(|bus| (track.as_str(), bus.track.as_str()))
        })
        .collect()
}
