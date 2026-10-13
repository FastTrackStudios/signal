//! Growing a part on `daw-standalone` — `session_daw::grow`, what
//! `TRACK_DOUBLE`, `TRACK_ADD_ARRANGEMENT` and `TRACK_ADD_PART` run —
//! through phases 10–13 of the Original Composition spec
//! (`docs/spec/session/original-composition.md`), with Q1 and Q2 as the
//! user decided them: a double's sides are `Main` and `DBL`, and a part
//! named for its stack takes its role's name when an arrangement joins it.
//!
//! Built by hand as phase 9 leaves the guitar: `Guitar Clean` with its
//! rig, its input, armed, five take lanes and a comp lane `C1` with a comp
//! area on it — beside a `Bass` that nothing here may touch.

use daw::service::{
    Effects, FxChainContext, FxRef, FxTarget, ItemRef, Items, ProjectContext, Projects, TakeRef,
    Takes, TrackRef, Tracks,
};
use daw::standalone::Standalone;
use daw_proto::ProjectInfo;
use daw_proto::primitives::{Duration, PositionInSeconds};
use daw_proto::track::{CompArea, RecordInput};
use session_daw::grow;

/// The guitar's rig, as `RIG_LOAD` names it.
const RIG: &str = "FTS Signal Rig";
/// Takes 1–5 on lanes 0–4, the comp on lane 5.
const TAKES: u32 = 5;
const COMP: u32 = 5;
/// What the guitar records from.
const GUITAR_IN: RecordInput = RecordInput::Audio { channel: 2 };

fn add(daw: &Standalone, project: &ProjectContext, name: &str) -> String {
    Tracks::add(daw, project.clone(), name, None).expect("a track")
}

fn at(guid: &str) -> TrackRef {
    TrackRef::Guid(guid.to_owned())
}

/// An item `start`..`start + 4` s on `lane` of `track` (no lane: `None`),
/// with a take `offset` into its recording.
fn item(
    daw: &Standalone,
    project: &ProjectContext,
    track: &str,
    lane: Option<u32>,
    (start, offset): (f64, f64),
) {
    let guid = Items::add_item(
        daw,
        project.clone(),
        at(track),
        PositionInSeconds::from_seconds(start),
        Duration::from_seconds(4.0),
    )
    .expect("an item");
    if let Some(lane) = lane {
        Items::set_fixed_lane(daw, project.clone(), ItemRef::Guid(guid.clone()), lane)
            .expect("on its lane");
    }
    Takes::add_take(daw, project.clone(), ItemRef::Guid(guid.clone())).expect("its take");
    Takes::set_start_offset(
        daw,
        project.clone(),
        ItemRef::Guid(guid),
        TakeRef::Active,
        Duration::from_seconds(offset),
    )
    .expect("into its recording");
}

fn comp_area() -> CompArea {
    CompArea {
        start: PositionInSeconds::from_seconds(3.0),
        end: PositionInSeconds::from_seconds(7.0),
        source_lane: 1,
        comp_lane: COMP,
        fade_in: Duration::from_seconds(0.005),
        fade_out: Duration::from_seconds(0.005),
    }
}

/// `Bass` and `Guitar Clean` as phase 9 leaves them. The project, the
/// guitar's GUID and its rig's.
fn phase_9(daw: &Standalone) -> (ProjectContext, String, String) {
    let guid = daw.seed_project(ProjectInfo {
        guid: "grow-parts".into(),
        name: "Original Composition".into(),
        path: String::new(),
    });
    let project = ProjectContext::Project(guid);
    add(daw, &project, "Bass");
    let guitar = add(daw, &project, "Guitar Clean");
    let rig = Effects::add(
        daw,
        project.clone(),
        FxChainContext::Track(guitar.clone()),
        RIG,
    )
    .expect("the rig");
    Tracks::set_record_input(daw, project.clone(), at(&guitar), GUITAR_IN).expect("its input");
    Tracks::set_armed(daw, project.clone(), at(&guitar), true).expect("armed");
    Tracks::set_lane_count(daw, project.clone(), at(&guitar), TAKES + 1).expect("lanes");
    let names = (0..TAKES)
        .map(|l| (l, (l + 1).to_string()))
        .chain([(COMP, "C1".to_owned())]);
    for (lane, name) in names {
        Tracks::set_lane_name(daw, project.clone(), at(&guitar), lane, &name).expect("named");
    }
    Tracks::set_lane_play_mask(daw, project.clone(), at(&guitar), 1 << COMP).expect("C1 plays");
    for lane in 0..TAKES {
        item(
            daw,
            &project,
            &guitar,
            Some(lane),
            (3.0, f64::from(lane) * 10.0),
        );
    }
    item(daw, &project, &guitar, Some(COMP), (3.0, 10.0));
    Tracks::set_comp_areas(daw, project.clone(), at(&guitar), vec![comp_area()]).expect("an area");
    Tracks::set_active_comp(daw, project.clone(), at(&guitar), Some(COMP)).expect("comping");
    (project, guitar, rig)
}

