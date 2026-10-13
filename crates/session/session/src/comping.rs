//! The comp stack: COMP, then EDIT, then TUNE — and the take lanes
//! underneath, never touched.
//!
//! Twenty takes of a kit are twenty items on the kick, twenty on the
//! snare and twenty on each tom, so comping happens on the folder. What
//! it writes is REAPER's own model: a comp **is** a lane, its name is
//! the lane's name, and the chosen stretches are `LINKEDLANE` areas
//! pointing back at the take lanes. Nothing here invents a parallel
//! comping model beside the one the DAW already has.
//!
//! # Why a stack, and why the lower stages are read-only
//!
//! **COMP** is chosen from the take lanes and is the record of *what
//! was chosen*. **EDIT** is derived from COMP and is where every edit
//! lands — the slips, the quantize, the alignment, the hit list.
//! Vocals add **TUNE** above EDIT, because a retune is new audio rather
//! than a retime and wants a stage of its own.
//!
//! The take lanes and COMP are never edited. That is what makes the
//! whole thing non-destructive in a way an engineer can act on: a bad
//! edit is recoverable by deriving a fresh EDIT, and a re-comp does not
//! throw away the edits — it makes a new COMP and a new EDIT from it.
//!
//! # Why a kit comps as a group
//!
//! Choosing a take's region on the folder chooses the same region on
//! every source track of the kit, because a kit cut between its mics is
//! phase-broken and no amount of later editing repairs it. A piece can
//! be taken out of the group deliberately — a fixed snare hit from
//! another take — and that is simply that track's areas differing from
//! its siblings'.

use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::track::{Comp, CompArea};
use daw_proto::{DawResult, ProjectContext, TrackRef, Tracks};

/// A stage of the comp stack.
///
/// The order is the derivation order, and it is the whole contract:
/// each stage is made from the one below and never writes down into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Chosen from the take lanes. The record of what was chosen.
    Comp,
    /// Derived from COMP. Every edit lands here.
    Edit,
    /// Derived from EDIT, vocals only. Retuned audio.
    ///
    /// A stage of its own rather than an edit on EDIT, and the
    /// difference is what the two do to the audio. Alignment is a
    /// *retime*: it moves stretch markers and the samples underneath
    /// are the ones that were recorded. A retune is **new audio**, so
    /// writing it onto EDIT would overwrite the very thing the
    /// alignment was done to, and there would be no way back to the
    /// untuned performance short of re-comping.
    ///
    /// r[impl flow.vocals.tuning]
    Tune,
}

impl Stage {
    /// The lane name this stage carries.
    #[must_use]
    pub const fn lane_name(self) -> &'static str {
        match self {
            Self::Comp => "COMP",
            Self::Edit => "EDIT",
            Self::Tune => "TUNE",
        }
    }

    /// The stage this one is derived from, if any.
    #[must_use]
    pub const fn below(self) -> Option<Self> {
        match self {
            Self::Comp => None,
            Self::Edit => Some(Self::Comp),
            Self::Tune => Some(Self::Edit),
        }
    }

    /// Every stage up to and including this one, in derivation order.
    #[must_use]
    pub fn up_to(self) -> Vec<Self> {
        match self {
            Self::Comp => vec![Self::Comp],
            Self::Edit => vec![Self::Comp, Self::Edit],
            Self::Tune => vec![Self::Comp, Self::Edit, Self::Tune],
        }
    }

    /// Whether this stage may be edited.
    ///
    /// Only the top one may. Anything below is a record of how the top
    /// was arrived at, and rewriting it loses the way back.
    #[must_use]
    pub fn is_editable(self, top: Self) -> bool {
        self == top
    }
}

/// One stretch chosen from a take, before it is a `CompArea`.
///
/// Separate from the wire type because a choice is made once on the
/// folder and then written to every source track: it names no comp lane
/// of its own, since each track's comp lane is its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub start: PositionInSeconds,
    pub end: PositionInSeconds,
    /// The take lane the audio comes from.
    pub source_lane: u32,
}

/// Turn choices into the areas one track's comp lane needs.
///
/// Every boundary gets the session's crossfade, except the outer edges
/// of the comp: a fade into silence at the start of the take is not a
/// crossfade, it is a fade-in nobody asked for.
#[must_use]
pub fn areas(choices: &[Choice], comp_lane: u32, crossfade: Duration) -> Vec<CompArea> {
    let last = choices.len().saturating_sub(1);
    choices
        .iter()
        .enumerate()
        .map(|(i, choice)| CompArea {
            start: choice.start,
            end: choice.end,
            source_lane: choice.source_lane,
            comp_lane,
            fade_in: if i == 0 {
                Duration::from_seconds(0.0)
            } else {
                crossfade
            },
            fade_out: if i == last {
                Duration::from_seconds(0.0)
            } else {
                crossfade
            },
        })
        .collect()
}

/// Find a track's comp by name.
fn comp_named<'a>(comps: &'a [Comp], name: &str) -> Option<&'a Comp> {
    comps.iter().find(|comp| comp.name == name)
}

/// Build the stack on one track up to `top`, and leave the top active.
///
/// Idempotent: a stage that already exists is kept, not duplicated, so
/// running this on an opened project costs nothing and never orphans an
/// earlier comp.
///
/// r[impl flow.drums.comping.folder-lanes]
pub fn establish<D: Tracks + ?Sized>(
    daw: &D,
    project: &ProjectContext,
    track: &TrackRef,
    top: Stage,
) -> DawResult<Vec<(Stage, u32)>> {
    let mut out = Vec::new();
    for stage in top.up_to() {
        let existing = daw.comps(project.clone(), track.clone())?;
        let lane = match comp_named(&existing, stage.lane_name()) {
            Some(comp) => comp.lane,
            None => daw.create_comp(project.clone(), track.clone(), stage.lane_name())?,
        };
        out.push((stage, lane));
    }
    if let Some((_, lane)) = out.last() {
        daw.set_active_comp(project.clone(), track.clone(), Some(*lane))?;
    }
    Ok(out)
}

/// What a group comp wrote.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Comped {
    /// Tracks that took the choices.
    pub tracks: usize,
    /// Areas written per track.
    pub areas: usize,
}

/// Comp a group of tracks as one: the same choices on every one of
/// them, so the kit is never cut between its mics.
///
/// Each track's areas land on **its own** comp lane, looked up by name
/// rather than assumed to be the same index — two tracks can reach the
/// same stage on different lanes when they were recorded differently.
///
/// r[impl flow.drums.comping.group]
/// r[impl flow.drums.comping.crossfade]
pub fn comp_group<D: Tracks + ?Sized>(
    daw: &D,
    project: &ProjectContext,
    tracks: &[TrackRef],
    choices: &[Choice],
    stage: Stage,
    crossfade: Duration,
) -> DawResult<Comped> {
    let mut out = Comped::default();
    for track in tracks {
        let comps = daw.comps(project.clone(), track.clone())?;
        let Some(comp) = comp_named(&comps, stage.lane_name()) else {
            continue;
        };
        let written = areas(choices, comp.lane, crossfade);
        out.areas = written.len();
        daw.set_comp_areas(project.clone(), track.clone(), written)?;
        out.tracks += 1;
    }
    Ok(out)
}
