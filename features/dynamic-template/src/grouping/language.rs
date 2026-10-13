//! The active-language switch: one write, and nothing left audible.
//!
//! A session in three languages has one **active** language, and every
//! vocal scene follows it. Switching is one action, and it has to be:
//! "look at the English version" that leaves a Spanish double audible
//! under it is worse than not switching at all, because the mix sounds
//! wrong and nothing on screen says why.
//!
//! # Why this writes each track's mute, and not a VCA's
//!
//! The decision (#30, amended) reached for a VCA per language and then
//! for the mute gang as a fallback. Measured against REAPER 7.75
//! (daw#12), **neither moves a follower**: muting a VCA lead leaves the
//! follower's mute alone, and the mute gang never engages because the
//! facade's `set_muted` passes `GroupingBehavior::PreventGrouping`, so
//! it exercises no gang of any family.
//!
//! So the switch writes each source's mute directly. That is not a
//! retreat: the set is known exactly (it is a projection of the
//! Language dimension), the write is a diff like every other gang's, and
//! it behaves identically on daw-standalone — which gates no audio
//! through REAPER's grouping at all. The language VCA track still earns
//! its slot as a **fader**; it is simply not how the muting happens.
//!
//! `All` is never muted. A wordless "Hey!" or a hummed pad belongs to
//! every version, and muting it with a language would silently thin
//! every render but the active one.

use daw_proto::{DawResult, ProjectContext, TrackRef, Tracks};

use crate::scenes::{Fact, Language};

/// What the switch did, for the span that reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switched {
    /// Sources muted because their language is not the active one.
    pub muted: usize,
    /// Sources unmuted because they are the active language, or `All`.
    pub unmuted: usize,
}

/// Whether a fact is a vocal source the switch owns.
///
/// Only tracks carrying a language are touched. The mix tracks above
/// them — a performer's own folder, `Main`, `DBL`, a BGV part — carry no
/// language, which is exactly what keeps them in view and audible
/// whatever is active.
fn sung(fact: &Fact) -> Option<Language> {
    fact.language.filter(|_| !fact.is_folder)
}

/// Whether a source should be heard when `active` is the language.
#[must_use]
pub fn audible(language: Language, active: Language) -> bool {
    language == Language::All || language == active
}

/// Set every vocal source's mute from the active language.
///
/// Writes only what differs, so running it twice writes nothing the
/// second time — the same echo guard as the gangs, for the same reason.
///
/// # Errors
///
/// When a mute cannot be written: the caller sees the first failure
/// rather than a partial switch reported as a success, because a
/// half-applied language is the one state this must never leave behind
/// quietly.
///
/// r[impl flow.vocals.language.active]
pub fn follow_language<D: Tracks + ?Sized>(
    daw: &D,
    project: &ProjectContext,
    facts: &[Fact],
    active: Language,
) -> DawResult<Switched> {
    let mut out = Switched::default();
    for fact in facts {
        let Some(language) = sung(fact) else { continue };
        let wanted = !audible(language, active);
        let current = daw
            .get(project.clone(), TrackRef::Guid(fact.guid.clone()))
            .is_some_and(|track| track.muted);
        if current == wanted {
            continue;
        }
        daw.set_muted(project.clone(), TrackRef::Guid(fact.guid.clone()), wanted)?;
        if wanted {
            out.muted = out.muted.saturating_add(1);
        } else {
            out.unmuted = out.unmuted.saturating_add(1);
        }
    }
    Ok(out)
}