/// Every track's path, in mixer order.
fn paths(daw: &Standalone, project: &ProjectContext) -> Vec<String> {
    let all = Tracks::all(daw, project.clone());
    all.iter()
        .map(|t| {
            let mut names = vec![t.name.clone()];
            let mut parent = t.parent_guid.clone();
            while let Some(p) = parent.and_then(|g| all.iter().find(|t| t.guid == g)) {
                names.push(p.name.clone());
                parent = p.parent_guid.clone();
            }
            names.reverse();
            names.join("/")
        })
        .collect()
}

/// The track whose whole path is `path`.
fn track(daw: &Standalone, project: &ProjectContext, path: &str) -> daw_proto::Track {
    let all = Tracks::all(daw, project.clone());
    let index = paths(daw, project)
        .iter()
        .position(|p| p == path)
        .unwrap_or_else(|| panic!("no track {path}: {:?}", paths(daw, project)));
    all[index].clone()
}

/// `guid`'s chain: each FX's name and GUID.
fn chain(daw: &Standalone, project: &ProjectContext, guid: &str) -> Vec<(String, String)> {
    Effects::list(daw, project.clone(), FxChainContext::Track(guid.to_owned()))
        .into_iter()
        .map(|f| (f.name, f.guid))
        .collect()
}

/// `guid`'s items: lane, start, offset into the recording — sorted.
fn items(daw: &Standalone, project: &ProjectContext, guid: &str) -> Vec<(Option<u32>, f64, f64)> {
    let mut out: Vec<(Option<u32>, f64, f64)> = Items::get_items(daw, project.clone(), at(guid))
        .into_iter()
        .map(|i| {
            (
                i.fixed_lane,
                i.position.as_seconds(),
                i.start_offset.as_seconds(),
            )
        })
        .collect();
    out.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
    });
    out
}

