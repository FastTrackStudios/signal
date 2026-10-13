//! The gangs: which tracks move together, and why.
//!
//! Two gangs, and they are not the same set. A performer's DI across
//! three parts is one *rig* gang — change the DI input on Cody's row
//! and every one of Cody's DI tracks takes it. The Main and DBL
//! channels of one part are one *layer* gang — arm one and the double
//! arms with it, so a take is never half-recorded.
//!
//! Both are **projections of the taxonomy**, not stored state. That is
//! what lets the watcher rebuild them on every track event without a
//! ledger to drift: the same facts always yield the same gangs.
//!
//! What rides a gang is booleans only — arm, input monitoring, record
//! input. Faders need playback-time, automation-aware scaling that only
//! a REAPER VCA gives, so they are [`super::vca`]'s job, not this one.

use std::collections::BTreeMap;

use crate::scenes::Fact;

/// What a gang is keyed by — kept as a value so a gang can be named in
/// a log line or a test failure without reconstructing it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GangKey {
    /// One performer's tracks of one source kind, across every part and
    /// channel they play: `flow.scenes.performer-rig`.
    Rig {
        performer: String,
        source_kind: String,
    },
    /// The channels of one layer — a double's L and R, a triple's three.
    Layer {
        performer: Option<String>,
        arrangement: Option<String>,
        layer: String,
    },
}

/// A set of tracks that move together, in project order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gang {
    pub key: GangKey,
    /// Member guids, in the order the facts gave them — project order,
    /// so a diff reads top-down the way the window does.
    pub members: Vec<String>,
}

/// Every gang the facts imply.
///
/// A gang of one is dropped: it can never move anything, and keeping it
/// would make every unpaired track look like a group in the logs.
#[must_use]
pub fn gangs(facts: &[Fact]) -> Vec<Gang> {
    let mut rig: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut layer: BTreeMap<(Option<String>, Option<String>, String), Vec<String>> =
        BTreeMap::new();

    for fact in facts.iter().filter(|f| !f.is_folder) {
        if let (Some(performer), Some(kind)) = (fact.performer.as_ref(), fact.multi_mic.as_ref()) {
            rig.entry((performer.clone(), kind.clone()))
                .or_default()
                .push(fact.guid.clone());
        }
        // A layer gang is the channels *of* one layer, so a track with
        // no channel dimension is not in one — it is the layer itself,
        // undoubled, and has nothing to move with.
        if let (Some(name), Some(_)) = (fact.layer.as_ref(), fact.channel.as_ref()) {
            layer
                .entry((
                    fact.performer.clone(),
                    fact.arrangement.clone(),
                    name.clone(),
                ))
                .or_default()
                .push(fact.guid.clone());
        }
    }

    let mut out = Vec::new();
    for ((performer, source_kind), members) in rig {
        if members.len() > 1 {
            out.push(Gang {
                key: GangKey::Rig {
                    performer,
                    source_kind,
                },
                members,
            });
        }
    }
    for ((performer, arrangement, name), members) in layer {
        if members.len() > 1 {
            out.push(Gang {
                key: GangKey::Layer {
                    performer,
                    arrangement,
                    layer: name,
                },
                members,
            });
        }
    }
    out
}

/// The writes that bring a gang to `wanted`, given what each member is
/// already at.
///
/// **This is the echo guard.** `TrackEvent` carries no origin and the
/// backend's own writes come back through the same 30 Hz poll diff, so
/// the watcher cannot tell its echo from a user's gesture. It does not
/// have to: it computes the desired state and emits only the members
/// that differ, so an echo produces an empty write set and stops there.
#[must_use]
pub fn diff<'a>(gang: &'a Gang, wanted: bool, current: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    gang.members
        .iter()
        .map(String::as_str)
        .filter(|guid| current(guid) != wanted)
        .collect()
}
