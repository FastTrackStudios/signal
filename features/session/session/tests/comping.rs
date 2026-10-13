//! The comp stack, driven against daw-standalone and read back.
//!
//! These prove the two things the stack exists for: a kit comps as one
//! so it is never cut between its mics, and the stages below the top
//! are never written, so a bad edit is always recoverable.

use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::{ProjectContext, ProjectInfo, TrackRef, Tracks};
use daw_standalone::sync::Standalone;
use session::comping::{Choice, Stage, areas, comp_group, establish};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const MICS: [&str; 4] = ["Kick In", "Snare Top", "Tom 1", "OH"];
/// The kit was played four times, so there are four take lanes to comp
/// between before the stack adds COMP and EDIT above them.
const TAKES: u32 = 4;

struct Kit {
    daw: Standalone,
    project: ProjectContext,
    tracks: Vec<TrackRef>,
}

fn kit() -> Kit {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "kit".into(),
        name: "kit".into(),
        path: String::new(),
    });
    let project = ProjectContext::Project(guid);
    // Four take lanes on every mic — the kit was played four times, and
    // a comp chooses between those, not between the comp lanes the
    // stack adds above them.
    let tracks: Vec<TrackRef> = MICS
        .iter()
        .map(|name| {
            let guid = Tracks::add(&daw, project.clone(), name, None).expect("a track");
            let track = TrackRef::Guid(guid);
            daw.set_lane_count(project.clone(), track.clone(), TAKES)
                .expect("the backend sets a lane count");
            track
        })
        .collect();
    Kit {
        daw,
        project,
        tracks,
    }
}

fn choices() -> Vec<Choice> {
    vec![
        Choice {
            start: PositionInSeconds::from_seconds(0.0),
            end: PositionInSeconds::from_seconds(4.0),
            source_lane: 0,
        },
        Choice {
            start: PositionInSeconds::from_seconds(4.0),
            end: PositionInSeconds::from_seconds(8.0),
            source_lane: 1,
        },
    ]
}

/// The stack is built in derivation order and the **top** is the active
/// comp — so an edit lands on EDIT and not on the record of what was
/// chosen.
///
/// r[verify flow.drums.comping.folder-lanes]
#[test]
fn the_stack_builds_in_order_and_leaves_the_top_active() -> Result {
    let kit = kit();
    let track = kit.tracks[0].clone();
    let built = establish(&kit.daw, &kit.project, &track, Stage::Edit)?;
    assert_eq!(
        built.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        [Stage::Comp, Stage::Edit],
        "COMP must exist before EDIT is derived from it"
    );

    let comps = kit.daw.comps(kit.project.clone(), track)?;
    let active: Vec<&str> = comps
        .iter()
        .filter(|c| c.is_active)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        active,
        ["EDIT"],
        "the editable stage must be the active one"
    );
    Ok(())
}

/// Establishing twice keeps the same lanes: an opened project costs
/// nothing and never orphans an earlier comp.
///
/// r[verify flow.drums.comping.folder-lanes]
#[test]
fn establishing_twice_is_idempotent() -> Result {
    let kit = kit();
    let track = kit.tracks[0].clone();
    let first = establish(&kit.daw, &kit.project, &track, Stage::Edit)?;
    let again = establish(&kit.daw, &kit.project, &track, Stage::Edit)?;
    assert_eq!(first, again, "a second pass made new lanes");
    Ok(())
}

/// **The rule the kit depends on.** A choice made on the folder lands
/// identically on every source track, because a kit cut between its
/// mics is phase-broken and no later edit repairs it.
///
/// r[verify flow.drums.comping.group]
#[test]
fn a_group_comp_writes_the_same_regions_to_every_mic() -> Result {
    let kit = kit();
    for track in &kit.tracks {
        establish(&kit.daw, &kit.project, track, Stage::Edit)?;
    }
    let out = comp_group(
        &kit.daw,
        &kit.project,
        &kit.tracks,
        &choices(),
        Stage::Edit,
        Duration::from_seconds(0.01),
    )?;
    assert_eq!(out.tracks, MICS.len(), "not every mic took the comp");

    let read = |track: &TrackRef| -> Vec<(f64, f64, u32)> {
        kit.daw
            .comping(kit.project.clone(), track.clone())
            .expect("comping reads back")
            .areas
            .iter()
            .map(|a| (a.start.as_seconds(), a.end.as_seconds(), a.source_lane))
            .collect()
    };
    let first = read(&kit.tracks[0]);
    assert_eq!(first.len(), 2, "both chosen stretches were written");
    for track in &kit.tracks[1..] {
        assert_eq!(read(track), first, "a mic was cut differently from the kit");
    }
    Ok(())
}