/// 10.1: the guitar becomes a folder over Main and DBL. The folder keeps
/// its GUID and its rig; Main has every take, lane and comp; DBL is empty
/// and records from the same input; they are panned hard left and right;
/// one undo puts the single track back with its takes and lanes.
#[test]
fn a_double_is_a_folder_over_main_and_dbl_with_the_rig_on_it() {
    let daw = Standalone::new();
    let (project, guitar, rig) = phase_9(&daw);
    let played = items(&daw, &project, &guitar);
    assert_eq!(played.len(), 6, "five takes and the comp");

    let said = grow::double(&daw, &project, &guitar).expect("doubled");
    assert_eq!(said, "Guitar Clean/{Main, DBL}");
    assert_eq!(
        paths(&daw, &project),
        [
            "Bass",
            "Guitar Clean",
            "Guitar Clean/Main",
            "Guitar Clean/DBL"
        ]
    );

    let folder = track(&daw, &project, "Guitar Clean");
    let main = track(&daw, &project, "Guitar Clean/Main");
    let dbl = track(&daw, &project, "Guitar Clean/DBL");
    assert_eq!(folder.guid, guitar, "the folder is the guitar's own track");
    assert_eq!(
        chain(&daw, &project, &guitar),
        [(RIG.to_owned(), rig)],
        "its rig, the same one"
    );
    assert!(
        chain(&daw, &project, &main.guid).is_empty(),
        "no rig on Main"
    );
    assert!(chain(&daw, &project, &dbl.guid).is_empty(), "no rig on DBL");

    assert_eq!(
        items(&daw, &project, &main.guid),
        played,
        "every take, each on its lane"
    );
    assert!(items(&daw, &project, &guitar).is_empty());
    assert!(items(&daw, &project, &dbl.guid).is_empty(), "DBL is empty");
    assert_eq!(main.lane_count, TAKES + 1);
    assert_eq!(main.lane_names, ["1", "2", "3", "4", "5", "C1"]);
    assert_eq!(main.lane_play_mask, 1 << COMP, "the comp plays");
    let comping = Tracks::comping(&daw, project.clone(), at(&main.guid)).expect("Main's comping");
    assert_eq!(comping.areas, [comp_area()], "the comp area came with it");
    assert_eq!(comping.comp_lane, Some(COMP));
    assert_eq!(folder.lane_count, 0, "a folder has no lanes");
    assert_eq!(dbl.lane_count, 0);

    assert!(
        (main.pan + 1.0).abs() < 1e-9,
        "Main hard left: {}",
        main.pan
    );
    assert!((dbl.pan - 1.0).abs() < 1e-9, "DBL hard right: {}", dbl.pan);
    assert_eq!(main.record_input, GUITAR_IN);
    assert_eq!(dbl.record_input, GUITAR_IN, "the double inherits the input");
    assert_eq!(folder.record_input, RecordInput::None);
    assert!(
        main.armed && !folder.armed,
        "the arm went with the performance"
    );
    let bass = track(&daw, &project, "Bass");
    assert!(
        chain(&daw, &project, &bass.guid).is_empty(),
        "the bass is left"
    );

    assert!(Projects::undo(&daw, project.clone()), "one step");
    assert_eq!(paths(&daw, &project), ["Bass", "Guitar Clean"]);
    assert_eq!(items(&daw, &project, &guitar), played, "the takes are back");
    let back = track(&daw, &project, "Guitar Clean");
    assert_eq!(back.lane_count, TAKES + 1, "and its lanes");
    assert_eq!(back.lane_play_mask, 1 << COMP);
}

/// 11.1, with Q2: Add Arrangement on the doubled guitar puts a Guitar
/// folder round it, renames it Rhythm, and adds a plain Lead with no rig
/// and no items. Left to name it, it names it Lead — the same tree as
/// asking for Lead.
#[test]
fn an_arrangement_renames_the_part_to_its_role_and_adds_the_lead() {
    for name in [None, Some("Lead")] {
        let daw = Standalone::new();
        let (project, guitar, rig) = phase_9(&daw);
        grow::double(&daw, &project, &guitar).expect("doubled");
        let played = items(
            &daw,
            &project,
            &track(&daw, &project, "Guitar Clean/Main").guid,
        );

        let said = grow::add_arrangement(&daw, &project, &guitar, name).expect("arranged");
        assert_eq!(said, "Guitar/{Rhythm, Lead}", "{name:?}");
        assert_eq!(
            paths(&daw, &project),
            [
                "Bass",
                "Guitar",
                "Guitar/Rhythm",
                "Guitar/Rhythm/Main",
                "Guitar/Rhythm/DBL",
                "Guitar/Lead",
            ]
        );
        let instrument = track(&daw, &project, "Guitar");
        let rhythm = track(&daw, &project, "Guitar/Rhythm");
        let lead = track(&daw, &project, "Guitar/Lead");
        assert!(
            chain(&daw, &project, &instrument.guid).is_empty(),
            "no rig on Guitar"
        );
        assert_eq!(rhythm.guid, guitar, "Rhythm is the part that was");
        assert_eq!(
            chain(&daw, &project, &guitar),
            [(RIG.to_owned(), rig.clone())]
        );
        assert!(
            chain(&daw, &project, &lead.guid).is_empty(),
            "the lead's rig is loaded later"
        );
        assert!(items(&daw, &project, &lead.guid).is_empty());
        assert!(!lead.is_folder, "neither doubled nor a folder");
        assert_eq!(
            lead.record_input, GUITAR_IN,
            "the same instrument, the same input"
        );
        let main = track(&daw, &project, "Guitar/Rhythm/Main");
        assert_eq!(
            items(&daw, &project, &main.guid),
            played,
            "the takes stay on Main"
        );

        assert!(Projects::undo(&daw, project.clone()), "one step");
        assert_eq!(
            paths(&daw, &project),
            [
                "Bass",
                "Guitar Clean",
                "Guitar Clean/Main",
                "Guitar Clean/DBL"
            ]
        );
    }
}

