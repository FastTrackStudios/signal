//! `flow.vocals.language` / `flow.vocals.language.active`, on
//! `daw-standalone` — no REAPER, no `.rpp` fixture.
//!
//! Builds a lead's own shape by hand (`Vocals / Ron / {Main, DBL} /
//! {EN, ES, PT}`) with `daw-standalone`'s own `Tracks::add` +
//! `set_folder_depth`, switches the active language through the same
//! `ExtState` the REAPER applier reads, and resolves the scene engine
//! against the live track list the way `session-daw`'s own `plan`
//! module does. This is the "scenario" half of #59's acceptance
//! criteria — the classifier's own unit tests live in
//! `dynamic-template`.

use daw::service::{ProjectContext, Track, TrackRef, Tracks};
use daw::standalone::Standalone;
use daw_proto::ProjectInfo;
use dynamic_template::scenes::{self, Audience, Effect, GroupBy, Language, Scene, Size, Surface};

/// Append a track, folded by `folder_depth` the way a `.RPP` would
/// encode it: positive opens that many folders, negative closes them,
/// zero is a plain track or the last child of one closing several at
/// once.
fn append(daw: &Standalone, project: ProjectContext, name: &str, folder_depth: i32) -> String {
    let guid = Tracks::add(daw, project.clone(), name, None).expect("add");
    if folder_depth != 0 {
        Tracks::set_folder_depth(daw, project, TrackRef::Guid(guid.clone()), folder_depth)
            .expect("set_folder_depth");
    }
    guid
}

/// `Vocals / Ron / {Main, DBL} / {EN, ES, PT}` — a lead's two layers,
/// each with a source per language, built with no taxonomy ext-state at
/// all: the language dimension is read from the tracks' own names.
fn ron_lead(daw: &Standalone, project: ProjectContext) {
    append(daw, project.clone(), "Vocals", 1);
    append(daw, project.clone(), "Ron", 1);
    append(daw, project.clone(), "Main", 1);
    append(daw, project.clone(), "EN", 0);
    append(daw, project.clone(), "ES", 0);
    append(daw, project.clone(), "PT", -1); // closes Main
    append(daw, project.clone(), "DBL", 1);
    append(daw, project.clone(), "EN", 0);
    append(daw, project.clone(), "ES", 0);
    append(daw, project, "PT", -3); // closes DBL, Ron, Vocals
}

/// The live track list, each with the depth it sits at — the same
/// formula `dynamic_template::daw_module::live_rows` runs over a real
/// REAPER project's `I_FOLDERDEPTH`.
fn rows(daw: &Standalone, project: ProjectContext) -> Vec<(Track, u32)> {
    let mut depth: i32 = 0;
    Tracks::all(daw, project)
        .into_iter()
        .map(|track| {
            let at = u32::try_from(depth.max(0)).unwrap_or(0);
            depth = depth.saturating_add(track.folder_depth);
            (track, at)
        })
        .collect()
}

/// A scene with no rules of its own: whatever the row list shows or
/// hides comes entirely from the common prelude, which is exactly what
/// this scenario is testing.
fn bare_scene() -> Scene {
    Scene {
        name: "Bare".to_owned(),
        slug: "bare".to_owned(),
        short: "Bar".to_owned(),
        instrument: "vocals".to_owned(),
        modes: Vec::new(),
        audience: Audience::Engineer,
        group_by: GroupBy::Arrangement,
        spec: Vec::new(),
        default: Effect::at(Size::Compact),
        rules: Vec::new(),
    }
}

/// Setting the active language and reading the resolved row list back:
/// that language's sources and `All` show, the other languages' sources
/// hide, and the mix tracks (`Vocals`, `Ron`, `Main`, `DBL`) stay in
/// view whatever the language.
// r[verify flow.vocals.language]
// r[verify flow.vocals.language.active]
#[test]
fn switching_the_active_language_hides_the_others_on_daw_standalone() {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "lead-ron".into(),
        name: "Lead Ron".into(),
        path: String::new(),
    });
    let project = ProjectContext::Project(guid);

    ron_lead(&daw, project.clone());

    // No switch yet: the scene shows every language, because the
    // project has never had one to hide by.
    assert_eq!(scenes::get_active_language(&daw, project.clone()), None);
    let tracks = rows(&daw, project.clone());
    let facts = scenes::from_tracks(
        &tracks,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    );
    let names = |rows: &[scenes::Row]| -> Vec<String> {
        rows.iter()
            .filter_map(scenes::Row::guid)
            .filter_map(|guid| tracks.iter().find(|(t, _)| t.guid == guid))
            .map(|(t, _)| t.name.clone())
            .collect()
    };
    let before = scenes::resolve(&bare_scene(), &facts, Surface::Mixer, None, None, None);
    assert_eq!(before.len(), tracks.len(), "nothing hidden before a switch");

    // One write.
    scenes::set_active_language(&daw, project.clone(), Language::Es).expect("one write");
    assert_eq!(
        scenes::get_active_language(&daw, project.clone()),
        Some(Language::Es)
    );

    let active = scenes::get_active_language(&daw, project.clone());
    let rows_after = scenes::resolve(&bare_scene(), &facts, Surface::Mixer, None, active, None);
    let shown = names(&rows_after);
    assert_eq!(
        shown,
        ["Vocals", "Ron", "Main", "ES", "DBL", "ES"],
        "only the active language's sources survive; the mix tracks stay"
    );

    // Switching again is one write, not an accumulation, and the row
    // list follows in the same step.
    scenes::set_active_language(&daw, project.clone(), Language::En).expect("one write");
    let active = scenes::get_active_language(&daw, project.clone());
    let rows_after = scenes::resolve(&bare_scene(), &facts, Surface::Mixer, None, active, None);
    assert_eq!(
        names(&rows_after),
        ["Vocals", "Ron", "Main", "EN", "DBL", "EN"]
    );
}