/// A piece can be taken out of the group deliberately — a fixed snare
/// hit from another take — and that is simply its areas differing from
/// its siblings'. The rest of the kit stays as it was.
///
/// r[verify flow.drums.comping.group]
#[test]
fn a_piece_taken_out_of_the_group_differs_and_the_rest_do_not() -> Result {
    let kit = kit();
    for track in &kit.tracks {
        establish(&kit.daw, &kit.project, track, Stage::Edit)?;
    }
    let fade = Duration::from_seconds(0.01);
    comp_group(
        &kit.daw,
        &kit.project,
        &kit.tracks,
        &choices(),
        Stage::Edit,
        fade,
    )?;

    // The snare alone takes a different take for the second stretch.
    let mut its_own = choices();
    its_own[1].source_lane = 3;
    let snare = kit.tracks[1].clone();
    comp_group(
        &kit.daw,
        &kit.project,
        std::slice::from_ref(&snare),
        &its_own,
        Stage::Edit,
        fade,
    )?;

    let lanes = |track: &TrackRef| -> Vec<u32> {
        kit.daw
            .comping(kit.project.clone(), track.clone())
            .expect("comping reads back")
            .areas
            .iter()
            .map(|a| a.source_lane)
            .collect()
    };
    assert_eq!(lanes(&snare), [0, 3], "the snare kept the group's choice");
    assert_eq!(
        lanes(&kit.tracks[0]),
        [0, 1],
        "taking the snare out moved another mic"
    );
    Ok(())
}

/// Crossfades sit at the boundaries between chosen stretches and **not**
/// at the outer edges: a fade into silence at the start of a take is
/// not a crossfade, it is a fade-in nobody asked for.
///
/// r[verify flow.drums.comping.crossfade]
#[test]
fn crossfades_are_at_the_joins_and_not_at_the_edges() {
    let fade = Duration::from_seconds(0.02);
    let written = areas(&choices(), 4, fade);
    assert_eq!(
        written[0].fade_in.as_seconds(),
        0.0,
        "faded in from nothing"
    );
    assert_eq!(
        written[0].fade_out.as_seconds(),
        fade.as_seconds(),
        "the join lost its fade"
    );
    assert_eq!(
        written[1].fade_in.as_seconds(),
        fade.as_seconds(),
        "the join lost its fade"
    );
    assert_eq!(
        written[1].fade_out.as_seconds(),
        0.0,
        "faded out into nothing"
    );
}

/// Only the top stage may be edited. The stages below are the record of
/// how it was arrived at, and rewriting one loses the way back.
#[test]
fn only_the_top_of_the_stack_is_editable() {
    assert!(Stage::Edit.is_editable(Stage::Edit));
    assert!(
        !Stage::Comp.is_editable(Stage::Edit),
        "COMP is not editable"
    );
    assert!(
        !Stage::Edit.is_editable(Stage::Tune),
        "under TUNE, EDIT is not"
    );
    assert_eq!(Stage::Tune.below(), Some(Stage::Edit));
    assert_eq!(
        Stage::Comp.below(),
        None,
        "COMP derives from the take lanes"
    );
}

// ── tuning: the third stage ──────────────────────────────────────────

/// **TUNE is a stage of its own, and that is the decision.**
///
/// Alignment is a retime, so it lives on EDIT as stretch markers and
/// the audio underneath never changes. Tuning is not: a retuned vocal
/// is *new audio*, and putting it on EDIT would overwrite the thing the
/// alignment was done to. So it gets a stage above, and EDIT stays the
/// way back.
///
/// r[verify flow.vocals.tuning]
#[test]
fn tuning_lands_above_edit_and_leaves_it_alone() -> Result {
    let kit = kit();
    let track = kit.tracks[0].clone();
    let built = establish(&kit.daw, &kit.project, &track, Stage::Tune)?;

    assert_eq!(
        built.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        [Stage::Comp, Stage::Edit, Stage::Tune],
        "TUNE is derived from EDIT, which is derived from COMP"
    );

    let comps = kit.daw.comps(kit.project.clone(), track.clone())?;
    let active: Vec<&str> = comps
        .iter()
        .filter(|c| c.is_active)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(active, ["TUNE"], "the top of the stack is what is edited");

    // EDIT is still there, below it, untouched — which is what makes a
    // retune recoverable.
    assert!(
        comps.iter().any(|c| c.name == "EDIT"),
        "tuning consumed the stage it was derived from: {comps:?}"
    );
    assert!(!Stage::Edit.is_editable(Stage::Tune));
    Ok(())
}

/// A vocal that is only comped and edited has no TUNE stage: the stack
/// grows to what a take actually needs, so an instrument that is never
/// tuned does not carry an empty stage pretending it might be.
///
/// r[verify flow.vocals.tuning]
#[test]
fn a_take_that_was_never_tuned_has_no_tune_stage() -> Result {
    let kit = kit();
    let track = kit.tracks[0].clone();
    establish(&kit.daw, &kit.project, &track, Stage::Edit)?;
    let comps = kit.daw.comps(kit.project.clone(), track)?;
    assert!(
        !comps.iter().any(|c| c.name == "TUNE"),
        "an untuned take grew a TUNE stage: {comps:?}"
    );
    Ok(())
}