/// Phases 10–13 one after another — the double, the lead, the lead's
/// double, the harmony and its double — give phase 14's guitar, with Q1's
/// names for the sides: a rig on exactly Rhythm, Lead/Main and
/// Lead/Harmony, none on a side; every side panned.
#[test]
fn the_guitar_grows_into_the_finished_songs_tree() {
    let daw = Standalone::new();
    let (project, guitar, _) = phase_9(&daw);
    grow::double(&daw, &project, &guitar).expect("10.1");
    grow::add_arrangement(&daw, &project, &guitar, Some("Lead")).expect("11.1");

    // 11.2 and 11.3, as RIG_LOAD and a recording would leave them.
    let lead = track(&daw, &project, "Guitar/Lead").guid;
    let lead_rig = Effects::add(
        &daw,
        project.clone(),
        FxChainContext::Track(lead.clone()),
        RIG,
    )
    .expect("the lead's rig");
    item(&daw, &project, &lead, None, (3.0, 0.0));
    item(&daw, &project, &lead, None, (15.0, 0.0));
    let lead_takes = items(&daw, &project, &lead);

    // 12.1: the lead doubled; its rig stays on it.
    let said = grow::double(&daw, &project, &lead).expect("12.1");
    assert_eq!(said, "Guitar/Lead/{Main, DBL}");
    assert_eq!(
        chain(&daw, &project, &lead),
        [(RIG.to_owned(), lead_rig.clone())]
    );

    // 13.1: Add Part on the lead. Its rig goes down with it onto Main —
    // the same instance — and Harmony gets a copy.
    let said = grow::add_part(&daw, &project, &lead, Some("Harmony")).expect("13.1");
    assert_eq!(said, "Guitar/Lead/{Main, Harmony}");
    let folder = track(&daw, &project, "Guitar/Lead");
    let main = track(&daw, &project, "Guitar/Lead/Main");
    let harmony = track(&daw, &project, "Guitar/Lead/Harmony");
    assert!(
        chain(&daw, &project, &folder.guid).is_empty(),
        "no rig on Lead"
    );
    assert_eq!(main.guid, lead, "Lead/Main is the lead that was");
    assert_eq!(
        chain(&daw, &project, &lead),
        [(RIG.to_owned(), lead_rig.clone())]
    );
    let copy = chain(&daw, &project, &harmony.guid);
    assert_eq!(copy.len(), 1, "{copy:?}");
    assert_eq!(copy[0].0, RIG, "a copy of the rig");
    assert_ne!(copy[0].1, lead_rig, "its own instance");
    assert!(items(&daw, &project, &harmony.guid).is_empty());
    let played = track(&daw, &project, "Guitar/Lead/Main/Main");
    assert_eq!(
        items(&daw, &project, &played.guid),
        lead_takes,
        "the lead's takes stay"
    );

    // 13.2: the harmony doubled.
    let said = grow::double(&daw, &project, &harmony.guid).expect("13.2");
    assert_eq!(said, "Guitar/Lead/Harmony/{Main, DBL}");

    // 14.1's guitar.
    assert_eq!(
        paths(&daw, &project),
        [
            "Bass",
            "Guitar",
            "Guitar/Rhythm",
            "Guitar/Rhythm/Main",
            "Guitar/Rhythm/DBL",
            "Guitar/Lead",
            "Guitar/Lead/Main",
            "Guitar/Lead/Main/Main",
            "Guitar/Lead/Main/DBL",
            "Guitar/Lead/Harmony",
            "Guitar/Lead/Harmony/Main",
            "Guitar/Lead/Harmony/DBL",
        ]
    );
    let all = Tracks::all(&daw, project.clone());
    let names = paths(&daw, &project);
    let rigged: Vec<&str> = all
        .iter()
        .zip(&names)
        .filter(|(t, _)| chain(&daw, &project, &t.guid).iter().any(|(n, _)| n == RIG))
        .map(|(_, p)| p.as_str())
        .collect();
    assert_eq!(
        rigged,
        ["Guitar/Rhythm", "Guitar/Lead/Main", "Guitar/Lead/Harmony"]
    );
    // The sides of a double: the leaves.
    for (t, path) in all
        .iter()
        .zip(&names)
        .filter(|(t, path)| !t.is_folder && path.contains('/'))
    {
        let want = if t.name == "Main" { -1.0 } else { 1.0 };
        assert!((t.pan - want).abs() < 1e-9, "{path} pan {}", t.pan);
    }
}

