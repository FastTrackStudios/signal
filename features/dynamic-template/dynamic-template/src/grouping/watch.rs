//! Applying the gangs: the thin half.
//!
//! Everything decided lives in [`super::gangs`]; this only reads the
//! current state, asks for the diff, and writes it. It is generic over
//! [`Tracks`] rather than written against a backend, so the same code
//! runs in-process against daw-standalone and inside REAPER — the map's
//! rule that the window is a control surface over REAPER's data model,
//! with no REAPER-only path to test separately.
//!
//! It holds **no state**. That is not tidiness: the gangs are a
//! projection of the facts, so there is nothing to keep, and nothing
//! that can drift out of step with a project someone edited elsewhere.

use daw_proto::{DawResult, ProjectContext, TrackRef, Tracks};

use super::{diff, gangs, Gang};
use crate::scenes::Fact;

/// What one gesture propagated, for the span that reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Followed {
    /// Members written — the ones that actually differed.
    pub wrote: usize,
    /// Gangs the changed track belonged to. A track can be in two: its
    /// rig and its layer.
    pub gangs: usize,
}

/// The gangs a track belongs to.
fn containing<'a>(all: &'a [Gang], guid: &str) -> Vec<&'a Gang> {
    all.iter()
        .filter(|gang| gang.members.iter().any(|m| m == guid))
        .collect()
}

/// Arm followed the gang: arming one of a performer's DI tracks arms
/// the rest, and arming one channel of a layer arms its siblings.
///
/// Returns what it wrote. An echo writes nothing and says so, which is
/// how a caller can tell a real gesture from its own reflection without
/// an origin field on the event.
///
/// # Errors
///
/// When a member's arm cannot be written. The caller sees the first
/// failure rather than a partly-armed gang reported as a success: half
/// a gang armed is the state this exists to prevent.
///
/// r[impl flow.scenes.performer-rig]
/// r[impl flow.scenes.groups]
pub fn follow_arm<D: Tracks + ?Sized>(
    daw: &D,
    project: &ProjectContext,
    facts: &[Fact],
    guid: &str,
    armed: bool,
) -> DawResult<Followed> {
    let all = gangs(facts);
    let mine = containing(&all, guid);
    let current = |g: &str| {
        daw.get(project.clone(), TrackRef::Guid(g.to_owned()))
            .is_some_and(|track| track.armed)
    };

    let mut wrote: usize = 0;
    for gang in &mine {
        for member in diff(gang, armed, &current) {
            daw.set_armed(project.clone(), TrackRef::Guid(member.to_owned()), armed)?;
            wrote = wrote.saturating_add(1);
        }
    }
    Ok(Followed {
        wrote,
        gangs: mine.len(),
    })
}