/// A part already doubled is not doubled again — picked as itself or as
/// a side — and a new part may not be called Main or DBL, nor an
/// arrangement by a name already there. A refusal leaves no step.
#[test]
fn a_grow_with_nothing_to_do_is_refused_and_leaves_no_step() {
    let daw = Standalone::new();
    let (project, guitar, _) = phase_9(&daw);
    grow::double(&daw, &project, &guitar).expect("doubled");
    let label = Projects::last_undo_label(&daw, project.clone());
    let main = track(&daw, &project, "Guitar Clean/Main").guid;
    let before = paths(&daw, &project);

    assert!(
        grow::double(&daw, &project, &guitar).is_err(),
        "doubled already"
    );
    assert!(
        grow::double(&daw, &project, &main).is_err(),
        "a side means its part"
    );
    assert!(grow::add_part(&daw, &project, &guitar, Some("DBL")).is_err());
    assert!(grow::add_part(&daw, &project, &guitar, Some("main")).is_err());
    assert!(
        grow::add_arrangement(&daw, &project, &guitar, Some("Rhythm")).is_err(),
        "the part's own new name"
    );
    assert!(grow::double(&daw, &project, "nowhere").is_err());

    assert_eq!(
        Projects::last_undo_label(&daw, project.clone()),
        label,
        "no step"
    );
    assert_eq!(paths(&daw, &project), before, "and nothing changed");
}

/// Add Part on a part never doubled makes it Main under its own name; on
/// a part already beside a Main, the next goes in beside them — each a
/// copy of Main's rig, its settings with it.
#[test]
fn parts_beside_a_main_each_take_a_copy_of_its_rig() {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "grow-layers".into(),
        name: "Layers".into(),
        path: String::new(),
    });
    let project = ProjectContext::Project(guid);
    let lead = add(&daw, &project, "Guitar Lead");
    let amp = Effects::add(
        &daw,
        project.clone(),
        FxChainContext::Track(lead.clone()),
        "Test Amp",
    )
    .expect("an amp");
    Effects::set_parameter(
        &daw,
        project.clone(),
        daw_proto::fx::SetParameterRequest {
            target: FxTarget::new(FxChainContext::Track(lead.clone()), FxRef::Guid(amp)),
            index: 0,
            value: 0.75,
        },
    )
    .expect("set");

    let said = grow::add_part(&daw, &project, &lead, None).expect("a part");
    assert_eq!(said, "Guitar Lead/{Main, Harmony}");
    let said = grow::add_part(&daw, &project, &lead, None).expect("another");
    assert_eq!(
        said, "Guitar Lead/{Main, Harmony, OCT}",
        "from Main, beside the others"
    );
    assert_eq!(
        paths(&daw, &project),
        [
            "Guitar Lead",
            "Guitar Lead/Main",
            "Guitar Lead/Harmony",
            "Guitar Lead/OCT",
        ]
    );
    for part in ["Guitar Lead/Harmony", "Guitar Lead/OCT"] {
        let copy = track(&daw, &project, part).guid;
        let fx = chain(&daw, &project, &copy);
        assert_eq!(fx.len(), 1, "{part}: {fx:?}");
        let value = Effects::parameters(
            &daw,
            project.clone(),
            FxTarget::new(
                FxChainContext::Track(copy.clone()),
                FxRef::Guid(fx[0].1.clone()),
            ),
        )
        .into_iter()
        .find(|p| p.index == 0)
        .map(|p| p.value);
        assert_eq!(value, Some(0.75), "{part}: Main's settings");
    }
    assert!(
        chain(&daw, &project, &track(&daw, &project, "Guitar Lead").guid).is_empty(),
        "no rig on the folder"
    );
}
